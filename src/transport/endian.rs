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

	/// Encode header fields in the native endianness of the platform.
	///
	/// NOTE: You should only use this when you know for sure that the other side of the connection
	/// is on the same platform, such as when using a Unix socket.
	/// Otherwise, both sides may select native endianness and end up using a different endianness.
	NativeEndian,
}

impl Endian {
	/// Read a [`u32`] from a buffer in the correct endianness.
	pub(crate) fn read_u32(self, buffer: &[u8]) -> u32 {
		let buffer = buffer[0..4].try_into().unwrap();
		match self {
			Self::LittleEndian => u32::from_le_bytes(buffer),
			Self::BigEndian => u32::from_be_bytes(buffer),
			Self::NativeEndian => u32::from_ne_bytes(buffer),
		}
	}

	/// Write a [`u32`] to a buffer in the correct endianness.
	pub(crate) fn write_u32(self, buffer: &mut [u8], value: u32) {
		let bytes = match self {
			Self::LittleEndian => value.to_le_bytes(),
			Self::BigEndian => value.to_be_bytes(),
			Self::NativeEndian => value.to_ne_bytes(),
		};
		buffer[0..4].copy_from_slice(&bytes);
	}

	/// Read a [`i32`] from a buffer in the correct endianness.
	pub(crate) fn read_i32(self, buffer: &[u8]) -> i32 {
		let buffer = buffer[0..4].try_into().unwrap();
		match self {
			Self::LittleEndian => i32::from_le_bytes(buffer),
			Self::BigEndian => i32::from_be_bytes(buffer),
			Self::NativeEndian => i32::from_ne_bytes(buffer),
		}
	}

	/// Write a [`i32`] to a buffer in the correct endianness.
	pub(crate) fn write_i32(self, buffer: &mut [u8], value: i32) {
		let bytes = match self {
			Self::LittleEndian => value.to_le_bytes(),
			Self::BigEndian => value.to_be_bytes(),
			Self::NativeEndian => value.to_ne_bytes(),
		};
		buffer[0..4].copy_from_slice(&bytes);
	}
}

#[cfg(test)]
mod test {
	use super::Endian;
	use assert2::assert;

	#[test]
	fn write_u32_litte_endian_works() {
		let mut buffer = [0u8; 4];
		Endian::LittleEndian.write_u32(&mut buffer, 0x01020304);
		assert!(buffer == [0x04, 0x03, 0x02, 0x01]);
	}

	#[test]
	fn write_u32_big_endian_works() {
		let mut buffer = [0u8; 4];
		Endian::BigEndian.write_u32(&mut buffer, 0x01020304);
		assert!(buffer == [0x01, 0x02, 0x03, 0x04]);
	}

	#[test]
	fn write_u32_native_endian_works() {
		let mut buffer = [0u8; 4];
		Endian::LittleEndian.write_u32(&mut buffer, 0x01020304);
		#[cfg(target_endian = "little")]
		assert!(buffer == [0x04, 0x03, 0x02, 0x01]);
		#[cfg(target_endian = "big")]
		assert!(buffer == [0x01, 0x02, 0x03, 0x04]);
	}

	#[test]
	fn read_u32_litte_endian_works() {
		assert!(Endian::LittleEndian.read_u32(&[0x04, 0x03, 0x02, 0x01]) == 0x01020304);
	}

	#[test]
	fn read_u32_big_endian_works() {
		assert!(Endian::BigEndian.read_u32(&[0x01, 0x02, 0x03, 0x04]) == 0x01020304);
	}

	#[test]
	fn read_u32_native_endian_works() {
		#[cfg(target_endian = "little")]
		assert!(Endian::NativeEndian.read_u32(&[0x04, 0x03, 0x02, 0x01]) == 0x01020304);
		#[cfg(target_endian = "big")]
		assert!(Endian::NativeEndian.read_u32(&[0x01, 0x02, 0x03, 0x04]) == 0x01020304);
	}

	#[test]
	fn write_i32_litte_endian_works() {
		let mut buffer = [0u8; 4];
		Endian::LittleEndian.write_i32(&mut buffer, 0x01020304);
		assert!(buffer == [0x04, 0x03, 0x02, 0x01]);

		// 0x80000000 - 0x7efdfcfd = 0x01020305
		Endian::LittleEndian.write_i32(&mut buffer, -0x7efdfcfb);
		assert!(buffer == [0x05, 0x03, 0x02, 0x81]);
	}

	#[test]
	fn write_i32_big_endian_works() {
		let mut buffer = [0u8; 4];
		Endian::BigEndian.write_i32(&mut buffer, 0x01020304);
		assert!(buffer == [0x01, 0x02, 0x03, 0x04]);

		// 0x80000000 - 0x7efdfcfd = 0x01020305
		Endian::BigEndian.write_i32(&mut buffer, -0x7efdfcfb);
		assert!(buffer == [0x81, 0x02, 0x03, 0x05]);
	}

	#[test]
	fn write_i32_native_endian_works() {
		let mut buffer = [0u8; 4];
		Endian::NativeEndian.write_i32(&mut buffer, 0x01020304);
		#[cfg(target_endian = "little")]
		assert!(buffer == [0x04, 0x03, 0x02, 0x01]);
		#[cfg(target_endian = "big")]
		assert!(buffer == [0x01, 0x02, 0x03, 0x04]);

		// 0x80000000 - 0x7efdfcfd = 0x01020305
		Endian::NativeEndian.write_i32(&mut buffer, -0x7efdfcfb);
		#[cfg(target_endian = "little")]
		assert!(buffer == [0x05, 0x03, 0x02, 0x81]);
		#[cfg(target_endian = "big")]
		assert!(buffer == [0x81, 0x02, 0x03, 0x05]);
	}

	#[test]
	fn read_i32_litte_endian_works() {
		assert!(Endian::LittleEndian.read_i32(&[0x04, 0x03, 0x02, 0x01]) == 0x01020304);
		// 0x80000000 - 0x7efdfcfd = 0x01020305
		assert!(Endian::LittleEndian.read_i32(&[0x05, 0x03, 0x02, 0x81]) == -0x7efdfcfb);
	}

	#[test]
	fn read_i32_big_endian_works() {
		assert!(Endian::BigEndian.read_i32(&[0x01, 0x02, 0x03, 0x04]) == 0x01020304);
		// 0x80000000 - 0x7efdfcfd = 0x01020305
		assert!(Endian::BigEndian.read_i32(&[0x81, 0x02, 0x03, 0x05]) == -0x7efdfcfb);
	}

	#[test]
	fn read_i32_native_endian_works() {
		#[cfg(target_endian = "little")]
		assert!(Endian::NativeEndian.read_i32(&[0x04, 0x03, 0x02, 0x01]) == 0x01020304);
		// 0x80000000 - 0x7efdfcfd = 0x01020305
		#[cfg(target_endian = "little")]
		assert!(Endian::NativeEndian.read_i32(&[0x05, 0x03, 0x02, 0x81]) == -0x7efdfcfb);

		#[cfg(target_endian = "big")]
		assert!(Endian::NativeEndian.read_i32(&[0x01, 0x02, 0x03, 0x04]) == 0x01020304);
		#[cfg(target_endian = "big")]
		assert!(Endian::NativeEndian.read_i32(&[0x81, 0x02, 0x03, 0x05]) == -0x7efdfcfb);
	}
}
