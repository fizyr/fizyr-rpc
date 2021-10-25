//! Error types.

use thiserror::Error;

/// Opaque error for all RPC operations.
#[derive(Debug, Error)]
#[error("{inner}")]
pub struct Error {
	#[from]
	pub(crate) inner: private::InnerError,
}

impl Error {
	/// Create a new error from an I/O error.
	pub fn io_error(error: std::io::Error) -> Self {
		private::InnerError::from(error).into()
	}

	/// Create a new error for a message that is too short to be valid.
	pub fn message_too_short(message_len: usize) -> Self {
		private::InnerError::MessageTooShort { message_len }.into()
	}

	/// Create a new error for a message with an invalid message type in the header.
	pub fn invalid_message_type(value: u32) -> Self {
		private::InnerError::InvalidMessageType { value }.into()
	}

	/// Create a new error for a message with an body that exceeds the allowed size.
	pub fn payload_too_large(body_len: usize, max_len: usize) -> Self {
		private::InnerError::PayloadTooLarge { body_len, max_len }.into()
	}

	/// Create a new error for an incoming message with an unexpected service ID.
	pub fn unexpected_service_id(service_id: i32) -> Self {
		private::InnerError::UnexpectedServiceId { service_id }.into()
	}

	/// Create a new error for an outgoing message body that could not be encoded.
	pub fn encode_failed(inner: Box<dyn std::error::Error + Send>) -> Self {
		private::InnerError::EncodeFailed(inner).into()
	}

	/// Create a new error for an incoming message with a body that could not be decoded.
	pub fn decode_failed(inner: Box<dyn std::error::Error + Send>) -> Self {
		private::InnerError::DecodeFailed(inner).into()
	}

	/// Create a new error for an incoming message that represent an error response from the remote peer.
	///
	/// A remote error does not indicate a communication or protocol violation.
	/// It is used when the remote peer correctly received and understood the request,
	/// but is unable to succesfully complete it.
	pub fn remote_error(message: String) -> Self {
		private::InnerError::RemoteError(message).into()
	}

	/// Create a new error with a custom message.
	pub fn custom(message: String) -> Self {
		private::InnerError::Custom(message).into()
	}

	/// Check if this error is caused by the remote peer closing the connection cleanly.
	pub fn is_connection_aborted(&self) -> bool {
		if let private::InnerError::Io(e) = &self.inner {
			e.kind() == std::io::ErrorKind::ConnectionAborted
		} else {
			false
		}
	}

	/// Check if an unexpected message type was received.
	///
	/// This can happen when you call [`recv_response()`][crate::SentRequestHandle::recv_response] while an update message is still queued.
	pub fn is_unexpected_message_type(&self) -> bool {
		matches!(&self.inner, private::InnerError::UnexpectedMessageType(_))
	}

	/// Check if this error represent an error response from the remote peer.
	///
	/// See [`Self::remote_error()`] for more details on what a remote error is.
	pub fn is_remote_error(&self) -> bool {
		matches!(&self.inner, private::InnerError::RemoteError(_))
	}

	/// Get this error as remote error message.
	///
	/// See [`Self::remote_error()`] for more details on what a remote error is.
	pub fn as_remote_error(&self) -> Option<&str> {
		if let private::InnerError::RemoteError(msg) = &self.inner {
			Some(msg)
		} else {
			None
		}
	}

	/// Get this error as remote error message.
	///
	/// See [`Self::remote_error()`] for more details on what a remote error is.
	pub fn into_remote_error(self) -> Option<String> {
		if let private::InnerError::RemoteError(msg) = self.inner {
			Some(msg)
		} else {
			None
		}
	}
}

impl From<std::io::Error> for Error {
	fn from(other: std::io::Error) -> Self {
		Self::io_error(other)
	}
}

pub(crate) mod private {
	use super::*;

	pub(crate) fn connection_aborted() -> Error {
		InnerError::from(std::io::Error::from(std::io::ErrorKind::ConnectionAborted)).into()
	}

	#[derive(Debug, Error)]
	#[error("{0}")]
	#[doc(hidden)]
	pub enum InnerError {
		/// An I/O error occurred.
		Io(#[from] std::io::Error),

		/// The received message is too short to be valid.
		#[error("the message is too short to be valid: need at least {} for the header, got only {message_len} bytes", crate::HEADER_LEN)]
		MessageTooShort {
			message_len: usize,
		},

		/// The received message has an invalid type.
		#[error("invalid message type: expected a value in the range [0..4], got {value}")]
		InvalidMessageType {
			/// The received value.
			value: u32,
		},

		/// The message body is too large.
		#[error("payload too large: maximum payload size is {max_len}, got {body_len}")]
		PayloadTooLarge {
			/// The actual length of the message body in bytes.
			body_len: usize,

			/// The maximum allowed length of a message body in bytes.
			max_len: usize,
		},

		/// The request ID is already associated with an open request.
		#[error("duplicate request ID: request ID {request_id} is already associated with an open request")]
		DuplicateRequestId {
			/// The duplicate request ID.
			request_id: u32,
		},

		/// The request ID is not associated with an open request.
		#[error("unknown request ID: request ID {request_id} is not associated with an open request")]
		UnknownRequestId {
			/// The unknown request ID.
			request_id: u32,
		},

		/// The received message has an unexpected message type.
		UnexpectedMessageType(#[from] UnexpectedMessageType),

		/// The received message has an unexpected service ID.
		#[error("unexpected service ID: {service_id}")]
		UnexpectedServiceId {
			/// The unrecognized/unexpected service ID.
			service_id: i32,
		},

		/// No free request ID was found.
		#[error("no free request ID was found")]
		NoFreeRequestIdFound,

		/// The request has already been closed.
		#[error("the request is already closed")]
		RequestClosed,

		/// Failed to encode the message.
		EncodeFailed(Box<dyn std::error::Error + Send>),

		/// Failed to decode the message.
		DecodeFailed(Box<dyn std::error::Error + Send>),

		/// The remote peer replied with an error instead of the regular response.
		RemoteError(String),

		/// A custom error message.
		Custom(String),
	}

	/// Check if a message size is large enough to contain a valid message.
	pub fn check_message_too_short(message_len: usize) -> Result<(), InnerError> {
		if message_len >= crate::HEADER_LEN as usize {
			Ok(())
		} else {
			Err(InnerError::MessageTooShort { message_len })
		}
	}

	/// Check if a payload length is small enough to fit in a message body.
	pub fn check_payload_too_large(body_len: usize, max_len: usize) -> Result<(), InnerError> {
		if body_len <= max_len as usize {
			Ok(())
		} else {
			Err(InnerError::PayloadTooLarge{ body_len, max_len })
		}
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
}
