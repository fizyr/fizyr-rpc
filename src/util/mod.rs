//! Utility traits.
//!
//! The traits in this module are used to implement utility functions on [`Peer`][crate::Peer] and [`Listener`][crate::Listener].
//! You do not normally need to use these traits directly.
//!
//! However, if you wish to implement a custom transport,
//! you may also wish to implement these traits.

mod accept;
mod connect;
mod into_transport;
mod select;

pub use accept::{Accept, Bind, Listener};
pub use connect::Connect;
pub use into_transport::IntoTransport;

// `select` is not a trait, but it's not exported publicly.
// So the module documentation is still fine.
pub(crate) use select::{select, Either};
