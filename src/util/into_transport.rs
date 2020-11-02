use crate::transport::Transport;

/// Trait to allow generic creation of transports from a socket.
pub trait IntoTransport: Sized + Send {
	/// The body type for messages transferred over the transport.
	type Body: crate::Body;

	/// The configuration type of the transport.
	type Config: Clone + Send + Sync + 'static;

	/// The transport type.
	type Transport: Transport<Body = Self::Body> + Send + 'static;

	/// Create a transport from `self` and a configuration struct.
	fn into_transport(self, config: Self::Config) -> Self::Transport;

	/// Create a transport from `self` using the default configuration.
	fn into_default_transport(self) -> Self::Transport
	where
		Self::Config: Default,
	{
		self.into_transport(Self::Config::default())
	}
}
