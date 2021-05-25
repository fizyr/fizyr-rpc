use quote::quote;
use proc_macro2::{Span, TokenStream};

use super::parse::cooked::{InterfaceDefinition, ServiceDefinition, UpdateDefinition};

/// Generate a client struct for the given interface.
pub fn generate_interface(fizyr_rpc: &syn::Ident, interface: &InterfaceDefinition) -> TokenStream {
	let mut item_tokens = TokenStream::new();
	let mut client_impl_tokens = TokenStream::new();

	let interface_name = interface.name();
	let interface_doc = if interface.doc().is_empty() {
		let text = format!("Support types for the {} RPC interface.", interface.name());
		quote!(#[doc = #text])
	} else {
		to_doc_attrs(interface.doc())
	};

	generate_services(&mut item_tokens, &mut client_impl_tokens, fizyr_rpc, interface);
	generate_client(&mut item_tokens, fizyr_rpc, interface, client_impl_tokens);
	generate_server(&mut item_tokens, fizyr_rpc, interface);

	let tokens = quote! {
		#interface_doc
		pub mod #interface_name {
			use super::*;

			#item_tokens
		}
	};

	tokens
}

/// Generate a client struct.
///
/// `extra_impl` is used to add additional functions to the main `impl` block.
fn generate_client(item_tokens: &mut TokenStream, fizyr_rpc: &syn::Ident, interface: &InterfaceDefinition, extra_impl: TokenStream) {
	let client_doc = format!("RPC client for the {} interface.", interface.name());
	item_tokens.extend(quote! {
		#[doc = #client_doc]
		pub struct Client<P: #fizyr_rpc::macros::Protocol> {
			peer: #fizyr_rpc::PeerWriteHandle<P::Body>,
		}

		impl<P: #fizyr_rpc::macros::Protocol> Client<P> {
			/// Create a new interface-specific RPC client from a raw write handle.
			pub fn new(peer: #fizyr_rpc::PeerWriteHandle<P::Body>) -> Self {
				Self { peer }
			}

			/// Connect to a remote server.
			///
			/// See [`fizyr_rpc::Peer::connect`](https://docs.rs/fizyr-rpc/latest/fizyr_rpc/struct.Peer.html#method.connect) for more details.
			pub async fn connect<'a, Transport, Address>(address: Address, config: Transport::Config) -> std::io::Result<Self>
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

			#extra_impl
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
	})
}

/// Generate a server struct.
///
/// `extra_impl` is used to add additional functions to the main `impl` block.
fn generate_server(item_tokens: &mut TokenStream, fizyr_rpc: &syn::Ident, interface: &InterfaceDefinition) {
	let unit_type = make_unit_type();

	// Where clause for the `recv_message` function.
	let mut recv_message_where = TokenStream::new();
	// Match arms for decoding a request message.
	let mut decode_request_arms = TokenStream::new();

	for service in interface.services() {
		let service_id = service.service_id();
		let service_name = service.name();
		let variant_name = syn::Ident::new(&to_upper_camel_case(&service_name.to_string()), Span::call_site());
		let request_type = service.request_type().unwrap_or(&unit_type);
		recv_message_where.extend(quote! {
			#request_type: #fizyr_rpc::macros::Decode<P>,
		});
		decode_request_arms.extend(quote! {
			#service_id =>  {
				let body = P::decode_body(body).map_err(#fizyr_rpc::error::RecvMessageError::DecodeBody)?;
				let request = #service_name::ReceivedRequest { request };
				Ok(Incoming::Request(ReceivedRequest::#variant_name(request, body)))
			},
		});
	}

	let server_doc = format!("RPC server for the {} interface.", interface.name());
	item_tokens.extend(quote! {
		#[doc = #server_doc]
		pub struct Server<P: #fizyr_rpc::macros::Protocol> {
			peer: #fizyr_rpc::PeerReadHandle<P::Body>,
		}

		impl<P: #fizyr_rpc::macros::Protocol> Server<P> {
			/// Create a new interface-specific RPC server from a raw write handle.
			fn new(peer: #fizyr_rpc::PeerReadHandle<P::Body>) -> Self {
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
			pub fn close_handle(&self) -> #fizyr_rpc::PeerCloseHandle<P::Body> {
				self.peer.close_handle()
			}

			/// Receive the next incoming message.
			pub async fn recv_message(&mut self) -> Result<Incoming<P>, #fizyr_rpc::error::RecvMessageError>
			where
				#recv_message_where
			{
				match self.peer.recv_message().await? {
					#fizyr_rpc::Incoming::Stream(x) => Ok(Incoming::Stream(x)),
					#fizyr_rpc::Incoming::Request(request, body) => {
						match request.service_id() {
							#decode_request_arms
							service_id => Err(#fizyr_rpc::error::UnexpectedServiceId { service_id }.into())
						}
					},
				}
			}
		}

		impl<P: #fizyr_rpc::macros::Protocol> ::core::convert::From<#fizyr_rpc::PeerReadHandle<P::Body>> for Server<P> {
			fn from(other: #fizyr_rpc::PeerReadHandle<P::Body>) -> Self {
				Self::new(other)
			}
		}

		impl<P: #fizyr_rpc::macros::Protocol> ::core::convert::From<#fizyr_rpc::PeerHandle<P::Body>> for Server<P> {
			fn from(other: #fizyr_rpc::PeerHandle<P::Body>) -> Self {
				let (read, _write) = other.split();
				Self::new(read)
			}
		}

		/// An incoming message from a remote peer.
		pub enum Incoming<P: #fizyr_rpc::macros::Protocol> {
			/// A raw streaming message.
			Stream(#fizyr_rpc::Message<P::Body>),

			/// A request message.
			Request(ReceivedRequest<P>),
		}
	});

	generate_received_request_enum(item_tokens, fizyr_rpc, interface);
}

/// Generate the support types and function definitions for each service.
fn generate_services(item_tokens: &mut TokenStream, client_impl_tokens: &mut TokenStream, fizyr_rpc: &syn::Ident, interface: &InterfaceDefinition) {
	for service in interface.services() {
		generate_service(item_tokens, client_impl_tokens, fizyr_rpc, service);
	}
}

/// Generate the support types and function definitions for each service.
fn generate_service(item_tokens: &mut TokenStream, client_impl_tokens: &mut TokenStream, fizyr_rpc: &syn::Ident, service: &ServiceDefinition) {
	let unit_type = make_unit_type();
	let service_name = service.name();
	let service_doc = to_doc_attrs(&service.doc());
	let service_id = service.service_id();

	let request_param = service.request_type().map(|x| quote!(request: #x));
	let request_type = service.request_type().unwrap_or(&unit_type);
	let request_body = if service.request_type().is_some() {
		quote!(P::encode_body(request))
	} else {
		quote!(P::encode_body(()))
	};

	let response_type = service.response_type().unwrap_or(&unit_type);
	let mut service_item_tokens = TokenStream::new();

	// Service without updates, so directly return the response (asynchronously).
	if service.request_updates().is_empty() && service.response_updates().is_empty() {
		client_impl_tokens.extend(quote! {
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
		generate_sent_request(&mut service_item_tokens, &fizyr_rpc, service);
		client_impl_tokens.extend(quote! {
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

	}

	generate_received_request(&mut service_item_tokens, fizyr_rpc, service);

	let mod_doc = format!("Support types for the {} service.", service.name());
	item_tokens.extend(quote! {
		#[doc = #mod_doc]
		pub mod #service_name {
			use super::*;
			#service_item_tokens
		}
	});
}

/// Generate a type for the sent request for a specific service.
///
/// Only used for service calls that have update messages.
/// Otherwise, the return type of a service call will simply be the response message.
fn generate_sent_request(item_tokens: &mut TokenStream, fizyr_rpc: &syn::Ident, service: &ServiceDefinition) {
	let unit_type = make_unit_type();
	let service_name = service.name();
	let response_type = service.response_type().unwrap_or(&unit_type);

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
		generate_send_update_functions(&mut impl_tokens, fizyr_rpc, &quote!(#service_name::RequestUpdate), service.request_updates());

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
		generate_recv_update_functions(&mut impl_tokens, fizyr_rpc, &quote!(#service_name::ResponseUpdate), service.response_updates(), UpdateKind::ResponseUpdate);

		item_tokens.extend(quote! {
			impl<P: #fizyr_rpc::macros::Protocol> SentRequest<P> {
				#impl_tokens
			}
		});
	}
}

#[derive(Debug, Eq, PartialEq)]
enum UpdateKind {
	RequestUpdate,
	ResponseUpdate,
}

fn generate_send_update_functions(impl_tokens: &mut TokenStream, fizyr_rpc: &syn::Ident, enum_type: &TokenStream, updates: &[UpdateDefinition]) {
	quote! {
		/// Send a request update to the remote peer.
		pub async fn send_update(&self, update: #enum_type) -> Result<(), #fizyr_rpc::error::SendUpdateError>
		where
			#enum_type: #fizyr_rpc::macros::ToMessage<P>,
		{
			let (service_id, body) = P::encode_message(update).map_err(#fizyr_rpc::error::SendUpdateError::EncodeBody)?;
			self.request.send_update(service_id, body).await?;
			Ok(())
		}
	};

	for update in updates {
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
}

fn generate_recv_update_functions(impl_tokens: &mut TokenStream, fizyr_rpc: &syn::Ident, enum_type: &TokenStream, updates: &[UpdateDefinition], kind: UpdateKind) {
	let mut doc = quote! {
		/// Receive an update from the remote peer.
	};
	if kind == UpdateKind::ResponseUpdate {
		doc.extend(quote! {
			///
			/// Once the final response is received,
			/// this function will keep returning `Ok(None)`.
			/// Use [`Self::recv_response`] to receive the response.
		})
	}

	impl_tokens.extend(quote! {
		#doc
		pub async fn recv_update(&mut self, timeout: std::time::Duration) -> Result<Option<#enum_type>, #fizyr_rpc::error::RecvMessageError>
		where
			#enum_type: #fizyr_rpc::macros::FromMessage<P>,
		{
			use #fizyr_rpc::macros::FromMessage;
			let update = match self.request.recv_update().await? {
				Some(x) => x,
				None => return Ok(None),
			};
			Ok(Some(P::decode_message(update)?))
		}
	});

	for update in updates {
		let function_name = syn::Ident::new(&format!("recv_{}_update", update.name()), Span::call_site());
		let body_type = update.body_type();
		let service_id = update.service_id();
		let doc = format!("Receive a {} update from the remote peer.", update.name());
		let mut doc = quote!(#[doc = #doc]);
		doc.extend(match kind {
			UpdateKind::ResponseUpdate => quote! {
				///
				/// If the received message is a response message or a different update message, this function returns an error.
				/// The message will remain in the read queue and can still be received by another `recv_*` function.
				/// As long as the message is in the read queue, this function will keep returning an error.
			},
			UpdateKind::RequestUpdate => quote! {
				///
				/// If the received message is a different update message, this function returns an error.
				/// The message will remain in the read queue and can still be received by another `recv_*` function.
				/// As long as the message is in the read queue, this function will keep returning an error.
			},
		});

		impl_tokens.extend(quote! {
			#doc
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

				let service_id = update.header.service_id;
				if service_id != #service_id {
					// Put the message back in the read queue so that a different `recv_*` call can read it.
					self.request._unpeek_message(update);
					return Err(#fizyr_rpc::error::UnexpectedServiceId { service_id }.into());
				}

				P::decode_body(update.body).map_err(#fizyr_rpc::error::RecvMessageError::DecodeBody)
			}
		})
	}
}

/// Generate an enum with all possible body types for a message.
fn generate_message_enum(item_tokens: &mut TokenStream, fizyr_rpc: &syn::Ident, updates: &[UpdateDefinition], enum_name: &syn::Ident, enum_doc: &str) {
	let mut variants = TokenStream::new();
	let mut from_message = TokenStream::new();
	let mut into_message = TokenStream::new();
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

		into_message.extend(quote! {
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
					service_id => Err(#fizyr_rpc::error::UnexpectedServiceId { service_id }.into()),
				}
			}
		}

		impl<P: #fizyr_rpc::macros::Protocol> #fizyr_rpc::macros::IntoMessage<P> for #enum_name
		where
			#encode_all
		{
			fn into_message(self) -> Result<(i32, P::Body), Box<dyn std::error::Error + Send>> {
				match self {
					#into_message
				}
			}
		}
	})
}

fn generate_received_request(item_tokens: &mut TokenStream, fizyr_rpc: &syn::Ident, service: &ServiceDefinition) {
	let unit_type = make_unit_type();
	let response_type = service.response_type().unwrap_or(&unit_type);
	let service_name = service.name();
	let service_id = service.service_id();

	let mut impl_tokens = TokenStream::new();

	if !service.request_updates().is_empty() {
		generate_send_update_functions(&mut impl_tokens, fizyr_rpc, &quote!(#service_name::RequestUpdate), service.response_updates());

		item_tokens.extend(quote! {
			impl<P: #fizyr_rpc::macros::Protocol> SentRequest<P> {
				#impl_tokens
			}
		});
	}

	if !service.response_updates().is_empty() {
		generate_recv_update_functions(&mut impl_tokens, fizyr_rpc, &quote!(#service_name::ResponseUpdate), service.request_updates(), UpdateKind::RequestUpdate);
	}

	item_tokens.extend(quote! {
		pub struct ReceivedRequest<P: #fizyr_rpc::macros::Protocol> {
			pub(super) request: #fizyr_rpc::ReceivedRequest<P::Body>,
		}

		impl<P: #fizyr_rpc::macros::Protocol> ReceivedRequest<P> {
			/// Get the raw request.
			///
			/// Note that the request body has been consumed when it was parsed.
			/// As a result, the raw request always has an empty body.
			pub fn inner(&self) -> &#fizyr_rpc::ReceivedRequest<P::Body> {
				&self.request
			}

			/// Get an exclusive reference to the raw request.
			///
			/// Note that the request body has been consumed when it was parsed.
			/// As a result, the raw request always has an empty body.
			pub fn inner_mut(&self) -> &#fizyr_rpc::ReceivedRequest<P::Body> {
				&self.request
			}

			/// Consume this object to get the raw request.
			///
			/// Note that the request body has been consumed when it was parsed.
			/// As a result, the raw request always has an empty body.
			pub fn into_inner(self) -> #fizyr_rpc::ReceivedRequest<P::Body> {
				self.request
			}

			/// Get the request ID.
			pub fn request_id(&self) -> u32 {
				self.request.request_id()
			}

			/// Get the service ID of the request.
			pub fn service_id(&self) -> i32 {
				self.request.service_id()
			}

			/// Send the final response.
			pub async fn send_response(self, response: #response_type) -> Result<(), #fizyr_rpc::error::SendUpdateError>
			where
				#response_type: #fizyr_rpc::macros::Encode<P>,
			{
				let encoded = P::encode_body(response).map_err(#fizyr_rpc::error::SendUpdateError::EncodeBody)?;
				let response = self.request.send_response(#service_id, encoded).await?;
				Ok(())
			}

			//#impl_tokens
		}
	})
}

/// Generate an enum for all possible received requests for a server.
fn generate_received_request_enum(item_tokens: &mut TokenStream, fizyr_rpc: &syn::Ident, interface: &InterfaceDefinition) {
	let unit_type = make_unit_type();
	let mut variant_tokens = TokenStream::new();
	for service in interface.services() {
		let service_name = service.name();
		let variant_name = syn::Ident::new(&to_upper_camel_case(&service_name.to_string()), Span::call_site());
		let request_type = service.request_type().unwrap_or(&unit_type);
		let doc = to_doc_attrs(service.doc());
		variant_tokens.extend(quote! {
			#doc
			#variant_name(#service_name::ReceivedRequest<P>, #request_type),
		})
	}

	let enum_doc = format!("Enum for all possible incoming requests of the {} interface.", interface.name());
	item_tokens.extend(quote! {
		#[doc = #enum_doc]
		pub enum ReceivedRequest<P: #fizyr_rpc::macros::Protocol> {
			#variant_tokens
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

fn make_unit_type() -> syn::Type {
	syn::Type::Tuple(
		syn::TypeTuple {
			paren_token: syn::token::Paren(Span::call_site()),
			elems: syn::punctuated::Punctuated::new(),
		}
	)
}
