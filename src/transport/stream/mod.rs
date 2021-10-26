use std::os::unix::fs::FileTypeExt;

mod body;
mod config;
mod transport;

pub use body::StreamBody;
pub use config::StreamConfig;
pub use transport::{StreamReadHalf, StreamTransport, StreamWriteHalf};

#[cfg(feature = "unix-stream")]
mod impl_unix_stream {
	use std::future::Future;
	use std::pin::Pin;
	use super::*;

	impl crate::transport::Transport for StreamTransport<tokio::net::UnixStream> {
		type Body = StreamBody;
		type Config = StreamConfig;
		type ReadHalf = ReadHalfType;
		type WriteHalf = WriteHalfType;

		fn split(&mut self) -> (StreamReadHalf<tokio::net::unix::ReadHalf>, StreamWriteHalf<tokio::net::unix::WriteHalf>) {
			let (read_half, write_half) = self.stream.split();
			let read_half = StreamReadHalf::new(read_half, self.config.max_body_len_read);
			let write_half = StreamWriteHalf::new(write_half, self.config.max_body_len_write);
			(read_half, write_half)
		}
	}

	impl crate::util::IntoTransport for tokio::net::UnixStream {
		type Body = StreamBody;
		type Config = StreamConfig;
		type Transport = StreamTransport<tokio::net::UnixStream>;

		fn into_transport(self, config: Self::Config) -> Self::Transport {
			StreamTransport::new(self, config)
		}
	}

	impl<'a, Address> crate::util::Connect<'a, Address> for StreamTransport<tokio::net::UnixStream>
	where
		Address: AsRef<std::path::Path> + 'a,
	{
		type Future = Pin<Box<dyn Future<Output = std::io::Result<Self>> + 'a>>;

		fn connect(address: Address, config: Self::Config) -> Self::Future {
			Box::pin(async move {
				let socket = tokio::net::UnixStream::connect(address).await?;
				Ok(Self::new(socket, config))
			})
		}
	}

	impl<'a, Address> crate::util::Bind<'a, Address> for tokio::net::UnixListener
	where
		Address: AsRef<std::path::Path> + 'a,
	{
		// TODO: Use more efficient custom future?
		type Future = Pin<Box<dyn Future<Output = std::io::Result<Self>> + 'a>>;

		fn bind(address: Address) -> Self::Future {
			// Try to unlink the socket before binding it, ignoring errors.
			if let Ok(metadata) = std::fs::metadata(&address) {
				if metadata.file_type().is_socket() {
					let _ = std::fs::remove_file(&address);
				}
			}

			Box::pin(async {
				Self::bind(address)
			})
		}
	}

	/// Helper struct to provide the read half types with a lifetime.
	pub struct ReadHalfType;

	/// Helper struct to provide the write half types with a lifetime.
	pub struct WriteHalfType;

	impl<'a> crate::transport::ReadHalfType<'a> for ReadHalfType {
		type Body = StreamBody;
		type ReadHalf = StreamReadHalf<tokio::net::unix::ReadHalf<'a>>;
	}

	impl<'a> crate::transport::WriteHalfType<'a> for WriteHalfType {
		type Body = StreamBody;
		type WriteHalf = StreamWriteHalf<tokio::net::unix::WriteHalf<'a>>;
	}
}

#[cfg(feature = "tcp")]
mod impl_tcp {
	use std::future::Future;
	use std::pin::Pin;
	use super::*;

	impl crate::transport::Transport for StreamTransport<tokio::net::TcpStream> {
		type Body = StreamBody;
		type Config = StreamConfig;
		type ReadHalf = ReadHalfType;
		type WriteHalf = WriteHalfType;

		fn split(&mut self) -> (StreamReadHalf<tokio::net::tcp::ReadHalf>, StreamWriteHalf<tokio::net::tcp::WriteHalf>) {
			let (read_half, write_half) = self.stream.split();
			let read_half = StreamReadHalf::new(read_half, self.config.max_body_len_read);
			let write_half = StreamWriteHalf::new(write_half, self.config.max_body_len_write);
			(read_half, write_half)
		}
	}

	impl crate::util::IntoTransport for tokio::net::TcpStream {
		type Body = StreamBody;
		type Config = StreamConfig;
		type Transport = StreamTransport<tokio::net::TcpStream>;

		fn into_transport(self, config: Self::Config) -> Self::Transport {
			StreamTransport::new(self, config)
		}
	}

	impl<'a, Address> crate::util::Connect<'a, Address> for StreamTransport<tokio::net::TcpStream>
	where
		Address: tokio::net::ToSocketAddrs + 'a,
	{
		type Future = Pin<Box<dyn Future<Output = std::io::Result<Self>> + 'a>>;

		fn connect(address: Address, config: Self::Config) -> Self::Future {
			Box::pin(async {
				let socket = tokio::net::TcpStream::connect(address).await?;
				Ok(Self::new(socket, config))
			})
		}
	}

	impl<'a, Address> crate::util::Bind<'a, Address> for tokio::net::TcpListener
	where
		Address: tokio::net::ToSocketAddrs + 'a,
	{
		type Future = Pin<Box<dyn Future<Output = std::io::Result<Self>> + 'a>>;

		fn bind(address: Address) -> Self::Future {
			Box::pin(Self::bind(address))
		}
	}

	/// Helper struct to provide the read half types with a lifetime.
	pub struct ReadHalfType;

	/// Helper struct to provide the write half types with a lifetime.
	pub struct WriteHalfType;

	impl<'a> crate::transport::ReadHalfType<'a> for ReadHalfType {
		type Body = StreamBody;
		type ReadHalf = StreamReadHalf<tokio::net::tcp::ReadHalf<'a>>;
	}

	impl<'a> crate::transport::WriteHalfType<'a> for WriteHalfType {
		type Body = StreamBody;
		type WriteHalf = StreamWriteHalf<tokio::net::tcp::WriteHalf<'a>>;
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

		use crate::transport::{Transport, TransportReadHalf, TransportWriteHalf};
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
