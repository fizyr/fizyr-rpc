//! Rust implementation of the Fizyr RPC procotol.

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
