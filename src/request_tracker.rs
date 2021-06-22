use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use thiserror::Error;
use tokio::sync::mpsc;

use crate::error;
use crate::peer::Command;
use crate::ReceivedMessage;
use crate::Message;
use crate::MessageType;
use crate::ReceivedRequestHandle;
use crate::SentRequestHandle;

/// An error occurred while processing an incoming message.
#[derive(Debug, Clone, Error)]
#[error("{0}")]
pub enum ProcessIncomingMessageError {
	/// The incoming request message has a request ID that is already associated with an open request.
	DuplicateRequestId(#[from] error::DuplicateRequestId),

	/// The incoming update or response message has a request ID that is not associated with an open request.
	UnknownRequestId(#[from] error::UnknownRequestId),
}

// Allow a ProcessIncomingMessageError to be converted to a RecvMessageError automatically.
impl From<ProcessIncomingMessageError> for error::RecvMessageError {
	fn from(other: ProcessIncomingMessageError) -> Self {
		match other {
			ProcessIncomingMessageError::DuplicateRequestId(e) => e.into(),
			ProcessIncomingMessageError::UnknownRequestId(e) => e.into(),
		}
	}
}

/// Tracker that manages open requests.
///
/// You normally do not need to work with a request tracker directly.
/// It is used by the different peer structs internally.
pub struct RequestTracker<Body> {
	/// The next ID to use for sending a request.
	next_sent_request_id: u32,

	/// Sender of the channel for command messages.
	///
	/// It is kept around here to prevent the channel from closing and so that we can clone it.
	command_tx: mpsc::UnboundedSender<Command<Body>>,

	/// Map of channels for incoming messages for sent requests.
	sent_requests: BTreeMap<u32, mpsc::UnboundedSender<Message<Body>>>,

	/// Map of channels for incoming messages for received requests.
	received_requests: BTreeMap<u32, mpsc::UnboundedSender<Message<Body>>>,
}

impl<Body> RequestTracker<Body> {
	/// Create a new request tracker.
	///
	/// The `command_tx` channel is used for command messages.
	/// All messages on the channel should be sent to the remote peer by a task with the receiving end of the channel.
	pub fn new(command_tx: mpsc::UnboundedSender<Command<Body>>) -> Self {
		Self {
			next_sent_request_id: 0,
			command_tx,
			sent_requests: BTreeMap::new(),
			received_requests: BTreeMap::new(),
		}
	}

	/// Allocate a request ID and register a new sent request.
	pub fn allocate_sent_request(&mut self, service_id: i32) -> Result<SentRequestHandle<Body>, error::NoFreeRequestIdFound> {
		// Try to find a free ID a bunch of times.
		for _ in 0..100 {
			let request_id = self.next_sent_request_id;
			self.next_sent_request_id = self.next_sent_request_id.wrapping_add(1);

			if let Entry::Vacant(entry) = self.sent_requests.entry(request_id) {
				let (incoming_tx, incoming_rx) = mpsc::unbounded_channel();
				entry.insert(incoming_tx);
				return Ok(SentRequestHandle::new(request_id, service_id, incoming_rx, self.command_tx.clone()));
			}
		}

		// But eventually give up.
		Err(error::NoFreeRequestIdFound)
	}

	/// Remove a sent request from the tracker.
	///
	/// This should be called when a request is finished to make the ID available again.
	/// Note that sent requests are also removed internally when they receive a response,
	/// or when they would receive a message but the [`SentRequestHandle`] was dropped.
	pub fn remove_sent_request(&mut self, request_id: u32) -> Result<(), error::UnknownRequestId> {
		self.sent_requests.remove(&request_id).ok_or(error::UnknownRequestId { request_id })?;
		Ok(())
	}

	/// Register a new sent request.
	///
	/// Returns an error if the request ID is already in use.
	pub fn register_received_request(
		&mut self,
		request_id: u32,
		service_id: i32,
		body: Body,
	) -> Result<(ReceivedRequestHandle<Body>, Body), error::DuplicateRequestId> {
		match self.received_requests.entry(request_id) {
			Entry::Occupied(_entry) => {
				// TODO: Check if the channel is closed so we don't error out unneccesarily.
				// Requires https://github.com/tokio-rs/tokio/pull/2726
				// if !entry.get().is_closed() {
				Err(error::DuplicateRequestId { request_id })

				// If the entry has a closed channel then received request has already been dropped.
				// That means the request ID is no longer in use.
				// } else {
				// 	let (incoming_tx, incoming_rx) = mpsc::unbounded_channel();
				// 	entry.insert(incoming_tx);
				// 	Ok(ReceivedRequestHandle::new(request_id, service_id, incoming_rx, self.command_tx.clone()))
				// }
			},

			// The request ID is available.
			Entry::Vacant(entry) => {
				let (incoming_tx, incoming_rx) = mpsc::unbounded_channel();
				entry.insert(incoming_tx);
				Ok((ReceivedRequestHandle::new(request_id, service_id, incoming_rx, self.command_tx.clone()), body))
			},
		}
	}

	/// Remove a received request from the tracker.
	///
	/// This should be called when a request is finished to make the ID available again.
	/// Note that received requests are also removed internally when they would receive a message but the [`ReceivedRequestHandle`] was dropped.
	#[allow(unused)] // TODO: Evaluate if Peer should be calling this sometimes.
	pub fn remove_received_request(&mut self, request_id: u32) -> Result<(), error::UnknownRequestId> {
		self.received_requests.remove(&request_id).ok_or(error::UnknownRequestId { request_id })?;
		Ok(())
	}

	/// Process an incoming message.
	///
	/// This will pass the message on to an open request if any matches.
	///
	/// Returns an error
	///  * if an incoming request message uses an already claimed request ID
	///  * if an incoming update or response message does not match an open request
	pub async fn process_incoming_message(&mut self, message: Message<Body>) -> Result<Option<ReceivedMessage<Body>>, ProcessIncomingMessageError> {
		match message.header.message_type {
			MessageType::Request => {
				let (received_request, body) = self.register_received_request(message.header.request_id, message.header.service_id, message.body)?;
				Ok(Some(ReceivedMessage::Request(received_request, body)))
			},
			MessageType::Response => {
				self.process_incoming_response(message).await?;
				Ok(None)
			},
			MessageType::RequesterUpdate => {
				self.process_incoming_requester_update(message).await?;
				Ok(None)
			},
			MessageType::ResponderUpdate => {
				self.process_incoming_responder_update(message).await?;
				Ok(None)
			},
			MessageType::Stream => Ok(Some(ReceivedMessage::Stream(message))),
		}
	}

	async fn process_incoming_response(&mut self, message: Message<Body>) -> Result<(), error::UnknownRequestId> {
		let request_id = message.header.request_id;
		match self.sent_requests.entry(request_id) {
			Entry::Vacant(_) => Err(error::UnknownRequestId { request_id }),
			Entry::Occupied(mut entry) => {
				// Forward the message to the sent_request, then remove the entry.
				let _: Result<_, _> = entry.get_mut().send(message);
				entry.remove();
				Ok(())
			},
		}
	}

	async fn process_incoming_requester_update(&mut self, message: Message<Body>) -> Result<(), error::UnknownRequestId> {
		let request_id = message.header.request_id;
		match self.received_requests.entry(request_id) {
			Entry::Vacant(_) => Err(error::UnknownRequestId { request_id }),
			Entry::Occupied(mut entry) => {
				// If the received_request is dropped, clear the entry.
				if entry.get_mut().send(message).is_err() {
					entry.remove();
					Err(error::UnknownRequestId { request_id })
				} else {
					Ok(())
				}
			},
		}
	}

	async fn process_incoming_responder_update(&mut self, message: Message<Body>) -> Result<(), error::UnknownRequestId> {
		let request_id = message.header.request_id;
		match self.sent_requests.entry(request_id) {
			Entry::Vacant(_) => Err(error::UnknownRequestId { request_id }),
			Entry::Occupied(mut entry) => {
				// If the sent_request is dropped, clear the entry.
				if entry.get_mut().send(message).is_err() {
					entry.remove();
					Err(error::UnknownRequestId { request_id })
				} else {
					Ok(())
				}
			},
		}
	}
}

#[cfg(test)]
mod test {
	use assert2::assert;
	use assert2::let_assert;

	use super::*;
	use crate::MessageHeader;

	struct Body;

	impl crate::Body for Body {
		fn empty() -> Self {
			Self
		}

		fn from_error(_message: &str) -> Self {
			Self
		}
	}

	#[tokio::test]
	async fn test_incoming_request() {
		let (command_tx, mut command_rx) = mpsc::unbounded_channel();
		let mut tracker = RequestTracker::new(command_tx);

		let command_task = tokio::spawn(async move {
			// Check that we get the command to send an update.
			let_assert!(Some(Command::SendRawMessage(command)) = command_rx.recv().await);
			assert!(command.message.header == MessageHeader::responder_update(1, 3));
			assert!(let Ok(()) = command.result_tx.send(Ok(())));

			// Check that we get the command to send a response.
			let_assert!(Some(Command::SendRawMessage(command)) = command_rx.recv().await);
			assert!(command.message.header == MessageHeader::response(1, 4));
			assert!(let Ok(()) = command.result_tx.send(Ok(())));

			// Shouldn't get any more commands.
			assert!(let None = command_rx.recv().await);
		});

		// Simulate an incoming request and an update.
		let_assert!(Ok(Some(ReceivedMessage::Request(mut received_request, _body))) = tracker.process_incoming_message(Message::request(1, 2, Body)).await);
		assert!(let Ok(None) = tracker.process_incoming_message(Message::requester_update(1, 10, Body)).await);

		// Receive the update.
		let_assert!(Some(update) = received_request.recv_update().await);
		assert!(update.header == MessageHeader::requester_update(1, 10));

		// Send and update and response.
		let_assert!(Ok(()) = received_request.send_update(3, Body).await);
		let_assert!(Ok(()) = received_request.send_response(4, Body).await);

		// The received request is now dropped, so lets check that new incoming message cause an error.
		let_assert!(
			Err(ProcessIncomingMessageError::UnknownRequestId(e)) =
				tracker.process_incoming_message(Message::requester_update(1, 11, Body)).await
		);
		assert!(e.request_id == 1);

		drop(tracker);
		assert!(let Ok(()) = command_task.await);
	}

	#[tokio::test]
	async fn test_outgoing_request() {
		let (command_tx, mut command_rx) = mpsc::unbounded_channel();
		let mut tracker = RequestTracker::new(command_tx);

		// Simulate an command request.
		let_assert!(Ok(mut sent_request) = tracker.allocate_sent_request(3));
		let request_id = sent_request.request_id();

		let command_task = tokio::spawn(async move {
			// Check that we get the command to send an update.
			let_assert!(Some(Command::SendRawMessage(command)) = command_rx.recv().await);
			assert!(command.message.header == MessageHeader::requester_update(request_id, 13));
			assert!(let Ok(()) = command.result_tx.send(Ok(())));

			// Shouldn't get any more commands.
			assert!(let None = command_rx.recv().await);
		});

		// Simulate and receive a responder update.
		assert!(let Ok(None) = tracker.process_incoming_message(Message::responder_update(sent_request.request_id(), 12, Body)).await);
		let_assert!(Some(update) = sent_request.recv_update().await);
		assert!(update.header == MessageHeader::responder_update(sent_request.request_id(), 12));

		// Send an update.
		let_assert!(Ok(()) = sent_request.send_update(13, Body).await);

		// Simulate and receive a response update.
		assert!(let Ok(None) = tracker.process_incoming_message(Message::response(sent_request.request_id(), 14, Body)).await);
		let_assert!(Ok(update) = sent_request.recv_response().await);
		assert!(update.header == MessageHeader::response(sent_request.request_id(), 14));

		// After receiving the response, the entry should be removed from the tracker.
		// So no more incoming messages for the request should be accepted.
		let_assert!(
			Err(ProcessIncomingMessageError::UnknownRequestId(e)) = tracker
				.process_incoming_message(Message::responder_update(sent_request.request_id(), 15, Body))
				.await
		);
		assert!(e.request_id == sent_request.request_id());

		drop(tracker);
		drop(sent_request);
		assert!(let Ok(()) = command_task.await);
	}
}
