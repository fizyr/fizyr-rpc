use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Trait for listeners that can accept new connections.
pub trait Listener {
	/// The type of the connections return by the [`Self::accept()`] function.
	type Connection: std::fmt::Debug;

	/// The type of the address returned by the [`Self::accept()`] function.
	type Address: std::fmt::Debug;

	/// Try to accept a new connection without blocking.
	///
	/// If no new connection is available, the current task is scheduled to wake up when a new connection is ready.
	fn poll_accept(self: Pin<&mut Self>, context: &mut Context) -> Poll<std::io::Result<(Self::Connection, Self::Address)>>;

	/// Asynchronously accept a new connection.
	fn accept(&mut self) -> Accept<Self>
	where
		Self: Unpin,
	{
		Accept { inner: self }
	}
}

/// Trait for creating a new listener bound to a specific address.
pub trait Bind<'a, Address: 'a>: Sized + Listener {
	/// The type of the future returned by `Self::bind`.
	type Future: Future<Output = std::io::Result<Self>>;

	/// Create a new listener bound to an address.
	fn bind(address: Address) -> Self::Future;
}

/// Future type returned by [`Listener::accept`].
pub struct Accept<'a, L: ?Sized> {
	inner: &'a mut L,
}

impl<L> Future for Accept<'_, L>
where
	L: Listener + Unpin + ?Sized,
{
	type Output = std::io::Result<(L::Connection, L::Address)>;

	fn poll(mut self: Pin<&mut Self>, context: &mut Context) -> Poll<Self::Output> {
		Pin::new(&mut *self.inner).poll_accept(context)
	}
}

#[cfg(feature = "tcp")]
impl Listener for tokio::net::TcpListener {
	type Address = std::net::SocketAddr;
	type Connection = tokio::net::TcpStream;

	fn poll_accept(self: Pin<&mut Self>, context: &mut Context) -> Poll<std::io::Result<(Self::Connection, Self::Address)>> {
		tokio::net::TcpListener::poll_accept(self.get_mut(), context)
	}
}

#[cfg(feature = "unix-stream")]
impl Listener for tokio::net::UnixListener {
	type Address = tokio::net::unix::SocketAddr;
	type Connection = tokio::net::UnixStream;

	fn poll_accept(self: Pin<&mut Self>, context: &mut Context) -> Poll<std::io::Result<(Self::Connection, Self::Address)>> {
		let accept = tokio::net::UnixListener::accept(self.get_mut());
		tokio::pin!(accept);
		accept.poll(context)
	}
}

#[cfg(feature = "unix-seqpacket")]
impl Listener for tokio_seqpacket::UnixSeqpacketListener {
	type Address = std::os::unix::net::SocketAddr;
	type Connection = tokio_seqpacket::UnixSeqpacket;

	fn poll_accept(self: Pin<&mut Self>, context: &mut Context) -> Poll<std::io::Result<(Self::Connection, Self::Address)>> {
		tokio_seqpacket::UnixSeqpacketListener::poll_accept(self.get_mut(), context)
	}
}

impl<T> Listener for &'_ mut T
where
	T: Listener + Unpin + ?Sized,
{
	type Address = T::Address;
	type Connection = T::Connection;

	fn poll_accept(mut self: Pin<&mut Self>, context: &mut Context) -> Poll<std::io::Result<(Self::Connection, Self::Address)>> {
		self.as_mut().poll_accept(context)
	}
}

impl<T> Listener for Box<T>
where
	T: Listener + Unpin + ?Sized,
{
	type Address = T::Address;
	type Connection = T::Connection;

	fn poll_accept(mut self: Pin<&mut Self>, context: &mut Context) -> Poll<std::io::Result<(Self::Connection, Self::Address)>> {
		self.as_mut().poll_accept(context)
	}
}

impl<P> Listener for Pin<P>
where
	P: std::ops::DerefMut + Unpin,
	P::Target: Listener,
{
	type Address = <P::Target as Listener>::Address;
	type Connection = <P::Target as Listener>::Connection;

	fn poll_accept(self: Pin<&mut Self>, context: &mut Context) -> Poll<std::io::Result<(Self::Connection, Self::Address)>> {
		self.get_mut().as_mut().poll_accept(context)
	}
}
