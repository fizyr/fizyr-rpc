//! Rust implementation of the Fizyr RPC procotol.
//!
//! The Fizyr RPC protocol is a request/response protocol,
//! with bi-directional feedback as long as a request is open.
//! Additionally, you can send individual stream messages that do not initiate a request.
//!
//! # Overview
//!
//! ## Peer and PeerHandle
//!
//! As a user of the library, you will mostly be using the [`PeerHandle`] object.
//! The [`PeerHandle`] is used to interact with a remote peer.
//! It is used to send and receive requests and stream messages.
//! It can also be split in a [`PeerReadHandle`] and a [`PeerWriteHandle`],
//! to allow moving the handles into different tasks.
//! The write handle can also be cloned and used in multiple tasks.
//!
//! To obtain a [`PeerHandle`], you can call [`Peer::connect()`].
//! This will connect to a remote listener and spawn a background task to read and write messages over the connection.
//! If you need full control over tasks, you can instead create a [`Peer`] object
//! and call [`Peer::run()`] manually.
//!
//! ## Listener
//!
//! The [`Listener`] struct is used to accept incoming connections
//! and gives you a [`PeerHandle`] for each incoming connection.
//! You can then use the handle to process incoming messages and to send messages to the peer.
//! Usually, you will want to spawn a task for each accepted connection that handles the communication.
//!
//! ## Transports
//!
//! Each peer internally uses a [`Transport`][transport::Transport].
//! The transport is responsible for reading and writing raw messages.
//! By abstracting away the message transport,
//! the library can expose a single generic [`Peer`] and [`Listener`] struct.
//!
//! There are different transports for different socket types.
//! Different transports may also use different types as message body.
//! For example, the [`TcpTransport`] and [`UnixStreamTransport`]
//! use messages with a [`StreamBody`].
//! This [`StreamBody`] body type contains raw bytes.
//!
//! The [`UnixSeqpacketTransport`] has messages with a [`UnixBody`],
//! which allows you to embed file descriptors with each message.
//!
//! # Features
//!
//! The library uses features to avoid unnecessarily large dependency trees.
//! Each feature corresponds to a different transport type.
//! None of the features are enabled by default.
//! Currently, the library has these features:
//!
//! * `tcp`: for the [`TcpTransport`]
//! * `unix-stream`: for the [`UnixStreamTransport`]
//! * `unix-seqpacket`: for the [`UnixSeqpacketTransport`]
//!
//! # Example
//!
//! ```no_run
//! use fizyr_rpc::{TcpPeer, StreamConfig};
//!
//! # async fn foo() -> Result<(), Box<dyn std::error::Error>> {
//! let mut peer = TcpPeer::connect("localhost:1337", StreamConfig::default()).await?;
//! let mut request = peer.send_request(1, &b"Hello World!"[..]).await?;
//!
//! while let Some(update) = request.recv_update().await {
//!     let body = std::str::from_utf8(&update.body)?;
//!     eprintln!("Received update: {}", body);
//! }
//!
//! let response = request.recv_response().await?;
//! let body = std::str::from_utf8(&response.body)?;
//! eprintln!("Received response: {}", body);
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]

/// Unwrap a [`Poll`](std::task::Poll) value and return from the enclosing function if it was [`Pending`](std::task::Poll::Pending).
///
/// This is like `try!()`, but for [`Poll`](std::task::Poll).
#[allow(unused_macros)]
macro_rules! ready {
	($e:expr) => {
		match $e {
			::core::task::Poll::Pending => return ::core::task::Poll::Pending,
			::core::task::Poll::Ready(x) => x,
		}
	};
}

#[cfg(feature = "macros")]
#[doc(hidden)]
pub mod macros;

#[cfg(feature = "macros")]
pub use macros::interface_example;

mod error;
mod listener;
mod message;
mod peer;
mod peer_handle;
mod request;
mod request_tracker;

pub mod introspection;
pub mod format;
pub mod transport;
pub mod util;

pub use error::{
	Error,
	ParseUpdateError,
	RecvMessageError,
};
pub use listener::{
	Listener,
	ListeningSocket,
};
pub use message::service_id;
pub use message::Body;
pub use message::Message;
pub use message::MessageHeader;
pub use message::MessageType;
pub use message::HEADER_LEN;
pub use message::MAX_PAYLOAD_LEN;
pub use peer::Peer;
pub use peer_handle::PeerHandle;
pub use peer_handle::PeerCloseHandle;
pub use peer_handle::PeerReadHandle;
pub use peer_handle::PeerWriteHandle;
pub use request::{
	ReceivedMessage,
	ReceivedRequestHandle,
	ReceivedRequestWriteHandle,
	SentRequestHandle,
	SentRequestWriteHandle,
};

pub use transport::stream::StreamBody;

pub use transport::stream::StreamConfig;

pub use transport::unix::UnixBody;

pub use transport::unix::UnixConfig;

/// Message transport for TCP.
#[cfg(feature = "tcp")]
pub type TcpTransport = transport::StreamTransport<tokio::net::TcpStream>;

/// Peer using the TCP transport.
#[cfg(feature = "tcp")]
pub type TcpPeer = Peer<TcpTransport>;

/// Listener for TCP sockets.
#[cfg(feature = "tcp")]
pub type TcpListener = Listener<tokio::net::TcpListener>;

/// Message transport for Unix stream sockets.
#[cfg(feature = "unix-stream")]
pub type UnixStreamTransport = transport::StreamTransport<tokio::net::UnixStream>;

/// Peer using the Unix stream transport.
#[cfg(feature = "unix-stream")]
pub type UnixStreamPeer = Peer<UnixStreamTransport>;

/// Listener for Unix stream sockets.
#[cfg(feature = "unix-stream")]
pub type UnixStreamListener = Listener<tokio::net::UnixListener>;

/// Message transport for Unix seqpacket sockets.
#[cfg(feature = "unix-seqpacket")]
pub type UnixSeqpacketTransport = transport::UnixTransport<tokio_seqpacket::UnixSeqpacket>;

/// Peer using the Unix seqpacket transport.
#[cfg(feature = "unix-seqpacket")]
pub type UnixSeqpacketPeer = Peer<UnixSeqpacketTransport>;

/// Listener for Unix seqpacket sockets.
#[cfg(feature = "unix-seqpacket")]
pub type UnixSeqpacketListener = Listener<tokio_seqpacket::UnixSeqpacketListener>;

#[doc(hidden)]
#[deprecated(note = "This type was renamed to ReceivedMessage. Please use that instead.", since = "0.5.0")]
pub type Incoming<Body> = ReceivedMessage<Body>;
