use crate::error;

/// The encoded length of a message header (excluding the frame size).
pub const HEADER_LEN: u32 = 12;

/// The maximum length of a message body.
///
/// This is the maximum possible length, limited by the 32 bit message length field and the presence of a message header.
/// Other (lower) limits may be enforced by the API or remote peers.
pub const MAX_PAYLOAD_LEN: u32 = u32::MAX - HEADER_LEN;

/// Trait for types that can be used as message body.
pub trait Body {
	/// Create a message body from an error message.
	fn from_error(message: &str) -> Self;
}

/// Well-known service IDs.
pub mod service_id {
	/// The service ID used for error responses.
	pub const ERROR: i32 = -1;
}

/// A complete RPC message, including header and body.
pub struct Message<Body> {
	/// The header of the message.
	pub header: MessageHeader,

	/// The body of the message.
	pub body: Body,
}

impl<Body> Message<Body> {
	/// Create a new message with a header and a body.
	pub fn new(header: MessageHeader, body: Body) -> Self {
		Self { header, body }
	}

	/// Create a new request message.
	pub fn request(request_id: u32, service_id: i32, body: Body) -> Self {
		Self::new(MessageHeader::request(request_id, service_id), body)
	}

	/// Create a new response message.
	pub fn response(request_id: u32, service_id: i32, body: Body) -> Self {
		Self::new(MessageHeader::response(request_id, service_id), body)
	}

	/// Create a new error response message.
	pub fn error_response(request_id: u32, message: &str) -> Self
	where
		Body: crate::Body,
	{
		Self::new(MessageHeader::response(request_id, service_id::ERROR), Body::from_error(message))
	}

	/// Create a new requester update message.
	pub fn requester_update(request_id: u32, service_id: i32, body: Body) -> Self {
		Self::new(MessageHeader::requester_update(request_id, service_id), body)
	}

	/// Create a new responder update message.
	pub fn responder_update(request_id: u32, service_id: i32, body: Body) -> Self {
		Self::new(MessageHeader::responder_update(request_id, service_id), body)
	}

	/// Create a new stream message.
	pub fn stream(request_id: u32, service_id: i32, body: Body) -> Self {
		Self::new(MessageHeader::stream(request_id, service_id), body)
	}
}

/// The type of a message.
#[repr(u32)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum MessageType {
	/// A message that initiates a request.
	Request = 0,

	/// A response message that terminates a request.
	Response = 1,

	/// An update message sent by the peer that initiated the request.
	RequesterUpdate = 2,

	/// A response message sent by the peer that received the request.
	ResponderUpdate = 3,

	/// A stream message that is sent outside of the context of a request.
	Stream = 4,
}

impl MessageType {
	/// Try to convert a [`u32`] into a [`MessageType`]
	pub fn from_u32(value: u32) -> Result<Self, error::InvalidMessageType> {
		match value {
			0 => Ok(Self::Request),
			1 => Ok(Self::Response),
			2 => Ok(Self::RequesterUpdate),
			3 => Ok(Self::ResponderUpdate),
			4 => Ok(Self::Stream),
			value => Err(error::InvalidMessageType { value }),
		}
	}

	/// Check if this message type is [`Self::Request`].
	pub fn is_request(self) -> bool {
		self == MessageType::Request
	}

	/// Check if this message type is [`Self::Response`].
	pub fn is_response(self) -> bool {
		self == MessageType::Response
	}

	/// Check if this message type is [`Self::RequesterUpdate`].
	pub fn is_requester_update(self) -> bool {
		self == MessageType::RequesterUpdate
	}

	/// Check if this message type is [`Self::ResponderUpdate`].
	pub fn is_responder_update(self) -> bool {
		self == MessageType::ResponderUpdate
	}

	/// Check if this message type is [`Self::Stream`].
	pub fn is_stream(self) -> bool {
		self == MessageType::Stream
	}
}

/// A message header.
#[repr(C)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct MessageHeader {
	/// The message type.
	pub message_type: MessageType,

	/// The request that the message is part of.
	///
	/// Unused for stream messages.
	pub request_id: u32,

	/// The service that the message is for.
	///
	/// For request messages, this indicates the service being requested.
	///
	/// For response messages this indicates success or failure.
	///
	/// For update messages this indicates the type of update.
	pub service_id: i32,
}

impl MessageHeader {
	/// Create a new request message header.
	pub fn request(request_id: u32, service_id: i32) -> Self {
		Self {
			message_type: MessageType::Request,
			request_id,
			service_id,
		}
	}

	/// Create a new response message header.
	pub fn response(request_id: u32, service_id: i32) -> Self {
		Self {
			message_type: MessageType::Response,
			request_id,
			service_id,
		}
	}

	/// Create a new error response message header.
	pub fn error_response(request_id: u32) -> Self {
		Self::response(request_id, service_id::ERROR)
	}

	/// Create a new requester update message header.
	pub fn requester_update(request_id: u32, service_id: i32) -> Self {
		Self {
			message_type: MessageType::RequesterUpdate,
			request_id,
			service_id,
		}
	}

	/// Create a new responder update message header.
	pub fn responder_update(request_id: u32, service_id: i32) -> Self {
		Self {
			message_type: MessageType::ResponderUpdate,
			request_id,
			service_id,
		}
	}

	/// Create a new stream message header.
	pub fn stream(request_id: u32, service_id: i32) -> Self {
		Self {
			message_type: MessageType::Stream,
			request_id,
			service_id,
		}
	}

	/// Decode a message header from a byte slice.
	///
	/// The byte slice should NOT contain the message size.
	///
	/// # Panic
	/// This function panics if the buffer does not contain a full header.
	pub fn decode(buffer: &[u8]) -> Result<Self, error::InvalidMessageType> {
		use byteorder::{ByteOrder, LE};
		let message_type = LE::read_u32(&buffer[0..]);
		let request_id = LE::read_u32(&buffer[4..]);
		let service_id = LE::read_i32(&buffer[8..]);

		let message_type = MessageType::from_u32(message_type)?;
		Ok(Self { message_type, request_id, service_id })
	}

	/// Encode a message header into a byte slice.
	///
	/// This will NOT add a message size (which would be impossible even if we wanted to).
	///
	/// # Panic
	/// This function panics if the buffer is not large enough to hold a full header.
	pub fn encode(&self, buffer: &mut [u8]) {
		use byteorder::{ByteOrder, LE};
		assert!(buffer.len() >= 12);
		LE::write_u32(&mut buffer[0..], self.message_type as u32);
		LE::write_u32(&mut buffer[4..], self.request_id);
		LE::write_i32(&mut buffer[8..], self.service_id);
	}
}

impl<Body> std::fmt::Debug for Message<Body> {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		// TODO: use finish_non_exhaustive when it hits stable.
		f.debug_struct("Message")
			.field("header", &self.header)
			.finish()
	}
}
