use byteorder::ByteOrder;
use byteorder::LE;
use tokio::sync::mpsc;
use tokio::io::AsyncRead;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWrite;
use tokio::io::AsyncWriteExt;

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
use body::StreamBody;

#[derive(Debug, Copy, Clone)]
pub struct StreamPeerConfig {
	/// The maximum body size for incoming messages.
	///
	/// If a message arrives with a larger body size, an error is returned.
	/// For stream sockets, that also means the stream is unusable because there is unread data left in the stream.
	pub max_body_len_read: u32,

	/// The maximum body size for outgoing messages.
	///
	/// If a message is given for sending with a larget body than this size,
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

pub struct StreamPeer<Socket> {
	socket: Socket,
	request_tracker: RequestTracker<StreamBody>,
	command_tx: mpsc::UnboundedSender<Command<StreamBody>>,
	command_rx: mpsc::UnboundedReceiver<Command<StreamBody>>,
	incoming_tx: mpsc::UnboundedSender<Result<Incoming<StreamBody>, error::NextMessageError>>,
	config: StreamPeerConfig,
}

impl<Socket> StreamPeer<Socket>
where
	for<'a> &'a mut Socket: SplitAsyncReadWrite,
{
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
		};

		let handle = PeerHandle::new(incoming_rx, command_tx);

		(peer, handle)
	}

	/// Run a peer loop on a socket.
	pub async fn run(mut self) {
		let Self {
			socket,
			request_tracker,
			command_tx,
			command_rx,
			incoming_tx,
			config,
		} = &mut self;

		let (read_half, write_half) = socket.split();
		tokio::pin!(read_half);
		tokio::pin!(write_half);

		let mut read_loop = ReadLoop {
			read_half,
			command_tx,
			max_body_len: config.max_body_len_read
		};

		let mut command_loop = CommandLoop {
			write_half,
			request_tracker,
			command_rx,
			incoming_tx,
			max_body_len: config.max_body_len_write,
		};

		tokio::join!(
			read_loop.run(),
			command_loop.run(),
		);
	}
}

struct ReadLoop<'a, R> {
	read_half: R,
	command_tx: &'a mut mpsc::UnboundedSender<Command<StreamBody>>,
	max_body_len: u32,
}

impl<R: AsyncRead + Unpin> ReadLoop<'_, R> {
	async fn run(&mut self) {
		loop {
			// Read a message, and stop the read loop on erorrs.
			let message = read_message(&mut self.read_half, self.max_body_len).await;
			let stop = message.is_err();

			// But first send the error to the command loop so it can be delivered to the peer.
			if self.command_tx.send(crate::peer::ProcessIncomingMessage { message }.into()).is_err() {
				// If command_tx.send() fails, the command loop already stopped so we can just break.
				break;
			}

			if stop {
				let _  = self.command_tx.send(crate::peer::Command::Stop);
				break;
			}
		}
	}
}

struct CommandLoop<'a, W> {
	write_half: W,
	request_tracker: &'a mut RequestTracker<StreamBody>,
	command_rx: &'a mut mpsc::UnboundedReceiver<Command<StreamBody>>,
	incoming_tx: &'a mut mpsc::UnboundedSender<Result<Incoming<StreamBody>, error::NextMessageError>>,
	max_body_len: u32,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum LoopFlow {
	/// Keep the loop running.
	Continue,

	/// Stop the loop.
	Stop,
}

impl<W: AsyncWrite + Unpin> CommandLoop<'_, W> {
	async fn run(&mut self) {
		loop {
			let command = self.command_rx.recv()
				.await
				.expect("all command channels closed, but we keep one open ourselves");

			let flow = match command {
				Command::SendRequest(command) => self.send_request(command).await,
				Command::SendRawMessage(command) => self.send_raw_message(command).await,
				Command::ProcessIncomingMessage(command) => self.process_incoming_message(command).await,
				Command::Stop => LoopFlow::Stop,
			};

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
		if let Err(e) = write_message(&mut self.write_half, &message, self.max_body_len).await {
			let stream_invalid = is_io_error(&e);
			let _ = command.result_tx.send(Err(e.into()));
			let _ = self.request_tracker.remove_sent_request(request_id);
			if stream_invalid {
				return LoopFlow::Stop;
			} else {
				return LoopFlow::Continue;
			}
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

		if let Err(e) = write_message(&mut self.write_half, &command.message, self.max_body_len).await {
			let stream_invalid = is_io_error(&e);
			let _ = command.result_tx.send(Err(e.into()));
			if stream_invalid {
				return LoopFlow::Stop;
			} else {
				return LoopFlow::Continue;
			}
		}

		let _ = command.result_tx.send(Ok(()));
		LoopFlow::Continue
	}

	async fn process_incoming_message(&mut self, command: crate::peer::ProcessIncomingMessage<StreamBody>) -> LoopFlow {
		let message = match command.message {
			Ok(x) => x,
			Err(e) => match self.incoming_tx.send(Err(e.into())) {
				Ok(()) => return LoopFlow::Continue,
				Err(_) => return LoopFlow::Stop,
			},
		};

		let incoming = match self.request_tracker.process_incoming_message(message).await {
			Ok(x) => x,
			Err(e) => match self.incoming_tx.send(Err(e.into())) {
				Ok(()) => return LoopFlow::Continue,
				Err(_) => return LoopFlow::Stop,
			},
		};

		if let Some(incoming) = incoming {
			match self.incoming_tx.send(Ok(incoming)) {
				Ok(()) => LoopFlow::Continue,
				Err(_) => LoopFlow::Stop,
			}
		} else {
			LoopFlow::Continue
		}
	}
}

fn is_io_error(e: &error::WriteMessageError) -> bool {
	if let error::WriteMessageError::Io(_) = e {
		true
	} else {
		false
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
	async fn test_request_tracker() {
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
		let_assert!(Ok(update) = received_request.read_message().await);
		assert!(update.header == MessageHeader::requester_update(request_id, 3));
		assert!(update.body.as_ref() == &[4]);

		// Send an update from B and receive it on A.
		let_assert!(Ok(()) = received_request.send_update(5, &[6][..]).await);
		let_assert!(Ok(update) = sent_request.read_message().await);
		assert!(update.header == MessageHeader::responder_update(request_id, 5));
		assert!(update.body.as_ref() == &[6]);

		// Send the response from B and receive it on A.
		let_assert!(Ok(()) = received_request.send_response(7, &[8][..]).await);
		let_assert!(Ok(response) = sent_request.read_message().await);
		assert!(response.header == MessageHeader::response(request_id, 7));
		assert!(response.body.as_ref() == &[8]);

		drop(handle_a);
		drop(handle_b);
		drop(sent_request);

		// TODO: dropping the handles should stop the tasks.
		// Doesn't do that yet though.
		// task_a.await;
		// task_b.await;
	}
}
