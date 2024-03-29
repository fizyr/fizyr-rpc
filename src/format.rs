//! Traits for converting between RPC messages and Rust values.
//!
//! These traits are used by generated interfaces from the [`interface!`] macro.
//! Normally, you would only implement these traits for your own serialization format.
//! However, the traits are covered by semver guarantees, so feel free to use them in your own code.

use crate::Error;

/// A message format, used to encode/decode RPC messages from/to Rust types.
pub trait Format {
	/// The body type for the RPC messages.
	type Body: crate::Body;

	/// Encode a Rust value to a message.
	///
	/// This function must return the service ID and the message body as tuple if it succeeds.
	fn encode_message<T: ToMessage<Self>>(value: &T) -> Result<(i32, Self::Body), Box<dyn std::error::Error + Send>> {
		value.to_message()
	}

	/// Decode a message to a Rust value.
	fn decode_message<T: FromMessage<Self>>(message: crate::Message<Self::Body>) -> Result<T, Error> {
		T::from_message(message)
	}
}

/// Trait for formats that can encode `T` to a message body.
pub trait EncodeBody<T: ?Sized>: Format {
	/// Encode the value to a message body.
	fn encode_body(value: &T) -> Result<Self::Body, Box<dyn std::error::Error + Send>>;
}

/// Trait for formats that can decode `T` from a message body.
pub trait DecodeBody<T: Sized>: Format {
	/// Decode a message body to the Rust value.
	fn decode_body(body: Self::Body) -> Result<T, Box<dyn std::error::Error + Send>>;
}

/// Trait for values that can be encoded to a message with a specific [`Format`].
///
/// Unlike the [`EncodeBody`] trait,
/// this trait requires that the service ID is derived from the Rust value.
/// It is intended for enums that represent all possible messages for a specific interface.
pub trait ToMessage<F: Format + ?Sized> {
	/// Encode a Rust value to a message.
	///
	/// This function must return the service ID and the message body as tuple if it succeeds.
	fn to_message(&self) -> Result<(i32, F::Body), Box<dyn std::error::Error + Send>>;
}

/// Trait for values that can be decoded from a message with a specific [`Format`].
///
/// Unlike the [`DecodeBody`] trait,
/// this trait also allows the decoding to use the service ID of the message.
/// It is intended for enums that represent all possible messages for a specific interface.
pub trait FromMessage<F: Format + ?Sized>: Sized {
	/// Decode a message to the Rust value.
	fn from_message(message: crate::Message<F::Body>) -> Result<Self, Error>;
}
