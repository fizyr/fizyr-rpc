use proc_macro2::{TokenStream, Span};
use quote::quote;

use crate::interface::parse::cooked::InterfaceDefinition;

use super::{to_upper_camel_case, to_doc_attrs};

/// Generate a server struct.
///
/// `extra_impl` is used to add additional functions to the main `impl` block.
pub fn generate_server(item_tokens: &mut TokenStream, fizyr_rpc: &syn::Ident, interface: &InterfaceDefinition) {
	// Generic parameters to the `ReceivedMessage` struct.
	let mut received_msg_generics = TokenStream::new();
	// Where clauses for the `ReceivedMessage` struct.
	let mut received_msg_where = TokenStream::new();
	// Variants for the `ReceivedMessage` enum.
	let mut received_msg_variants = TokenStream::new();
	// Match arms for the `ReceivedMessage` debug implementation.
	let mut received_msg_debug_arms = TokenStream::new();
	// Where clause for the `recv_message` function.
	let mut recv_message_where = TokenStream::new();
	// Match arms for decoding a request message.
	let mut decode_stream_arms = TokenStream::new();
	// Match arms for decoding a request message.
	let mut decode_request_arms = TokenStream::new();

	for stream in interface.streams() {
		let service_id = stream.service_id();
		let stream_name = stream.name();
		let variant_name = syn::Ident::new(&to_upper_camel_case(&stream_name.to_string()), Span::call_site());
		let body_type = stream.body_type();
		recv_message_where.extend(quote! {
			F: #fizyr_rpc::format::DecodeBody<#body_type>,
		});
		decode_stream_arms.extend(quote! {
			#service_id =>  {
				match F::decode_body(message.body) {
					::core::result::Result::Ok(body) => {
						::core::result::Result::Ok(ReceivedMessage::Stream(StreamMessage::#variant_name(body)))
					},
					::core::result::Result::Err(e) => {
						::core::result::Result::Err(#fizyr_rpc::RecvMessageError::InvalidStream(message.header, e))
					},
				}
			},
		});
	}

	if !interface.streams().is_empty() {
		recv_message_where.extend(quote! {
			StreamMessage: #fizyr_rpc::format::FromMessage<F>,
		});
		received_msg_variants.extend(quote! {
			/// A stream message.
			Stream(StreamMessage),
		});
		received_msg_debug_arms.extend(quote! {
			Self::Stream(message) => {
				f.debug_tuple("Stream")
					.field(&message)
					.finish()
			},
		});
	}

	for service in interface.services() {
		let service_id = service.service_id();
		let service_name = service.name();
		let variant_name = syn::Ident::new(&to_upper_camel_case(&service_name.to_string()), Span::call_site());
		let request_type = service.request_type();
		recv_message_where.extend(quote! {
			F: #fizyr_rpc::format::DecodeBody<#request_type>,
		});
		decode_request_arms.extend(quote! {
			#service_id =>  {
				match F::decode_body(body) {
					::core::result::Result::Ok(body) => {
						let request = #service_name::ReceivedRequestHandle { request };
						::core::result::Result::Ok(ReceivedMessage::Request(ReceivedRequestHandle::#variant_name(request, body)))
					},
					::core::result::Result::Err(e) => {
						::core::result::Result::Err(#fizyr_rpc::RecvMessageError::InvalidRequest(request, e))
					},
				}
			},
		});
	}

	if !interface.services().is_empty() {
		received_msg_generics.extend(quote!(F));
		received_msg_where.extend(quote! {
			F: #fizyr_rpc::format::Format,
		});
		received_msg_variants.extend(quote! {
			/// A request message.
			Request(ReceivedRequestHandle<F>),
		});
		received_msg_debug_arms.extend(quote! {
			Self::Request(request_handle) => {
				f.debug_tuple("Request")
					.field(&request_handle)
					.finish()
			},
		});
	}

	let visibility = interface.visibility();
	let server_doc = format!("RPC server for the {} interface.", interface.name());
	item_tokens.extend(quote! {
		#[doc = #server_doc]
		#visibility struct Server<F: #fizyr_rpc::format::Format> {
			peer: #fizyr_rpc::PeerReadHandle<F::Body>,
		}

		impl<F: #fizyr_rpc::format::Format> ::core::fmt::Debug for Server<F> {
			fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
				f.debug_struct(::core::any::type_name::<Self>())
					.field("peer", &self.peer)
					.finish()
			}
		}

		impl<F: #fizyr_rpc::format::Format> Server<F> {
			/// Create a new interface-specific RPC server from a raw write handle.
			fn new(peer: #fizyr_rpc::PeerReadHandle<F::Body>) -> Self {
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

			/// Receive the next incoming message.
			pub async fn recv_message(&mut self) -> ::core::result::Result<ReceivedMessage<#received_msg_generics>, #fizyr_rpc::RecvMessageError<F::Body>>
			where
				#recv_message_where
			{
				match self.peer.recv_message().await? {
					#fizyr_rpc::ReceivedMessage::Stream(message) => {
						match message.header.service_id {
							#decode_stream_arms
							_ => ::core::result::Result::Err(#fizyr_rpc::RecvMessageError::UnknownStream(message)),
						}
					},
					#fizyr_rpc::ReceivedMessage::Request(request, body) => {
						match request.service_id() {
							#decode_request_arms
							_ => ::core::result::Result::Err(#fizyr_rpc::RecvMessageError::UnknownRequest(request, body)),
						}
					},
				}
			}
		}

		impl<F: #fizyr_rpc::format::Format> ::core::convert::From<#fizyr_rpc::PeerReadHandle<F::Body>> for Server<F> {
			fn from(other: #fizyr_rpc::PeerReadHandle<F::Body>) -> Self {
				Self::new(other)
			}
		}

		impl<F: #fizyr_rpc::format::Format> ::core::convert::From<#fizyr_rpc::PeerHandle<F::Body>> for Server<F> {
			fn from(other: #fizyr_rpc::PeerHandle<F::Body>) -> Self {
				let (read, _write) = other.split();
				Self::new(read)
			}
		}

		/// An incoming message from a remote peer.
		#visibility enum ReceivedMessage<#received_msg_generics>
		where
			#received_msg_where
		{
			#received_msg_variants
		}

		impl<#received_msg_generics> ::core::fmt::Debug for ReceivedMessage<#received_msg_generics>
		where
			#received_msg_where
		{
			fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
				match self {
					#received_msg_debug_arms
				}
			}
		}
	});

	if !interface.services().is_empty() {
		generate_received_request_enum(item_tokens, fizyr_rpc, interface);
	}
}

/// Generate an enum for all possible received requests for a server.
fn generate_received_request_enum(item_tokens: &mut TokenStream, fizyr_rpc: &syn::Ident, interface: &InterfaceDefinition) {
	let mut variant_tokens = TokenStream::new();
	let mut debug_tokens = TokenStream::new();
	for service in interface.services() {
		let service_name = service.name();
		let variant_name_string = to_upper_camel_case(&service_name.to_string());
		let variant_name = syn::Ident::new(&variant_name_string, Span::call_site());
		let request_type = service.request_type();
		let doc = to_doc_attrs(service.doc());
		variant_tokens.extend(quote! {
			#doc
			#variant_name(#service_name::ReceivedRequestHandle<F>, #request_type),
		});
		debug_tokens.extend(quote! {
			Self::#variant_name(request, _body) => ::core::write!(f, "{}({:?})", #variant_name_string, request),
		});
	}

	let enum_doc = format!("Enum for all possible incoming requests of the {} interface.", interface.name());
	let visibility = interface.visibility();
	item_tokens.extend(quote! {
		#[doc = #enum_doc]
		#visibility enum ReceivedRequestHandle<F: #fizyr_rpc::format::Format> {
			#variant_tokens
		}

		impl<F: #fizyr_rpc::format::Format> ::core::fmt::Debug for ReceivedRequestHandle<F> {
			fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
				match self {
					#debug_tokens
				}
			}
		}
	})
}
