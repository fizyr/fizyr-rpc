use std::pin::Pin;
use std::task::{Context, Poll};
use std::future::Future;

use crate::{Message, MessageHeader};
use crate::error::{ReadMessageError, WriteMessageError};

/// Trait for types that represent a bi-direction message transport.
///
/// Note that you can not use the transport itself directly.
/// Instead, you must split it in a read and write half and use those.
pub trait Transport {
	/// The body type for the messages.
	type Body;

	/// The type of the read half.
	type ReadHalf: TransportReadHalf<Body = Self::Body> + Unpin + Send;

	/// The type of the write half.
	type WriteHalf: TransportWriteHalf<Body = Self::Body> + Unpin + Send;

	fn split(self) -> (Self::ReadHalf, Self::WriteHalf);
}

/// Trait to allow generic creation of transports from a socket.
pub trait IntoTransport: Sized + Send {
	type Body;
	type Config;
	type Transport;

	/// Create a transport from `self` and a configuration struct.
	fn into_transport(self, config: Self::Config) -> Self::Transport;

	/// Create a transport from `self` using the default configuration.
	fn into_transport_default(self) -> Self::Transport
	where
		Self::Config: Default,
	{
		self.into_transport(Self::Config::default())
	}
}

/// Trait for the read half of a transport type.
pub trait TransportReadHalf {
	/// The body type of the message.
	type Body;

	/// Try to read a message from the transport without blocking.
	///
	/// This function may read partial messages into an internal buffer.
	///
	/// If the function returns [`Poll::Pending`],
	/// the current task is scheduled to wake when more data is available.
	fn poll_read_msg(self: Pin<&mut Self>, context: &mut Context) -> Poll<Result<Message<Self::Body>, ReadMessageError>>;

	/// Asynchronously read a complete message from the transport.
	fn read_msg(&mut self) -> ReadMsg<Self>
	where
		Self: Unpin,
	{
		ReadMsg { inner: self }
	}
}

/// Trait for transport types that you can write message to.
pub trait TransportWriteHalf {
	type Body;

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
	fn poll_write_msg(self: Pin<&mut Self>, context: &mut Context, header: &MessageHeader, body: &Self::Body) -> Poll<Result<(), WriteMessageError>>;

	/// Asynchronously write a message to the transport.
	fn write_msg<'c>(&'c mut self, header: &'c MessageHeader, body: &'c Self::Body) -> WriteMsg<Self> {
		WriteMsg {
			inner: self,
			header,
			body,
		}
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
	type Output = Result<Message<T::Body>, ReadMessageError>;

	fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
		Pin::new(&mut *self.get_mut().inner).poll_read_msg(cx)
	}
}

impl<T> Future for WriteMsg<'_, T>
where
	T: TransportWriteHalf + ?Sized + Unpin,
{
	type Output = Result<(), WriteMessageError>;

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

	fn poll_read_msg(mut self: Pin<&mut Self>, context: &mut Context) -> Poll<Result<Message<Self::Body>, ReadMessageError>> {
		self.as_mut().poll_read_msg(context)
	}
}

impl<T> TransportReadHalf for Box<T>
where
	T: TransportReadHalf + Unpin + ?Sized,
{
	type Body = T::Body;

	fn poll_read_msg(mut self: Pin<&mut Self>, context: &mut Context) -> Poll<Result<Message<Self::Body>, ReadMessageError>> {
		self.as_mut().poll_read_msg(context)
	}
}

impl<P> TransportReadHalf for Pin<P>
where
	P: std::ops::DerefMut + Unpin,
	P::Target: TransportReadHalf,
{
	type Body = <P::Target as TransportReadHalf>::Body;

	fn poll_read_msg(self: Pin<&mut Self>, context: &mut Context) -> Poll<Result<Message<Self::Body>, ReadMessageError>> {
		self.get_mut().as_mut().poll_read_msg(context)
	}
}

impl<T> TransportWriteHalf for &'_ mut T
where
	T: TransportWriteHalf + Unpin + ?Sized
{
	type Body = T::Body;

	fn poll_write_msg(mut self: Pin<&mut Self>, context: &mut Context, header: &MessageHeader, body: &Self::Body) -> Poll<Result<(), WriteMessageError>> {
		self.as_mut().poll_write_msg(context, header, body)
	}
}

impl<T> TransportWriteHalf for Box<T>
where
	T: TransportWriteHalf + Unpin + ?Sized
{
	type Body = T::Body;

	fn poll_write_msg(mut self: Pin<&mut Self>, context: &mut Context, header: &MessageHeader, body: &Self::Body) -> Poll<Result<(), WriteMessageError>> {
		self.as_mut().poll_write_msg(context, header, body)
	}
}

impl<P> TransportWriteHalf for Pin<P>
where
	P: std::ops::DerefMut + Unpin,
	P::Target: TransportWriteHalf,
{
	type Body = <P::Target as TransportWriteHalf>::Body;

	fn poll_write_msg(self: Pin<&mut Self>, context: &mut Context, header: &MessageHeader, body: &Self::Body) -> Poll<Result<(), WriteMessageError>> {
		self.get_mut().as_mut().poll_write_msg(context, header, body)
	}
}
