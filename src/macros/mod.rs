#[doc(hidden)]
pub use fizyr_rpc_macros::interface as interface_impl;

pub mod error;

pub trait FromMessageBody<Body>: Sized {
	type Error;

	fn from_message_body(body: &Body) -> Result<Self, Self::Error>;
}

pub trait ToMessageBody<Body> {
	type Error;

	fn to_message_body(&self) -> Result<Body, Self::Error>;
}

pub trait Protocol {
	type EncodeError: std::error::Error + 'static;
	type DecodeError: std::error::Error + 'static;
	type Body: crate::Body;

	fn encode<T: Encode<Self>>(value: T) -> Result<Self::Body, Self::EncodeError> {
		value.encode()
	}

	fn decode<T: Decode<Self>>(body: Self::Body) -> Result<T, Self::DecodeError> {
		T::decode(body)
	}
}

pub trait Encode<P: Protocol + ?Sized> {
	fn encode(self) -> Result<P::Body, P::EncodeError>;
}

pub trait Decode<P: Protocol + ?Sized>: Sized {
	fn decode(body: P::Body) -> Result<Self, P::DecodeError>;
}

pub trait ToMessage<P: Protocol + ?Sized> {
	fn to_message(self) -> Result<(i32, P::Body), P::EncodeError>;
}

pub trait FromMessage<P: Protocol + ?Sized>: Sized {
	fn from_message(message: crate::Message<P::Body>) -> Result<Self, error::FromMessageError<P::DecodeError>>;
}

#[macro_export]
/// Define an RPC interface.
macro_rules! interface {
	($($tokens:tt)*) => {
		$crate::macros::interface_impl!{$crate; $($tokens)*}
	}
}
