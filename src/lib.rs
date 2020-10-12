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
//! As a user of the library, you will mostly be using the [`PeerHandle`][PeerHandle] and [`Server`][Server] objects.
//! The `PeerHandle` is used to interact with a remote peer.
//! It is used to send and receive requests and stream messages.
//! It can also be split in a [`PeerReadHandle`][PeerReadHandle] and a [`PeerWriteHandle`][PeerWriteHandle],
//! to allow moving the handles into different tasks.
//! The write handle can also be cloned and used in multiple tasks.
//!
//! To obtain a `PeerHandle`, you must first create a [`Peer`][Peer] object.
//! The `Peer` object is responsible for reading and writing messages with the peer,
//! but you can't use it for sending or receiving messages directly.
//! Instead, you must ensure that the [`Peer::run()`][Peer::run] future is being polled.
//! The easiest way to do that is by calling [`Peer::spawn(...)`][Peer::spawn],
//! which will run the future in a background task and returns a `PeerHandle`.
//!
//! ## Server
//!
//! The [`Server`] struct is used to accept incoming connections and gives you a `PeerHandle` for each incoming connection.
//! You can then use the handle to process incoming messages and to send messages to the peer.
//! Usually, you will want to spawn a task for each accepted connection that handles the communication.
//!
//! ## Transports
//!
//! Each peer internally uses a [`Transport`][Transport].
//! The transport is responsible for reading and writing raw messages.
//! By abstracting away the message transport, the library can expose a single generic `Peer` and `Server` struct.
//!
//! There are different transports for different socket types.
//! Different transports may also use different types as message body.
//! For example, the [`TcpTransport`][TcpTransport] and [`UnixStreamTransport`][UnixStreamTransport]
//! use messages with a [`StreamBody`][StreamBody].
//! This `StreamBody` body type contains raw bytes.
//!
//! The [`UnixSeqpacketTransport`][UnixSeqpacketTransport] has messages with a [`UnixBody`][UnixBody],
//! which allows you to embed file descriptors with each message.

#![warn(missing_docs)]

#[macro_use]
mod macros;

mod message;
mod peer;
mod peer_handle;
mod request;
mod request_tracker;
mod server;
mod transport;
mod util;
pub mod error;

#[cfg(any(feature = "unix-stream", feature = "tcp"))]
mod stream;

#[cfg(feature = "unix-seqpacket")]
mod unix;

pub use message::Body;
pub use message::HEADER_LEN;
pub use message::MAX_PAYLOAD_LEN;
pub use message::Message;
pub use message::MessageHeader;
pub use message::MessageType;
pub use message::service_id;
pub use peer::Peer;
pub use peer_handle::PeerHandle;
pub use peer_handle::PeerReadHandle;
pub use peer_handle::PeerWriteHandle;
pub use request::Incoming;
pub use request::Outgoing;
pub use request::ReceivedRequest;
pub use request::SentRequest;
pub use request_tracker::RequestTracker;
pub use server::Server;
pub use server::ServerListener;
pub use transport::IntoTransport;
pub use transport::Transport;
pub use transport::TransportReadHalf;
pub use transport::TransportWriteHalf;

#[cfg(any(feature = "unix-stream", feature = "tcp"))]
pub use stream::StreamBody;
#[cfg(any(feature = "unix-stream", feature = "tcp"))]
pub use stream::StreamConfig;

#[cfg(any(feature = "unix-stream", feature = "tcp"))]
pub use stream::StreamTransport;

#[cfg(feature = "unix-seqpacket")]
pub use unix::UnixBody;

#[cfg(feature = "unix-seqpacket")]
pub use unix::UnixConfig;

#[cfg(feature = "unix-seqpacket")]
pub use unix::UnixTransport;

/// Message transport for TCP.
#[cfg(feature = "tcp")]
pub type TcpTransport = StreamTransport<tokio::net::TcpStream>;

/// Peer using the TCP transport.
#[cfg(feature = "tcp")]
pub type TcpPeer = Peer<StreamBody, TcpTransport>;

/// Message transport for Unix stream sockets.
#[cfg(feature = "unix-stream")]
pub type UnixStreamTransport = StreamTransport<tokio::net::UnixStream>;

/// Peer using the Unix stream transport.
#[cfg(feature = "unix-stream")]
pub type UnixStreamPeer = Peer<StreamBody, UnixStreamTransport>;

/// Message transport for Unix seqpacket sockets.
#[cfg(feature = "unix-seqpacket")]
pub type UnixSeqpacketTransport = UnixTransport<tokio_seqpacket::UnixSeqpacket>;

/// Peer using the Unix seqpacket transport.
#[cfg(feature = "unix-seqpacket")]
pub type UnixSeqpacketPeer = Peer<UnixBody, UnixSeqpacketTransport>;
