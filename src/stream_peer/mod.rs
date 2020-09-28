use byteorder::ByteOrder;
use byteorder::LE;
use tokio::sync::mpsc;
use tokio::io::AsyncRead;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWrite;
use tokio::io::AsyncWriteExt;
use crate::util::{select, Either};

use crate::HEADER_LEN;
use crate::Incoming;
use crate::MAX_PAYLOAD_LEN;
use crate::Message;
use crate::MessageHeader;
use crate::MessageType;
use crate::PeerHandle;
use crate::RequestTracker;
use crate::error;
use crate::peer::Command;
use crate::util::SplitAsyncReadWrite;

mod body;
pub use body::StreamBody;

mod server;
pub use server::StreamServer;

#[derive(Debug, Copy, Clone)]
pub struct StreamPeerConfig {
	/// The maximum body size for incoming messages.
	///
	/// If a message arrives with a larger body size, an error is returned.
	/// For stream sockets, that also means the stream is unusable because there is unread data left in the stream.
	pub max_body_len_read: u32,

	/// The maximum body size for outgoing messages.
	///
	/// If a message is given for sending with a larger body than this size,
	/// the message is discarded and an error is returned.
	/// Stream sockets remain usable since the message header will not be sent either.
	pub max_body_len_write: u32,
}

impl Default for StreamPeerConfig {
	fn default() -> Self {
		Self {
			max_body_len_read: 8 * 1024,
			max_body_len_write: 8 * 1024,
		}
	}
}

/// Implementation a peer for byte-stream sockets.
///
/// This struct represents is used to run the read/write loop of the peer.
/// To send or receive requests and stream messages,
/// you need to use the [`PeerHandle`] instead.
pub struct StreamPeer<Socket> {
	socket: Socket,
	request_tracker: RequestTracker<StreamBody>,
	command_tx: mpsc::UnboundedSender<Command<StreamBody>>,
	command_rx: mpsc::UnboundedReceiver<Command<StreamBody>>,
	incoming_tx: mpsc::UnboundedSender<Result<Incoming<StreamBody>, error::NextMessageError>>,
	config: StreamPeerConfig,
	write_handles: usize,
}

impl<Socket> StreamPeer<Socket>
where
	for<'a> &'a mut Socket: SplitAsyncReadWrite,
{
	/// Create a new peer and a handle to it.
	///
	/// The [`StreamPeer`] itself can be used to run the read/write loop.
	/// The returned [`PeerHandle`] can be used to send and receive requests and stream messages.
	///
	/// If [`Self::run()`] is not called (or aborted),
	/// then none of the functions of the [`PeerHandle`] will work.
	/// They will just wait forever.
	///
	/// You can also use [`Self::spawn()`] to run the read/write loop in a newly spawned task,
	/// and only get a [`PeerHandle`].
	pub fn new(
		socket: Socket,
		config: StreamPeerConfig,
	) -> (Self, PeerHandle<StreamBody>) {
		let (incoming_tx, incoming_rx) = mpsc::unbounded_channel();
		let (command_tx, command_rx) = mpsc::unbounded_channel();
		let request_tracker = RequestTracker::new(command_tx.clone());

		let peer = Self {
			socket,
			request_tracker,
			command_tx: command_tx.clone(),
			command_rx,
			incoming_tx,
			config,
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
	pub async fn spawn(socket: Socket, config: StreamPeerConfig) -> PeerHandle<StreamBody>
	where
		Socket: Send + 'static,
	{
		let (peer, handle) = Self::new(socket, config);
		tokio::spawn(peer.run());
		handle
	}

	/// Run the read/write loop.
	pub async fn run(mut self) {
		let Self {
			socket,
			request_tracker,
			command_tx,
			command_rx,
			incoming_tx,
			config,
			write_handles,
		} = &mut self;

		let (read_half, write_half) = socket.split();
		tokio::pin!(read_half);
		tokio::pin!(write_half);

		let mut read_loop = ReadLoop {
			read_half,
			command_tx: command_tx.clone(),
			max_body_len: config.max_body_len_read
		};

		let mut command_loop = CommandLoop {
			write_half,
			request_tracker,
			command_rx,
			incoming_tx,
			max_body_len: config.max_body_len_write,
			read_handle_dropped: false,
			write_handles,
		};

		let read_loop = read_loop.run();
		let command_loop = command_loop.run();

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

/// Implementation of the read loop for [`StreamPeer`].
struct ReadLoop<R> {
	/// The read half of the socket.
	read_half: R,

	/// The channel used to send command to the command loop.
	command_tx: mpsc::UnboundedSender<Command<StreamBody>>,

	/// The maximum body length for incoming messages.
	max_body_len: u32,
}

impl<R: AsyncRead + Unpin> ReadLoop<R> {
	/// Run the read loop.
	async fn run(&mut self) {
		loop {
			// Read a message, and stop the read loop on erorrs.
			let message = read_message(&mut self.read_half, self.max_body_len).await;
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

/// Implementation of the command loop for [`StreamPeer`].
struct CommandLoop<'a, W> {
	/// The write half of the socket.
	write_half: W,

	/// The request tracker.
	request_tracker: &'a mut RequestTracker<StreamBody>,

	/// The channel for incoming commands.
	command_rx: &'a mut mpsc::UnboundedReceiver<Command<StreamBody>>,

	/// The channel for sending incoming messages to the [`PeerHandle`].
	incoming_tx: &'a mut mpsc::UnboundedSender<Result<Incoming<StreamBody>, error::NextMessageError>>,

	/// The maximum body length for outgoing messages.
	max_body_len: u32,

	/// Flag to indicate if the peer read handle has already been stopped.
	read_handle_dropped: bool,

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

impl<W: AsyncWrite + Unpin> CommandLoop<'_, W> {
	/// Run the command loop.
	async fn run(&mut self) {
		loop {
			// Stop the command loop if both halves of the PeerHandle are dropped.
			if self.read_handle_dropped && *self.write_handles == 0 {
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
					self.read_handle_dropped = true;
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
	async fn send_request(&mut self, command: crate::peer::SendRequest<StreamBody>) -> LoopFlow {
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
	async fn send_raw_message(&mut self, command: crate::peer::SendRawMessage<StreamBody>) -> LoopFlow {
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
	async fn process_incoming_message(&mut self, command: crate::peer::ProcessIncomingMessage<StreamBody>) -> LoopFlow {
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
	async fn send_incoming(&mut self, incoming: Result<Incoming<StreamBody>, error::NextMessageError>) -> Result<(), ()> {
		if let Err(_) = self.incoming_tx.send(incoming) {
			self.read_handle_dropped = true;
			Err(())
		} else {
			Ok(())
		}
	}

	async fn write_message(&mut self, message: &Message<StreamBody>) -> Result<(), (error::WriteMessageError, LoopFlow)> {
		match write_message(&mut self.write_half, &message, self.max_body_len).await {
			Ok(()) => Ok(()),
			Err(e @ error::WriteMessageError::Io(_)) => Err((e, LoopFlow::Stop)),
			Err(e @ error::WriteMessageError::PayloadTooLarge(_)) => Err((e, LoopFlow::Continue)),
		}
	}
}

/// Read a message from an [`AsyncRead`] stream.
pub async fn read_message<R: AsyncRead + Unpin>(stream: &mut R, max_body_len: u32) -> Result<Message<StreamBody>, error::ReadMessageError> {
	// Read header.
	let mut buffer = [0u8; 16];
	stream.read_exact(&mut buffer).await?;

	// Parse header.
	let length = LE::read_u32(&buffer[0..]);
	let message_type = LE::read_u32(&buffer[4..]);
	let request_id = LE::read_u32(&buffer[8..]);
	let service_id = LE::read_i32(&buffer[12..]);

	let body_len = length - HEADER_LEN;
	error::PayloadTooLarge::check(body_len as usize, max_body_len)?;

	let message_type = MessageType::from_u32(message_type)?;
	let header = MessageHeader {
		message_type,
		request_id,
		service_id,
	};

	// TODO: Use Box::new_uninit_slice() when it hits stable.
	let mut buffer = vec![0u8; body_len as usize];
	stream.read_exact(&mut buffer).await?;
	Ok(Message::new(header, buffer.into()))
}

/// Write a message to an [`AsyncWrite`] stream.
pub async fn write_message<W: AsyncWrite + Unpin>(stream: &mut W, message: &Message<StreamBody>, max_body_len: u32) -> Result<(), error::WriteMessageError> {
	write_raw_message(stream, &message.header, message.body.as_ref(), max_body_len).await
}

/// Write a message to an [`AsyncWrite`] stream.
pub async fn write_raw_message<W: AsyncWrite + Unpin>(stream: &mut W, header: &MessageHeader, body: &[u8], max_body_len: u32) -> Result<(), error::WriteMessageError> {
	error::PayloadTooLarge::check(body.len(), max_body_len.min(MAX_PAYLOAD_LEN))?;

	let mut buffer = [0u8; 16];
	LE::write_u32(&mut buffer[0..], body.len() as u32 + HEADER_LEN);
	LE::write_u32(&mut buffer[4..], header.message_type as u32);
	LE::write_u32(&mut buffer[8..], header.request_id);
	LE::write_i32(&mut buffer[12..], header.service_id);

	// TODO: Use AsyncWriteExt::write_all_vectored once it hits stable.
	stream.write_all(&buffer).await?;
	stream.write_all(&body).await?;
	Ok(())
}

#[cfg(test)]
mod test {
	use super::*;
	use assert2::assert;
	use assert2::let_assert;

	use tokio::net::UnixStream;

	#[tokio::test]
	async fn test_raw_message() {
		let_assert!(Ok((mut peer_a, mut peer_b)) = UnixStream::pair());

		assert!(let Ok(()) = write_raw_message(&mut peer_a, &MessageHeader::request(1, 10), b"Hello peer_b!", 1024).await);

		let_assert!(Ok(message) = read_message(&mut peer_b, 1024).await);
		assert!(message.header == MessageHeader::request(1, 10));
		assert!(message.body.as_ref() == b"Hello peer_b!");
	}

	#[tokio::test]
	async fn test_peer() {
		let_assert!(Ok((peer_a, peer_b)) = UnixStream::pair());

		let (peer_a, mut handle_a) = StreamPeer::<UnixStream>::new(peer_a, Default::default());
		let (peer_b, mut handle_b) = StreamPeer::<UnixStream>::new(peer_b, Default::default());

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
