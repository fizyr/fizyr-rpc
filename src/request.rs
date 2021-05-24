use tokio::sync::mpsc;
use tokio::sync::oneshot;

use crate::error;
use crate::peer::Command;
use crate::Message;

/// A handle for a sent request.
///
/// The handle can be used to receive updates and the response from the remote peer,
/// and to send update messages to the remote peer.
pub struct SentRequest<Body> {
	request_id: u32,
	service_id: i32,
	incoming_rx: mpsc::UnboundedReceiver<Message<Body>>,
	command_tx: mpsc::UnboundedSender<Command<Body>>,
	peek_buffer: Option<Message<Body>>,
}

/// A handle for a received request.
///
/// The handle can be used to receive updates from the remote peer,
/// and to send updates and the response to the remote peer.
pub struct ReceivedRequest<Body> {
	request_id: u32,
	service_id: i32,
	body: Body,
	incoming_rx: mpsc::UnboundedReceiver<Message<Body>>,
	command_tx: mpsc::UnboundedSender<Command<Body>>,
}

/// An incoming request or stream message.
pub enum Incoming<Body> {
	/// An incoming request.
	Request(ReceivedRequest<Body>),

	/// An incoming stream message.
	Stream(Message<Body>),
}

/// An outgoing request or stream message.
pub enum Outgoing<Body> {
	/// An outgoing request.
	Request(SentRequest<Body>),

	/// An outgoing stream message.
	Stream(Message<Body>),
}

impl<Body> SentRequest<Body> {
	/// Create a new sent request.
	pub(crate) fn new(
		request_id: u32,
		service_id: i32,
		incoming_rx: mpsc::UnboundedReceiver<Message<Body>>,
		command_tx: mpsc::UnboundedSender<Command<Body>>,
	) -> Self {
		Self {
			request_id,
			service_id,
			incoming_rx,
			command_tx,
			peek_buffer: None,
		}
	}

	/// Get the request ID of the sent request.
	pub fn request_id(&self) -> u32 {
		self.request_id
	}

	/// Get the service ID of the initial request message.
	pub fn service_id(&self) -> i32 {
		self.service_id
	}

	/// Receive the next update message of the request from the remote peer.
	///
	/// This function returns `Ok(None)` if the final response is received instead of an update message.
	/// If that happens, the response message can be read using [`Self::recv_response`].
	pub async fn recv_update(&mut self) -> Result<Option<Message<Body>>, error::RecvMessageError> {
		let message = self.recv_message().await?;
		if message.header.message_type.is_responder_update() {
			Ok(Some(message))
		} else {
			self.peek_buffer = Some(message);
			Ok(None)
		}
	}

	/// Receive the final response of the request from the remote peer.
	///
	/// This function returns an [`InvalidMessageType`][error::InvalidMessageType] if the received message is an update message.
	/// If that happens, the update message can be read using [`Self::recv_update`].
	/// To ensure that there are no update messages left, keep calling [`Self::recv_update`] untill it returns `Ok(None)`.
	pub async fn recv_response(&mut self) -> Result<Message<Body>, error::RecvMessageError> {
		let message = self.recv_message().await?;
		let kind = message.header.message_type;
		if kind.is_response() {
			Ok(message)
		} else {
			self.peek_buffer = Some(message);
			Err(error::UnexpectedMessageType {
				value: kind,
				expected: crate::MessageType::Response,
			}.into())
		}
	}

	/// Receive the next message of the request from the remote peer.
	///
	/// This could be an update message or a response message.
	async fn recv_message(&mut self) -> Result<Message<Body>, error::RecvMessageError> {
		if let Some(message) = self.peek_buffer.take() {
			Ok(message)
		} else {
			Ok(self.incoming_rx.recv().await.ok_or_else(error::connection_aborted)?)
		}
	}

	/// Send an update for the request to the remote peer.
	pub async fn send_update(&self, service_id: i32, body: impl Into<Body>) -> Result<(), error::SendUpdateError> {
		use crate::peer::SendRawMessage;
		let body = body.into();
		let (result_tx, result_rx) = oneshot::channel();
		let message = Message::requester_update(self.request_id, service_id, body);
		self.command_tx
			.send(SendRawMessage { message, result_tx }.into())
			.map_err(|_| error::connection_aborted())?;
		result_rx.await.map_err(|_| error::connection_aborted())??;
		Ok(())
	}

	/// Put a message back in the peek buffer.
	///
	/// Do not call this function,
	/// it is not convered by the version number API stability guarantee.
	///
	/// # Panics
	/// This function panics if there already is a message in the peek buffer.
	#[doc(hidden)]
	pub fn _unpeek_message(&mut self, message: Message<Body>) {
		assert!(self.peek_buffer.is_none());
		self.peek_buffer = Some(message);
	}
}

impl<Body> ReceivedRequest<Body> {
	/// Create a new received request.
	pub(crate) fn new(
		request_id: u32,
		service_id: i32,
		body: Body,
		incoming_rx: mpsc::UnboundedReceiver<Message<Body>>,
		command_tx: mpsc::UnboundedSender<Command<Body>>,
	) -> Self {
		Self {
			request_id,
			service_id,
			body,
			incoming_rx,
			command_tx,
		}
	}

	/// Get the request ID of the received request.
	pub fn request_id(&self) -> u32 {
		self.request_id
	}

	/// Get the service ID of the initial request message.
	pub fn service_id(&self) -> i32 {
		self.service_id
	}

	/// Get the body of the initial request message.
	pub fn body(&self) -> &Body {
		&self.body
	}

	/// Receive the next update message of the request from the remote peer.
	pub async fn recv_update(&mut self) -> Result<Message<Body>, error::ReadMessageError> {
		Ok(self.incoming_rx.recv().await.ok_or_else(error::connection_aborted)?)
	}

	/// Send an update for the request to the remote peer.
	pub async fn send_update(&mut self, service_id: i32, body: impl Into<Body>) -> Result<(), error::WriteMessageError> {
		let body = body.into();
		self.send_raw_message(Message::responder_update(self.request_id, service_id, body)).await
	}

	/// Send the final response for the request to the remote peer.
	pub async fn send_response(mut self, service_id: i32, body: impl Into<Body>) -> Result<(), error::WriteMessageError> {
		let body = body.into();
		self.send_raw_message(Message::response(self.request_id, service_id, body)).await
	}

	/// Send the final response with an error message.
	pub async fn send_error_response(mut self, message: &str) -> Result<(), error::WriteMessageError>
	where
		Body: crate::Body,
	{
		self.send_raw_message(Message::error_response(self.request_id, message)).await
	}

	/// Send a raw message.
	async fn send_raw_message(&mut self, message: Message<Body>) -> Result<(), error::WriteMessageError> {
		use crate::peer::SendRawMessage;
		let (result_tx, result_rx) = oneshot::channel();
		self.command_tx
			.send(SendRawMessage { message, result_tx }.into())
			.map_err(|_| error::connection_aborted())?;
		result_rx.await.map_err(|_| error::connection_aborted())?
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

impl<Body> std::fmt::Debug for Incoming<Body> {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		match self {
			Self::Request(x) => {
				write!(f, "ReceivedRequest(")?;
				x.fmt(f)?;
				write!(f, ")")?;
			},
			Self::Stream(x) => {
				write!(f, "Stream(")?;
				x.fmt(f)?;
				write!(f, ")")?;
			},
		}
		Ok(())
	}
}
