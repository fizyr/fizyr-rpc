use tokio::sync::mpsc;
use tokio::sync::oneshot;

use crate::error::private::connection_aborted;
use crate::peer::{Command, SendRawMessage, SendRequest};
use crate::{Error, Message, ReceivedMessage, SentRequestHandle};

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
	incoming_rx: mpsc::UnboundedReceiver<Result<ReceivedMessage<Body>, Error>>,

	/// Channel for sending commands to the peer loop.
	///
	/// Used by [`ReceivedRequestHandle`][crate::ReceivedRequestHandle] for sending updates and the response,
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
	/// and copied into [`SentRequestHandle`] to send update messages.
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
		incoming_rx: mpsc::UnboundedReceiver<Result<ReceivedMessage<Body>, Error>>,
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

	/// Receive the next request or stream message from the remote peer.
	///
	/// Errors for invalid incoming messages are also reported by this function.
	/// For example: incoming update messages that are not associated with a received request will be reported as an error here.
	pub async fn recv_message(&mut self) -> Result<ReceivedMessage<Body>, Error> {
		self.read_handle.recv_message().await
	}

	/// Send a new request to the remote peer.
	pub async fn send_request(&self, service_id: i32, body: impl Into<Body>) -> Result<SentRequestHandle<Body>, Error> {
		self.write_handle.send_request(service_id, body).await
	}

	/// Send a stream message to the remote peer.
	pub async fn send_stream(&self, service_id: i32, body: impl Into<Body>) -> Result<(), Error> {
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
	/// Receive the next request or stream message from the remote peer.
	///
	/// Errors for invalid incoming messages are also reported by this function.
	/// For example: incoming update messages that are not associated with a received request will be reported as an error here.
	pub async fn recv_message(&mut self) -> Result<ReceivedMessage<Body>, Error> {
		self.incoming_rx.recv()
			.await
			.ok_or_else(connection_aborted)?
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
	pub async fn send_request(&self, service_id: i32, body: impl Into<Body>) -> Result<SentRequestHandle<Body>, Error> {
		let body = body.into();
		let (result_tx, result_rx) = oneshot::channel();
		self.command_tx
			.send(SendRequest { service_id, body, result_tx }.into())
			.map_err(|_| connection_aborted())?;

		result_rx.await.map_err(|_| connection_aborted())?
	}

	/// Send a stream message to the remote peer.
	pub async fn send_stream(&self, service_id: i32, body: impl Into<Body>) -> Result<(), Error> {
		let body = body.into();
		let (result_tx, result_rx) = oneshot::channel();
		let message = Message::stream(0, service_id, body);
		self.command_tx
			.send(SendRawMessage { message, result_tx }.into())
			.map_err(|_| connection_aborted())?;

		result_rx.await.map_err(|_| connection_aborted())?
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

	/// Check if this handle has the same underlying channel as `other`.
	pub fn same_peer(&self, other: &Self) -> bool {
		self.command_tx.same_channel(&other.command_tx)
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

impl<Body> std::fmt::Debug for PeerHandle<Body> {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		f.debug_struct(core::any::type_name::<Self>())
			.finish_non_exhaustive()
	}
}

impl<Body> std::fmt::Debug for PeerReadHandle<Body> {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		f.debug_struct(core::any::type_name::<Self>())
			.finish_non_exhaustive()
	}
}

impl<Body> std::fmt::Debug for PeerWriteHandle<Body> {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		f.debug_struct(core::any::type_name::<Self>())
			.finish_non_exhaustive()
	}
}

#[cfg(test)]
mod test {
	use fizyr_rpc::UnixSeqpacketTransport;

	use assert2::assert;
	use assert2::let_assert;
	use tokio_seqpacket::UnixSeqpacket;

	#[tokio::test]
	async fn test_same_peer() {
		let_assert!(Ok((peer_a, peer_b)) = UnixSeqpacket::pair());
		let transport_a = UnixSeqpacketTransport::new(peer_a, Default::default());
		let peer_handle = fizyr_rpc::UnixSeqpacketPeer::spawn(transport_a);

		let (_, write_handle_a) = peer_handle.split();
		let duplicate = write_handle_a.clone();
		assert!(write_handle_a.same_peer(&duplicate));

		let transport_b = UnixSeqpacketTransport::new(peer_b, Default::default());
		let peer_handle = fizyr_rpc::UnixSeqpacketPeer::spawn(transport_b);
		let (_, write_handle_b) = peer_handle.split();
		assert!(!write_handle_a.same_peer(&write_handle_b));
	}
}
