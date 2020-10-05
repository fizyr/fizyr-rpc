use filedesc::FileDesc;
use std::io::{IoSlice, IoSliceMut};
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::Message;
use crate::MessageHeader;
use crate::error::{PayloadTooLarge, ReadMessageError, WriteMessageError};
use super::{UnixBody, UnixConfig};

/// Transport layer for byte-stream sockets.
pub struct UnixTransport<Socket> {
	/// The socket to use for sending/receiving messages.
	pub(super) socket: Socket,

	/// The configuration of the transport.
	pub(super) config: UnixConfig,
}

/// The read half of a [`UnixTransport`].
pub struct UnixReadHalf<R> {
	/// The read half of the underlying socket.
	pub(super) stream: R,

	/// The maximum body length to accept when reading messages.
	pub(super) max_body_len: u32,

	/// The maximum number of file descriptors to accept when reading messages.
	pub(super) max_fds: u32,

	/// Buffer for reading the message body.
	pub(super) body_buffer: Vec<u8>,
}

/// The write half of a [`UnixTransport`].
pub struct UnixWriteHalf<W> {
	/// The write half of the underlying socket.
	pub(super) stream: W,

	/// The maximum body length to enforce for messages.
	pub(super) max_body_len: u32,

	/// The maximum number of file descriptors to accept when writing messages.
	pub(super) max_fds: u32,
}

impl<Socket> UnixTransport<Socket>
where
	for <'a> &'a mut Self: crate::Transport,
{
	/// Create a new transport with custom configuration.
	pub fn new(socket: Socket, config: UnixConfig) -> Self {
		Self { socket, config }
	}

	/// Create a new transport using the default configuration.
	pub fn new_default(socket: Socket) -> Self {
		Self::new(socket, UnixConfig::default())
	}
}

impl<R> UnixReadHalf<R> {
	pub(super) fn new(stream: R, max_body_len: u32, max_fds: u32) -> Self {
		Self {
			stream,
			max_body_len,
			max_fds,
			body_buffer: Vec::new(),
		}
	}
}

impl<W> UnixWriteHalf<W> {
	pub(super) fn new(stream: W, max_body_len: u32, max_fds: u32) -> Self {
		Self {
			stream,
			max_body_len,
			max_fds,
		}
	}
}

#[cfg(feature = "unix-seqpacket")]
impl crate::TransportReadHalf for UnixReadHalf<tokio_seqpacket::ReadHalf<'_>> {
	type Body = UnixBody;

	fn poll_read_msg(self: Pin<&mut Self>, context: &mut Context) -> Poll<Result<Message<Self::Body>, ReadMessageError>> {
		use tokio_seqpacket::ancillary::SocketAncillary;

		let this = self.get_mut();

		// Prepare buffers for the message header and body.
		let mut header_buffer = [0u8; crate::HEADER_LEN as usize];
		this.body_buffer.resize(this.max_body_len as usize, 0u8);

		// Prepare a buffer for the ancillary data.
		// TODO: properly compute size of ancillary buffer.
		let mut ancillary = vec![0u8; 32 + 16 * this.max_fds as usize];
		let mut ancillary = SocketAncillary::new(&mut ancillary);

		// Read the incoming datagram.
		let bytes_read = ready!(this.stream.poll_recv_vectored_with_ancillary(context, &mut [
			IoSliceMut::new(&mut header_buffer),
			IoSliceMut::new(&mut this.body_buffer),
		], &mut ancillary))?;

		// Immediately wrap all file descriptors to prevent leaking any of them.
		// We must always do this directly after a successful read.
		let fds = extract_file_descriptors(&ancillary)?;

		// Parse the header.
		let header = MessageHeader::decode(&header_buffer)?;

		// Resize the body buffer to the actual body size.
		let mut body = std::mem::take(&mut this.body_buffer);
		body.resize(bytes_read - crate::HEADER_LEN as usize, 0);

		Poll::Ready(Ok(Message::new(header, UnixBody::new(body, fds))))
	}
}

#[cfg(feature = "unix-seqpacket")]
impl crate::TransportWriteHalf for UnixWriteHalf<tokio_seqpacket::WriteHalf<'_>> {
	type Body = UnixBody;

	fn poll_write_msg(self: Pin<&mut Self>, context: &mut Context, header: &MessageHeader, body: &Self::Body) -> Poll<Result<(), WriteMessageError>> {
		use tokio_seqpacket::ancillary::SocketAncillary;

		let this = self.get_mut();

		// Check the outgoing body size.
		PayloadTooLarge::check(body.data.len(), this.max_body_len)?;

		// Prepare a buffer for the message header.
		let mut header_buffer = [0; crate::HEADER_LEN as usize];
		header.encode(&mut header_buffer);

		// Prepare a buffer for the ancillary data.
		// TODO: properly compute size of ancillary buffer.
		let mut ancillary = vec![0u8; 32 + 16 * this.max_fds as usize];
		let mut ancillary = SocketAncillary::new(&mut ancillary);

		let raw_fds: Vec<_> = body.fds.iter().map(|fd| fd.as_raw_fd()).collect();
		ancillary.add_fds(&raw_fds);

		ready!(this.stream.poll_send_vectored_with_ancillary(context, &[
			IoSlice::new(&header_buffer),
			IoSlice::new(&body.data),
		], &mut ancillary))?;

		Poll::Ready(Ok(()))
	}
}

/// Extract all file descriptors from ancillary data.
///
/// If the function encounters an unknown or malformed control message in the ancillary data,
/// all received file descriptors will be closed.
/// This includes file descriptors from later control messages.
/// This is done to ensure no file descriptors are leaked.
#[cfg(feature = "unix-seqpacket")]
fn extract_file_descriptors(ancillary: &tokio_seqpacket::ancillary::SocketAncillary<'_>) -> Result<Vec<FileDesc>, std::io::Error> {
	use tokio_seqpacket::ancillary::AncillaryData;

	let mut fds = Vec::new();
	let mut error = None;
	for msg in ancillary.messages() {
		match msg {
			// Wrap received file descriptors after wrapping.
			Ok(AncillaryData::ScmRights(msg)) => {
				if error.is_none() {
					fds.extend(msg.map(|fd| unsafe { FileDesc::from_raw_fd(fd) }));
				} else {
					for fd in msg {
						unsafe { FileDesc::from_raw_fd(fd); }
					}
				}
			},

			// Ignore Unix credentials.
			Ok(AncillaryData::ScmCredentials(_)) => (),

			// Can't return yet until we processed all file descriptors,
			// so store the error in an Option.
			Err(e) => if error.is_none() {
				error = Some(convert_ancillary_error(e));
			},
		}
	}

	if let Some(error) = error {
		Err(error)
	} else {
		Ok(fds)
	}
}

/// Convert an AncillaryError into an I/O error.
#[cfg(feature = "unix-seqpacket")]
fn convert_ancillary_error(error: tokio_seqpacket::ancillary::AncillaryError) -> std::io::Error {
	use tokio_seqpacket::ancillary::AncillaryError;
	let message = match error {
		AncillaryError::Unknown { cmsg_level, cmsg_type } => format!("unknown cmsg in ancillary data with cmsg_level {} and cmsg_type {}", cmsg_level, cmsg_type),
		e => format!("error in ancillary data: {:?}", e),
	};

	std::io::Error::new(std::io::ErrorKind::Other, message)
}
