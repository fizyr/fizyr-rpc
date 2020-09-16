use crate::error;

/// The encoded length of a message header (excluding the frame size).
pub const HEADER_LEN: u32 = 12;

/// The maximum length of a message body.
///
/// This is the maxmimum possible length, limited by the 32 bit message length field and the presence of a message header.
/// Other (lower) limits may be enforced by the API or remote peers.
pub const MAX_PAYLOAD_LEN: u32 = u32::MAX - HEADER_LEN;

/// Well-known service IDs.
pub mod service_id {
	/// The service ID used for error responses.
	pub const ERROR: i32 = -1;
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

	/// Create a new reponse message header.
	pub fn response(request_id: u32, service_id: i32) -> Self {
		Self {
			message_type: MessageType::Response,
			request_id,
			service_id,
		}
	}

	/// Create a new error reponse message header.
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
}
