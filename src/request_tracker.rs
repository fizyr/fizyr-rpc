use futures::channel::mpsc;
use futures::sink::SinkExt;
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use crate::Message;
use crate::MessageType;
use crate::ReceivedRequest;
use crate::SentRequest;
use crate::error;
use crate::util::BidiChannel;

/// A message that was not fully processed by the request tracker.
///
/// When you pass a message to `RequestTracker::process_incoming_message`,
/// not all messages are fully handled internally by a request tracker.
///
/// Specifically, request and stream messages still need to be processed by the caller.
pub enum UnhandledMessage<Body> {
	ReceivedRequest(ReceivedRequest<Body>),
	Stream(Message<Body>),
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

	/// Remove a sent request from the tracker.
	///
	/// This should be called when a request is finished to make the ID available again.
	/// Note that sent requests are also removed internally when they receive a response,
	/// or when they would receive a message but the [`SentRequest`] was dropped.
	pub fn remove_sent_request(&mut self, request_id: u32) -> Result<(), error::UnknownRequestId> {
		self.sent_requests.remove(&request_id)
			.ok_or_else(|| error::UnknownRequestId { request_id })?;
		Ok(())
	}

	/// Register a new sent request.
	///
	/// Returns an error if the request ID is already in use.
	pub fn register_received_request(&mut self, request_id: u32, service_id: i32) -> Result<ReceivedRequest<Body>, error::DuplicateRequestId> {
		match self.received_requests.entry(request_id) {
			Entry::Occupied(mut entry) => {
				// If the existing entry has an open channel, the request ID is still in use.
				if !entry.get().is_closed() {
					Err(error::DuplicateRequestId { request_id })

				// If the entry has a closed channel then received request has already been dropped.
				// That means the request ID is no longer in use.
				} else {
					let (incoming_tx, request_channel) = make_message_channel(self.outgoing_tx.clone());
					entry.insert(incoming_tx);
					Ok(ReceivedRequest::new(request_id, service_id, request_channel))
				}
			}

			// The request ID is available.
			Entry::Vacant(entry) => {
				let (incoming_tx, request_channel) = make_message_channel(self.outgoing_tx.clone());
				entry.insert(incoming_tx);
				Ok(ReceivedRequest::new(request_id, service_id, request_channel))
			}
		}
	}

	/// Remove a received request from the tracker.
	///
	/// This should be called when a request is finished to make the ID available again.
	/// Note that received requests are also removed internally when they would receive a message but the [`ReceivedRequest`] was dropped.
	pub fn remove_received_request(&mut self, request_id: u32) -> Result<(), error::UnknownRequestId> {
		self.received_requests.remove(&request_id)
			.ok_or_else(|| error::UnknownRequestId { request_id })?;
		Ok(())
	}

	/// Process an incoming message.
	///
	/// This will pass the message on to an open request if any matches.
	///
	/// Returns an error
	///  * if an incoming request message uses an already claimed request ID
	///  * if an incoming update or response message does not match an open request
	pub async fn process_incoming_message(&mut self, message: Message<Body>) -> Result<Option<UnhandledMessage<Body>>, error::ProcessIncomingMessageError> {
		match message.header.message_type {
			MessageType::Request => {
				let received_request = self.register_received_request(message.header.request_id, message.header.service_id)?;
				Ok(Some(UnhandledMessage::ReceivedRequest(received_request)))
			}
			MessageType::Response => {
				self.process_incoming_response(message).await?;
				Ok(None)
			}
			MessageType::RequesterUpdate => {
				self.process_incoming_requester_update(message).await?;
				Ok(None)
			}
			MessageType::ResponderUpdate => {
				self.process_incoming_responder_update(message).await?;
				Ok(None)
			}
			MessageType::Stream => {
				Ok(Some(UnhandledMessage::Stream(message)))
			}
		}
	}

	async fn process_incoming_response(&mut self, message: Message<Body>) -> Result<(), error::UnknownRequestId> {
		let request_id = message.header.request_id;
		match self.sent_requests.entry(request_id) {
			Entry::Vacant(_) => {
				Err(error::UnknownRequestId { request_id })
			}
			Entry::Occupied(mut entry) => {
				// Forward the message to the sent_request, then remove the entry.
				let _ = entry.get_mut().send(message).await;
				entry.remove();
				Ok(())
			}
		}
	}

	async fn process_incoming_requester_update(&mut self, message: Message<Body>) -> Result<(), error::UnknownRequestId> {
		let request_id = message.header.request_id;
		match self.received_requests.entry(request_id) {
			Entry::Vacant(_) => {
				Err(error::UnknownRequestId { request_id })
			}
			Entry::Occupied(mut entry) => {
				// If the received_request is dropped, clear the entry.
				if let Err(_) = entry.get_mut().send(message).await {
					entry.remove();
					Err(error::UnknownRequestId { request_id })
				} else {
					Ok(())
				}
			}
		}
	}

	async fn process_incoming_responder_update(&mut self, message: Message<Body>) -> Result<(), error::UnknownRequestId> {
		let request_id = message.header.request_id;
		match self.sent_requests.entry(request_id) {
			Entry::Vacant(_) => {
				Err(error::UnknownRequestId { request_id })
			}
			Entry::Occupied(mut entry) => {
				// If the sent_request is dropped, clear the entry.
				if let Err(_) = entry.get_mut().send(message).await {
					entry.remove();
					Err(error::UnknownRequestId { request_id })
				} else {
					Ok(())
				}
			}
		}
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

#[cfg(test)]
mod test {
	use assert2::assert;
	use assert2::let_assert;
	use futures::stream::StreamExt;

	use super::*;
	use crate::MessageHeader;

	struct Body;

	impl crate::Body for Body {
		fn from_error(_message: &str) -> Self {
			Self
		}
	}

	#[async_std::test]
	async fn test_incoming_request() {
		let (outgoing_tx, mut outgoing_rx) = mpsc::unbounded();
		let mut tracker = RequestTracker::new(outgoing_tx);

		// Simulate an incoming request and an update.
		let_assert!(Ok(Some(UnhandledMessage::ReceivedRequest(mut received_request))) = tracker.process_incoming_message(Message::request(1, 2, Body)).await);
		assert!(let Ok(None) = tracker.process_incoming_message(Message::requester_update(1, 10, Body)).await);

		// Receive the update.
		let_assert!(Ok(message) = received_request.read_message().await);
		assert!(message.header == MessageHeader::requester_update(1, 10));

		// Send and receive the response.
		let_assert!(Ok(()) = received_request.send_response(3, Body).await);
		let_assert!(Some(response) = outgoing_rx.next().await);
		assert!(response.header.request_id == 1);
		assert!(response.header.service_id == 3);

		// The received request is now dropped, so lets check that new incoming message cause an error.
		let_assert!(Err(error::ProcessIncomingMessageError::UnknownRequestId(e)) = tracker.process_incoming_message(Message::requester_update(1, 11, Body)).await);
		eprintln!("{:?}", tracker.process_incoming_message(Message::requester_update(1, 11, Body)).await);
		assert!(e.request_id == 1);
	}

	#[async_std::test]
	async fn test_outgoing_request() {
		let (outgoing_tx, mut outgoing_rx) = mpsc::unbounded();
		let mut tracker = RequestTracker::new(outgoing_tx);

		// Simute an outgoing request.
		let_assert!(Ok(mut sent_request) = tracker.allocate_sent_request(3));

		// Simulate and receive a responder update.
		assert!(let Ok(None) = tracker.process_incoming_message(Message::responder_update(sent_request.request_id(), 12, Body)).await);
		let_assert!(Ok(update) = sent_request.read_message().await);
		assert!(update.header == MessageHeader::responder_update(sent_request.request_id(), 12));

		// Send a response.
		let_assert!(Ok(()) = sent_request.send_update(13, Body).await);
		let_assert!(Some(update) = outgoing_rx.next().await);
		assert!(update.header == MessageHeader::requester_update(sent_request.request_id(), 13));

		// Simulate and receive a response update.
		assert!(let Ok(None) = tracker.process_incoming_message(Message::response(sent_request.request_id(), 14, Body)).await);
		let_assert!(Ok(update) = sent_request.read_message().await);
		assert!(update.header == MessageHeader::response(sent_request.request_id(), 14));

		// After receiving the response, the entry should be removed from the tracker.
		// So no more incoming messages for the request should be accepted.
		let_assert!(Err(error::ProcessIncomingMessageError::UnknownRequestId(e)) = tracker.process_incoming_message(Message::responder_update(sent_request.request_id(), 15, Body)).await);
		assert!(e.request_id == sent_request.request_id());
	}
}

impl<Body> std::fmt::Debug for UnhandledMessage<Body> {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		match self {
			Self::ReceivedRequest(x) => {
				write!(f, "ReceivedRequest(")?;
				x.fmt(f)?;
				write!(f, ")")?;
			}
			Self::Stream(x) => {
				write!(f, "Stream(")?;
				x.fmt(f)?;
				write!(f, ")")?;
			}
		}
		Ok(())
	}
}
