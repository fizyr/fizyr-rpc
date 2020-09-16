use byteorder::ByteOrder;
use byteorder::LE;
use futures::io::AsyncRead;
use futures::io::AsyncReadExt;
use futures::io::AsyncWrite;
use futures::io::AsyncWriteExt;

use crate::HEADER_LEN;
use crate::MAX_PAYLOAD_LEN;
use crate::MessageHeader;
use crate::MessageType;
use crate::error;

#[derive(Debug, Copy, Clone)]
pub struct StreamPeerConfig {
	/// The maximum body size for incoming messages.
	///
	/// If a message arrives with a larger body size, an error is returned.
	/// For stream sockets, that also means the stream is unusable because there is unread data left in the stream.
	pub max_body_len_read: u32,

	/// The maximum body size for outgoing messages.
	///
	/// If a message is given for sending with a larget body than this size,
	/// the message is discarded and an error is returned.
	/// Stream sockets remain usable since the message header will not be sent either.
	pub max_body_len_write: u32,
}

impl Default for StreamPeerConfig {
	fn default() -> Self {
		Self {
			max_body_len_read: 8 * 1024,
			max_body_len_write: 8 * 1024,
		}
	}
}

/// RPC peer using a stream socket.
pub struct StreamPeer<Socket> {
	socket: Socket,
	config: StreamPeerConfig,
}

impl<Socket> StreamPeer<Socket>
where
	Socket: AsyncRead + AsyncWrite + Unpin,
{
	pub fn new(socket: Socket, config: StreamPeerConfig) -> Self {
		Self { socket, config }
	}

	pub async fn read_message(&mut self) -> Result<(MessageHeader, Vec<u8>), error::ReadMessageError> {
		let max_body_len = self.config.max_body_len_read;
		read_message(&mut self.socket, max_body_len).await
	}

	pub async fn send_message(&mut self, header: &MessageHeader, body: &[u8]) -> Result<(), error::WriteMessageError> {
		let max_body_len = self.config.max_body_len_write;
		write_message(&mut self.socket, header, body, max_body_len).await
	}
}

/// Read a message from an [`AsyncRead`] stream.
pub async fn read_message<R: AsyncRead + Unpin>(stream: &mut R, max_body_len: u32) -> Result<(MessageHeader, Vec<u8>), error::ReadMessageError> {
	// Read header.
	let mut buffer = [0u8; 16];
	stream.read_exact(&mut buffer).await?;

	// Parse header.
	let length = LE::read_u32(&buffer[0..]);
	let message_type = LE::read_u32(&buffer[4..]);
	let request_id = LE::read_u32(&buffer[8..]);
	let service_id = LE::read_i32(&buffer[12..]);

	let body_len = length - HEADER_LEN;
	error::PayloadTooLarge::check(body_len as usize, max_body_len)?;

	let message_type = MessageType::from_u32(message_type)?;
	let header = MessageHeader {
		message_type,
		request_id,
		service_id,
	};

	// TODO: Use Box::new_uninit_slice() when it hits stable.
	let mut buffer = vec![0u8; body_len as usize];
	stream.read_exact(&mut buffer).await?;
	Ok((header, buffer))
}

/// Write a message to an [`AsyncWrite`] stream.
pub async fn write_message<W: AsyncWrite + Unpin>(stream: &mut W, header: &MessageHeader, body: &[u8], max_body_len: u32) -> Result<(), error::WriteMessageError> {
	error::PayloadTooLarge::check(body.len(), max_body_len.min(MAX_PAYLOAD_LEN))?;

	let mut buffer = [0u8; 16];
	LE::write_u32(&mut buffer[0..], body.len() as u32 + HEADER_LEN);
	LE::write_u32(&mut buffer[4..], header.message_type as u32);
	LE::write_u32(&mut buffer[8..], header.request_id);
	LE::write_i32(&mut buffer[12..], header.service_id);

	// TODO: Use AsyncWriteExt::write_all_vectored once it hits stable.
	stream.write_all(&buffer).await?;
	stream.write_all(&body).await?;
	Ok(())
}

#[cfg(test)]
mod test {
	use super::*;
	use assert2::assert;
	use assert2::let_assert;

	use async_std::os::unix::net::UnixStream;

	#[async_std::test]
	async fn test_raw_message() {
		let_assert!(Ok((peer_a, peer_b)) = UnixStream::pair());
		let mut peer_a = StreamPeer::new(peer_a, Default::default());
		let mut peer_b = StreamPeer::new(peer_b, Default::default());

		assert!(let Ok(()) = peer_a.send_message(&MessageHeader::request(1, 10), b"Hello peer_b!").await);

		let_assert!(Ok((header, body)) = peer_b.read_message().await);
		assert!(header == MessageHeader::request(1, 10));
		assert!(body == b"Hello peer_b!");
	}
}
