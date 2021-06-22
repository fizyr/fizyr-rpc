use tokio::sync::mpsc;
use tokio::sync::oneshot;

use crate::error;
use crate::peer::Command;
use crate::Message;

pub(crate) enum RequestHandleCommand<Body> {
	Close,
	Message(Message<Body>),
}

/// A handle for a sent request.
///
/// The handle can be used to receive updates and the response from the remote peer,
/// and to send update messages to the remote peer.
pub struct SentRequestHandle<Body> {
	write_handle: SentRequestWriteHandle<Body>,
	incoming_rx: mpsc::UnboundedReceiver<RequestHandleCommand<Body>>,
	peek_buffer: Option<Message<Body>>,
}

/// A write handle for a sent request.
///
/// Unlike [`SentRequestHandle`], write handles can be cloned.
/// But unlike regular handles, they can not be used to receive updates or the response from the remote peer.
///
/// Write handles can be used to send updates even when the regular handle is mutably borrowed.
///
/// You can get more write handles using [`SentRequestHandle::write_handle()`] or by cloning an existing one.
pub struct SentRequestWriteHandle<Body> {
	request_id: u32,
	service_id: i32,
	command_tx: mpsc::UnboundedSender<Command<Body>>,
}

/// A handle for a received request.
///
/// The handle can be used to receive updates from the remote peer,
/// and to send updates and the response to the remote peer.
pub struct ReceivedRequestHandle<Body> {
	write_handle: ReceivedRequestWriteHandle<Body>,
	incoming_rx: mpsc::UnboundedReceiver<RequestHandleCommand<Body>>,
}

/// A write handle for a received request.
///
/// Unlike [`ReceivedRequestHandle`], write handles can be cloned.
/// But unlike regular handles, they can not be used to receive updates or the response from the remote peer.
///
/// Write handles can be used to send updates even when the regular handle is mutably borrowed.
///
/// You can get more write handles using [`ReceivedRequestHandle::write_handle()`] or by cloning an existing one.
pub struct ReceivedRequestWriteHandle<Body> {
	request_id: u32,
	service_id: i32,
	command_tx: mpsc::UnboundedSender<Command<Body>>,
}

/// An incoming request or stream message.
pub enum ReceivedMessage<Body> {
	/// An incoming request.
	Request(ReceivedRequestHandle<Body>, Body),

	/// An incoming stream message.
	Stream(Message<Body>),
}

impl<Body> SentRequestHandle<Body> {
	/// Create a new sent request.
	pub(crate) fn new(
		request_id: u32,
		service_id: i32,
		incoming_rx: mpsc::UnboundedReceiver<RequestHandleCommand<Body>>,
		command_tx: mpsc::UnboundedSender<Command<Body>>,
	) -> Self {
		let write_handle = SentRequestWriteHandle {
			request_id,
			service_id,
			command_tx,
		};
		Self {
			write_handle,
			incoming_rx,
			peek_buffer: None,
		}
	}

	/// Get the request ID of the sent request.
	pub fn request_id(&self) -> u32 {
		self.write_handle.request_id()
	}

	/// Get the service ID of the initial request message.
	pub fn service_id(&self) -> i32 {
		self.write_handle.service_id()
	}

	/// Create a write handle for this request.
	///
	/// The write handle can be cloned and used even while this handle is mutably borrowed.
	pub fn write_handle(&self) -> SentRequestWriteHandle<Body> {
		self.write_handle.clone()
	}

	/// Receive the next update message of the request from the remote peer.
	///
	/// This function returns `None` if the final response is received instead of an update message.
	/// If that happens, the response message can be read using [`Self::recv_response`].
	pub async fn recv_update(&mut self) -> Option<Message<Body>> {
		let message = self.recv_message().await?;
		if message.header.message_type.is_responder_update() {
			Some(message)
		} else {
			self.peek_buffer = Some(message);
			None
		}
	}

	/// Receive the final response of the request from the remote peer.
	///
	/// This function returns an [`InvalidMessageType`][error::InvalidMessageType] if the received message is an update message.
	/// If that happens, the update message can be read using [`Self::recv_update`].
	/// To ensure that there are no update messages left, keep calling [`Self::recv_update`] untill it returns `Ok(None)`.
	pub async fn recv_response(&mut self) -> Result<Message<Body>, error::RecvMessageError> {
		let message = self.recv_message().await.ok_or_else(error::connection_aborted)?;
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
	async fn recv_message(&mut self) -> Option<Message<Body>> {
		if let Some(message) = self.peek_buffer.take() {
			Some(message)
		} else {
			match self.incoming_rx.recv().await? {
				RequestHandleCommand::Message(message) => {
					// Close the channel when reading a response message.
					if message.header.message_type.is_response() {
						self.incoming_rx.close();
					}
					Some(message)
				},
				// Close the channel when instructed to do so.
				// This is sent by the request tracker when unregistering the request.
				RequestHandleCommand::Close => {
					self.incoming_rx.close();
					None
				},
			}
		}
	}

	/// Send an update for the request to the remote peer.
	pub async fn send_update(&self, service_id: i32, body: impl Into<Body>) -> Result<(), error::SendMessageError> {
		self.write_handle.send_update(service_id, body).await
	}
}

impl<Body> SentRequestWriteHandle<Body> {
	/// Get the request ID of the sent request.
	pub fn request_id(&self) -> u32 {
		self.request_id
	}

	/// Get the service ID of the initial request message.
	pub fn service_id(&self) -> i32 {
		self.service_id
	}

	/// Send an update for the request to the remote peer.
	pub async fn send_update(&self, service_id: i32, body: impl Into<Body>) -> Result<(), error::SendMessageError> {
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
}

impl<Body> ReceivedRequestHandle<Body> {
	/// Create a new received request.
	pub(crate) fn new(
		request_id: u32,
		service_id: i32,
		incoming_rx: mpsc::UnboundedReceiver<RequestHandleCommand<Body>>,
		command_tx: mpsc::UnboundedSender<Command<Body>>,
	) -> Self {
		let write_handle = ReceivedRequestWriteHandle {
			request_id,
			service_id,
			command_tx,
		};
		Self {
			write_handle,
			incoming_rx,
		}
	}

	/// Get the request ID of the received request.
	pub fn request_id(&self) -> u32 {
		self.write_handle.request_id()
	}

	/// Get the service ID of the received request message.
	pub fn service_id(&self) -> i32 {
		self.write_handle.service_id()
	}

	/// Create a write handle for this request.
	///
	/// The write handle can be cloned and used even while this handle is mutably borrowed.
	pub fn write_handle(&self) -> ReceivedRequestWriteHandle<Body> {
		self.write_handle.clone()
	}

	/// Receive the next update message of the request from the remote peer.
	pub async fn recv_update(&mut self) -> Option<Message<Body>> {
		match self.incoming_rx.recv().await? {
			RequestHandleCommand::Message(x) => Some(x),
			// Close the channel when instructed to do so.
			// This is sent by the request tracker when unregistering the request.
			RequestHandleCommand::Close => {
				self.incoming_rx.close();
				None
			},
		}
	}

	/// Send an update for the request to the remote peer.
	pub async fn send_update(&self, service_id: i32, body: impl Into<Body>) -> Result<(), error::WriteMessageError> {
		self.write_handle.send_update(service_id, body).await
	}

	/// Send the final response for the request to the remote peer.
	pub async fn send_response(&self, service_id: i32, body: impl Into<Body>) -> Result<(), error::WriteMessageError> {
		self.write_handle.send_response(service_id, body).await
	}

	/// Send the final response with an error message.
	pub async fn send_error_response(&self, message: &str) -> Result<(), error::WriteMessageError>
	where
		Body: crate::Body,
	{
		self.write_handle.send_error_response(message).await
	}
}

impl<Body> ReceivedRequestWriteHandle<Body> {
	/// Get the request ID of the sent request.
	pub fn request_id(&self) -> u32 {
		self.request_id
	}

	/// Get the service ID of the initial request message.
	pub fn service_id(&self) -> i32 {
		self.service_id
	}

	/// Send an update for the request to the remote peer.
	pub async fn send_update(&self, service_id: i32, body: impl Into<Body>) -> Result<(), error::WriteMessageError> {
		let body = body.into();
		self.send_raw_message(Message::responder_update(self.request_id, service_id, body)).await
	}

	/// Send the final response for the request to the remote peer.
	pub async fn send_response(&self, service_id: i32, body: impl Into<Body>) -> Result<(), error::WriteMessageError> {
		let body = body.into();
		self.send_raw_message(Message::response(self.request_id, service_id, body)).await
	}

	/// Send the final response with an error message.
	pub async fn send_error_response(&self, message: &str) -> Result<(), error::WriteMessageError>
	where
		Body: crate::Body,
	{
		self.send_raw_message(Message::error_response(self.request_id, message)).await
	}

	/// Send a raw message.
	async fn send_raw_message(&self, message: Message<Body>) -> Result<(), error::WriteMessageError> {
		use crate::peer::SendRawMessage;
		let (result_tx, result_rx) = oneshot::channel();
		self.command_tx
			.send(SendRawMessage { message, result_tx }.into())
			.map_err(|_| error::connection_aborted())?;
		result_rx.await.map_err(|_| error::connection_aborted())?
	}
}

impl<Body> std::fmt::Debug for SentRequestHandle<Body> {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		// TODO: use finish_non_exhaustive when it hits stable.
		f.debug_struct("SentRequestHandle")
			.field("request_id", &self.request_id())
			.field("service_id", &self.service_id())
			.finish()
	}
}

impl<Body> std::fmt::Debug for SentRequestWriteHandle<Body> {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		// TODO: use finish_non_exhaustive when it hits stable.
		f.debug_struct("SentRequestWriteHandle")
			.field("request_id", &self.request_id())
			.field("service_id", &self.service_id())
			.finish()
	}
}

impl<Body> std::fmt::Debug for ReceivedRequestHandle<Body> {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		// TODO: use finish_non_exhaustive when it hits stable.
		f.debug_struct("ReceivedRequestHandle")
			.field("request_id", &self.request_id())
			.field("service_id", &self.service_id())
			.finish()
	}
}

impl<Body> std::fmt::Debug for ReceivedRequestWriteHandle<Body> {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		// TODO: use finish_non_exhaustive when it hits stable.
		f.debug_struct("ReceivedRequestWriteHandle")
			.field("request_id", &self.request_id())
			.field("service_id", &self.service_id())
			.finish()
	}
}

impl<Body> std::fmt::Debug for ReceivedMessage<Body> {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		match self {
			Self::Request(x, _body) => write!(f, "Request({:?})", x),
			Self::Stream(x) => write!(f, "Stream({:?})", x),
		}
	}
}

impl<Body> Clone for SentRequestWriteHandle<Body> {
	fn clone(&self) -> Self {
		Self {
			request_id: self.request_id,
			service_id: self.service_id,
			command_tx: self.command_tx.clone(),
		}
	}
}

impl<Body> Clone for ReceivedRequestWriteHandle<Body> {
	fn clone(&self) -> Self {
		Self {
			request_id: self.request_id,
			service_id: self.service_id,
			command_tx: self.command_tx.clone(),
		}
	}
}
