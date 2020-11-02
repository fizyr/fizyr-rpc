use filedesc::FileDesc;

/// Body for the unix tranport.
///
/// The body includes data for a datagram,
/// and a list of file descriptors to attach.
pub struct UnixBody {
	/// The contents for the datagram.
	pub data: Vec<u8>,

	/// The file descriptors to attach.
	pub fds: Vec<FileDesc>,
}

impl UnixBody {
	/// Create a new unix body with datagram contents and file descriptors to attach.
	pub fn new<Data, FileDescs>(data: Data, fds: FileDescs) -> Self
	where
		Vec<u8>: From<Data>,
		Vec<FileDesc>: From<FileDescs>,
	{
		Self {
			data: data.into(),
			fds: fds.into(),
		}
	}
}

impl crate::Body for UnixBody {
	fn from_error(message: &str) -> Self {
		Self::from(message.as_bytes())
	}
}

impl From<Vec<u8>> for UnixBody {
	fn from(other: Vec<u8>) -> Self {
		Self {
			data: other,
			fds: Vec::new(),
		}
	}
}

impl<'a> From<&'a [u8]> for UnixBody {
	fn from(other: &'a [u8]) -> Self {
		Box::<[u8]>::from(other).into()
	}
}

impl From<Box<[u8]>> for UnixBody {
	fn from(other: Box<[u8]>) -> Self {
		Vec::from(other).into()
	}
}

impl<Data, FileDescs> From<(Data, FileDescs)> for UnixBody
where
	Vec<u8>: From<Data>,
	Vec<FileDesc>: From<FileDescs>,
{
	fn from(other: (Data, FileDescs)) -> Self {
		let (data, fds) = other;
		Self::new(data, fds)
	}
}
