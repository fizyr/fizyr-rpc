//! Traits for converting between RPC messages and Rust values.

/// A message format, used to encode/decode RPC messages from/to Rust types.
pub trait Format {
	/// The body type for the RPC messages.
	type Body: crate::Body;

	/// Encode a Rust value to a message.
	///
	/// This function must return the service ID and the message body as tuple if it succeeds.
	fn encode_message<T: IntoMessage<Self>>(value: T) -> Result<(i32, Self::Body), Box<dyn std::error::Error + Send>> {
		value.into_message()
	}

	/// Decode a message to a Rust value.
	fn decode_message<T: FromMessage<Self>>(message: crate::Message<Self::Body>) -> Result<T, crate::error::FromMessageError> {
		T::from_message(message)
	}
}

/// Trait for formats that can encode `T` to a message body.
pub trait EncodeBody<T>: Format {
	/// Encode the value to a message body.
	fn encode_body(value: T) -> Result<Self::Body, Box<dyn std::error::Error + Send>>;
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
pub trait IntoMessage<F: Format + ?Sized> {
	/// Encode a Rust value to a message.
	///
	/// This function must return the service ID and the message body as tuple if it succeeds.
	fn into_message(self) -> Result<(i32, F::Body), Box<dyn std::error::Error + Send>>;
}

/// Trait for values that can be decoded from a message with a specific [`Format`].
///
/// Unlike the [`DecodeBody`] trait,
/// this trait also allows the decoding to use the service ID of the message.
/// It is intended for enums that represent all possible messages for a specific interface.
pub trait FromMessage<F: Format + ?Sized>: Sized {
	/// Decode a message to the Rust value.
	fn from_message(message: crate::Message<F::Body>) -> Result<Self, crate::error::FromMessageError>;
}
