use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Trait for listeners that can accept new connections.
pub trait Listener {
	type Connection: std::fmt::Debug;
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

pub struct Accept<'a, L: ?Sized> {
	inner: &'a mut L,
}

impl<L> Future for Accept<'_, L>
where
	L: Listener + Unpin + ?Sized
{
	type Output = std::io::Result<(L::Connection, L::Address)>;

	fn poll(mut self: Pin<&mut Self>, context: &mut Context) -> Poll<Self::Output> {
		Pin::new(&mut *self.inner).poll_accept(context)
	}

}

#[cfg(feature = "tcp")]
impl Listener for tokio::net::TcpListener {
	type Connection = tokio::net::TcpStream;
	type Address = std::net::SocketAddr;

	fn poll_accept(self: Pin<&mut Self>, context: &mut Context) -> Poll<std::io::Result<(Self::Connection, Self::Address)>> {
		tokio::net::TcpListener::poll_accept(self.get_mut(), context)
	}
}

#[cfg(feature = "unix")]
impl Listener for tokio::net::UnixListener {
	type Connection = tokio::net::UnixStream;
	type Address = std::os::unix::net::SocketAddr;

	fn poll_accept(self: Pin<&mut Self>, context: &mut Context) -> Poll<std::io::Result<(Self::Connection, Self::Address)>> {
		let accept = tokio::net::UnixListener::accept(self.get_mut());
		tokio::pin!(accept);
		accept.poll(context)
	}
}

impl<T> Listener for &'_ mut T
where
	T: Listener + Unpin + ?Sized,
{
	type Connection = T::Connection;
	type Address = T::Address;

	fn poll_accept(mut self: Pin<&mut Self>, context: &mut Context) -> Poll<std::io::Result<(Self::Connection, Self::Address)>> {
		self.as_mut().poll_accept(context)
	}
}

impl<T> Listener for Box<T>
where
	T: Listener + Unpin + ?Sized,
{
	type Connection = T::Connection;
	type Address = T::Address;

	fn poll_accept(mut self: Pin<&mut Self>, context: &mut Context) -> Poll<std::io::Result<(Self::Connection, Self::Address)>> {
		self.as_mut().poll_accept(context)
	}
}

impl<P> Listener for Pin<P>
where
	P: std::ops::DerefMut + Unpin,
	P::Target: Listener,
{
	type Connection = <P::Target as Listener>::Connection;
	type Address = <P::Target as Listener>::Address;

	fn poll_accept(self: Pin<&mut Self>, context: &mut Context) -> Poll<std::io::Result<(Self::Connection, Self::Address)>> {
		self.get_mut().as_mut().poll_accept(context)
	}
}
