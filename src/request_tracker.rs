use futures::channel::mpsc;
use std::collections::BTreeMap;

use crate::Message;
use crate::MessageType;
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

/// Tracker that manages open requests.
///
/// You normally do not need to work with a request tracker directly.
/// It is used by the different peer structs internally.
pub struct RequestTracker<Body> {
	/// The next ID to use for sending a request.
	next_sent_request_id: u32,

	/// Sender of the channel for outgoing messages.
	///
	/// It is kept around here to prevent the channel from closing and so that we can clone it.
	outgoing_tx: mpsc::UnboundedSender<Message<Body>>,

	/// Map of channels for incoming messages for sent requests.
	sent_requests: BTreeMap<u32, mpsc::UnboundedSender<Message<Body>>>,

	/// Map of channels for incoming messages for received requests.
	received_requests: BTreeMap<u32, mpsc::UnboundedSender<Message<Body>>>,
}

impl<Body: crate::Body> SentRequest<Body> {
	/// Create a new sent request.
	fn new(request_id: u32, service_id: i32, channel: BidiChannel<Message<Body>>) -> Self {
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
	fn new(request_id: u32, service_id: i32, channel: BidiChannel<Message<Body>>) -> Self {
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

impl<Body: crate::Body> RequestTracker<Body> {
	/// Create a new request tracker.
	///
	/// The `outgoing_tx` channel is used for outgoing messages.
	/// All messages on the channel should be sent to the remote peer by a task with the receiving end of the channel.
	pub fn new(outgoing_tx: mpsc::UnboundedSender<Message<Body>>) -> Self {
		Self {
			next_sent_request_id: 0,
			outgoing_tx,
			sent_requests: BTreeMap::new(),
			received_requests: BTreeMap::new(),
		}
	}

	/// Allocate a request ID and register a new sent request.
	pub fn allocate_sent_request(&mut self, service_id: i32) -> Result<SentRequest<Body>, error::NoFreeRequestIdFound> {
		use std::collections::btree_map::Entry;

		// Try to find a free ID a bunch of times.
		for _ in 0..100 {
			let request_id = self.next_sent_request_id;
			self.next_sent_request_id = self.next_sent_request_id.wrapping_add(1);

			if let Entry::Vacant(entry) = self.sent_requests.entry(request_id) {
				let (incoming_tx, request_channel) = make_message_channel(self.outgoing_tx.clone());
				entry.insert(incoming_tx);
				return Ok(SentRequest::new(request_id, service_id, request_channel));
			}
		}

		// But eventually give up.
		return Err(error::NoFreeRequestIdFound)
	}

	/// Register a new sent request.
	///
	/// Returns an error if the request ID is already in use.
	pub fn register_received_request(&mut self, request_id: u32, service_id: i32) -> Result<ReceivedRequest<Body>, error::DuplicateRequestId> {
		use std::collections::btree_map::Entry;

		match self.received_requests.entry(request_id) {
			Entry::Occupied(_) => {
				Err(error::DuplicateRequestId { request_id })
			}
			Entry::Vacant(entry) => {
				let (incoming_tx, request_channel) = make_message_channel(self.outgoing_tx.clone());
				entry.insert(incoming_tx);
				Ok(ReceivedRequest::new(request_id, service_id, request_channel))
			}
		}
	}

	/// Process an incoming request, update or response message.
	///
	/// # Panics
	/// This function panics if you pass it a stream message.
	pub fn process_incoming_message(&mut self, message: Message<Body>) -> Result<Option<ReceivedRequest<Body>>, error::ProcessIncomingMessageError> {
		match message.header.message_type {
			MessageType::Request => Ok(self.process_incoming_request(message).map(Some)?),
			MessageType::Response => Ok(self.process_incoming_response(message).map(|_| None)?),
			MessageType::RequesterUpdate => Ok(self.process_incoming_requester_update(message).map(|_| None)?),
			MessageType::ResponderUpdate => Ok(self.process_incoming_responder_update(message).map(|_| None)?),
			MessageType::Stream => panic!("stream message passed to process_incoming_message"),
		}
	}

	fn process_incoming_request(&mut self, message: Message<Body>) -> Result<ReceivedRequest<Body>, error::DuplicateRequestId> {
		todo!();
	}

	fn process_incoming_response(&mut self, message: Message<Body>) -> Result<(), error::DuplicateRequestId> {
		todo!();
	}

	fn process_incoming_requester_update(&mut self, message: Message<Body>) -> Result<(), error::UnknownRequestId> {
		todo!();
	}

	fn process_incoming_responder_update(&mut self, message: Message<Body>) -> Result<(), error::UnknownRequestId> {
		todo!();
	}
}

/// Create a new channel for incoming and outgoing messages.
///
/// This creates a new MPSC channel for incoming message,
/// and combines it with the existing channel for outgoing messages.
fn make_message_channel<Body>(outgoing_tx: mpsc::UnboundedSender<Message<Body>>) -> (mpsc::UnboundedSender<Message<Body>>, BidiChannel<Message<Body>>) {
	let (incoming_tx, incoming_rx) = mpsc::unbounded();
	let request_channel = BidiChannel::from_halves(outgoing_tx.clone(), incoming_rx);
	(incoming_tx, request_channel)
}
