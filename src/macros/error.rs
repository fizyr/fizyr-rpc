use thiserror::Error;

/// An error decoding a message.
#[derive(Debug, Error)]
#[error("{0}")]
pub enum FromMessageError {
	/// The service ID was not recognized.
	UnexpectedServiceId(#[from] crate::error::UnexpectedServiceId),

	/// Failed to decode the message body.
	DecodeBody(Box<dyn std::error::Error + Send>),
}

impl From<FromMessageError> for crate::error::RecvMessageError {
	fn from(other: FromMessageError) -> Self {
		match other {
			FromMessageError::UnexpectedServiceId(x) => x.into(),
			FromMessageError::DecodeBody(e) => Self::DecodeBody(e),
		}
	}
}
