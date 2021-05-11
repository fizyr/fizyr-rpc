use std::error::Error;
use thiserror::Error;

/// An error occurred while performing a service call.
#[derive(Debug, Error)]
#[error("{0}")]
pub enum ServiceCallError<EncodeError, DecodeError>
where
	EncodeError: Error + 'static,
	DecodeError: Error + 'static,
{
	/// Failed to encode or send the request.
	SendRequest(#[from] SendMessageError<EncodeError>),

	/// Failed to receive or decode the response.
	ReceiveResponse(#[from] RecvMessageError<DecodeError>),
}

impl<EncodeError, DecodeError> From<crate::error::SendRequestError> for ServiceCallError<EncodeError, DecodeError>
where
	EncodeError: Error + 'static,
	DecodeError: Error + 'static,
{
	fn from(other: crate::error::SendRequestError) -> Self {
		Self::SendRequest(other.into())
	}
}

impl<EncodeError, DecodeError> From<FromMessageError<DecodeError>> for ServiceCallError<EncodeError, DecodeError>
where
	EncodeError: Error + 'static,
	DecodeError: Error + 'static,
{
	fn from(other: FromMessageError<DecodeError>) -> Self {
		Self::ReceiveResponse(other.into())
	}
}

impl<EncodeError, DecodeError> From<crate::error::RecvMessageError> for ServiceCallError<EncodeError, DecodeError>
where
	EncodeError: Error + 'static,
	DecodeError: Error + 'static,
{
	fn from(other: crate::error::RecvMessageError) -> Self {
		Self::ReceiveResponse(other.into())
	}
}

/// An error occurred while reading or decoding a message.
#[derive(Debug, Error)]
#[error("{0}")]
pub enum SendMessageError<EncodeError: Error + 'static> {
	/// Failed to encode the message body.
	EncodeBody(EncodeError),

	/// Failed to send the message.
	SendMessage(#[from] crate::error::SendRequestError),
}

/// An error occurred while reading or decoding a message.
#[derive(Debug, Error)]
#[error("{0}")]
pub enum RecvMessageError<DecodeError: Error + 'static> {
	/// Failed to receive the response.
	RecvMessage(#[from] crate::error::RecvMessageError),

	/// Failed to decode the message.
	FromMessage(#[from] FromMessageError<DecodeError>),
}

/// An error decoding a message.
#[derive(Debug, Error)]
#[error("{0}")]
pub enum FromMessageError<DecodeError: Error + 'static> {
	/// The service ID was not recognized.
	UnknownServiceId(#[from] UnknownServiceId),

	/// Failed to decode the message body.
	DecodeBody(DecodeError),
}

/// The service ID was not recognized.
#[derive(Debug, Error)]
#[error("unknown service ID: {service_id}")]
pub struct UnknownServiceId {
	pub service_id: i32,
}
