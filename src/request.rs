use crate::Message;
use crate::error;
use crate::util::BidiChannel;

/// A handle for a sent request.
///
/// The handle can be used to receive updates and the response from the remote peer,
/// and to send update messages to the remote peer.
pub struct SentRequest<Body> {
	request_id: u32,
	service_id: i32,
	channel: BidiChannel<Message<Body>>,
}

/// A handle for a received request.
///
/// The handle can be used to receive updates from the remote peer,
/// and to send updates and the response to the remote peer.
pub struct ReceivedRequest<Body> {
	request_id: u32,
	service_id: i32,
	channel: BidiChannel<Message<Body>>,
}

impl<Body: crate::Body> SentRequest<Body> {
	/// Create a new sent request.
	pub(crate) fn new(request_id: u32, service_id: i32, channel: BidiChannel<Message<Body>>) -> Self {
		Self { request_id, service_id, channel }
	}

	/// Get the request ID of the sent request.
	pub fn request_id(&self) -> u32 {
		self.request_id
	}

	/// Get the service ID of the initial request message.
	pub fn service_id(&self) -> i32 {
		self.service_id
	}

	/// Read the next message from the request.
	///
	/// This could be an update message or a response message.
	// TODO: change return type to eliminate impossible message types?
	pub async fn read_message(&mut self) -> Result<Message<Body>, error::ConnectionClosed> {
		self.channel.receive().await
	}

	/// Send an update for the request.
	pub async fn send_update(&mut self, service_id: i32, body: Body) -> Result<(), error::ConnectionClosed> {
		let message = Message::requester_update(self.request_id, service_id, body);
		self.channel.send(message).await
	}
}

impl<Body: crate::Body> ReceivedRequest<Body> {
	/// Create a new received request.
	pub(crate) fn new(request_id: u32, service_id: i32, channel: BidiChannel<Message<Body>>) -> Self {
		Self { request_id, service_id, channel }
	}

	/// Get the request ID of the received request.
	pub fn request_id(&self) -> u32 {
		self.request_id
	}

	/// Get the service ID of the initial request message.
	pub fn service_id(&self) -> i32 {
		self.service_id
	}

	/// Read the next message from the request.
	///
	/// This can only be an update message.
	// TODO: change return type to eliminate impossible message types?
	pub async fn read_message(&mut self) -> Result<Message<Body>, error::ConnectionClosed> {
		self.channel.receive().await
	}

	/// Send an update for the request.
	pub async fn send_update(&mut self, service_id: i32, body: Body) -> Result<(), error::ConnectionClosed> {
		let message = Message::responder_update(self.request_id, service_id, body);
		self.channel.send(message).await
	}

	/// Send the final response.
	pub async fn send_response(mut self, service_id: i32, body: Body) -> Result<(), error::ConnectionClosed> {
		let message = Message::response(self.request_id, service_id, body);
		self.channel.send(message).await
	}

	/// Send the final response with an error message.
	pub async fn send_error_response(self, message: &str) -> Result<(), error::ConnectionClosed> {
		self.send_response(crate::service_id::ERROR, Body::from_error(message)).await
	}
}

impl<Body> std::fmt::Debug for SentRequest<Body> {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		// TODO: use finish_non_exhaustive when it hits stable.
		f.debug_struct("SentRequest")
			.field("request_id", &self.request_id)
			.field("service_id", &self.service_id)
			.finish()
	}
}

impl<Body> std::fmt::Debug for ReceivedRequest<Body> {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		// TODO: use finish_non_exhaustive when it hits stable.
		f.debug_struct("ReceivedRequest")
			.field("request_id", &self.request_id)
			.field("service_id", &self.service_id)
			.finish()
	}
}
