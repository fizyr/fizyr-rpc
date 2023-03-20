use crate::UnixConfig;

/// Transport layer for Unix datagram/seqpacket sockets.
#[allow(dead_code)] // Fields are not used when transports are disabled.
pub struct UnixTransport<Socket> {
	/// The socket to use for sending/receiving messages.
	pub(super) socket: Socket,

	/// The configuration of the transport.
	pub(super) config: UnixConfig,
}

/// The read half of a [`UnixTransport`].
#[allow(dead_code)] // Not used when transports are disabled.
pub struct UnixReadHalf<SocketReadHalf> {
	/// The read half of the underlying socket.
	pub(super) socket: SocketReadHalf,

	/// The maximum body length to accept when reading messages.
	pub(super) max_body_len: u32,

	/// The maximum number of file descriptors to accept when reading messages.
	pub(super) max_fds: u32,

	/// Buffer for reading the message body.
	pub(super) body_buffer: Vec<u8>,
}

/// The write half of a [`UnixTransport`].
#[allow(dead_code)] // Not used when transports are disabled.
pub struct UnixWriteHalf<SocketWriteHalf> {
	/// The write half of the underlying socket.
	pub(super) socket: SocketWriteHalf,

	/// The maximum body length to enforce for messages.
	pub(super) max_body_len: u32,

	/// The maximum number of file descriptors to accept when writing messages.
	pub(super) max_fds: u32,
}

impl<Socket> UnixTransport<Socket>
where
	Self: crate::transport::Transport,
{
	/// Create a new transport with custom configuration.
	pub fn new(socket: Socket, config: UnixConfig) -> Self {
		Self { socket, config }
	}

	/// Create a new transport using the default configuration.
	pub fn new_default(socket: Socket) -> Self {
		Self::new(socket, UnixConfig::default())
	}

	/// Get direct access to the underlying socket.
	pub fn socket(&self) -> &Socket {
		&self.socket
	}

	/// Get direct mutable access to the underlying socket.
	pub fn socket_mut(&mut self) -> &Socket {
		&mut self.socket
	}

	/// Consume the socket transport to retrieve the underlying socket.
	pub fn into_socket(self) -> Socket {
		self.socket
	}
}

impl<SocketReadHalf> UnixReadHalf<SocketReadHalf> {
	#[allow(dead_code)] // Not used when transports are disabled.
	pub(super) fn new(socket: SocketReadHalf, max_body_len: u32, max_fds: u32) -> Self {
		Self {
			socket,
			max_body_len,
			max_fds,
			body_buffer: Vec::new(),
		}
	}

	/// Get direct access to the underlying socket.
	#[allow(dead_code)] // Not used when transports are disabled.
	pub fn socket(&self) -> &SocketReadHalf {
		&self.socket
	}

	/// Get direct mutable access to the underlying socket.
	#[allow(dead_code)] // Not used when transports are disabled.
	pub fn socket_mut(&mut self) -> &SocketReadHalf {
		&mut self.socket
	}
}

impl<SocketWriteHalf> UnixWriteHalf<SocketWriteHalf> {
	#[allow(dead_code)] // Not used when transports are disabled.
	pub(super) fn new(socket: SocketWriteHalf, max_body_len: u32, max_fds: u32) -> Self {
		Self {
			socket,
			max_body_len,
			max_fds,
		}
	}

	/// Get direct access to the underlying socket.
	#[allow(dead_code)] // Not used when transports are disabled.
	pub fn socket(&self) -> &SocketWriteHalf {
		&self.socket
	}

	/// Get direct mutable access to the underlying socket.
	#[allow(dead_code)] // Not used when transports are disabled.
	pub fn socket_mut(&mut self) -> &SocketWriteHalf {
		&mut self.socket
	}
}

#[cfg(feature = "unix-seqpacket")]
mod implementation {
	use super::*;

	use filedesc::FileDesc;
	use tokio_seqpacket::ancillary::{OwnedAncillaryMessage, AncillaryMessageWriter};
	use std::io::{IoSlice, IoSliceMut};
	use std::pin::Pin;
	use std::task::{Context, Poll};

	use crate::error::private::{
		check_message_too_short,
		check_payload_too_large, connection_aborted,
	};
	use crate::transport::TransportError;
	use crate::{Message, MessageHeader, UnixBody};

	impl crate::transport::TransportReadHalf for UnixReadHalf<&tokio_seqpacket::UnixSeqpacket> {
		type Body = UnixBody;

		fn poll_read_msg(self: Pin<&mut Self>, context: &mut Context) -> Poll<Result<Message<Self::Body>, TransportError>> {
			let this = self.get_mut();

			// Prepare buffers for the message header and body.
			let mut header_buffer = [0u8; crate::HEADER_LEN as usize];
			this.body_buffer.resize(this.max_body_len as usize, 0u8);

			// Prepare a buffer for the ancillary data.
			// TODO: properly compute size of ancillary buffer.
			let mut ancillary = vec![0u8; 32 + 16 * this.max_fds as usize];

			// Read the incoming datagram.
			let mut buffers = [IoSliceMut::new(&mut header_buffer), IoSliceMut::new(&mut this.body_buffer)];
			let (bytes_read, ancillary) = ready!(this.socket.poll_recv_vectored_with_ancillary(context, &mut buffers, &mut ancillary))
				.map_err(TransportError::new_fatal)?;

			// Immediately wrap all file descriptors to prevent leaking any of them.
			// We must always do this directly after a successful read.
			let mut fds = Vec::new();
			for msg in ancillary.into_messages() {
				if let OwnedAncillaryMessage::FileDescriptors(msg) = msg {
					fds.extend(msg.map(FileDesc::new))
				}
			};

			if bytes_read == 0 {
				return Poll::Ready(Err(TransportError::new_fatal(connection_aborted())));
			}

			// Make sure we received an entire header.
			check_message_too_short(bytes_read)
				.map_err(TransportError::new_fatal)?;

			// Parse the header.
			let header = MessageHeader::decode(&header_buffer)
				.map_err(TransportError::new_fatal)?;

			// Resize the body buffer to the actual body size.
			let mut body = std::mem::take(&mut this.body_buffer);
			body.resize(bytes_read - crate::HEADER_LEN as usize, 0);

			Poll::Ready(Ok(Message::new(header, UnixBody::new(body, fds))))
		}
	}

	impl crate::transport::TransportWriteHalf for UnixWriteHalf<&tokio_seqpacket::UnixSeqpacket> {
		type Body = UnixBody;

		fn poll_write_msg(self: Pin<&mut Self>, context: &mut Context, header: &MessageHeader, body: &Self::Body) -> Poll<Result<(), TransportError>> {
			let this = self.get_mut();

			// Check the outgoing body size.
			check_payload_too_large(body.data.len(), this.max_body_len as usize)
				.map_err(TransportError::new_non_fatal)?;

			// Prepare a buffer for the message header.
			let mut header_buffer = [0; crate::HEADER_LEN as usize];
			header.encode(&mut header_buffer);

			// Prepare a buffer for the ancillary data.
			// TODO: properly compute size of ancillary buffer.
			let mut ancillary = vec![0u8; 32 + 16 * this.max_fds as usize];
			let mut ancillary = AncillaryMessageWriter::new(&mut ancillary);

			let fds: Vec<_> = body.fds.iter().collect();
			ancillary.add_fds(&fds)
				.map_err(|_e| TransportError::new_non_fatal(std::io::Error::new(
					std::io::ErrorKind::Other,
					"not enough space for file descriptors",
				)))?;

			let buffers = [IoSlice::new(&header_buffer), IoSlice::new(&body.data)];
			ready!(this.socket.poll_send_vectored_with_ancillary(context, &buffers, &mut ancillary))
				.map_err(TransportError::new_fatal)?;

			Poll::Ready(Ok(()))
		}
	}
}
