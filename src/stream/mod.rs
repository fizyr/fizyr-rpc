mod body;
mod config;
mod transport;

pub use body::StreamBody;
pub use config::StreamConfig;
pub use transport::{StreamReadHalf, StreamTransport, StreamWriteHalf};

#[cfg(feature = "unix")]
impl<'a> crate::Transport for &'a mut StreamTransport<tokio::net::UnixStream> {
	type Body = StreamBody;
	type ReadHalf = StreamReadHalf<tokio::net::unix::ReadHalf<'a>>;
	type WriteHalf = StreamWriteHalf<tokio::net::unix::WriteHalf<'a>>;

	fn split(self) -> (Self::ReadHalf, Self::WriteHalf) {
		let (read_half, write_half) = self.socket.split();
		let read_half = StreamReadHalf::new(read_half, self.config.max_body_len_read);
		let write_half = StreamWriteHalf::new(write_half, self.config.max_body_len_write);
		(read_half, write_half)
	}
}

#[cfg(feature = "unix")]
impl crate::IntoTransport for tokio::net::UnixStream {
	type Body = StreamBody;
	type Transport = StreamTransport<tokio::net::UnixStream>;
	type Config = StreamConfig;

	fn into_transport(self, config: Self::Config) -> Self::Transport {
		StreamTransport::new(self, config)
	}
}

#[cfg(feature = "tcp")]
impl<'a> crate::Transport for &'a mut StreamTransport<tokio::net::TcpStream> {
	type Body = StreamBody;
	type ReadHalf = StreamReadHalf<tokio::net::tcp::ReadHalf<'a>>;
	type WriteHalf = StreamWriteHalf<tokio::net::tcp::WriteHalf<'a>>;

	fn split(self) -> (Self::ReadHalf, Self::WriteHalf) {
		let (read_half, write_half) = self.socket.split();
		let read_half = StreamReadHalf::new(read_half, self.config.max_body_len_read);
		let write_half = StreamWriteHalf::new(write_half, self.config.max_body_len_write);
		(read_half, write_half)
	}
}

#[cfg(feature = "tcp")]
impl crate::IntoTransport for tokio::net::TcpStream {
	type Body = StreamBody;
	type Transport = StreamTransport<tokio::net::TcpStream>;
	type Config = StreamConfig;

	fn into_transport(self, config: Self::Config) -> Self::Transport {
		StreamTransport::new(self, config)
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use assert2::assert;
	use assert2::let_assert;

	use tokio::net::UnixStream;

	use crate::MessageHeader;

	#[tokio::test]
	async fn test_stream_transport() {
		let_assert!(Ok((peer_a, peer_b)) = UnixStream::pair());

		let mut transport_a = StreamTransport::new(peer_a, StreamConfig::default());
		let mut transport_b = StreamTransport::new(peer_b, StreamConfig::default());

		use crate::{Transport, TransportReadHalf, TransportWriteHalf};
		let (mut read_a, mut write_a) = transport_a.split();
		let (mut read_b, mut write_b) = transport_b.split();

		for i in 0..10 {
			assert!(let Ok(()) = write_a.write_msg(&MessageHeader::request(i * 2, 10), &b"Hello peer_b!"[..].into()).await);
			let_assert!(Ok(message) = read_b.read_msg().await);
			assert!(message.header == MessageHeader::request(i * 2, 10));
			assert!(message.body.as_ref() == b"Hello peer_b!");

			assert!(let Ok(()) = write_b.write_msg(&MessageHeader::request(i * 2 + 1, 11), &b"Hello peer_a!"[..].into()).await);
			let_assert!(Ok(message) = read_a.read_msg().await);
			assert!(message.header == MessageHeader::request(i * 2 + 1, 11));
			assert!(message.body.as_ref() == b"Hello peer_a!");
		}
	}
}
