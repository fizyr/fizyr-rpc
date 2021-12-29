use proc_macro2::TokenStream;
use quote::quote;

use crate::interface::parse::cooked::InterfaceDefinition;

/// Generate a struct representing the interface.
///
/// TODO: Add RPC introspection support to this struct.
pub fn generate_interface_struct(item_tokens: &mut TokenStream, _fizyr_rpc: &syn::Ident, interface: &InterfaceDefinition) {
	let mut doc = String::new();
	for line in interface.doc() {
		doc.push_str(&line.value);
		doc.push('\n');
	}

	let interface_doc = format!("Introspection for the {} RPC interface.", interface.name());
	let visibility = interface.visibility();

	item_tokens.extend(quote! {
		#[doc = #interface_doc]
		#[derive(Debug)]
		#visibility struct Interface {
			_priv: (),
		}

		impl Interface {
			/// Get the documentation of the interface.
			pub const fn doc() -> &'static str {
				#doc
			}
		}
	})
}
