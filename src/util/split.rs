pub trait SplitAsyncReadWrite {
	type ReadHalf: tokio::io::AsyncRead;
	type WriteHalf: tokio::io::AsyncWrite;

	fn split(self) -> (Self::ReadHalf, Self::WriteHalf);
}

#[cfg(feature = "tcp")]
mod tcp {
	impl<'a> super::SplitAsyncReadWrite for &'a mut tokio::net::TcpStream {
		type ReadHalf = tokio::net::tcp::ReadHalf<'a>;
		type WriteHalf = tokio::net::tcp::WriteHalf<'a>;

		fn split(self) -> (Self::ReadHalf, Self::WriteHalf) {
			self.split()
		}
	}

	impl super::SplitAsyncReadWrite for tokio::net::TcpStream {
		type ReadHalf = tokio::net::tcp::OwnedReadHalf;
		type WriteHalf = tokio::net::tcp::OwnedWriteHalf;

		fn split(self) -> (Self::ReadHalf, Self::WriteHalf) {
			self.into_split()
		}
	}
}

#[cfg(feature = "unix")]
mod unix {
	impl<'a> super::SplitAsyncReadWrite for &'a mut tokio::net::UnixStream {
		type ReadHalf = tokio::net::unix::ReadHalf<'a>;
		type WriteHalf = tokio::net::unix::WriteHalf<'a>;

		fn split(self) -> (Self::ReadHalf, Self::WriteHalf) {
			self.split()
		}
	}
}
