use super::Peer;
use crate::IntoTransport;
use crate::PeerHandle;

/// Server that spawns peers for all accepted connections.
pub struct Server<Listener>
where
	Listener: ServerListener,
{
	listener: Listener,
	config: Listener::Config,
}

/// Helper trait for [`Server`].
///
/// This trait encapsulates all requirements for the `Listener` type of a [`Server`].
///
/// The trait has a blanket implementation,
/// so you can not implement it for your own types.
///
/// You *can* use it as trait bound for generic arguments,
/// but you should not rely on any of the items in this trait.
pub trait ServerListener: crate::util::Listener + Unpin {
	#[doc(hidden)]
	type Body: crate::Body;

	#[doc(hidden)]
	type Config: Clone + Send + Sync + 'static;

	#[doc(hidden)]
	type Transport: Send + 'static;

	#[doc(hidden)]
	fn spawn(connection: Self::Connection, config: Self::Config) -> PeerHandle<Self::Body>;
}

impl<Listener> ServerListener for Listener
where
	Listener: crate::util::Listener + Unpin,
	Listener::Connection: IntoTransport,
{
	type Body = <Listener::Connection as IntoTransport>::Body;
	type Config = <Listener::Connection as IntoTransport>::Config;
	type Transport = <Listener::Connection as IntoTransport>::Transport;

	fn spawn(connection: Self::Connection, config: Self::Config) -> PeerHandle<Self::Body> {
		Peer::spawn(connection.into_transport(config))
	}
}

impl<Listener: ServerListener> Server<Listener> {
	/// Create a server on a listening socket.
	///
	/// The passed in config is used to create transports and peers for all accepted connections.
	pub fn new(listener: Listener, config: Listener::Config) -> Self {
		Self { listener, config }
	}

	/// Create a server with a new listening socket bound to the given address.
	///
	/// The type of address accepted depends on the listener.
	/// For internet transports such as TCP, the address must implement [`tokio::net::ToSocketAddrs`].
	/// For unix transports, the address must implement [`AsRef<std::path::Path>`].
	///
	/// This function is asynchronous because it may perform a DNS lookup for some address types.
	pub async fn bind<'a, Address: 'a>(address: Address, config: Listener::Config) -> std::io::Result<Self>
	where
		Listener: crate::util::Bind<'a, Address>,
	{
		Ok(Self::new(Listener::bind(address).await?, config))
	}

	/// Run the server.
	///
	/// The server will accept connections in a loop and spawn a user task for each new peer.
	pub async fn run<F, R>(&mut self, task: F) -> std::io::Result<()>
	where
		F: FnMut(PeerHandle<Listener::Body>) -> R,
		R: std::future::Future<Output = ()> + Send + 'static,
	{
		let mut task = task;
		loop {
			let peer = self.accept().await?;
			let join_handle = tokio::spawn((task)(peer));
			// TODO: keep join handles around so we can await them later.
			// If we do, we should also clean them from time to time though.
			drop(join_handle);
		}
	}

	/// Accept a connection and spawn a peer for it.
	///
	/// A [`Peer`] is spawned for the new connection,
	/// and a [`PeerHandle`] is returned to allow interaction with the peer.
	pub async fn accept(&mut self) -> std::io::Result<PeerHandle<Listener::Body>> {
		let (connection, _addr) = self.listener.accept().await?;
		Ok(Listener::spawn(connection, self.config.clone()))
	}
}
