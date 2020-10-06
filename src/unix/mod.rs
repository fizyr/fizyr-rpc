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
	use filedesc::FileDesc;
	use std::os::unix::io::FromRawFd;

	use crate::MessageHeader;
	use crate::IntoTransport;
	use crate::UnixBody;

	#[tokio::test]
	async fn test_unix_transport() {
		let_assert!(Ok((socket_a, socket_b)) = UnixSeqpacket::pair());

		let mut transport_a = socket_a.into_transport_default();
		let mut transport_b = socket_b.into_transport_default();

		use crate::{Transport, TransportReadHalf, TransportWriteHalf};
		let (mut read_a, mut write_a) = transport_a.split();
		let (mut read_b, mut write_b) = transport_b.split();

		for i in 0..10 {
			let blob_0 = make_blob("blob 0", b"Message in a blob 0");
			let blob_1 = make_blob("blob 1", b"Message in a blob 1");

			let body = UnixBody::new(&b"Hello peer_b!"[..], vec![blob_0, blob_1]);
			assert!(let Ok(()) = write_a.write_msg(&MessageHeader::request(i * 2, 10), &body).await);
			let_assert!(Ok(message) = read_b.read_msg().await);
			assert!(message.header == MessageHeader::request(i * 2, 10));
			assert!(message.body.data.as_ref() == b"Hello peer_b!");
			assert!(message.body.fds.len() == 2);
			let_assert!(Ok(blob_0) = read_blob(&message.body.fds[0]));
			let_assert!(Ok(blob_1) = read_blob(&message.body.fds[1]));
			assert!(blob_0 == b"Message in a blob 0");
			assert!(blob_1 == b"Message in a blob 1");

			assert!(let Ok(()) = write_b.write_msg(&MessageHeader::request(i * 2 + 1, 11), &b"Hello peer_a!"[..].into()).await);
			let_assert!(Ok(message) = read_a.read_msg().await);
			assert!(message.header == MessageHeader::request(i * 2 + 1, 11));
			assert!(message.body.data.as_ref() == b"Hello peer_a!");
		}
	}

	fn make_blob(name: &str, data: &[u8]) -> filedesc::FileDesc {
		use std::io::{Seek, Write};
		let_assert!(Ok(fd) = memfd::MemfdOptions::new().close_on_exec(true).create(name));
		let mut file = fd.into_file();
		let_assert!(Ok(_) = file.write_all(data));
		assert!(let Ok(_) = file.seek(std::io::SeekFrom::Start(0)));
		filedesc::FileDesc::new(file)
	}

	fn read_blob(fd: &FileDesc) -> std::io::Result<Vec<u8>> {
		use std::io::Read;

		let mut output = Vec::new();
		let mut file = unsafe { std::fs::File::from_raw_fd(fd.as_raw_fd()) };
		let result = file.read_to_end(&mut output);
		std::mem::forget(file);

		result?;
		Ok(output)
	}
}
