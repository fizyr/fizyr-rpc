use tokio::sync::mpsc;
use crate::util::{select, Either};

use tokio::sync::oneshot;
use crate::Incoming;
use crate::Message;
use crate::PeerHandle;
use crate::RequestTracker;
use crate::error;
use crate::SentRequest;

/// Message for the internal peer command loop.
pub enum Command<Body> {
	SendRequest(SendRequest<Body>),
	SendRawMessage(SendRawMessage<Body>),
	ProcessIncomingMessage(ProcessIncomingMessage<Body>),
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
pub struct Peer<Body, Transport> {
	transport: Transport,
	request_tracker: RequestTracker<Body>,
	command_tx: mpsc::UnboundedSender<Command<Body>>,
	command_rx: mpsc::UnboundedReceiver<Command<Body>>,
	incoming_tx: mpsc::UnboundedSender<Result<Incoming<Body>, error::NextMessageError>>,
	write_handles: usize,
}

impl<Body, Transport> Peer<Body, Transport>
where
	Body: crate::Body + Send + Sync + 'static,
	Transport: 'static,
	for<'a> &'a mut Transport: crate::Transport<Body = Body>,
{
	/// Create a new peer and a handle to it.
	///
	/// The [`Peer`] itself is used to run the read/write loop.
	/// The returned [`PeerHandle`] is used to send and receive requests and stream messages.
	///
	/// If [`Self::run()`] is not called (or aborted),
	/// then none of the functions of the [`PeerHandle`] will work.
	/// They will just wait forever.
	///
	/// You can also use [`Self::spawn()`] to run the read/write loop in a newly spawned task,
	/// and only get a [`PeerHandle`].
	pub fn new(transport: Transport) -> (Self, PeerHandle<Body>) {
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
	/// The spawned handle will immediately be detached.
	/// It can not be joined.
	///
	/// The returned [`PeerHandle`] can be used to send and receive requests and stream messages.
	///
	/// If you need more control of the execution of the peer read/write loop,
	/// you should use [`Self::new()`] instead.
	pub fn spawn(transport: Transport) -> PeerHandle<Body>
	where
		Transport: Send + 'static,
	{
		let (peer, handle) = Self::new(transport);
		tokio::spawn(peer.run());
		handle
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

		use crate::Transport;
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
				command_tx.send(Command::Stop).map_err(drop).expect("command loop did not stop yet but command channel is closed");
				command_loop.await;
			},
			Either::Right((read_loop, ())) => {
				// If the command loop stopped, the read loop is pointless.
				// Nobody will ever observe any effects of the read loop without the command loop.
				drop(read_loop);
			},
		}
	}
}

/// Implementation of the read loop of a peer.
struct ReadLoop<Body, R> {
	/// The read half of the socket.
	read_half: R,

	/// The channel used to inject things into the peer read/write loop.
	command_tx: mpsc::UnboundedSender<Command<Body>>,
}

impl<Body, R> ReadLoop<Body, R>
where
	Body: crate::Body,
	R: crate::TransportReadHalf<Body = Body> + Unpin,
{
	/// Run the read loop.
	async fn run(&mut self) {
		loop {
			// Read a message, and stop the read loop on erorrs.
			let message = self.read_half.read_msg().await;
			let stop = message.is_err();

			// But first send the error to the command loop so it can be delivered to the peer.
			// If that fails the command loop already closed, so just stop the read loop.
			if self.command_tx.send(crate::peer::ProcessIncomingMessage { message }.into()).is_err() {
				break;
			}

			if stop {
				break;
			}
		}
	}
}

/// Implementation of the command loop of a peer.
struct CommandLoop<'a, Body, W> {
	/// The write half of the socket.
	write_half: W,

	/// The request tracker.
	request_tracker: &'a mut RequestTracker<Body>,

	/// The channel for incoming commands.
	command_rx: &'a mut mpsc::UnboundedReceiver<Command<Body>>,

	/// The channel for sending incoming messages to the [`PeerHandle`].
	incoming_tx: &'a mut mpsc::UnboundedSender<Result<Incoming<Body>, error::NextMessageError>>,

	/// Flag to indicate if the peer read handle has already been stopped.
	read_handle_dropped: &'a mut bool,

	/// Number of open write handles.
	write_handles: &'a mut usize,
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

impl<Body, W> CommandLoop<'_, Body, W>
where
	Body: crate::Body,
	W: crate::TransportWriteHalf<Body = Body> + Unpin,
{
	/// Run the command loop.
	async fn run(&mut self) {
		loop {
			// Stop the command loop if both halves of the PeerHandle are dropped.
			if *self.read_handle_dropped && *self.write_handles == 0 {
				break;
			}

			// Get the next command from the channel.
			let command = self.command_rx.recv()
				.await
				.expect("all command channels closed, but we keep one open ourselves");

			// Process the command.
			let flow = match command {
				Command::SendRequest(command) => self.send_request(command).await,
				Command::SendRawMessage(command) => self.send_raw_message(command).await,
				Command::ProcessIncomingMessage(command) => self.process_incoming_message(command).await,
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
	async fn send_request(&mut self, command: crate::peer::SendRequest<Body>) -> LoopFlow {
		let request = match self.request_tracker.allocate_sent_request(command.service_id) {
			Ok(x) => x,
			Err(e) => {
				let _ = command.result_tx.send(Err(e.into()));
				return LoopFlow::Continue;
			}
		};

		let request_id = request.request_id();

		let message = Message::request(request.request_id(), request.service_id(), command.body);
		if let Err((e, flow)) = self.write_message(&message).await {
			let _ = command.result_tx.send(Err(e.into()));
			let _ = self.request_tracker.remove_sent_request(request_id);
			return flow;
		}

		// If sending fails, the result_rx was dropped.
		// Then remove the request from the tracker.
		if command.result_tx.send(Ok(request)).is_err() {
			let _ = self.request_tracker.remove_sent_request(request_id);
		}

		LoopFlow::Continue
	}

	/// Process a SendRawMessage command.
	async fn send_raw_message(&mut self, command: crate::peer::SendRawMessage<Body>) -> LoopFlow {
		// Remove tracked received requests when we send a response.
		if command.message.header.message_type.is_response() {
			let _ = self.request_tracker.remove_sent_request(command.message.header.request_id);
		}

		// TODO: replace SendRawMessage with specific command for different message types.
		// Then we can use that to remove the appropriate request from the tracker if result_tx is dropped.
		// Or just parse the message header to determine which request to remove.
		//
		// Actually, should we remove the request if result_tx is dropped?
		// Needs more thought.

		if let Err((e, flow)) = self.write_message(&command.message).await {
			let _ = command.result_tx.send(Err(e.into()));
			return flow;
		}

		let _ = command.result_tx.send(Ok(()));
		LoopFlow::Continue
	}

	/// Process an incoming message.
	async fn process_incoming_message(&mut self, command: crate::peer::ProcessIncomingMessage<Body>) -> LoopFlow {
		// Forward errors to the peer read handle.
		let message = match command.message {
			Ok(x) => x,
			Err(e) => {
				let _ = self.send_incoming(Err(e.into()));
				return LoopFlow::Continue;
			},
		};

		// Forward errors from the request tracker too.
		let incoming = match self.request_tracker.process_incoming_message(message).await {
			Ok(None) => return LoopFlow::Continue,
			Ok(Some(x)) => x,
			Err(e) => {
				let _ = self.send_incoming(Err(e.into()));
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
				Incoming::Request(request) => {
					let error_msg = format!("unexpected request for service {}", request.service_id());
					let response = Message::error_response(request.request_id(), &error_msg);
					if self.write_message(&response).await.is_err() {
						LoopFlow::Stop
					} else {
						LoopFlow::Continue
					}
				},
				Incoming::Stream(_) => LoopFlow::Continue,
			}
		}
	}

	/// Send an incoming message to the PeerHandle.
	async fn send_incoming(&mut self, incoming: Result<Incoming<Body>, error::NextMessageError>) -> Result<(), ()> {
		if let Err(_) = self.incoming_tx.send(incoming) {
			*self.read_handle_dropped = true;
			Err(())
		} else {
			Ok(())
		}
	}

	async fn write_message(&mut self, message: &Message<Body>) -> Result<(), (error::WriteMessageError, LoopFlow)> {
		match self.write_half.write_msg(&message.header, &message.body).await {
			Ok(()) => Ok(()),
			Err(e @ error::WriteMessageError::Io(_)) => Err((e, LoopFlow::Stop)),
			Err(e @ error::WriteMessageError::PayloadTooLarge(_)) => Err((e, LoopFlow::Continue)),
		}
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

impl<Body> std::fmt::Debug for Command<Body> {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		let mut debug = f.debug_struct("Command");
		match self {
			Self::SendRequest(x) => debug.field("SendRequest", x),
			Self::SendRawMessage(x) => debug.field("SendRawMessage", x),
			Self::ProcessIncomingMessage(x) => debug.field("ProcessIncomingMessage", x),
			Self::Stop => debug.field("Stop", &()),
			Self::UnregisterReadHandle => debug.field("UnregisterReadHandle", &()),
			Self::RegisterWriteHandle => debug.field("RegisterWriteHandle", &()),
			Self::UnregisterWriteHandle => debug.field("UnregisterWriteHandle", &()),

		}.finish()
	}
}

impl<Body> std::fmt::Debug for SendRequest<Body> {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		f.debug_struct("SendRequest")
			.field("service_id", &self.service_id)
			.finish()
	}
}

impl<Body> std::fmt::Debug for SendRawMessage<Body> {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		f.debug_struct("SendRawMessage")
			.field("message", &self.message)
			.finish()
	}
}

impl<Body> std::fmt::Debug for ProcessIncomingMessage<Body> {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		f.debug_struct("ProcessIncomingMessage")
			.field("message", &self.message)
			.finish()
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

impl<Body> From<ProcessIncomingMessage<Body>> for Command<Body> {
	fn from(other: ProcessIncomingMessage<Body>) -> Self {
		Self::ProcessIncomingMessage(other)
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use assert2::assert;
	use assert2::let_assert;

	use tokio::net::UnixStream;
	use crate::{MessageHeader, StreamTransport};

	#[tokio::test]
	async fn test_peer() {
		let_assert!(Ok((peer_a, peer_b)) = UnixStream::pair());

		let (peer_a, mut handle_a) = Peer::new(StreamTransport::new(peer_a, Default::default()));
		let (peer_b, mut handle_b) = Peer::new(StreamTransport::new(peer_b, Default::default()));

		let task_a = tokio::spawn(peer_a.run());
		let task_b = tokio::spawn(peer_b.run());

		// Send a request from A.
		let_assert!(Ok(mut sent_request) = handle_a.send_request(1, &[2][..]).await);
		let request_id = sent_request.request_id();

		// Receive the request on B.
		let_assert!(Ok(Incoming::Request(mut received_request)) = handle_b.next_message().await);

		// Send an update from A and receive it on B.
		let_assert!(Ok(()) = sent_request.send_update(3, &[4][..]).await);
		let_assert!(Ok(update) = received_request.next_message().await);
		assert!(update.header == MessageHeader::requester_update(request_id, 3));
		assert!(update.body.as_ref() == &[4]);

		// Send an update from B and receive it on A.
		let_assert!(Ok(()) = received_request.send_update(5, &[6][..]).await);
		let_assert!(Ok(update) = sent_request.next_message().await);
		assert!(update.header == MessageHeader::responder_update(request_id, 5));
		assert!(update.body.as_ref() == &[6]);

		// Send the response from B and receive it on A.
		let_assert!(Ok(()) = received_request.send_response(7, &[8][..]).await);
		let_assert!(Ok(response) = sent_request.next_message().await);
		assert!(response.header == MessageHeader::response(request_id, 7));
		assert!(response.body.as_ref() == &[8]);

		drop(handle_a);
		drop(handle_b);
		drop(sent_request);

		assert!(let Ok(()) = task_a.await);
		assert!(let Ok(()) = task_b.await);
	}
}
