mod body;
mod config;
mod transport;

pub use body::UnixBody;
pub use config::UnixConfig;
pub use transport::{UnixReadHalf, UnixTransport, UnixWriteHalf};

/// Information about the remote peer of a Unix stream.
#[derive(Debug, Clone)]
#[cfg(feature = "unix-seqpacket")]
pub struct UnixSeqpacketInfo {
	/// The user ID of the remote process.
	user_id: u32,

	/// The group ID of the remote process.
	group_id: u32,

	/// The process ID of the remote process.
	process_id: Option<i32>,
}

#[cfg(feature = "unix-seqpacket")]
impl UnixSeqpacketInfo {
	/// Get the user ID of the process.
	pub fn user_id(&self) -> u32 {
		self.user_id
	}

	/// Get the group ID of the process.
	pub fn group_id(&self) -> u32 {
		self.group_id
	}

	/// Get the process ID of the remote process (if available).
	pub fn process_id(&self) -> Option<i32> {
		self.process_id
	}
}

#[cfg(feature = "unix-seqpacket")]
mod impl_unix_seqpacket {
	use std::future::Future;
	use std::pin::Pin;
	use super::*;

	impl crate::transport::Transport for UnixTransport<tokio_seqpacket::UnixSeqpacket> {
		type Body = UnixBody;
		type Info = UnixSeqpacketInfo;
		type Config = UnixConfig;
		type ReadHalf<'a> = UnixReadHalf<&'a tokio_seqpacket::UnixSeqpacket>;
		type WriteHalf<'a> = UnixWriteHalf<&'a tokio_seqpacket::UnixSeqpacket>;

		fn split(&mut self) -> (UnixReadHalf<&tokio_seqpacket::UnixSeqpacket>, UnixWriteHalf<&tokio_seqpacket::UnixSeqpacket>) {
			let (read_half, write_half) = (&self.socket, &self.socket);
			let read_half = UnixReadHalf::new(read_half, self.config.max_body_len_read, self.config.max_fds_read, self.config.endian);
			let write_half = UnixWriteHalf::new(write_half, self.config.max_body_len_write, self.config.max_fds_write, self.config.endian);
			(read_half, write_half)
		}

		fn info(&self) -> std::io::Result<Self::Info> {
			let creds = self.socket.peer_cred()?;
			Ok(Self::Info {
				user_id: creds.uid(),
				group_id: creds.gid(),
				process_id: creds.pid(),
			})
		}
	}

	impl crate::util::IntoTransport for tokio_seqpacket::UnixSeqpacket {
		type Body = UnixBody;
		type Config = UnixConfig;
		type Transport = UnixTransport<tokio_seqpacket::UnixSeqpacket>;

		fn into_transport(self, config: Self::Config) -> Self::Transport {
			UnixTransport::new(self, config)
		}
	}

	impl<'a, Address> crate::util::Connect<'a, Address> for UnixTransport<tokio_seqpacket::UnixSeqpacket>
	where
		Address: AsRef<std::path::Path> + 'a,
	{
		type Future = Pin<Box<dyn Future<Output = std::io::Result<Self>> + 'a>>;

		fn connect(address: Address, config: Self::Config) -> Self::Future {
			Box::pin(async move {
				let socket = tokio_seqpacket::UnixSeqpacket::connect(address).await?;
				Ok(Self::new(socket, config))
			})
		}
	}

	impl<'a, Address> crate::util::Bind<'a, Address> for tokio_seqpacket::UnixSeqpacketListener
	where
		Address: AsRef<std::path::Path> + 'a,
	{
		// TODO: Use more efficient custom future?
		type Future = Pin<Box<dyn Future<Output = std::io::Result<Self>> + 'a>>;

		fn bind(address: Address) -> Self::Future {
			use std::os::unix::fs::FileTypeExt;

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
}

#[cfg(test)]
mod test {
	use assert2::assert;
	use assert2::let_assert;

	use filedesc::FileDesc;
	use std::os::unix::io::FromRawFd;
	use tokio_seqpacket::UnixSeqpacket;

	use crate::util::IntoTransport;
	use crate::MessageHeader;
	use crate::UnixBody;

	#[tokio::test]
	async fn test_unix_transport() {
		let_assert!(Ok((socket_a, socket_b)) = UnixSeqpacket::pair());

		let mut transport_a = socket_a.into_default_transport();
		let mut transport_b = socket_b.into_default_transport();

		use crate::transport::{Transport, TransportReadHalf, TransportWriteHalf};
		let (mut read_a, mut write_a) = transport_a.split();
		let (mut read_b, mut write_b) = transport_b.split();

		for i in 0..10 {
			let blob_0 = make_blob("blob 0", b"Message in a blob 0");
			let blob_1 = make_blob("blob 1", b"Message in a blob 1");

			let body = UnixBody::new(&b"Hello peer_b!"[..], vec![blob_0, blob_1]);
			assert!(let Ok(()) = write_a.write_msg(&MessageHeader::request(i * 2, 10), &body).await);
			let_assert!(Ok(message) = read_b.read_msg().await);
			assert!(message.header == MessageHeader::request(i * 2, 10));
			assert!(message.body.data == b"Hello peer_b!");
			assert!(message.body.fds.len() == 2);
			let_assert!(Ok(blob_0) = read_blob(&message.body.fds[0]));
			let_assert!(Ok(blob_1) = read_blob(&message.body.fds[1]));
			assert!(blob_0 == b"Message in a blob 0");
			assert!(blob_1 == b"Message in a blob 1");

			assert!(let Ok(()) = write_b.write_msg(&MessageHeader::request(i * 2 + 1, 11), &b"Hello peer_a!"[..].into()).await);
			let_assert!(Ok(message) = read_a.read_msg().await);
			assert!(message.header == MessageHeader::request(i * 2 + 1, 11));
			assert!(message.body.data == b"Hello peer_a!");
		}
	}

	fn make_blob(name: &str, data: &[u8]) -> filedesc::FileDesc {
		use std::io::{Seek, Write};
		let_assert!(Ok(fd) = memfile::MemFile::create_default(name));
		let mut file = fd.into_file();
		let_assert!(Ok(_) = file.write_all(data));
		assert!(let Ok(_) = file.rewind());
		filedesc::FileDesc::new(file.into())
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
