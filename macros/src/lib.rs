mod client;
mod interface;
mod util;

/// Define an RPC interface.
#[proc_macro]
pub fn interface(tokens: proc_macro::TokenStream) -> proc_macro::TokenStream {
	interface::generate_interface(tokens.into()).into()
}
