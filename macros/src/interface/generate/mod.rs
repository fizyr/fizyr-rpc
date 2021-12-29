use proc_macro2::TokenStream;

use super::parse::cooked::InterfaceDefinition;

mod client;
mod interface_struct;
mod message_enum;
mod server;
mod services;
mod streams;

/// Generate a client struct for the given interface.
pub fn generate_interface(fizyr_rpc: &syn::Ident, interface: &InterfaceDefinition) -> TokenStream {
	let mut item_tokens = TokenStream::new();
	let mut client_impl_tokens = TokenStream::new();

	interface_struct::generate_interface_struct(&mut item_tokens, fizyr_rpc, interface);
	services::generate_services(&mut item_tokens, &mut client_impl_tokens, fizyr_rpc, interface);
	streams::generate_streams(&mut item_tokens, &mut client_impl_tokens, fizyr_rpc, interface);
	client::generate_client(&mut item_tokens, fizyr_rpc, interface, client_impl_tokens);
	server::generate_server(&mut item_tokens, fizyr_rpc, interface);

	item_tokens
}

fn to_upper_camel_case(input: &str) -> String {
	let mut output = String::new();
	let mut capitalize = true;

	for c in input.chars() {
		if c == '_' {
			capitalize = true;
		} else if capitalize {
			output.push(c.to_ascii_uppercase());
			capitalize = false;
		} else {
			output.push(c);
		}
	}
	output
}

fn to_doc_attrs(docs: &[crate::util::WithSpan<String>]) -> TokenStream {
	let mut tokens = TokenStream::new();
	for doc in docs {
		let text = &doc.value;
		tokens.extend(quote::quote_spanned!(doc.span => #[doc = #text]));
	}
	tokens
}

fn is_unit_type(ty: &syn::Type) -> bool {
	if let syn::Type::Tuple(ty) = ty {
		ty.elems.is_empty()
	} else {
		false
	}
}
