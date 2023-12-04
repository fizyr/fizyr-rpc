/// The endianness to use for encoding header fields.
///
/// The encoding and serialization of message bodies is up to the application code,
/// and it not affected by this configuration parameter.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum Endian {
	/// Encode header fields in little endian.
	LittleEndian,

	/// Encode header fields in big endian.
	BigEndian,
}

impl Endian {
	/// Read a [`u32`] from a buffer in the correct endianness.
	pub(crate) fn read_u32(self, buffer: &[u8]) -> u32 {
		let buffer = buffer[0..4].try_into().unwrap();
		match self {
			Self::LittleEndian => u32::from_le_bytes(buffer),
			Self::BigEndian => u32::from_be_bytes(buffer),
		}
	}

	/// Write a [`u32`] to a buffer in thcorrect endianness.
	pub(crate) fn write_u32(self, buffer: &mut [u8], value: u32) {
		let bytes = match self {
			Self::LittleEndian => value.to_le_bytes(),
			Self::BigEndian => value.to_be_bytes(),
		};
		buffer[0..4].copy_from_slice(&bytes);
	}

	/// Read a [`i32`] from a buffer in the correct endianness.
	pub(crate) fn read_i32(self, buffer: &[u8]) -> i32 {
		let buffer = buffer[0..4].try_into().unwrap();
		match self {
			Self::LittleEndian => i32::from_le_bytes(buffer),
			Self::BigEndian => i32::from_be_bytes(buffer),
		}
	}

	/// Write a [`i32`] to a buffer in thcorrect endianness.
	pub(crate) fn write_i32(self, buffer: &mut [u8], value: i32) {
		let bytes = match self {
			Self::LittleEndian => value.to_le_bytes(),
			Self::BigEndian => value.to_be_bytes(),
		};
		buffer[0..4].copy_from_slice(&bytes);
	}
}
