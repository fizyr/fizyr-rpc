//! Transport traits and concrete implementations.
//!
//! Transports are responsible for passing raw messages to a remote peer.
//! They are used by the [`Peer`][crate::Peer] struct to implement higher level RPC communication.
//!
//! Specific transports must be enabled with individual feature flags.
//! None of the concrete transport implementations are enabled by default.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::{Error, Message, MessageHeader};

pub(crate) mod stream;
pub use stream::StreamTransport;

pub(crate) mod unix;
pub use unix::UnixTransport;

/// Trait for types that represent a bi-direction message transport.
///
/// Note that you can not use the transport itself directly.
/// Instead, you must split it in a read and write half and use those.
pub trait Transport: Send + 'static {
	/// The body type for the messages.
	type Body: crate::Body;

	/// The configuration type for the transport.
	type Config: Clone + Default + Send + Sync + 'static;

	/// The type of the read half of the transport.
	type ReadHalf: for<'a> ReadHalfType<'a, Body = Self::Body>;

	/// The type of the write half of the transport.
	type WriteHalf: for<'a> WriteHalfType<'a, Body = Self::Body>;

	/// Split the transport into a read half and a write half.
	#[allow(clippy::needless_lifetimes)]
	fn split<'a>(&'a mut self) -> (<Self::ReadHalf as ReadHalfType<'a>>::ReadHalf, <Self::WriteHalf as WriteHalfType<'a>>::WriteHalf);
}

// TODO: Replace this with a generic associated type once it hits stable.
/// Helper trait to define the type of a read half for a transport.
///
/// Used to work around the lack of generic associated types.
pub trait ReadHalfType<'a> {
	/// The body type for the transport.
	type Body: crate::Body;

	/// The concrete type of the read half.
	type ReadHalf: TransportReadHalf<Body = Self::Body>;
}

// TODO: Replace this with a generic associated type once it hits stable.
/// Helper trait to define the type of a write half for a transport.
///
/// Used to work around the lack of generic associated types.
pub trait WriteHalfType<'a> {
	/// The body type for the transport.
	type Body: crate::Body;

	/// The concrete type of the write half.
	type WriteHalf: TransportWriteHalf<Body = Self::Body>;
}

/// Trait for the read half of a transport type.
pub trait TransportReadHalf: Send + Unpin {
	/// The body type for messages transferred over the transport.
	type Body: crate::Body;

	/// Try to read a message from the transport without blocking.
	///
	/// This function may read partial messages into an internal buffer.
	///
	/// If the function returns [`Poll::Pending`],
	/// the current task is scheduled to wake when more data is available.
	fn poll_read_msg(self: Pin<&mut Self>, context: &mut Context) -> Poll<Result<Message<Self::Body>, Error>>;

	/// Asynchronously read a complete message from the transport.
	fn read_msg(&mut self) -> ReadMsg<Self>
	where
		Self: Unpin,
	{
		ReadMsg { inner: self }
	}
}

/// Trait for transport types that you can write message to.
pub trait TransportWriteHalf: Send + Unpin {
	/// The body type for messages transferred over the transport.
	type Body: crate::Body;

	/// Try to write a message to the transport without blocking.
	///
	/// This function may write only part of the message.
	/// The next invocation will skip the already written bytes.
	///
	/// It is an error to change the value of the `header` and `body` parameters between invocations
	/// as long as the function returns [`Poll::Pending`].
	/// An implementation may write spliced messages over the transport if you do.
	/// It is allowed to *move* the header and body in between invocations though,
	/// as long as the values remain the same.
	///
	/// If the function returns [`Poll::Pending`],
	/// the current task is scheduled to wake when the transport is ready for more data.
	fn poll_write_msg(self: Pin<&mut Self>, context: &mut Context, header: &MessageHeader, body: &Self::Body) -> Poll<Result<(), Error>>;

	/// Asynchronously write a message to the transport.
	fn write_msg<'c>(&'c mut self, header: &'c MessageHeader, body: &'c Self::Body) -> WriteMsg<Self> {
		WriteMsg { inner: self, header, body }
	}
}

/// Future type for [`TransportReadHalf::read_msg`].
pub struct ReadMsg<'c, T>
where
	T: TransportReadHalf + ?Sized,
{
	inner: &'c mut T,
}

/// Future type for [`TransportWriteHalf::write_msg`].
pub struct WriteMsg<'c, T>
where
	T: TransportWriteHalf + ?Sized,
{
	inner: &'c mut T,
	header: &'c MessageHeader,
	body: &'c T::Body,
}

impl<T> Future for ReadMsg<'_, T>
where
	T: TransportReadHalf + ?Sized + Unpin,
{
	type Output = Result<Message<T::Body>, Error>;

	fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
		Pin::new(&mut *self.get_mut().inner).poll_read_msg(cx)
	}
}

impl<T> Future for WriteMsg<'_, T>
where
	T: TransportWriteHalf + ?Sized + Unpin,
{
	type Output = Result<(), Error>;

	fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
		let header = self.header;
		let body = self.body;
		Pin::new(&mut *self.get_mut().inner).poll_write_msg(cx, header, body)
	}
}

impl<T> TransportReadHalf for &'_ mut T
where
	T: TransportReadHalf + Unpin + ?Sized,
{
	type Body = T::Body;

	fn poll_read_msg(self: Pin<&mut Self>, context: &mut Context) -> Poll<Result<Message<Self::Body>, Error>> {
		T::poll_read_msg(Pin::new(*self.get_mut()), context)
	}
}

impl<T> TransportReadHalf for Box<T>
where
	T: TransportReadHalf + Unpin + ?Sized,
{
	type Body = T::Body;

	fn poll_read_msg(self: Pin<&mut Self>, context: &mut Context) -> Poll<Result<Message<Self::Body>, Error>> {
		T::poll_read_msg(Pin::new(&mut *self.get_mut()), context)
	}
}

impl<P> TransportReadHalf for Pin<P>
where
	P: std::ops::DerefMut + Send + Unpin,
	P::Target: TransportReadHalf,
{
	type Body = <P::Target as TransportReadHalf>::Body;

	fn poll_read_msg(self: Pin<&mut Self>, context: &mut Context) -> Poll<Result<Message<Self::Body>, Error>> {
		P::Target::poll_read_msg(Pin::new(&mut *self.get_mut()), context)
	}
}

impl<T> TransportWriteHalf for &'_ mut T
where
	T: TransportWriteHalf + Unpin + ?Sized,
{
	type Body = T::Body;

	fn poll_write_msg(
		self: Pin<&mut Self>,
		context: &mut Context,
		header: &MessageHeader,
		body: &Self::Body,
	) -> Poll<Result<(), Error>> {
		T::poll_write_msg(Pin::new(*self.get_mut()), context, header, body)
	}
}

impl<T> TransportWriteHalf for Box<T>
where
	T: TransportWriteHalf + Unpin + ?Sized,
{
	type Body = T::Body;

	fn poll_write_msg(
		self: Pin<&mut Self>,
		context: &mut Context,
		header: &MessageHeader,
		body: &Self::Body,
	) -> Poll<Result<(), Error>> {
		T::poll_write_msg(Pin::new(&mut *self.get_mut()), context, header, body)
	}
}

impl<P> TransportWriteHalf for Pin<P>
where
	P: std::ops::DerefMut + Send + Unpin,
	P::Target: TransportWriteHalf,
{
	type Body = <P::Target as TransportWriteHalf>::Body;

	fn poll_write_msg(self: Pin<&mut Self>, context: &mut Context, header: &MessageHeader, body: &Self::Body) -> Poll<Result<(), Error>> {
		P::Target::poll_write_msg(Pin::new(&mut *self.get_mut()), context, header, body)
	}
}
