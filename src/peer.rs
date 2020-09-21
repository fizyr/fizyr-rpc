use futures::channel::mpsc;
use futures::channel::oneshot;
use futures::sink::SinkExt;
use futures::stream::FusedStream;
use futures::stream::StreamExt;

use crate::Incoming;
use crate::Message;
use crate::SentRequest;
use crate::error;

/// Handle to a peer.
pub struct Peer<Body> {
	write_half: PeerWriteHalf<Body>,
	read_half: PeerReadHalf<Body>,
}

/// The read half of a peer.
///
/// The read half can be used to receive incoming requests and stream messages.
pub struct PeerReadHalf<Body> {
	incoming_rx: mpsc::UnboundedReceiver<Result<Incoming<Body>, error::NextMessageError>>,
}

/// The write half of a peer.
///
/// The write half can be used to send request and stream messages.
pub struct PeerWriteHalf<Body> {
	command_tx: mpsc::UnboundedSender<Command<Body>>,
}

/// Internal message sent by the write half to the peer task.
pub enum Command<Body> {
	SendRequest(SendRequest<Body>),
	SendRawMessage(SendRawMessage<Body>),
	ProcessIncomingMessage(ProcessIncomingMessage<Body>),
}

impl<Body> Peer<Body> {
	pub(crate) fn new(
		incoming_rx: mpsc::UnboundedReceiver<Result<Incoming<Body>, error::NextMessageError>>,
		command_tx: mpsc::UnboundedSender<Command<Body>>,
	) -> Self {
		let write_half = PeerWriteHalf { command_tx };
		let read_half = PeerReadHalf { incoming_rx };
		Self { write_half, read_half }
	}

	/// Split the peer in a read half and a write half.
	///
	/// Splitting the peer allows you to move both halfs into different tasks.
	pub fn split(self) -> (PeerReadHalf<Body>, PeerWriteHalf<Body>) {
		(self.read_half, self.write_half)
	}

	/// Get the next request or stream message from the remote peer.
	///
	/// Errors for invalid incoming messages are also reported by this function.
	/// For example: incoming update messages that are not associated with a received request will be reported as an error here.
	pub async fn next_message(&mut self) -> Result<Incoming<Body>, error::NextMessageError> {
		self.read_half.next_message().await
	}

	/// Send a new request to the remote peer.
	pub async fn send_request(&mut self, service_id: i32, body: impl Into<Body>) -> Result<SentRequest<Body>, error::SendRequestError> {
		self.write_half.send_request(service_id, body).await
	}

	/// Send a stream message to the remote peer.
	pub async fn send_stream(&mut self, service_id: i32, body: impl Into<Body>) -> Result<(), error::WriteMessageError> {
		self.write_half.send_stream(service_id, body).await
	}
}

impl<Body> PeerReadHalf<Body> {
	/// Get the next request or stream message from the remote peer.
	///
	/// Errors for invalid incoming messages are also reported by this function.
	/// For example: incoming update messages that are not associated with a received request will be reported as an error here.
	pub async fn next_message(&mut self) -> Result<Incoming<Body>, error::NextMessageError> {
		if self.incoming_rx.is_terminated() {
			return Err(error::not_connected().into());
		}
		self.incoming_rx.next().await.ok_or_else(error::not_connected)?
	}
}

impl<Body> PeerWriteHalf<Body> {
	/// Send a new request to the remote peer.
	pub async fn send_request(&mut self, service_id: i32, body: impl Into<Body>) -> Result<SentRequest<Body>, error::SendRequestError> {
		let body = body.into();
		let (result_tx, result_rx) = oneshot::channel();
		self.command_tx.send(SendRequest { service_id, body, result_tx }.into())
			.await
			.map_err(|_| error::not_connected())?;

		result_rx.await.map_err(|_| error::not_connected())?
	}

	/// Send a stream message to the remote peer.
	pub async fn send_stream(&mut self, service_id: i32, body: impl Into<Body>) -> Result<(), error::WriteMessageError> {
		let body = body.into();
		let (result_tx, result_rx) = oneshot::channel();
		let message = Message::stream(0, service_id, body);
		self.command_tx.send(SendRawMessage { message, result_tx }.into())
			.await
			.map_err(|_| error::not_connected())?;

		result_rx.await.map_err(|_| error::not_connected())?
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
