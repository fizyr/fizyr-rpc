use tokio::sync::{mpsc, oneshot};

use crate::{
	util,
	Error,
	Message,
	PeerHandle,
	ReceivedMessage,
	SentRequestHandle,
};
use crate::request_tracker::RequestTracker;
use crate::util::{select, Either};

/// Message for the internal peer command loop.
pub enum Command<Body> {
	SendRequest(SendRequest<Body>),
	SendRawMessage(SendRawMessage<Body>),
	ProcessReceivedMessage(ProcessReceivedMessage<Body>),
	Stop,
	UnregisterReadHandle,
	RegisterWriteHandle,
	UnregisterWriteHandle,
}

/// Peer read/write loop.
///
/// This struct is used to run the read/write loop of the peer.
/// To send or receive requests and stream messages,
/// you need to use the [`PeerHandle`] instead.
pub struct Peer<Transport: crate::transport::Transport> {
	/// The transport to use for sending/receiving messages.
	transport: Transport,

	/// The request tracker to track open requests.
	request_tracker: RequestTracker<Transport::Body>,

	/// Sending end of the command channel, so we can send commands to ourselves.
	///
	/// This is used to have the read loop inject things into the command loop.
	/// That way, the read loop doesn't need a mutable reference to the request tracker,
	/// which simplifies the implementation.
	command_tx: mpsc::UnboundedSender<Command<Transport::Body>>,

	/// Receiving end of the command channel.
	///
	/// Used to make the command loop do the things we want.
	command_rx: mpsc::UnboundedReceiver<Command<Transport::Body>>,

	/// Sending end of the channel for incoming requests and stream messages.
	incoming_tx: mpsc::UnboundedSender<Result<ReceivedMessage<Transport::Body>, Error>>,

	/// The number of [`PeerWriteHandle`][crate::PeerWriteHandle] objects for this peer.
	///
	/// When it hits zero, and the [`PeerReadHandle`][crate::PeerReadHandle] is dropped,
	/// the internal loops are stopped.
	write_handles: usize,
}

impl<Transport: crate::transport::Transport> Peer<Transport> {
	/// Create a new peer and a handle to it.
	///
	/// The [`Peer`] itself is used to run the read/write loop.
	/// The returned [`PeerHandle`] is used to send and receive requests and stream messages.
	///
	/// If [`Self::run()`] is not called (or aborted),
	/// none of the functions of the [`PeerHandle`] will work.
	/// They will just wait forever.
	///
	/// You can also use [`Self::spawn()`] to run the read/write loop in a newly spawned task,
	/// and only get a [`PeerHandle`].
	/// You should only use [`Self::spawn()`] if you do not need full control over the execution of the read/write loop.
	pub fn new(transport: Transport) -> (Self, PeerHandle<Transport::Body>) {
		let (incoming_tx, incoming_rx) = mpsc::unbounded_channel();
		let (command_tx, command_rx) = mpsc::unbounded_channel();
		let request_tracker = RequestTracker::new(command_tx.clone());

		let peer = Self {
			transport,
			request_tracker,
			command_tx: command_tx.clone(),
			command_rx,
			incoming_tx,
			write_handles: 1,
		};

		let handle = PeerHandle::new(incoming_rx, command_tx);

		(peer, handle)
	}

	/// Spawn a peer in a new task, and get a handle to the peer.
	///
	/// The spawned task will immediately be detached.
	/// It can not be joined.
	///
	/// The returned [`PeerHandle`] can be used to send and receive requests and stream messages.
	///
	/// If you need more control of the execution of the peer read/write loop,
	/// you should use [`Self::new()`] instead.
	pub fn spawn(transport: Transport) -> PeerHandle<Transport::Body> {
		let (peer, handle) = Self::new(transport);
		tokio::spawn(peer.run());
		handle
	}

	/// Connect to a remote server.
	///
	/// Similar to [`Self::spawn()`], this spawns a background task for the peer.
	///
	/// The returned [`PeerHandle`] can be used to send and receive requests and stream messages.
	///
	/// The type of address accepted depends on the transport.
	/// For internet transports such as TCP, the address must implement [`tokio::net::ToSocketAddrs`].
	/// For unix transports, the address must implement [`AsRef<std::path::Path>`].
	pub async fn connect<'a, Address>(address: Address, config: Transport::Config) -> std::io::Result<(PeerHandle<Transport::Body>, Transport::Info)>
	where
		Address: 'a,
		Transport: util::Connect<'a, Address>,
	{
		let transport = Transport::connect(address, config).await?;
		let info = transport.info()?;
		Ok((Self::spawn(transport), info))
	}

	/// Run the read/write loop.
	pub async fn run(mut self) {
		let Self {
			transport,
			request_tracker,
			command_tx,
			command_rx,
			incoming_tx,
			write_handles,
		} = &mut self;

		let (read_half, write_half) = transport.split();

		let mut read_loop = ReadLoop {
			read_half,
			command_tx: command_tx.clone(),
		};

		let mut command_loop = CommandLoop {
			write_half,
			request_tracker,
			command_rx,
			incoming_tx,
			read_handle_dropped: &mut false,
			write_handles,
		};

		let read_loop = read_loop.run();
		let command_loop = command_loop.run();

		// Futures must be pinned in order to poll them.
		tokio::pin!(read_loop);
		tokio::pin!(command_loop);

		match select(read_loop, command_loop).await {
			Either::Left(((), command_loop)) => {
				// If the read loop stopped we should still flush all queued incoming messages, then stop.
				command_tx
					.send(Command::Stop)
					.map_err(drop)
					.expect("command loop did not stop yet but command channel is closed");
				command_loop.await;
			},
			Either::Right((_read_loop, ())) => {
				// If the command loop stopped, the read loop is pointless.
				// Nobody will ever observe any effects of the read loop without the command loop.
				// The read loop is dropped here.
			},
		}
	}

	/// Get direct access to the underlying transport.
	pub fn transport(&self) -> &Transport {
		&self.transport
	}

	/// Get direct mutable access to the underlying transport.
	pub fn transport_mut(&mut self) -> &mut Transport {
		&mut self.transport
	}
}

/// Implementation of the read loop of a peer.
struct ReadLoop<R>
where
	R: crate::transport::TransportReadHalf,
{
	/// The read half of the message transport.
	read_half: R,

	/// The channel used to inject things into the peer read/write loop.
	command_tx: mpsc::UnboundedSender<Command<R::Body>>,
}

impl<R> ReadLoop<R>
where
	R: crate::transport::TransportReadHalf,
{
	/// Run the read loop.
	async fn run(&mut self) {
		loop {
			// Read a message, and stop the read loop on errors.
			let message = self.read_half.read_msg().await;
			let stop = matches!(&message, Err(e) if e.is_fatal());
			let message = message.map_err(|e| e.into_inner());

			// But first send the error to the command loop so it can be delivered to the peer.
			// If that fails the command loop already closed, so just stop the read loop.
			if self.command_tx.send(crate::peer::ProcessReceivedMessage { message }.into()).is_err() {
				break;
			}

			if stop {
				break;
			}
		}
	}
}

/// Implementation of the command loop of a peer.
struct CommandLoop<'a, W>
where
	W: crate::transport::TransportWriteHalf,
{
	/// The write half of the message transport.
	write_half: W,

	/// The request tracker.
	request_tracker: &'a mut RequestTracker<W::Body>,

	/// The channel for incoming commands.
	command_rx: &'a mut mpsc::UnboundedReceiver<Command<W::Body>>,

	/// The channel for sending incoming messages to the [`PeerHandle`].
	incoming_tx: &'a mut mpsc::UnboundedSender<Result<ReceivedMessage<W::Body>, Error>>,

	/// Flag to indicate if the peer read handle has already been stopped.
	read_handle_dropped: &'a mut bool,

	/// Number of open write handles.
	write_handles: &'a mut usize,
}

impl<W> CommandLoop<'_, W>
where
	W: crate::transport::TransportWriteHalf,
{
	/// Run the command loop.
	async fn run(&mut self) {
		loop {
			// Stop the command loop if both halves of the PeerHandle are dropped.
			if *self.read_handle_dropped && *self.write_handles == 0 {
				break;
			}

			// Get the next command from the channel.
			let command = self
				.command_rx
				.recv()
				.await
				.expect("all command channels closed, but we keep one open ourselves");

			// Process the command.
			let flow = match command {
				Command::SendRequest(command) => self.send_request(command).await,
				Command::SendRawMessage(command) => self.send_raw_message(command).await,
				Command::ProcessReceivedMessage(command) => self.process_incoming_message(command).await,
				Command::Stop => LoopFlow::Stop,
				Command::UnregisterReadHandle => {
					*self.read_handle_dropped = true;
					LoopFlow::Continue
				},
				Command::RegisterWriteHandle => {
					*self.write_handles += 1;
					LoopFlow::Continue
				},
				Command::UnregisterWriteHandle => {
					*self.write_handles -= 1;
					LoopFlow::Continue
				},
			};

			// Stop the loop if the command dictates it.
			match flow {
				LoopFlow::Stop => break,
				LoopFlow::Continue => continue,
			}
		}
	}

	/// Process a SendRequest command.
	async fn send_request(&mut self, command: crate::peer::SendRequest<W::Body>) -> LoopFlow {
		let request = match self.request_tracker.allocate_sent_request(command.service_id) {
			Ok(x) => x,
			Err(e) => {
				let _: Result<_, _> = command.result_tx.send(Err(e));
				return LoopFlow::Continue;
			},
		};

		let request_id = request.request_id();

		let message = Message::request(request.request_id(), request.service_id(), command.body);
		if let Err((e, flow)) = self.write_message(&message).await {
			let _: Result<_, _> = command.result_tx.send(Err(e));
			let _: Result<_, _> = self.request_tracker.remove_sent_request(request_id);
			return flow;
		}

		// If sending fails, the result_rx was dropped.
		// Then remove the request from the tracker.
		if command.result_tx.send(Ok(request)).is_err() {
			let _: Result<_, _> = self.request_tracker.remove_sent_request(request_id);
		}

		LoopFlow::Continue
	}

	/// Process a SendRawMessage command.
	async fn send_raw_message(&mut self, command: crate::peer::SendRawMessage<W::Body>) -> LoopFlow {
		// Remove tracked received requests when we send a response.
		if command.message.header.message_type.is_response() {
			let _: Result<_, _> = self.request_tracker.remove_received_request(command.message.header.request_id);
		}

		// TODO: replace SendRawMessage with specific command for different message types.
		// Then we can use that to remove the appropriate request from the tracker if result_tx is dropped.
		// Or just parse the message header to determine which request to remove.
		//
		// Actually, should we remove the request if result_tx is dropped?
		// Needs more thought.

		if let Err((e, flow)) = self.write_message(&command.message).await {
			let _: Result<_, _> = command.result_tx.send(Err(e));
			return flow;
		}

		let _: Result<_, _> = command.result_tx.send(Ok(()));
		LoopFlow::Continue
	}

	/// Process an incoming message.
	async fn process_incoming_message(&mut self, command: crate::peer::ProcessReceivedMessage<W::Body>) -> LoopFlow {
		// Forward errors to the peer read handle.
		let message = match command.message {
			Ok(x) => x,
			Err(e) => {
				let _: Result<_, _> = self.send_incoming(Err(e)).await;
				return LoopFlow::Continue;
			},
		};

		// Forward errors from the request tracker too.
		let incoming = match self.request_tracker.process_incoming_message(message).await {
			Ok(None) => return LoopFlow::Continue,
			Ok(Some(x)) => x,
			Err(e) => {
				let _: Result<_, _> = self.send_incoming(Err(e)).await;
				return LoopFlow::Continue;
			},
		};

		// Deliver the message to the peer read handle.
		match self.incoming_tx.send(Ok(incoming)) {
			Ok(()) => LoopFlow::Continue,

			// The read handle was dropped.
			// `msg` must be Ok(), because we checked it before.
			Err(mpsc::error::SendError(msg)) => match msg.unwrap() {
				// Respond to requests with an error.
				ReceivedMessage::Request(request, _body) => {
					let error_msg = format!("unexpected request for service {}", request.service_id());
					let response = Message::error_response(request.request_id(), &error_msg);
					if self.write_message(&response).await.is_err() {
						// If we can't send the error to the remote peer, just close the connection.
						// Even if the transport doesn't say that the write error is fatal.
						LoopFlow::Stop
					} else {
						LoopFlow::Continue
					}
				},
				ReceivedMessage::Stream(_) => LoopFlow::Continue,
			},
		}
	}

	/// Send an incoming message to the PeerHandle.
	async fn send_incoming(&mut self, incoming: Result<ReceivedMessage<W::Body>, Error>) -> Result<(), ()> {
		if self.incoming_tx.send(incoming).is_err() {
			*self.read_handle_dropped = true;
			Err(())
		} else {
			Ok(())
		}
	}

	async fn write_message(&mut self, message: &Message<W::Body>) -> Result<(), (Error, LoopFlow)> {
		match self.write_half.write_msg(&message.header, &message.body).await {
			Ok(()) => Ok(()),
			Err(e) => {
				let flow = if e.is_fatal() {
					LoopFlow::Stop
				} else {
					LoopFlow::Continue
				};
				Err((e.into_inner(), flow))
			},
		}
	}
}

/// Loop control flow command.
///
/// Allows other methods to make decisions on loop control flow.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum LoopFlow {
	/// Keep the loop running.
	Continue,

	/// Stop the loop.
	Stop,
}

/// Command to send a request to the remote peer.
pub struct SendRequest<Body> {
	/// The service ID for the request.
	pub service_id: i32,

	/// The body for the request.
	pub body: Body,

	/// One-shot channel to transmit back the created [`SentRequestHandle`] object, or an error.
	pub result_tx: oneshot::Sender<Result<SentRequestHandle<Body>, Error>>,
}

/// Command to send a raw message to the remote peer.
pub struct SendRawMessage<Body> {
	/// The message to send.
	pub message: Message<Body>,

	/// One-shot channel to receive the result of sending the message.
	pub result_tx: oneshot::Sender<Result<(), Error>>,
}

/// Command to process an incoming message from the remote peer.
pub struct ProcessReceivedMessage<Body> {
	/// The message from the remote peer, or an error.
	pub message: Result<Message<Body>, Error>,
}

impl<Body> std::fmt::Debug for Command<Body> {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		let mut debug = f.debug_struct("Command");
		match self {
			Self::SendRequest(x) => debug.field("SendRequest", x),
			Self::SendRawMessage(x) => debug.field("SendRawMessage", x),
			Self::ProcessReceivedMessage(x) => debug.field("ProcessReceivedMessage", x),
			Self::Stop => debug.field("Stop", &()),
			Self::UnregisterReadHandle => debug.field("UnregisterReadHandle", &()),
			Self::RegisterWriteHandle => debug.field("RegisterWriteHandle", &()),
			Self::UnregisterWriteHandle => debug.field("UnregisterWriteHandle", &()),

		}.finish()
	}
}

impl<Body> std::fmt::Debug for SendRequest<Body> {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		f.debug_struct("SendRequest").field("service_id", &self.service_id).finish()
	}
}

impl<Body> std::fmt::Debug for SendRawMessage<Body> {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		f.debug_struct("SendRawMessage").field("message", &self.message).finish()
	}
}

impl<Body> std::fmt::Debug for ProcessReceivedMessage<Body> {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		f.debug_struct("ProcessReceivedMessage").field("message", &self.message).finish()
	}
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

impl<Body> From<ProcessReceivedMessage<Body>> for Command<Body> {
	fn from(other: ProcessReceivedMessage<Body>) -> Self {
		Self::ProcessReceivedMessage(other)
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use assert2::assert;
	use assert2::let_assert;

	use crate::MessageHeader;
	use crate::transport::StreamTransport;
	use tokio::net::UnixStream;

	#[tokio::test]
	async fn test_peer() {
		let_assert!(Ok((peer_a, peer_b)) = UnixStream::pair());

		let (peer_a, handle_a) = Peer::new(StreamTransport::new(peer_a, Default::default()));
		let (peer_b, mut handle_b) = Peer::new(StreamTransport::new(peer_b, Default::default()));

		let task_a = tokio::spawn(peer_a.run());
		let task_b = tokio::spawn(peer_b.run());

		// Send a request from A.
		let_assert!(Ok(mut sent_request) = handle_a.send_request(1, &[2][..]).await);
		let request_id = sent_request.request_id();

		// Receive the request on B.
		let_assert!(Ok(ReceivedMessage::Request(mut received_request, _body)) = handle_b.recv_message().await);

		// Send an update from A and receive it on B.
		let_assert!(Ok(()) = sent_request.send_update(3, &[4][..]).await);
		let_assert!(Some(update) = received_request.recv_update().await);
		assert!(update.header == MessageHeader::requester_update(request_id, 3));
		assert!(update.body.as_ref() == &[4]);

		// Send an update from B and receive it on A.
		let_assert!(Ok(()) = received_request.send_update(5, &[6][..]).await);
		let_assert!(Some(update) = sent_request.recv_update().await);
		assert!(update.header == MessageHeader::responder_update(request_id, 5));
		assert!(update.body.as_ref() == &[6]);

		// Send the response from B and receive it on A.
		let_assert!(Ok(()) = received_request.send_response(7, &[8][..]).await);
		let_assert!(Ok(response) = sent_request.recv_response().await);
		assert!(response.header == MessageHeader::response(request_id, 7));
		assert!(response.body.as_ref() == &[8]);

		drop(handle_a);
		drop(handle_b);
		drop(sent_request);

		assert!(let Ok(()) = task_a.await);
		assert!(let Ok(()) = task_b.await);
	}

	#[tokio::test]
	async fn peeked_response_is_not_gone() {
		let_assert!(Ok((peer_a, peer_b)) = UnixStream::pair());
		let handle_a = Peer::spawn(StreamTransport::new(peer_a, Default::default()));
		let mut handle_b = Peer::spawn(StreamTransport::new(peer_b, Default::default()));

		// Send a request from A.
		let_assert!(Ok(mut sent_request) = handle_a.send_request(1, &[2][..]).await);
		let request_id = sent_request.request_id();

		// Receive the request on B.
		let_assert!(Ok(ReceivedMessage::Request(received_request, _body)) = handle_b.recv_message().await);

		// Send two updates and a response from B to A.
		let_assert!(Ok(()) = received_request.send_update(5, &b"Hello world!"[..]).await);
		let_assert!(Ok(()) = received_request.send_update(6, &b"Hello world!"[..]).await);
		let_assert!(Ok(()) = received_request.send_response(7, &b"Goodbye!"[..]).await);

		// Try to receive three responses.
		// This should stuff the response in the internal peek buffer.
		assert!(let Some(_) = sent_request.recv_update().await);
		assert!(let Some(_) = sent_request.recv_update().await);
		assert!(let None = sent_request.recv_update().await);

		// Now receive the response, which should be returned intact from the peek buffer exactly once.
		let_assert!(Ok(response) = sent_request.recv_response().await);
		assert!(let Err(_) = sent_request.recv_response().await);

		assert!(response.header == MessageHeader::response(request_id, 7));
		assert!(response.body.as_ref() == b"Goodbye!");
	}

	#[tokio::test]
	async fn peeked_update_is_not_gone() {
		let_assert!(Ok((peer_a, peer_b)) = UnixStream::pair());
		let handle_a = Peer::spawn(StreamTransport::new(peer_a, Default::default()));
		let mut handle_b = Peer::spawn(StreamTransport::new(peer_b, Default::default()));

		// Send a request from A.
		let_assert!(Ok(mut sent_request) = handle_a.send_request(1, &[2][..]).await);
		let request_id = sent_request.request_id();

		// Receive the request on B.
		let_assert!(Ok(ReceivedMessage::Request(received_request, _body)) = handle_b.recv_message().await);

		// Send one update and a response from B to A.
		let_assert!(Ok(()) = received_request.send_update(5, &b"Hello world!"[..]).await);
		let_assert!(Ok(()) = received_request.send_response(6, &b"Goodbye!"[..]).await);

		// Trying to read a response should stuff the update in the internal peek buffer.
		assert!(let Err(_) = sent_request.recv_response().await);

		// Now we should receive the update intact from the peek buffer exactly once.
		let_assert!(Some(update) = sent_request.recv_update().await);
		assert!(update.header == MessageHeader::responder_update(request_id, 5));
		assert!(update.body.as_ref() == b"Hello world!");
		assert!(let None = sent_request.recv_update().await);

		// Now receive the response.
		let_assert!(Ok(response) = sent_request.recv_response().await);
		assert!(response.header == MessageHeader::response(request_id, 6));
		assert!(response.body.as_ref() == b"Goodbye!");
	}
}
