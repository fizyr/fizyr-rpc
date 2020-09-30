#[macro_export(crate)]
macro_rules! ready {
	($e:expr) => {
		match $e {
			::core::task::Poll::Pending => return ::core::task::Poll::Pending,
			::core::task::Poll::Ready(x) => x,
		}
	};
}
