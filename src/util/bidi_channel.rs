use futures::channel::mpsc;

use crate::error;

/// A bi-directional asynchronous channel.
///
/// Implemented by wrapping a receiver and sender for two distinct channels.
pub struct BidiChannel<T> {
	rx: mpsc::UnboundedReceiver<T>,
	tx: mpsc::UnboundedSender<T>,
}

impl<T> BidiChannel<T> {
	/// Create a bi-directional channel from two channel halves.
	///
	/// The two halves should generally be from distinct channels,
	/// otherwise the channel just sends to and receives from itself.
	pub fn from_halves(tx: mpsc::UnboundedSender<T>, rx: mpsc::UnboundedReceiver<T>) -> Self {
		Self { tx, rx }
	}

	/// Send a value over the channel.
	///
	/// Returns an error if the reading side of the send half is closed.
	pub async fn send(&mut self, value: T) -> Result<(), error::ConnectionClosed> {
		use futures::sink::SinkExt;

		self.tx.send(value).await.map_err(|e| {
			debug_assert!(e.is_disconnected(), "using unbounded channels, so only error we should get is disconnect error, yet we got {:?}", e);
			error::ConnectionClosed
		})
	}

	/// Receive a value from the channel.
	///
	/// Returns an error if the sending side of the read half is closed.
	pub async fn receive(&mut self) -> Result<T, error::ConnectionClosed> {
		use futures::stream::StreamExt;
		use futures::stream::FusedStream;

		if self.rx.is_terminated() {
			Err(error::ConnectionClosed)
		} else {
			self.rx.next().await.ok_or(error::ConnectionClosed)
		}
	}
}
