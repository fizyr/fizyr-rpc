//! Error types.

use thiserror::Error;

pub(crate) fn connection_aborted() -> std::io::Error {
	std::io::ErrorKind::ConnectionAborted.into()
}

/// An error occurred while reading a message.
#[derive(Debug, Error)]
#[error("{0}")]
pub enum ReadMessageError {
	/// An I/O error occurred.
	Io(#[from] std::io::Error),

	/// The received message is too short to be valid.
	MessageTooShort(#[from] MessageTooShort),

	/// The received message has an invalid type.
	InvalidMessageType(#[from] InvalidMessageType),

	/// The payload of the message is too large to receive.
	PayloadTooLarge(#[from] PayloadTooLarge),
}

impl ReadMessageError {
	/// Check if the error is an I/O error indicating that the connection was aborted by the remote peer.
	pub fn is_connection_aborted(&self) -> bool {
		if let Self::Io(e) = &self {
			e.kind() == std::io::ErrorKind::ConnectionAborted
		} else {
			false
		}
	}
}

/// An error occurred while writing a message.
#[derive(Debug, Error)]
#[error("{0}")]
pub enum WriteMessageError {
	/// An I/O error occurred.
	Io(#[from] std::io::Error),

	/// The payload of the message is too large to send.
	PayloadTooLarge(#[from] PayloadTooLarge),
}

impl WriteMessageError {
	/// Check if the error is an I/O error indicating that the connection was aborted by the remote peer.
	pub fn is_connection_aborted(&self) -> bool {
		if let Self::Io(e) = &self {
			e.kind() == std::io::ErrorKind::ConnectionAborted
		} else {
			false
		}
	}
}

/// The received message is too short to contain a valid message.
#[derive(Debug, Clone, Error)]
#[error("the message is too short to be valid: need atleast 12 bytes for the header, got only {message_len} bytes")]
pub struct MessageTooShort {
	/// The actual size of the received message.
	pub message_len: usize,
}

impl MessageTooShort {
	/// Check if a message size is large enough to contain a valid message.
	pub fn check(message_len: usize) -> Result<(), Self> {
		if message_len >= crate::HEADER_LEN as usize {
			Ok(())
		} else {
			Err(Self { message_len })
		}
	}
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
	/// The actual length of the message body in bytes.
	pub body_len: usize,

	/// The maximum allowed length of a message body in bytes.
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

/// No free request ID was found.
#[derive(Debug, Clone, Error)]
#[error("no free request ID was found")]
pub struct NoFreeRequestIdFound;

/// The request ID is already associated with an open request.
#[derive(Debug, Clone, Error)]
#[error("duplicate request ID: request ID {request_id} is already associated with an open request")]
pub struct DuplicateRequestId {
	/// The duplicate request ID.
	pub request_id: u32,
}

/// The request ID is not associated with an open request.
#[derive(Debug, Clone, Error)]
#[error("unknown request ID: request ID {request_id} is not associated with an open request")]
pub struct UnknownRequestId {
	/// The unknown request ID.
	pub request_id: u32,
}

/// The received message had an unexpected type.
#[derive(Debug, Clone, Error)]
pub struct UnexpectedMessageType {
	/// The actual type of the received message.
	pub value: crate::MessageType,

	/// The expected type of the received message.
	pub expected: crate::MessageType,
}

impl std::fmt::Display for UnexpectedMessageType {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		// NOTE: we can use the same string for requester and responder updates,
		// because they can never get mixed up.
		// If that would happen, it means the message got routed wrong because it went in the wrong direction.
		let to_str = |kind| match kind {
			crate::MessageType::Request => "a request message",
			crate::MessageType::Response => "a response message",
			crate::MessageType::RequesterUpdate => "an update message",
			crate::MessageType::ResponderUpdate => "an update message",
			crate::MessageType::Stream => "a streaming message",
		};
		write!(f, "unexpected message type: expected {}, got {}", to_str(self.expected), to_str(self.value))
	}
}

/// An error occurred while reading an incoming message.
#[derive(Debug, Error)]
#[error("{0}")]
pub enum NextMessageError {
	/// An I/O error occurred.
	Io(#[from] std::io::Error),

	/// The received message is too short to be valid.
	MessageTooShort(#[from] MessageTooShort),

	/// The incoming message has an invalid message type.
	InvalidMessageType(#[from] InvalidMessageType),

	/// The payload of the incoming message is too large to receive.
	PayloadTooLarge(#[from] PayloadTooLarge),

	/// The incoming request message has a request ID that is already associated with an open request.
	DuplicateRequestId(#[from] DuplicateRequestId),

	/// The incoming update or response message has a request ID that is not associated with an open request.
	UnknownRequestId(#[from] UnknownRequestId),

	/// The received message has an unexpected message type.
	UnexpectedMessageType(#[from] UnexpectedMessageType),
}

impl NextMessageError {
	/// Check if the error is an I/O error indicating that the connection was aborted by the remote peer.
	pub fn is_connection_aborted(&self) -> bool {
		if let Self::Io(e) = &self {
			e.kind() == std::io::ErrorKind::ConnectionAborted
		} else {
			false
		}
	}
}

/// An error occurred while sending a request.
#[derive(Debug, Error)]
#[error("{0}")]
pub enum SendRequestError {
	/// An I/O error occurred.
	Io(#[from] std::io::Error),

	/// The payload of the message is too large to send.
	PayloadTooLarge(#[from] PayloadTooLarge),

	/// No free request ID was found.
	NoFreeRequestIdFound(#[from] NoFreeRequestIdFound),
}

impl SendRequestError {
	/// Check if the error is an I/O error indicating that the connection was aborted by the remote peer.
	pub fn is_connection_aborted(&self) -> bool {
		if let Self::Io(e) = &self {
			e.kind() == std::io::ErrorKind::ConnectionAborted
		} else {
			false
		}
	}
}

// Allow a ReadMessageError to be converted to a NextMessageError automatically.
impl From<ReadMessageError> for NextMessageError {
	fn from(other: ReadMessageError) -> Self {
		match other {
			ReadMessageError::Io(e) => e.into(),
			ReadMessageError::MessageTooShort(e) => e.into(),
			ReadMessageError::InvalidMessageType(e) => e.into(),
			ReadMessageError::PayloadTooLarge(e) => e.into(),
		}
	}
}

// Allow a WriteMessageError to be converted to a SendRequestError automatically.
impl From<WriteMessageError> for SendRequestError {
	fn from(other: WriteMessageError) -> Self {
		match other {
			WriteMessageError::Io(e) => e.into(),
			WriteMessageError::PayloadTooLarge(e) => e.into(),
		}
	}
}
