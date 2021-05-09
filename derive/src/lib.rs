use fizyr_rpc::Message;
pub use fizyr_rpc_derive_macros::interface;

pub mod error;
use error::FromMessageError;

pub trait FromMessageBody<Body>: Sized {
	type Error;

	fn from_message_body(body: &Body) -> Result<Self, Self::Error>;
}

pub trait ToMessageBody<Body> {
	type Error;

	fn to_message_body(&self) -> Result<Body, Self::Error>;
}

pub trait FromMessage<Body>: Sized {
	type ParseError;

	fn from_message(message: &Message<Body>) -> Result<Self, FromMessageError<Self::ParseError>>;
}

pub trait ToMessage<Body>: Sized {
	type Error;

	fn to_message(&self) -> Result<Self, Self::Error>;
}
