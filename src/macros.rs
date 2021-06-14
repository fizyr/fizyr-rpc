#[doc(hidden)]
pub use fizyr_rpc_macros::interface as interface_impl;

pub trait Protocol {
	type Body: crate::Body;
	type Transport: crate::transport::Transport<Body = Self::Body>;

	fn encode_body<T: Encode<Self>>(value: T) -> Result<Self::Body, Box<dyn std::error::Error + Send>> {
		value.encode()
	}

	fn decode_body<T: Decode<Self>>(body: Self::Body) -> Result<T, Box<dyn std::error::Error + Send>> {
		T::decode(body)
	}

	fn encode_message<T: IntoMessage<Self>>(value: T) -> Result<(i32, Self::Body), Box<dyn std::error::Error + Send>> {
		value.into_message()
	}

	fn decode_message<T: FromMessage<Self>>(message: crate::Message<Self::Body>) -> Result<T, crate::error::FromMessageError> {
		T::from_message(message)
	}
}

pub trait Encode<P: Protocol + ?Sized> {
	fn encode(self) -> Result<P::Body, Box<dyn std::error::Error + Send>>;
}

pub trait Decode<P: Protocol + ?Sized>: Sized {
	fn decode(body: P::Body) -> Result<Self, Box<dyn std::error::Error + Send>>;
}

pub trait IntoMessage<P: Protocol + ?Sized> {
	fn into_message(self) -> Result<(i32, P::Body), Box<dyn std::error::Error + Send>>;
}

pub trait FromMessage<P: Protocol + ?Sized>: Sized {
	fn from_message(message: crate::Message<P::Body>) -> Result<Self, crate::error::FromMessageError>;
}

#[macro_export]
/// Define an RPC interface.
macro_rules! interface {
	($($tokens:tt)*) => {
		$crate::macros::interface_impl!{$crate; $($tokens)*}
	}
}
