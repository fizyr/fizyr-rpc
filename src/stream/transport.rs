use byteorder::ByteOrder;
use byteorder::LE;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite};

use crate::HEADER_LEN;
use crate::Message;
use crate::MessageHeader;
use crate::MessageType;
use crate::error::{PayloadTooLarge, ReadMessageError, WriteMessageError};
use super::{StreamBody, StreamConfig};

/// Transport layer for byte-stream sockets.
pub struct StreamTransport<Socket> {
	/// The socket to use for sending/receiving messages.
	pub(super) socket: Socket,

	/// The configuration of the transport.
	pub(super) config: StreamConfig,
}

/// The read half of a [`StreamTransport`].
pub struct StreamReadHalf<R> {
	/// The read half of the underlying socket.
	pub(super) stream: R,

	/// The maximum body length to accept when reading messages.
	pub(super) max_body_len: u32,

	/// The buffer for reading the message header.
	pub(super) header_buffer: [u8; 16],

	/// The number of bytes read for the current message header.
	pub(super) header_read: usize,

	/// The parsed header.
	pub(super) parsed_header: MessageHeader,

	/// The buffer for reading the message body.
	pub(super) body_buffer: Vec<u8>,

	/// The number of bytes read for the current message body.
	pub(super) body_read: usize,
}

/// The write half of a [`StreamTransport`].
pub struct StreamWriteHalf<W> {
	/// The write half of the underlying socket.
	pub(super) stream: W,

	/// The maximum body length to enforce for messages.
	pub(super) max_body_len: u32,

	/// The buffer for the encoded header.
	pub(super) header_buffer: Option<[u8; 16]>,

	/// The number of bytes written for the current message.
	pub(super) written: usize,
}

impl<Socket> StreamTransport<Socket>
where
	for <'a> &'a mut Self: crate::Transport,
{
	/// Create a new transport with custom configuration.
	pub fn new(socket: Socket, config: StreamConfig) -> Self {
		Self { socket, config }
	}

	/// Create a new transport using the default configuration.
	pub fn new_default(socket: Socket) -> Self {
		Self::new(socket, StreamConfig::default())
	}
}

impl<R> StreamReadHalf<R> {
	pub(super) fn new(stream: R, max_body_len: u32) -> Self {
		Self {
			stream,
			max_body_len,
			header_buffer: [0u8; 16],
			header_read: 0,
			parsed_header: MessageHeader::request(0, 0),
			body_buffer: Vec::new(),
			body_read: 0,
		}
	}
}

impl<W> StreamWriteHalf<W> {
	pub(super) fn new(stream: W, max_body_len: u32) -> Self {
		Self {
			stream,
			max_body_len,
			header_buffer: None,
			written: 0,
		}
	}
}

/// Wrapper around [`AsyncRead::poll_read`] that turns zero-sized reads into ConnectionAborted errors.
fn poll_read<R: AsyncRead>(stream: Pin<&mut R>, context: &mut Context, buf: &mut [u8]) -> Poll<std::io::Result<usize>> {
	match stream.poll_read(context, buf) {
		Poll::Pending => Poll::Pending,
		Poll::Ready(Ok(0)) => Poll::Ready(Err(std::io::ErrorKind::ConnectionAborted.into())),
		Poll::Ready(Ok(n)) => Poll::Ready(Ok(n)),
		Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
	}
}

impl<R> crate::TransportReadHalf for StreamReadHalf<R>
where
	R: AsyncRead + Send + Unpin,
{
	type Body = StreamBody;

	fn poll_read_msg(self: Pin<&mut Self>, context: &mut Context) -> Poll<Result<Message<Self::Body>, ReadMessageError>> {
		let this = self.get_mut();
		while this.header_read < 16 {
			let stream = Pin::new(&mut this.stream);
			this.header_read += ready!(poll_read(stream, context, &mut this.header_buffer[this.header_read..]))?;
			assert!(this.header_read <= 16);

			if this.header_read == 16 {
				// Parse header.
				let length = LE::read_u32(&this.header_buffer[0..]);
				let message_type = LE::read_u32(&this.header_buffer[4..]);
				let request_id = LE::read_u32(&this.header_buffer[8..]);
				let service_id = LE::read_i32(&this.header_buffer[12..]);

				let body_len = length - HEADER_LEN;
				PayloadTooLarge::check(body_len as usize, this.max_body_len)?;

				let message_type = MessageType::from_u32(message_type)?;

				this.body_buffer = vec![0; body_len as usize];
				this.parsed_header = MessageHeader {
					message_type,
					request_id,
					service_id,
				};
			}
		}

		while this.body_read < this.body_buffer.len() {
			let stream = Pin::new(&mut this.stream);
			this.body_read += ready!(poll_read(stream, context, &mut this.body_buffer[this.body_read..]))?;
			assert!(this.body_read <= this.body_buffer.len());

			if this.body_read == this.body_buffer.len() {
				let header = this.parsed_header;
				let body = std::mem::replace(&mut this.body_buffer, Vec::new());
				this.header_read = 0;
				this.body_read = 0;
				return Poll::Ready(Ok(Message::new(header, body.into())))
			}
		}

		unreachable!()
	}
}

impl<W: AsyncWrite + Unpin> crate::TransportWriteHalf for StreamWriteHalf<W> {
	type Body = StreamBody;

	fn poll_write_msg(self: Pin<&mut Self>, context: &mut Context, header: &MessageHeader, body: &Self::Body) -> Poll<Result<(), WriteMessageError>> {
		let this = self.get_mut();

		// Make sure the body length doesn't exceed the maximum.
		PayloadTooLarge::check(body.len(), this.max_body_len)?;

		let header_buffer = this.header_buffer.get_or_insert_with(|| {
			let mut buffer = [0u8; 16];
			LE::write_u32(&mut buffer[0..], body.len() as u32 + HEADER_LEN);
			LE::write_u32(&mut buffer[4..], header.message_type as u32);
			LE::write_u32(&mut buffer[8..], header.request_id);
			LE::write_i32(&mut buffer[12..], header.service_id);
			buffer
		});

		while this.written < 16 {
			let stream = Pin::new(&mut this.stream);
			this.written += ready!(stream.poll_write(context, &header_buffer[this.written..]))?;
		}

		while this.written - 16 < body.len() {
			let stream = Pin::new(&mut this.stream);
			this.written += ready!(stream.poll_write(context, &body.data[this.written - 16..]))?;
		}

		this.written = 0;
		this.header_buffer = None;
		Poll::Ready(Ok(()))
	}
}
