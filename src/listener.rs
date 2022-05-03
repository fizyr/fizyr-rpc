use crate::Peer;
use crate::PeerHandle;
use crate::util;
use crate::transport::Transport;

/// Listener that spawns peers for all accepted connections.
pub struct Listener<Socket>
where
	Socket: ListeningSocket,
{
	listener: Socket,
	config: Socket::Config,
}

/// Helper trait for [`Listener`].
///
/// This trait encapsulates all requirements for the `Socket` type of a [`Listener`].
///
/// The trait has a blanket implementation,
/// so you can not implement it for your own types.
///
/// You *can* use it as trait bound for generic arguments,
/// but you should not rely on any of the items in this trait.
pub trait ListeningSocket: util::Listener + Unpin {
	#[doc(hidden)]
	type Body: crate::Body;

	#[doc(hidden)]
	type Config: Clone + Send + Sync + 'static;

	#[doc(hidden)]
	type Transport: Transport + Send + 'static;

	#[doc(hidden)]
	type TransportInfo: Send + 'static;

	#[doc(hidden)]
	fn into_transport(connection: Self::Connection, config: Self::Config) -> Self::Transport;

	#[doc(hidden)]
	fn transport_info(connection: &Self::Transport) -> std::io::Result<Self::TransportInfo>;

	#[doc(hidden)]
	fn spawn(transport: Self::Transport) -> PeerHandle<Self::Body>;
}

impl<Socket> ListeningSocket for Socket
where
	Socket: util::Listener + Unpin,
	Socket::Connection: util::IntoTransport,
{
	type Body = <Socket::Connection as util::IntoTransport>::Body;
	type Config = <Socket::Connection as util::IntoTransport>::Config;
	type Transport = <Socket::Connection as util::IntoTransport>::Transport;
	type TransportInfo = <Self::Transport as Transport>::Info;

	fn into_transport(connection: Self::Connection, config: Self::Config) -> Self::Transport {
		use util::IntoTransport;
		connection.into_transport(config)
	}

	fn transport_info(connection: &Self::Transport) -> std::io::Result<Self::TransportInfo> {
		connection.info()
	}

	fn spawn(transport: Self::Transport) -> PeerHandle<Self::Body> {
		Peer::spawn(transport)
	}
}

impl<Socket: ListeningSocket> Listener<Socket> {
	/// Create a server on a listening socket.
	///
	/// The passed in config is used to create transports and peers for all accepted connections.
	pub fn new(listener: Socket, config: Socket::Config) -> Self {
		Self { listener, config }
	}

	/// Create a server with a new listening socket bound to the given address.
	///
	/// The type of address accepted depends on the listener.
	/// For internet transports such as TCP, the address must implement [`tokio::net::ToSocketAddrs`].
	/// For unix transports, the address must implement [`AsRef<std::path::Path>`].
	///
	/// This function is asynchronous because it may perform a DNS lookup for some address types.
	pub async fn bind<'a, Address: 'a>(address: Address, config: Socket::Config) -> std::io::Result<Self>
	where
		Socket: util::Bind<'a, Address>,
	{
		Ok(Self::new(Socket::bind(address).await?, config))
	}

	/// Run the server.
	///
	/// The server will accept connections in a loop and spawn a user task for each new peer.
	pub async fn run<F, R>(&mut self, task: F) -> std::io::Result<()>
	where
		F: FnMut(PeerHandle<Socket::Body>, Socket::TransportInfo) -> R,
		R: std::future::Future<Output = ()> + Send + 'static,
	{
		let mut task = task;
		loop {
			let (peer, info) = self.accept().await?;
			let join_handle = tokio::spawn((task)(peer, info));
			// TODO: keep join handles around so we can await them later.
			// If we do, we should also clean them from time to time though.
			drop(join_handle);
		}
	}

	/// Accept a connection and spawn a peer for it.
	///
	/// A [`Peer`] is spawned for the new connection,
	/// and a [`PeerHandle`] is returned to allow interaction with the peer.
	pub async fn accept(&mut self) -> std::io::Result<(PeerHandle<Socket::Body>, Socket::TransportInfo)> {
		let (connection, _addr) = self.listener.accept().await?;
		let transport = Socket::into_transport(connection, self.config.clone());
		let info = Socket::transport_info(&transport)?;
		Ok((Socket::spawn(transport), info))
	}
}
