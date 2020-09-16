use thiserror::Error;

/// An error occured while reading a message.
#[derive(Debug, Error)]
#[error("{0}")]
pub enum ReadMessageError {
	Io(#[from] std::io::Error),

	InvalidMessageType(#[from] InvalidMessageType),

	PayloadTooLarge(#[from] PayloadTooLarge),
}

/// An error occured while writing a message.
#[derive(Debug, Error)]
#[error("{0}")]
pub enum WriteMessageError {
	Io(#[from] std::io::Error),

	PayloadTooLarge(#[from] PayloadTooLarge),
}

/// The message type is invalid.
#[derive(Debug, Clone, Error)]
#[error("invalid message type: expected a value in the range [0..4], got {value}")]
pub struct InvalidMessageType {
	/// The received value.
	pub value: u32,
}

/// The message body is too large.
#[derive(Debug, Clone, Error)]
#[error("payload too large: maximum payload size is {max_len}, got {body_len}")]
pub struct PayloadTooLarge {
	pub body_len: usize,
	pub max_len: u32,
}

impl PayloadTooLarge {
	/// Check if a payload length is small enough to fit in a message body.
	pub fn check(body_len: usize, max_len: u32) -> Result<(), Self> {
		if body_len <= max_len as usize {
			Ok(())
		} else {
			Err(Self { body_len, max_len })
		}
	}
}
