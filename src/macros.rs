/// Unwrap a [`Poll`](std::task::Poll) value and return from the enclosing function if it was [`Pending`](std::task::Poll::Pending).
///
/// This is like `try!()`, but for [`Poll`](std::task::Poll).
macro_rules! ready {
	($e:expr) => {
		match $e {
			::core::task::Poll::Pending => return ::core::task::Poll::Pending,
			::core::task::Poll::Ready(x) => x,
		}
	};
}
