use crate::transport::Endian;

/// Configuration for a byte-stream transport.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct StreamConfig {
	/// The maximum body size for incoming messages.
	///
	/// If a message arrives with a larger body size, an error is returned.
	/// For stream sockets, that also means the stream is unusable because there is unread data left in the stream.
	pub max_body_len_read: u32,

	/// The maximum body size for outgoing messages.
	///
	/// If a message is given for sending with a larger body than this size,
	/// the message is discarded and an error is returned.
	/// Stream sockets remain usable since the message header will not be sent either.
	pub max_body_len_write: u32,

	/// The endianness to use when encoding/decoding header fields.
	///
	/// The encoding and serialization of message bodies is up to the application code,
	/// and it not affected by this configuration parameter.
	pub endian: Endian,
}

impl Default for StreamConfig {
	fn default() -> Self {
		Self {
			max_body_len_read: 8 * 1024,
			max_body_len_write: 8 * 1024,
			endian: Endian::LittleEndian,
		}
	}
}
