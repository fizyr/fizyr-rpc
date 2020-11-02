use std::future::Future;
use crate::transport::Transport;

/// Trait for connecting transports to a remote address.
pub trait Connect<'a, Address: 'a>: Sized + Transport {
	/// The type of the future returned by `Self::connect`.
	type Future: Future<Output = std::io::Result<Self>>;

	/// Create a new transport connected to a remote address.
	fn connect(address: Address, config: Self::Config) -> Self::Future;
}
