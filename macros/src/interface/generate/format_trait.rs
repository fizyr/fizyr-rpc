use proc_macro2::TokenStream;
use quote::quote;

use crate::interface::parse::cooked::InterfaceDefinition;

/// Generate a format trait specifically for the given RPC interface.
pub fn generate_format_trait(item_tokens: &mut TokenStream, fizyr_rpc: &syn::Ident, interface: &InterfaceDefinition) {
	let mut types = Vec::new();

	for service in interface.services() {
		types.push(service.request_type());
		types.push(service.response_type());
		for update in service.request_updates() {
			types.push(update.body_type());
		}
		for update in service.response_updates() {
			types.push(update.body_type());
		}
	}

	for stream in interface.streams() {
		types.push(stream.body_type())
	}

	let mut bounds = quote!(#fizyr_rpc::util::format::Format);
	for typ in &types {
		bounds.extend(quote!( + #fizyr_rpc::util::format::EncodeBody<#typ>));
		bounds.extend(quote!( + #fizyr_rpc::util::format::DecodeBody<#typ>));
	}

	let visibility = interface.visibility();
	item_tokens.extend(quote! {
		/// Trait for formats that are compatible with this interface.
		///
		/// A format is compatible when it can encode and decode all messages that appear in the interface.
		/// It is automatically implemented for all compatible formats.
		#visibility trait Format: #bounds {}

		impl<T> Format for T
		where
			T: #bounds
		{}
	})
}
