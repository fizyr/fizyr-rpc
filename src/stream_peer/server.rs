use tokio::stream::Stream;
use tokio::stream::StreamExt;

use crate::PeerHandle;
use crate::util::SplitAsyncReadWrite;
use super::StreamBody;
use super::StreamPeer;
use super::StreamPeerConfig;

pub struct StreamServer<Listener> {
	listener: Listener,
	config: StreamPeerConfig,
}

impl<Listener, Socket> StreamServer<Listener>
where
	Listener: Stream<Item = std::io::Result<Socket>> + Unpin,
	Socket: Send + 'static,
	for<'a> &'a mut Socket: SplitAsyncReadWrite,
{
	/// Create a server on a listening socket.
	///
	/// The passed in config is used to create [`StreamPeer`] objects for all accepted connections.
	pub fn new(listener: Listener, config: StreamPeerConfig) -> Self {
		Self { listener, config }
	}

	/// Run the server.
	///
	/// The server will accept connections in a loop and spawn a user task for each new peer.
	pub async fn run<F, R>(&mut self, task: F) -> std::io::Result<()>
	where
		F: FnMut(PeerHandle<StreamBody>) -> R,
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
	/// A [`StreamPeer`] is spawned for the new connection,
	/// and a [`PeerHandle`] is returned to allow interaction with the peer.
	pub async fn accept(&mut self) -> std::io::Result<PeerHandle<StreamBody>> {
		let socket = self.listener.next().await;
		let socket = socket.ok_or_else(crate::error::connection_aborted)??;
		Ok(StreamPeer::spawn(socket, self.config.clone()).await)
	}
}
