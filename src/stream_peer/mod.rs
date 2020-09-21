use byteorder::ByteOrder;
use byteorder::LE;
use futures::channel::mpsc;
use futures::io::AsyncRead;
use futures::io::AsyncReadExt;
use futures::io::AsyncWrite;
use futures::io::AsyncWriteExt;
use futures::pin_mut;
use futures::sink::SinkExt;
use futures::stream::StreamExt;
use futures::task::SpawnExt;

use crate::HEADER_LEN;
use crate::Incoming;
use crate::MAX_PAYLOAD_LEN;
use crate::Message;
use crate::MessageHeader;
use crate::MessageType;
use crate::Peer;
use crate::RequestTracker;
use crate::error;
use crate::peer::Command;

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

pub fn stream_peer<Executor, Socket>(
	executor: Executor,
	socket: Socket,
	config: StreamPeerConfig,
) -> Result<Peer<StreamBody>, futures::task::SpawnError>
where
	Executor: futures::task::Spawn,
	Socket: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
	let (incoming_tx, incoming_rx) = mpsc::unbounded();
	let (command_tx, command_rx) = mpsc::unbounded();
	let request_tracker = RequestTracker::new(command_tx.clone());

	executor.spawn(run_peer(socket, request_tracker, command_tx.clone(), command_rx, incoming_tx, config))?;

	Ok(Peer::new(incoming_rx, command_tx))
}

/// Run a peer loop on a socket.
async fn run_peer<Socket>(
	socket: Socket,
	request_tracker: RequestTracker<StreamBody>,
	command_tx: mpsc::UnboundedSender<Command<StreamBody>>,
	command_rx: mpsc::UnboundedReceiver<Command<StreamBody>>,
	incoming_tx: mpsc::UnboundedSender<Result<Incoming<StreamBody>, error::NextMessageError>>,
	config: StreamPeerConfig,
)
where
	Socket: AsyncRead + AsyncWrite + Unpin,
{
	let (read_half, write_half) = socket.split();

	let write_loop = command_loop(write_half, request_tracker, command_rx, incoming_tx, config.max_body_len_write);
	let read_loop = read_loop(read_half,command_tx, config.max_body_len_read);

	pin_mut!(write_loop);
	pin_mut!(read_loop);

	let ((), other) = futures::future::select(write_loop, read_loop)
		.await
		.factor_first();

	other.await;
}

async fn read_loop<R: AsyncRead + Unpin>(
	mut stream: R,
	mut command_rx: mpsc::UnboundedSender<Command<StreamBody>>,
	max_body_len: u32,
) {
	loop {
		let stream_broken;
		let message;
		match read_message(&mut stream, max_body_len).await {
			x @ Err(error::ReadMessageError::Io(_)) => {
				stream_broken = true;
				message = x;
			}
			x => {
				stream_broken = false;
				message = x;
			}
		}

		let send_result = command_rx.send(crate::peer::ProcessIncomingMessage { message }.into()).await;
		if send_result.is_err() || stream_broken {
			break;
		}
	}
}

async fn command_loop<W: AsyncWrite + Unpin>(
	mut stream: W,
	mut request_tracker: RequestTracker<StreamBody>,
	mut command_rx: mpsc::UnboundedReceiver<Command<StreamBody>>,
	mut incoming_tx: mpsc::UnboundedSender<Result<Incoming<StreamBody>, error::NextMessageError>>,
	max_body_len: u32,
) {
	loop {
		let command = command_rx.next()
			.await
			.expect("all command channels closed, but we keep one open ourselves");

		match command {
			Command::SendRequest(command) => {
				let request = match request_tracker.allocate_sent_request(command.service_id) {
					Ok(x) => x,
					Err(e) => {
						let _ = command.result_tx.send(Err(e.into()));
						continue;
					}
				};

				let request_id = request.request_id();

				let message = Message::request(request.request_id(), request.service_id(), command.body);
				if let Err(e) = write_message(&mut stream, &message.header, message.body.as_ref(), max_body_len).await {
					let stream_invalid = is_io_error(&e);
					let _ = command.result_tx.send(Err(e.into()));
					let _ = request_tracker.remove_sent_request(request_id);
					if stream_invalid {
						break;
					} else {
						continue;
					}
				}

				// If sending fails, the result_rx was dropped.
				// Then remove the request from the tracker.
				if command.result_tx.send(Ok(request)).is_err() {
					let _ = request_tracker.remove_sent_request(request_id);
				}
			}

			// TODO: replace SendRawMessage with specific command for different message types.
			// Then we can use that to remove the appropriate request from the tracker if result_tx is dropped.
			// Or just parse the message header to determine which request to remove.
			//
			// Actually, should we remove the request if result_tx is dropped?
			// Needs more thought.
			Command::SendRawMessage(command) => {
				if command.message.header.message_type.is_response() {
					let _ = request_tracker.remove_sent_request(command.message.header.request_id);
				}
				if let Err(e) = write_message(&mut stream, &command.message.header, command.message.body.as_ref(), max_body_len).await {
					let stream_invalid = is_io_error(&e);
					let _ = command.result_tx.send(Err(e.into()));
					if stream_invalid {
						break;
					} else {
						continue;
					}
				}

				let _ = command.result_tx.send(Ok(()));
			}

			Command::ProcessIncomingMessage(command) => {
				let message = match command.message {
					Ok(x) => x,
					Err(e) => match incoming_tx.send(Err(e.into())).await {
						Ok(()) => continue,
						Err(_) => break,
					},
				};

				let incoming = match request_tracker.process_incoming_message(message).await {
					Ok(x) => x,
					Err(e) => match incoming_tx.send(Err(e.into())).await {
						Ok(()) => continue,
						Err(_) => break,
					},
				};

				if let Some(incoming) = incoming {
					match incoming_tx.send(Ok(incoming)).await {
						Ok(()) => continue,
						Err(_) => break,
					}
				}
			}
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
pub async fn write_message<W: AsyncWrite + Unpin>(stream: &mut W, header: &MessageHeader, body: &[u8], max_body_len: u32) -> Result<(), error::WriteMessageError> {
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

	use async_std::os::unix::net::UnixStream;

	#[async_std::test]
	async fn test_raw_message() {
		let_assert!(Ok((mut peer_a, mut peer_b)) = UnixStream::pair());

		assert!(let Ok(()) = write_message(&mut peer_a, &MessageHeader::request(1, 10), b"Hello peer_b!", 1024).await);

		let_assert!(Ok(message) = read_message(&mut peer_b, 1024).await);
		assert!(message.header == MessageHeader::request(1, 10));
		assert!(message.body.as_ref() == b"Hello peer_b!");
	}
}
