/// The body of a stream message.
#[derive(Debug, Clone)]
pub struct StreamBody {
	pub data: Box<[u8]>,
}

impl<T: Into<Box<[u8]>>> From<T> for StreamBody {
	fn from(other: T) -> Self {
		Self { data: other.into() }
	}
}

impl crate::Body for StreamBody {
	fn from_error(message: &str) -> Self {
		Self::new(message.as_bytes().into())
	}
}

impl StreamBody {
	/// Create a new stream body.
	fn new(data: Box<[u8]>) -> Self {
		Self { data }
	}
}

impl AsRef<[u8]> for StreamBody {
	fn as_ref(&self) -> &[u8] {
		&self.data
	}
}

impl std::ops::Deref for StreamBody {
	type Target = [u8];

	fn deref(&self) -> &[u8] {
		&self.data
	}
}
