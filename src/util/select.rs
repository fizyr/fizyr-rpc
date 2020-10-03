use std::future::Future;
use std::marker::Unpin;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

pub enum Either<A, B> {
	Left(A),
	Right(B),
}

pub struct Select<A, B> {
	inner: Option<(A, B)>
}

impl<A, B> std::future::Future for Select<A, B>
where
	A: Future + Unpin,
	B: Future + Unpin,
{
	type Output = Either<(A::Output, B), (A, B::Output)>;

	fn poll(mut self: Pin<&mut Self>, context: &mut Context) -> Poll<Self::Output> {
		let mut this = self.as_mut();

		let pin_a = Pin::new(&mut this.inner.as_mut().unwrap().0);
		if let Poll::Ready(a) = pin_a.poll(context) {
			let b = self.inner.take().unwrap().1;
			return Poll::Ready(Either::Left((a, b)))
		}

		let pin_b = Pin::new(&mut this.inner.as_mut().unwrap().1);
		if let Poll::Ready(b) = pin_b.poll(context) {
			let a = self.inner.take().unwrap().0;
			return Poll::Ready(Either::Right((a, b)))
		}

		Poll::Pending
	}
}

pub fn select<A, B>(a: A, b: B) -> Select<A, B>
where
	A: Future + Unpin,
	B: Future + Unpin,
{
	Select { inner: Some((a, b)) }
}
