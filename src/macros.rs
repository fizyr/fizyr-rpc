#[doc(hidden)]
pub use fizyr_rpc_macros::interface as interface_impl;

#[macro_export]
/// Define an RPC interface.
macro_rules! interface {
	($($tokens:tt)*) => {
		$crate::macros::interface_impl!{$crate; $($tokens)*}
	}
}
