use proc_macro2::TokenStream;
use quote::quote;

use crate::interface::parse::cooked::InterfaceDefinition;

/// Generate a client struct.
///
/// `extra_impl` is used to add additional functions to the main `impl` block.
pub fn generate_client(item_tokens: &mut TokenStream, fizyr_rpc: &syn::Ident, interface: &InterfaceDefinition, extra_impl: TokenStream) {
	let client_doc = format!("RPC client for the {} interface.", interface.name());
	let visibility = interface.visibility();
	item_tokens.extend(quote! {
		#[doc = #client_doc]
		#visibility struct Client<F: #fizyr_rpc::util::format::Format> {
			peer: #fizyr_rpc::PeerWriteHandle<F::Body>,
		}

		impl<F: #fizyr_rpc::util::format::Format> ::core::fmt::Debug for Client<F> {
			fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
				f.debug_struct(::core::any::type_name::<Self>())
					.field("peer", &self.peer)
					.finish()
			}
		}

		impl<F: #fizyr_rpc::util::format::Format> ::core::clone::Clone for Client<F> {
			fn clone(&self) -> Self {
				Self {
					peer: self.peer.clone(),
				}
			}
		}

		impl<F: #fizyr_rpc::util::format::Format> ::core::convert::From<#fizyr_rpc::PeerWriteHandle<F::Body>> for Client<F> {
			fn from(other: #fizyr_rpc::PeerWriteHandle<F::Body>) -> Self {
				Self::new(other)
			}
		}

		impl<F: #fizyr_rpc::util::format::Format> ::core::convert::From<#fizyr_rpc::PeerHandle<F::Body>> for Client<F> {
			fn from(other: #fizyr_rpc::PeerHandle<F::Body>) -> Self {
				let (_read, write) = other.split();
				Self::new(write)
			}
		}

		impl<F: #fizyr_rpc::util::format::Format> Client<F> {
			/// Create a new interface-specific RPC client from a raw write handle.
			pub fn new(peer: #fizyr_rpc::PeerWriteHandle<F::Body>) -> Self {
				Self { peer }
			}

			/// Close the connection with the remote peer.
			pub fn close(self) {
				self.peer.close()
			}

			/// Make a close handle for the peer.
			///
			/// The close handle can be used to close the connection with the remote peer.
			/// It can be cloned and moved around independently.
			pub fn close_handle(&self) -> #fizyr_rpc::PeerCloseHandle<F::Body> {
				self.peer.close_handle()
			}

			#extra_impl
		}
	})
}
