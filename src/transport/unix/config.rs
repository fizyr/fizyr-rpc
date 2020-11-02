/// Configuration for Unix datagram transports.
#[derive(Debug, Clone)]
pub struct UnixConfig {
	/// The maximum body size for incoming messages.
	///
	/// If a message arrives with a larger body size,
	/// an error is returned and the message itself is dropped.
	///
	/// Datagram transports remain usable when a message is dropped.
	pub max_body_len_read: u32,

	/// The maximum body size for outgoing messages.
	///
	/// If a message is given for sending with a larger body than this size,
	/// the message is discarded and an error is returned.
	///
	/// Datagram transports remain usable when a message is dropped.
	pub max_body_len_write: u32,

	/// The maximum number of attached file descriptors when reading messages.
	pub max_fds_read: u32,

	/// The maximum number of attached file descriptors for sending messages.
	pub max_fds_write: u32,
}

impl Default for UnixConfig {
	fn default() -> Self {
		Self {
			max_body_len_read: 4 * 1024,
			max_body_len_write: 4 * 1024,
			max_fds_read: 10,
			max_fds_write: 10,
		}
	}
}
