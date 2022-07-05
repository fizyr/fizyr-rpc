//! Error types.

use std::fmt::{Display, Formatter, Result};

/// Opaque error for all RPC operations.
#[derive(Debug)]
pub struct Error {
	pub(crate) inner: private::InnerError,
}

impl std::error::Error for Error {}

impl Display for Error {
	fn fmt(&self, f: &mut Formatter<'_>) -> Result {
		write!(f, "(inner)")
	}
}

impl From<private::InnerError> for Error {
	fn from(error: private::InnerError) -> Error {
		Self { inner: error }
	}
}

/// The received update is unknown or invalid.
///
/// This error is used in generated interfaces only.
pub enum ParseUpdateError<Body> {
	/// The received update has an unknown service ID.
	UnknownUpdate(crate::Message<Body>),

	/// The received update has a known service ID, but an invalid body.
	///
	/// The body has been consumed in the parse attempt,
	/// so only the message header and parse error are available.
	InvalidUpdate(crate::MessageHeader, Box<dyn std::error::Error + Send>),
}

/// Error that can occur when receiving a message from a peer using a generated interface.
///
/// Apart from the [`struct@Error`] reported by [`PeerHandle::recv_message()`][crate::PeerHandle::recv_message],
/// this error is used when the received message has an unknown service ID or an invalid body.
pub enum RecvMessageError<Body> {
	/// The underlying call to [`PeerHandle::recv_message()`][crate::PeerHandle::recv_message] returned an error.
	Other(Error),

	/// The received stream message has an unknown service ID.
	UnknownStream(crate::Message<Body>),

	/// The received request has an unknown service ID.
	UnknownRequest(crate::ReceivedRequestHandle<Body>, Body),

	/// The received stream message has a known service ID, but an invalid body.
	///
	/// The body has been consumed in the parse attempt,
	/// so only the message header and parse error are available.
	InvalidStream(crate::MessageHeader, Box<dyn std::error::Error + Send>),

	/// The received request has a known service ID, but an invalid body.
	///
	/// The body has been consumed in the parse attempt,
	/// so only the request handle and parse error are available.
	InvalidRequest(crate::ReceivedRequestHandle<Body>, Box<dyn std::error::Error + Send>),
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

impl<Body> RecvMessageError<Body> {
	/// Check if this error is caused by the remote peer closing the connection cleanly.
	pub fn is_connection_aborted(&self) -> bool {
		if let Self::Other(e) = self {
			e.is_connection_aborted()
		} else {
			false
		}
	}

	/// Get the raw request handle associated with the received message.
	///
	/// The request handle can be used to send an error response to unknown or invalid requests.
	///
	/// For errors other than [`Self::UnknownRequest`] and [`Self::InvalidRequest`],
	/// this function returns [`None`].
	pub fn request_handle(&self) -> Option<&crate::ReceivedRequestHandle<Body>> {
		match self {
			Self::Other(_error) => None,
			Self::UnknownStream(_message) => None,
			Self::UnknownRequest(request, _body) => Some(request),
			Self::InvalidStream(_message, _error) => None,
			Self::InvalidRequest(request, _error) => Some(request),
		}
	}

	/// Get the a mutable reference to the raw request handle associated with the received message.
	///
	/// The request handle can be used to send an error response to unknown or invalid requests.
	///
	/// For errors other than [`Self::UnknownRequest`] and [`Self::InvalidRequest`],
	/// this function returns [`None`].
	pub fn request_handle_mut(&mut self) -> Option<&mut crate::ReceivedRequestHandle<Body>> {
		match self {
			Self::Other(_error) => None,
			Self::UnknownStream(_message) => None,
			Self::UnknownRequest(request, _body) => Some(request),
			Self::InvalidStream(_message, _error) => None,
			Self::InvalidRequest(request, _error) => Some(request),
		}
	}
}

impl From<std::io::Error> for Error {
	fn from(other: std::io::Error) -> Self {
		Self::io_error(other)
	}
}

impl<Body> From<Error> for RecvMessageError<Body> {
	fn from(other: Error) -> Self {
		Self::Other(other)
	}
}

impl<Body> std::error::Error for ParseUpdateError<Body> {}
impl<Body> std::error::Error for RecvMessageError<Body> {}

impl<Body> std::fmt::Display for ParseUpdateError<Body> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::UnknownUpdate(message) => write!(f, "received unknown update with service ID {}", message.header.service_id),
			Self::InvalidUpdate(header, error) => write!(f, "received invalid update with service ID {}: {}", header.service_id, error),
		}
	}
}

impl<Body> std::fmt::Display for RecvMessageError<Body> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Other(e) => write!(f, "{}", e),
			Self::UnknownStream(message) => write!(f, "received unknown stream message with service ID {}", message.header.service_id),
			Self::InvalidStream(header, error) => write!(f, "received invalid stream message with service ID {}: {}", header.service_id, error),
			Self::UnknownRequest(request, _body) => write!(f, "received unknown request message with service ID {}", request.service_id()),
			Self::InvalidRequest(request, error) => write!(f, "received invalid request message with service ID {}: {}", request.service_id(), error),
		}
	}
}

impl<Body> std::fmt::Debug for ParseUpdateError<Body> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::UnknownUpdate(message) => f.debug_tuple("UnknownUpdate").field(message).finish(),
			Self::InvalidUpdate(header, error) => f.debug_tuple("InvalidUpdate").field(header).field(error).finish(),
		}
	}
}

impl<Body> std::fmt::Debug for RecvMessageError<Body> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Other(e) => f.debug_tuple("Other").field(e).finish(),
			Self::UnknownStream(message) => f.debug_tuple("UnknownStream").field(message).finish(),
			Self::UnknownRequest(request, _body) => f.debug_tuple("UnknownStream").field(request).finish(),
			Self::InvalidStream(header, error) => f.debug_tuple("InvalidStread").field(header).field(error).finish(),
			Self::InvalidRequest(request, error) => f.debug_tuple("InvalidRequest").field(request).field(error).finish(),
		}
	}
}

pub(crate) mod private {
	use super::*;

	pub(crate) fn connection_aborted() -> Error {
		InnerError::from(std::io::Error::from(std::io::ErrorKind::ConnectionAborted)).into()
	}

	#[derive(Debug)]
	#[doc(hidden)]
	pub enum InnerError {
		/// An I/O error occurred.
		Io(std::io::Error),

		/// The received message is too short to be valid.
		MessageTooShort { message_len: usize },

		/// The received message has an invalid type.
		InvalidMessageType {
			/// The received value.
			value: u32,
		},

		/// The message body is too large.
		PayloadTooLarge {
			/// The actual length of the message body in bytes.
			body_len: usize,

			/// The maximum allowed length of a message body in bytes.
			max_len: usize,
		},

		/// The request ID is already associated with an open request.
		DuplicateRequestId {
			/// The duplicate request ID.
			request_id: u32,
		},

		/// The request ID is not associated with an open request.
		UnknownRequestId {
			/// The unknown request ID.
			request_id: u32,
		},

		/// The received message has an unexpected message type.
		UnexpectedMessageType(UnexpectedMessageType),

		/// The received message has an unexpected service ID.
		UnexpectedServiceId {
			/// The unrecognized/unexpected service ID.
			service_id: i32,
		},

		/// No free request ID was found.
		NoFreeRequestIdFound,

		/// The request has already been closed.
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

	impl From<std::io::Error> for private::InnerError {
		fn from(error: std::io::Error) -> Self {
			private::InnerError::Io(error)
		}
	}

	impl From<UnexpectedMessageType> for private::InnerError {
		fn from(error: UnexpectedMessageType) -> Self {
			private::InnerError::UnexpectedMessageType(error)
		}
	}

	impl std::error::Error for InnerError {}

	impl Display for InnerError {
		fn fmt(&self, f: &mut Formatter<'_>) -> Result {
			match self {
				InnerError::MessageTooShort { message_len } => write!(
					f,
					"the message is too short to be valid: need at least {} for the header, got only {message_len} bytes",
					crate::HEADER_LEN
				),
				InnerError::InvalidMessageType { value } => write!(f, "invalid message type: expected a value in the range [0..4], got {value}"),
				InnerError::PayloadTooLarge { body_len, max_len } => {
					write!(f, "payload too large: maximum payload size is {max_len}, got {body_len}")
				},
				InnerError::DuplicateRequestId { request_id } => write!(
					f,
					"duplicate request ID: request ID {request_id} is already associated with an open request"
				),
				InnerError::UnknownRequestId { request_id } => {
					write!(f, "unknown request ID: request ID {request_id} is not associated with an open request")
				},
				InnerError::UnexpectedServiceId { service_id } => write!(f, "unexpected service ID: {service_id}"),
				InnerError::NoFreeRequestIdFound => write!(f, "no free request ID was found"),
				InnerError::RequestClosed => write!(f, "the request is already closed"),
				_ => write!(f, "{{0}}"),
			}
		}
	}

	/// Check if a message size is large enough to contain a valid message.
	#[allow(dead_code)] // not used when all transports are disabled.
	pub fn check_message_too_short(message_len: usize) -> std::result::Result<(), InnerError> {
		if message_len >= crate::HEADER_LEN as usize {
			Ok(())
		} else {
			Err(InnerError::MessageTooShort { message_len })
		}
	}

	/// Check if a payload length is small enough to fit in a message body.
	pub fn check_payload_too_large(body_len: usize, max_len: usize) -> std::result::Result<(), InnerError> {
		if body_len <= max_len as usize {
			Ok(())
		} else {
			Err(InnerError::PayloadTooLarge { body_len, max_len })
		}
	}

	/// The received message had an unexpected type.
	#[derive(Debug, Clone)]
	pub struct UnexpectedMessageType {
		/// The actual type of the received message.
		pub value: crate::MessageType,

		/// The expected type of the received message.
		pub expected: crate::MessageType,
	}

	impl std::error::Error for UnexpectedMessageType {}

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
			write!(
				f,
				"unexpected message type: expected {}, got {}",
				to_str(self.expected),
				to_str(self.value)
			)
		}
	}
}
