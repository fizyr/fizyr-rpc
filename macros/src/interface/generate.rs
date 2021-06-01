use quote::quote;
use proc_macro2::{Span, TokenStream};

use super::parse::cooked::{InterfaceDefinition, ServiceDefinition, UpdateDefinition};

/// Generate a client struct for the given interface.
pub fn generate_client(fizyr_rpc: &syn::Ident, interface: &InterfaceDefinition) -> TokenStream {
	let mut item_tokens = TokenStream::new();
	let mut impl_tokens = TokenStream::new();

	let interface_name = interface.name();
	let interface_doc = if interface.doc().is_empty() {
		let text = format!("Support types for the {} RPC interface.", interface.name());
		quote!(#[doc = #text])
	} else {
		to_doc_attrs(interface.doc())
	};
	let client_doc = format!("RPC client for the {} interface.", interface.name());
	generate_services(&mut item_tokens, &mut impl_tokens, fizyr_rpc, interface);

	let tokens = quote! {
		#interface_doc
		pub mod #interface_name {
			use super::*;

			#[doc = #client_doc]
			pub struct Client<P: #fizyr_rpc::macros::Protocol> {
				peer: #fizyr_rpc::PeerWriteHandle<P::Body>,
			}

			impl<P: #fizyr_rpc::macros::Protocol> Client<P> {
				/// Create a new interface-specific RPC client from a raw write handle.
				fn new(peer: #fizyr_rpc::PeerWriteHandle<P::Body>) -> Self {
					Self { peer }
				}

				/// Connect to a remote server.
				///
				/// See [`fizyr_rpc::Peer::connect`](https://docs.rs/fizyr-rpc/latest/fizyr_rpc/struct.Peer.html#method.connect) for more details.
				async fn connect<'a, Transport, Address>(address: Address, config: Transport::Config) -> std::io::Result<Self>
				where
					Address: 'a,
					Transport: #fizyr_rpc::transport::Transport<Body = P::Body> + #fizyr_rpc::util::Connect<'a, Address>,
				{
					Ok(#fizyr_rpc::Peer::<Transport>::connect(address, config).await?.into())
				}

				/// Close the connection with the remote peer.
				pub fn close(self) {
					self.peer.close()
				}

				/// Make a close handle for the peer.
				///
				/// The close handle can be used to close the connection with the remote peer.
				/// It can be cloned and moved around independently.
				pub fn close_handle(&self) -> #fizyr_rpc::PeerCloseHandle<P::Body> {
					self.peer.close_handle()
				}

				#impl_tokens
			}

			impl<P: #fizyr_rpc::macros::Protocol> ::core::convert::From<#fizyr_rpc::PeerWriteHandle<P::Body>> for Client<P> {
				fn from(other: #fizyr_rpc::PeerWriteHandle<P::Body>) -> Self {
					Self::new(other)
				}
			}

			impl<P: #fizyr_rpc::macros::Protocol> ::core::convert::From<#fizyr_rpc::PeerHandle<P::Body>> for Client<P> {
				fn from(other: #fizyr_rpc::PeerHandle<P::Body>) -> Self {
					let (_read, write) = other.split();
					Self::new(write)
				}
			}

			#item_tokens
		}
	};

	tokens
}

/// Generate the support types and function definitions for each service.
fn generate_services(item_tokens: &mut TokenStream, impl_tokens: &mut TokenStream, fizyr_rpc: &syn::Ident, interface: &InterfaceDefinition) {
	for service in interface.services() {
		let service_name = service.name();
		let service_doc = to_doc_attrs(&service.doc());
		let service_id = service.service_id();

		let request_param = service.request_type().map(|x| quote!(request: #x));
		let request_type = service.request_type().map(|x| quote!(#x)).unwrap_or_else(|| quote!(()));
		let request_body = if service.request_type().is_some() {
			quote!(P::encode_body(request))
		} else {
			quote!(P::encode_body(()))
		};

		let response_type = service.response_type().map(|x| quote!(#x)).unwrap_or_else(|| quote!(()));

		// Service without updates, so directly return the response (asynchronously).
		if service.request_updates().is_empty() && service.response_updates().is_empty() {
			impl_tokens.extend(quote! {
				#service_doc
				pub async fn #service_name(&self, #request_param) -> Result<#response_type, #fizyr_rpc::error::ServiceCallError>
				where
					#request_type: #fizyr_rpc::macros::Encode<P>,
					#response_type: #fizyr_rpc::macros::Decode<P>,
				{
					let request_body = #request_body.map_err(#fizyr_rpc::error::SendRequestError::EncodeBody)?;
					let mut request = self.peer.send_request(#service_id, request_body).await?;
					let response = request.recv_response().await?;
					let decoded = P::decode_body(response.body).map_err(#fizyr_rpc::error::RecvMessageError::DecodeBody)?;
					Ok(decoded)
				}
			})
		} else {
			let mut service_item_tokens = TokenStream::new();
			generate_sent_request(&mut service_item_tokens, &fizyr_rpc, service);
			impl_tokens.extend(quote! {
				#service_doc
				pub async fn #service_name(&self, #request_param) -> Result<#service_name::SentRequest<P>, #fizyr_rpc::error::SendRequestError>
				where
					#request_type: #fizyr_rpc::macros::Encode<P>,
					#response_type: #fizyr_rpc::macros::Decode<P>,
				{
					let request_body = #request_body.map_err(#fizyr_rpc::error::SendRequestError::EncodeBody)?;
					let mut request = self.peer.send_request(#service_id, request_body).await?;
					Ok(#service_name::SentRequest { request })
				}
			});

			let mod_doc = format!("Support types for the {} service.", service.name());
			item_tokens.extend(quote! {
				#[doc = #mod_doc]
				pub mod #service_name {
					use super::*;
					#service_item_tokens
				}
			});
		}
	}
}

/// Generate a type for the sent request for a specific service.
///
/// Only used for service calls that have update messages.
/// Otherwise, the return type of a service call will simply be the response message.
fn generate_sent_request(item_tokens: &mut TokenStream, fizyr_rpc: &syn::Ident, service: &ServiceDefinition) {
	let service_name = service.name();
	let response_type = service.response_type().map(|x| quote!(#x)).unwrap_or_else(|| quote!(()));

	let doc_recv_update = match service.response_updates().is_empty() {
		true => quote! {
			/// This service call does not support update messages, so there is no way to retrieve it.
		},
		false => quote! {
			/// You can still receive the update message by calling [`Self::recv_update`].
			/// This function will keep returning an error until the update message is received.
		},
	};

	let struct_doc = format!("A sent request for the {} service.", service.name());

	item_tokens.extend(quote! {
		#[doc = #struct_doc]
		pub struct SentRequest<P: #fizyr_rpc::macros::Protocol> {
			pub(super) request: #fizyr_rpc::SentRequest<P::Body>,
		}

		impl<P: #fizyr_rpc::macros::Protocol> SentRequest<P> {
			/// Receive the final response.
			///
			/// If an update message is received instead of the final response an error is returned.
			/// The update message will remain in the message queue and must be read before the response can be received.
			#doc_recv_update
			pub async fn recv_response(&mut self) -> Result<#response_type, #fizyr_rpc::error::RecvMessageError>
			where
				#response_type: #fizyr_rpc::macros::Decode<P>,
			{
				let response = self.request.recv_response().await?;
				let decoded = P::decode_body(response.body).map_err(#fizyr_rpc::error::RecvMessageError::DecodeBody)?;
				Ok(decoded)
			}

			/// Get the raw request.
			pub fn inner(&self) -> &#fizyr_rpc::SentRequest<P::Body> {
				&self.request
			}

			/// Get an exclusive reference to the raw request.
			pub fn inner_mut(&self) -> &#fizyr_rpc::SentRequest<P::Body> {
				&self.request
			}

			/// Consume this object to get the raw request.
			pub fn into_inner(self) -> #fizyr_rpc::SentRequest<P::Body> {
				self.request
			}
		}
	});

	if !service.request_updates().is_empty() {
		let mut impl_tokens = TokenStream::new();

		generate_message_enum(
			item_tokens,
			fizyr_rpc,
			service.request_updates(),
			&syn::Ident::new("RequestUpdate", Span::call_site()),
			&format!("A request update for the {} service", service.name()),
		);
		quote! {
			/// Send a request update to the remote peer.
			pub async fn send_update(&self, update: #service_name::RequestUpdate) -> Result<(), #fizyr_rpc::error::SendUpdateError>
			where
				#service_name::RequestUpdate: #fizyr_rpc::macros::ToMessage<P>,
			{
				let (service_id, body) = P::encode_message(update).map_err(#fizyr_rpc::error::SendUpdateError::EncodeBody)?;
				self.request.send_update(service_id, body).await?;
				Ok(())
			}
		};

		for update in service.request_updates() {
			let function_name = syn::Ident::new(&format!("send_{}_update", update.name()), Span::call_site());
			let body_type = update.body_type();
			let service_id = update.service_id();
			let doc = format!("Send a {} update to the remote peer.", update.name());
			let body_arg;
			let body_val;
			if is_unit_type(body_type) {
				body_arg = None;
				body_val = quote!(());
			} else {
				body_arg = Some(quote!(update: #body_type));
				body_val = quote!(update);
			}
			impl_tokens.extend(quote! {
				#[doc = #doc]
				pub async fn #function_name(&self, #body_arg) -> Result<(), #fizyr_rpc::error::SendUpdateError>
				where
					#body_type: #fizyr_rpc::macros::Encode<P>,
				{
					let body = P::encode_body(#body_val).map_err(#fizyr_rpc::error::SendUpdateError::EncodeBody)?;
					self.request.send_update(#service_id, body).await?;
					Ok(())
				}
			})
		}

		item_tokens.extend(quote! {
			impl<P: #fizyr_rpc::macros::Protocol> SentRequest<P> {
				#impl_tokens
			}
		});
	}

	if !service.response_updates().is_empty() {
		generate_message_enum(
			item_tokens,
			fizyr_rpc,
			service.response_updates(),
			&syn::Ident::new("ResponseUpdate", Span::call_site()),
			&format!("A response update for the {} service", service.name()),
		);

		let mut impl_tokens = TokenStream::new();
		impl_tokens.extend(quote! {
			/// Receive a request update from the remote peer.
			///
			/// Once the final response is received,
			/// this function will keep returning `Ok(None)`.
			/// Use [`Self::recv_response`] to receive the response.
			pub async fn recv_update(&mut self, timeout: std::time::Duration) -> Result<Option<#service_name::ResponseUpdate>, #fizyr_rpc::error::RecvMessageError>
			where
				#service_name::ResponseUpdate: #fizyr_rpc::macros::FromMessage<P>,
			{
				use #fizyr_rpc::macros::FromMessage;
				let update = match self.request.recv_update().await? {
					Some(x) => x,
					None => return Ok(None),
				};
				Ok(Some(P::decode_message(update)?))
			}
		});

		for update in service.response_updates() {
			let function_name = syn::Ident::new(&format!("recv_{}_update", update.name()), Span::call_site());
			let body_type = update.body_type();
			let service_id = update.service_id();
			let doc = format!("Receive a {} update from the remote peer.", update.name());
			impl_tokens.extend(quote! {
				#[doc = #doc]
				///
				/// If the received message is a response message or a different update message, this function returns an error.
				/// The message will remain in the read queue and can still be received by another `recv_*` function.
				/// As long as the message is in the read queue, this function will keep returning an error.
				pub async fn #function_name(&mut self) -> Result<#body_type, #fizyr_rpc::error::RecvMessageError>
				where
					#body_type: #fizyr_rpc::macros::Decode<P>,
				{
					let update = match self.request.recv_update().await? {
						None => return Err(#fizyr_rpc::error::UnexpectedMessageType {
							value: #fizyr_rpc::MessageType::Response,
							expected: #fizyr_rpc::MessageType::ResponderUpdate,
						}.into()),
						Some(x) => x,
					};

					let actual_service_id = update.header.service_id;

					if actual_service_id != #service_id {
						// Put the message back in the read queue so that a different `recv_*` call can read it.
						self.request._unpeek_message(update);
						return Err(#fizyr_rpc::error::UnexpectedServiceId { actual_service_id }.into());
					}

					P::decode_body(update.body).map_err(#fizyr_rpc::error::RecvMessageError::DecodeBody)
				}
			})
		}

		item_tokens.extend(quote! {
			impl<P: #fizyr_rpc::macros::Protocol> SentRequest<P> {
				#impl_tokens
			}
		});
	}
}

/// Generate an enum with all possible body types for a message.
fn generate_message_enum(item_tokens: &mut TokenStream, fizyr_rpc: &syn::Ident, updates: &[UpdateDefinition], enum_name: &syn::Ident, enum_doc: &str) {
	let mut variants = TokenStream::new();
	let mut from_message = TokenStream::new();
	let mut to_message = TokenStream::new();
	let mut decode_all = TokenStream::new();
	let mut encode_all = TokenStream::new();
	for update in updates {
		let variant_name = to_upper_camel_case(&update.name().to_string());
		let variant_name = syn::Ident::new(&variant_name, update.name().span());
		let variant_doc = to_doc_attrs(update.doc());
		let body_type = update.body_type();

		let service_id = update.service_id();
		variants.extend(quote!{
			#variant_doc
			#variant_name(#body_type),
		});

		from_message.extend(quote! {
			#service_id => Ok(Self::#variant_name(P::decode_body(message.body).map_err(#fizyr_rpc::macros::error::FromMessageError::DecodeBody)?)),
		});

		decode_all.extend(quote!(
			#body_type: #fizyr_rpc::macros::Decode<P>,
		));

		to_message.extend(quote! {
			Self::#variant_name(update) => Ok((#service_id, P::encode_body(update)?)),
		});

		encode_all.extend(quote!(
			#body_type: #fizyr_rpc::macros::Encode<P>,
		));
	}

	item_tokens.extend(quote! {
		#[doc = #enum_doc]
		pub enum #enum_name {
			#variants
		}

		impl<P: #fizyr_rpc::macros::Protocol> #fizyr_rpc::macros::FromMessage<P> for #enum_name
		where
			#decode_all
		{
			fn from_message(message: #fizyr_rpc::Message<P::Body>) -> Result<Self, #fizyr_rpc::macros::error::FromMessageError> {
				match message.header.service_id {
					#from_message
					actual_service_id => Err(#fizyr_rpc::error::UnexpectedServiceId { actual_service_id }.into()),
				}
			}
		}

		impl<P: #fizyr_rpc::macros::Protocol> #fizyr_rpc::macros::ToMessage<P> for #enum_name
		where
			#encode_all
		{
			fn to_message(self) -> Result<(i32, P::Body), Box<dyn std::error::Error + Send>> {
				match self {
					#to_message
				}
			}
		}
	})
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
