/// The body of a stream message.
#[derive(Debug, Clone)]
pub struct StreamBody {
	/// The message data.
	pub data: Vec<u8>,
}

impl StreamBody {
	/// Create a new stream body.
	fn new(data: Vec<u8>) -> Self {
		Self { data }
	}
}

impl crate::Body for StreamBody {
	fn empty() -> Self {
		Self::new(Vec::new())
	}

	fn from_error(message: &str) -> Self {
		Self::new(message.as_bytes().into())
	}
}

impl<T> From<T> for StreamBody
where
	Vec<u8>: From<T>,
{
	fn from(other: T) -> Self {
		Self { data: other.into() }
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
