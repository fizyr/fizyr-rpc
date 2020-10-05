mod body;
mod transport;
mod config;

pub use body::UnixBody;
pub use config::UnixConfig;
pub use transport::{UnixReadHalf, UnixTransport, UnixWriteHalf};

#[cfg(feature = "unix-seqpacket")]
impl<'a> crate::Transport for &'a mut UnixTransport<tokio_seqpacket::UnixSeqpacket> {
	type Body = UnixBody;
	type ReadHalf = UnixReadHalf<tokio_seqpacket::ReadHalf<'a>>;
	type WriteHalf = UnixWriteHalf<tokio_seqpacket::WriteHalf<'a>>;

	fn split(self) -> (Self::ReadHalf, Self::WriteHalf) {
		let (read_half, write_half) = self.socket.split();
		let read_half = UnixReadHalf::new(read_half, self.config.max_body_len_read, self.config.max_fds_read);
		let write_half = UnixWriteHalf::new(write_half, self.config.max_body_len_write, self.config.max_fds_write);
		(read_half, write_half)
	}
}

#[cfg(feature = "unix-seqpacket")]
impl crate::IntoTransport for tokio_seqpacket::UnixSeqpacket {
	type Body = UnixBody;
	type Config = UnixConfig;
	type Transport = UnixTransport<tokio_seqpacket::UnixSeqpacket>;

	fn into_transport(self, config: Self::Config) -> Self::Transport {
		UnixTransport::new(self, config)
	}
}

#[cfg(test)]
mod test {
	use assert2::assert;
	use assert2::let_assert;

	use tokio_seqpacket::UnixSeqpacket;

	use crate::MessageHeader;
	use crate::IntoTransport;

	#[tokio::test]
	async fn test_unix_transport() {
		let_assert!(Ok((socket_a, socket_b)) = UnixSeqpacket::pair());

		let mut transport_a = socket_a.into_transport_default();
		let mut transport_b = socket_b.into_transport_default();

		use crate::{Transport, TransportReadHalf, TransportWriteHalf};
		let (mut read_a, mut write_a) = transport_a.split();
		let (mut read_b, mut write_b) = transport_b.split();

		for i in 0..10 {
			assert!(let Ok(()) = write_a.write_msg(&MessageHeader::request(i * 2, 10), &b"Hello peer_b!"[..].into()).await);
			let_assert!(Ok(message) = read_b.read_msg().await);
			assert!(message.header == MessageHeader::request(i * 2, 10));
			assert!(message.body.data.as_ref() == b"Hello peer_b!");

			assert!(let Ok(()) = write_b.write_msg(&MessageHeader::request(i * 2 + 1, 11), &b"Hello peer_a!"[..].into()).await);
			let_assert!(Ok(message) = read_a.read_msg().await);
			assert!(message.header == MessageHeader::request(i * 2 + 1, 11));
			assert!(message.body.data.as_ref() == b"Hello peer_a!");
		}
	}
}
