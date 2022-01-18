use byteorder::ByteOrder;
use byteorder::LE;
use std::io::IoSlice;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite};

use super::{StreamBody, StreamConfig};
use crate::error::private::check_payload_too_large;
use crate::{Error, Message, MessageHeader};

/// Length of a message frame and header.
const FRAMED_HEADER_LEN: usize = 4 + crate::HEADER_LEN as usize;

/// Transport layer for byte-stream sockets.
#[allow(dead_code)] // Fields are not used when transports are disabled.
pub struct StreamTransport<Stream> {
	/// The stream to use for sending/receiving messages.
	pub(super) stream: Stream,

	/// The configuration of the transport.
	pub(super) config: StreamConfig,
}

/// The read half of a [`StreamTransport`].
#[allow(dead_code)] // Not used when transports are disabled.
pub struct StreamReadHalf<R> {
	/// The read half of the underlying stream.
	pub(super) stream: R,

	/// The maximum body length to accept when reading messages.
	pub(super) max_body_len: u32,

	/// The number of bytes read for the current message.
	pub(super) bytes_read: usize,

	/// The buffer for reading the message header.
	pub(super) header_buffer: [u8; FRAMED_HEADER_LEN],

	/// The parsed header.
	pub(super) parsed_header: MessageHeader,

	/// The buffer for reading the message body.
	pub(super) body_buffer: Vec<u8>,
}

/// The write half of a [`StreamTransport`].
#[allow(dead_code)] // Not used when transports are disabled.
pub struct StreamWriteHalf<W> {
	/// The write half of the underlying stream.
	pub(super) stream: W,

	/// The maximum body length to enforce for messages.
	pub(super) max_body_len: u32,

	/// The number of bytes written for the current message.
	pub(super) bytes_written: usize,

	/// The buffer for the encoded message size and header.
	pub(super) header_buffer: Option<[u8; FRAMED_HEADER_LEN]>,
}

impl<Stream> StreamTransport<Stream>
where
	Self: crate::transport::Transport,
{
	/// Create a new transport with custom configuration.
	pub fn new(stream: Stream, config: StreamConfig) -> Self {
		Self { stream, config }
	}

	/// Create a new transport using the default configuration.
	pub fn new_default(stream: Stream) -> Self {
		Self::new(stream, StreamConfig::default())
	}
}

impl<R> StreamReadHalf<R> {
	#[allow(dead_code)] // Not used when transports are disabled.
	pub(super) fn new(stream: R, max_body_len: u32) -> Self {
		Self {
			stream,
			max_body_len,
			header_buffer: [0u8; FRAMED_HEADER_LEN],
			bytes_read: 0,
			parsed_header: MessageHeader::request(0, 0),
			body_buffer: Vec::new(),
		}
	}
}

impl<W> StreamWriteHalf<W> {
	#[allow(dead_code)] // Not used when transports are disabled.
	pub(super) fn new(stream: W, max_body_len: u32) -> Self {
		Self {
			stream,
			max_body_len,
			header_buffer: None,
			bytes_written: 0,
		}
	}
}

/// Wrapper around [`AsyncRead::poll_read`] that turns zero-sized reads into ConnectionAborted errors.
fn poll_read<R: AsyncRead>(stream: Pin<&mut R>, context: &mut Context, buf: &mut [u8]) -> Poll<std::io::Result<usize>> {
	let mut buf = tokio::io::ReadBuf::new(buf);
	ready!(stream.poll_read(context, &mut buf))?;
	if buf.filled().is_empty() {
		Poll::Ready(Err(std::io::ErrorKind::ConnectionAborted.into()))
	} else {
		Poll::Ready(Ok(buf.filled().len()))
	}
}

impl<R> crate::transport::TransportReadHalf for StreamReadHalf<R>
where
	R: AsyncRead + Send + Unpin,
{
	type Body = StreamBody;

	fn poll_read_msg(self: Pin<&mut Self>, context: &mut Context) -> Poll<Result<Message<Self::Body>, Error>> {
		// Get the original &mut Self from the pin.
		let this = self.get_mut();

		// Keep polling until the whole frame + header is received.
		while this.bytes_read < FRAMED_HEADER_LEN {
			// Read more header data.
			let stream = Pin::new(&mut this.stream);
			this.bytes_read += ready!(poll_read(stream, context, &mut this.header_buffer[this.bytes_read..]))?;
			assert!(this.bytes_read <= FRAMED_HEADER_LEN);

			// Check if we have the whole frame + header.
			if this.bytes_read == FRAMED_HEADER_LEN {
				// Parse frame and header.
				let length = LE::read_u32(&this.header_buffer[0..]);
				this.parsed_header = MessageHeader::decode(&this.header_buffer[4..])?;

				// Check body length and create body buffer.
				let body_len = length - crate::HEADER_LEN as u32;
				check_payload_too_large(body_len as usize, this.max_body_len as usize)?;
				this.body_buffer = vec![0; body_len as usize];
			}
		}

		// Keep polling until we have the whole body.
		while this.bytes_read - FRAMED_HEADER_LEN < this.body_buffer.len() {
			// Read body data.
			let stream = Pin::new(&mut this.stream);
			let body_read = this.bytes_read - FRAMED_HEADER_LEN;
			this.bytes_read += ready!(poll_read(stream, context, &mut this.body_buffer[body_read..]))?;
			let body_read = this.bytes_read - FRAMED_HEADER_LEN;
			assert!(body_read <= this.body_buffer.len());
		}

		// Reset internal state and return the read message.
		let header = this.parsed_header;
		let body = std::mem::take(&mut this.body_buffer);
		this.bytes_read = 0;
		Poll::Ready(Ok(Message::new(header, body.into())))
	}
}

impl<W> crate::transport::TransportWriteHalf for StreamWriteHalf<W>
where
	W: AsyncWrite + Send + Unpin,
{
	type Body = StreamBody;

	fn poll_write_msg(self: Pin<&mut Self>, context: &mut Context, header: &MessageHeader, body: &Self::Body) -> Poll<Result<(), Error>> {
		let this = self.get_mut();

		// Make sure the body length doesn't exceed the maximum.
		check_payload_too_large(body.len(), this.max_body_len as usize)?;

		// Encode the header if we haven't done that yet.
		let header_buffer = this.header_buffer.get_or_insert_with(|| {
			let mut buffer = [0u8; FRAMED_HEADER_LEN];
			LE::write_u32(&mut buffer[0..], body.len() as u32 + crate::HEADER_LEN);
			header.encode(&mut buffer[4..]);
			buffer
		});

		// Keep writing the header until it is done.
		while this.bytes_written < FRAMED_HEADER_LEN + body.len() {
			let stream = Pin::new(&mut this.stream);
			let bufs: &[_] = &[IoSlice::new(&header_buffer[..]), IoSlice::new(&body.data)];
			this.bytes_written += ready!(stream.poll_write_vectored(context, &bufs))?;
		}

		// Reset internal state and return success.
		this.bytes_written = 0;
		this.header_buffer = None;
		Poll::Ready(Ok(()))
	}
}
