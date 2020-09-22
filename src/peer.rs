use tokio::sync::mpsc;
use tokio::sync::oneshot;

use crate::Incoming;
use crate::Message;
use crate::SentRequest;
use crate::error;

/// Handle to a peer.
pub struct PeerHandle<Body> {
	read_handle: PeerReadHandle<Body>,
	write_handle: PeerWriteHandle<Body>,
}

/// Handle to receive messages from a peer.
///
/// The read handle can be used to receive incoming requests and stream messages.
pub struct PeerReadHandle<Body> {
	incoming_rx: mpsc::UnboundedReceiver<Result<Incoming<Body>, error::NextMessageError>>,
	command_tx: mpsc::UnboundedSender<Command<Body>>,
}

/// Handle to send messages to a peer.
///
/// The write handle can be used to send requests and stream messages.
pub struct PeerWriteHandle<Body> {
	command_tx: mpsc::UnboundedSender<Command<Body>>,
}

/// Handle to close the connection with a peer.
#[derive(Clone)]
pub struct PeerCloseHandle<Body> {
	command_tx: mpsc::UnboundedSender<Command<Body>>,
}

/// Internal message sent by the write half to the peer task.
pub enum Command<Body> {
	SendRequest(SendRequest<Body>),
	SendRawMessage(SendRawMessage<Body>),
	ProcessIncomingMessage(ProcessIncomingMessage<Body>),
	Stop,
	StopWriteHalf,
	StopReadHalf,
}

impl<Body> PeerHandle<Body> {
	/// Create a new peer handle from the separate channels.
	pub(crate) fn new(
		incoming_rx: mpsc::UnboundedReceiver<Result<Incoming<Body>, error::NextMessageError>>,
		command_tx: mpsc::UnboundedSender<Command<Body>>,
	) -> Self {
		let read_handle = PeerReadHandle { incoming_rx, command_tx: command_tx.clone() };
		let write_handle = PeerWriteHandle { command_tx };
		Self { read_handle, write_handle }
	}

	/// Split the peer in a read handle and a write handle.
	///
	/// Splitting the peer allows you to move both handles into different tasks.
	pub fn split(self) -> (PeerReadHandle<Body>, PeerWriteHandle<Body>) {
		(self.read_handle, self.write_handle)
	}

	/// Get the next request or stream message from the remote peer.
	///
	/// Errors for invalid incoming messages are also reported by this function.
	/// For example: incoming update messages that are not associated with a received request will be reported as an error here.
	pub async fn next_message(&mut self) -> Result<Incoming<Body>, error::NextMessageError> {
		self.read_handle.next_message().await
	}

	/// Send a new request to the remote peer.
	pub async fn send_request(&mut self, service_id: i32, body: impl Into<Body>) -> Result<SentRequest<Body>, error::SendRequestError> {
		self.write_handle.send_request(service_id, body).await
	}

	/// Send a stream message to the remote peer.
	pub async fn send_stream(&mut self, service_id: i32, body: impl Into<Body>) -> Result<(), error::WriteMessageError> {
		self.write_handle.send_stream(service_id, body).await
	}

	/// Close the connection with the remote peer.
	///
	/// This is equivalent to just dropping the handle.
	pub fn close(self) {
		drop(self)
	}

	/// Make a close handle for the peer.
	///
	/// The close handle can be used to close the connection with the remote peer.
	/// It can be cloned and moved around independently.
	pub fn close_handle(&self) -> PeerCloseHandle<Body> {
		self.read_handle.close_handle()
	}
}

impl<Body> PeerReadHandle<Body> {
	/// Get the next request or stream message from the remote peer.
	///
	/// Errors for invalid incoming messages are also reported by this function.
	/// For example: incoming update messages that are not associated with a received request will be reported as an error here.
	pub async fn next_message(&mut self) -> Result<Incoming<Body>, error::NextMessageError> {
		self.incoming_rx.recv().await.ok_or_else(error::not_connected)?
	}

	/// Close the connection with the remote peer.
	///
	/// Dropping both the read and write handle will close the channel.
	/// This function can close the channel even if the other handle is still around.
	pub fn close(&self) {
		let _ = self.command_tx.send(Command::Stop);
	}

	/// Make a close handle for the peer.
	///
	/// The close handle can be used to close the connection with the remote peer.
	/// It can be cloned and moved around independently.
	pub fn close_handle(&self) -> PeerCloseHandle<Body> {
		PeerCloseHandle {
			command_tx: self.command_tx.clone(),
		}
	}
}

impl<Body> Drop for PeerReadHandle<Body> {
	fn drop(&mut self) {
		let _ = self.command_tx.send(Command::StopReadHalf);
	}
}

impl<Body> PeerWriteHandle<Body> {
	/// Send a new request to the remote peer.
	pub async fn send_request(&mut self, service_id: i32, body: impl Into<Body>) -> Result<SentRequest<Body>, error::SendRequestError> {
		let body = body.into();
		let (result_tx, result_rx) = oneshot::channel();
		self.command_tx.send(SendRequest { service_id, body, result_tx }.into())
			.map_err(|_| error::not_connected())?;

		result_rx.await.map_err(|_| error::not_connected())?
	}

	/// Send a stream message to the remote peer.
	pub async fn send_stream(&mut self, service_id: i32, body: impl Into<Body>) -> Result<(), error::WriteMessageError> {
		let body = body.into();
		let (result_tx, result_rx) = oneshot::channel();
		let message = Message::stream(0, service_id, body);
		self.command_tx.send(SendRawMessage { message, result_tx }.into())
			.map_err(|_| error::not_connected())?;

		result_rx.await.map_err(|_| error::not_connected())?
	}

	/// Close the connection with the remote peer.
	///
	/// Dropping both the read and write handle will close the channel.
	/// This function can close the channel even if the other handle is still around.
	pub fn close(&self) {
		let _ = self.command_tx.send(Command::Stop);
	}

	/// Make a close handle for the peer.
	///
	/// The close handle can be used to close the connection with the remote peer.
	/// It can be cloned and moved around independently.
	pub fn close_handle(&self) -> PeerCloseHandle<Body> {
		PeerCloseHandle {
			command_tx: self.command_tx.clone(),
		}
	}
}

impl<Body> Drop for PeerWriteHandle<Body> {
	fn drop(&mut self) {
		let _ = self.command_tx.send(Command::StopWriteHalf);
	}
}

impl<Body> PeerCloseHandle<Body> {
	/// Close the connection with the remote peer.
	pub fn close(&self) {
		let _ = self.command_tx.send(Command::Stop);
	}
}

pub struct SendRequest<Body> {
	pub service_id: i32,
	pub body: Body,
	pub result_tx: oneshot::Sender<Result<SentRequest<Body>, error::SendRequestError>>,
}

pub struct SendRawMessage<Body> {
	pub message: Message<Body>,
	pub result_tx: oneshot::Sender<Result<(), error::WriteMessageError>>,
}

pub struct ProcessIncomingMessage<Body> {
	pub message: Result<Message<Body>, error::ReadMessageError>,
}

impl<Body> From<SendRequest<Body>> for Command<Body> {
	fn from(other: SendRequest<Body>) -> Self {
		Self::SendRequest(other)
	}
}

impl<Body> From<SendRawMessage<Body>> for Command<Body> {
	fn from(other: SendRawMessage<Body>) -> Self {
		Self::SendRawMessage(other)
	}
}

impl<Body> From<ProcessIncomingMessage<Body>> for Command<Body> {
	fn from(other: ProcessIncomingMessage<Body>) -> Self {
		Self::ProcessIncomingMessage(other)
	}
}
