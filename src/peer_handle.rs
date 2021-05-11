use tokio::sync::mpsc;
use tokio::sync::oneshot;

use crate::error;
use crate::peer::{Command, SendRawMessage, SendRequest};
use crate::Incoming;
use crate::Message;
use crate::SentRequest;

/// Handle to a peer.
///
/// The handle can be used to receive incoming requests and stream messages,
/// and to send requests and stream messages.
///
/// When the handle is dropped, the peer loop is stopped.
/// Any open requests will also be terminated.
pub struct PeerHandle<Body> {
	/// The read handle for receiving incoming requests and stream messages,
	read_handle: PeerReadHandle<Body>,

	/// The write handle for sending requests and stream messages.
	write_handle: PeerWriteHandle<Body>,
}

/// Handle to receive messages from a peer.
///
/// The read handle can be used to receive incoming requests and stream messages.
///
/// When all read and write handles are dropped, the peer loop is stopped.
/// Any open requests will also be terminated.
pub struct PeerReadHandle<Body> {
	/// Channel for incoming request and stream messages.
	incoming_rx: mpsc::UnboundedReceiver<Result<Incoming<Body>, error::RecvMessageError>>,

	/// Channel for sending commands to the peer loop.
	///
	/// Used by [`ReceivedRequest`][crate::ReceivedRequest] for sending updates and the response,
	/// and to notify the peer loop when the read handle is dropped.
	command_tx: mpsc::UnboundedSender<Command<Body>>,
}

/// Handle to send messages to a peer.
///
/// The write handle can be used to send requests and stream messages.
///
/// When all read and write handles are dropped, the peer loop is stopped.
/// Any open requests will also be terminated.
pub struct PeerWriteHandle<Body> {
	/// Channel for sending commands to the peer loop.
	///
	/// Use amongst others to send outoing requests and stream messages,
	/// and copied into [`SentRequest`] to send update messages.
	///
	/// Also used to register and unregister the cloned/dropped write handles with the peer.
	command_tx: mpsc::UnboundedSender<Command<Body>>,
}

/// Handle to close the connection with a peer.
///
/// The peer handle can be cloned and moved independent from the [`PeerReadHandle`] or [`PeerWriteHandle`] it was created from.
/// It does not keep the peer loop running if all other handle types are dropped.
#[derive(Clone)]
pub struct PeerCloseHandle<Body> {
	/// Channel for sending commands to the peer loop.
	///
	/// Used to stop the peer loop.
	command_tx: mpsc::UnboundedSender<Command<Body>>,
}

impl<Body> PeerHandle<Body> {
	/// Create a new peer handle from the separate channels.
	pub(crate) fn new(
		incoming_rx: mpsc::UnboundedReceiver<Result<Incoming<Body>, error::RecvMessageError>>,
		command_tx: mpsc::UnboundedSender<Command<Body>>,
	) -> Self {
		let read_handle = PeerReadHandle {
			incoming_rx,
			command_tx: command_tx.clone(),
		};
		let write_handle = PeerWriteHandle { command_tx };
		Self { read_handle, write_handle }
	}

	/// Split the peer in a read handle and a write handle.
	///
	/// Splitting the peer allows you to move both handles into different tasks.
	///
	/// The original handle is consumed, but the peer loop will keep going until all read and write handles are dropped.
	pub fn split(self) -> (PeerReadHandle<Body>, PeerWriteHandle<Body>) {
		(self.read_handle, self.write_handle)
	}

	/// Get the next request or stream message from the remote peer.
	///
	/// Errors for invalid incoming messages are also reported by this function.
	/// For example: incoming update messages that are not associated with a received request will be reported as an error here.
	pub async fn next_message(&mut self) -> Result<Incoming<Body>, error::RecvMessageError> {
		self.read_handle.next_message().await
	}

	/// Send a new request to the remote peer.
	pub async fn send_request(&self, service_id: i32, body: impl Into<Body>) -> Result<SentRequest<Body>, error::SendRequestError> {
		self.write_handle.send_request(service_id, body).await
	}

	/// Send a stream message to the remote peer.
	pub async fn send_stream(&self, service_id: i32, body: impl Into<Body>) -> Result<(), error::WriteMessageError> {
		self.write_handle.send_stream(service_id, body).await
	}

	/// Close the connection with the remote peer.
	pub fn close(self) {
		self.read_handle.close()
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
	pub async fn next_message(&mut self) -> Result<Incoming<Body>, error::RecvMessageError> {
		self.incoming_rx.recv().await.ok_or_else(error::connection_aborted)?
	}

	/// Close the connection with the remote peer.
	pub fn close(&self) {
		let _: Result<_, _> = self.command_tx.send(Command::Stop);
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
		let _: Result<_, _> = self.command_tx.send(Command::UnregisterReadHandle);
	}
}

impl<Body> PeerWriteHandle<Body> {
	/// Send a new request to the remote peer.
	pub async fn send_request(&self, service_id: i32, body: impl Into<Body>) -> Result<SentRequest<Body>, error::SendRequestError> {
		let body = body.into();
		let (result_tx, result_rx) = oneshot::channel();
		self.command_tx
			.send(SendRequest { service_id, body, result_tx }.into())
			.map_err(|_| error::connection_aborted())?;

		result_rx.await.map_err(|_| error::connection_aborted())?
	}

	/// Send a stream message to the remote peer.
	pub async fn send_stream(&self, service_id: i32, body: impl Into<Body>) -> Result<(), error::WriteMessageError> {
		let body = body.into();
		let (result_tx, result_rx) = oneshot::channel();
		let message = Message::stream(0, service_id, body);
		self.command_tx
			.send(SendRawMessage { message, result_tx }.into())
			.map_err(|_| error::connection_aborted())?;

		result_rx.await.map_err(|_| error::connection_aborted())?
	}

	/// Close the connection with the remote peer.
	pub fn close(&self) {
		let _: Result<_, _> = self.command_tx.send(Command::Stop);
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

impl<Body> Clone for PeerWriteHandle<Body> {
	fn clone(&self) -> Self {
		let command_tx = self.command_tx.clone();
		let _: Result<_, _> = command_tx.send(Command::RegisterWriteHandle);
		Self { command_tx }
	}
}

impl<Body> Drop for PeerWriteHandle<Body> {
	fn drop(&mut self) {
		let _: Result<_, _> = self.command_tx.send(Command::UnregisterWriteHandle);
	}
}

impl<Body> PeerCloseHandle<Body> {
	/// Close the connection with the remote peer.
	pub fn close(&self) {
		let _: Result<_, _> = self.command_tx.send(Command::Stop);
	}
}
