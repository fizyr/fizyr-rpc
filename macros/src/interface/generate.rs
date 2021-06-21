use quote::quote;
use proc_macro2::{Span, TokenStream};

use super::parse::cooked::{InterfaceDefinition, ServiceDefinition, StreamDefinition, UpdateDefinition};
use crate::util::WithSpan;

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
	generate_streams(&mut item_tokens, &mut client_impl_tokens, fizyr_rpc, interface);
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
		pub struct Client<F: #fizyr_rpc::util::format::Format> {
			peer: #fizyr_rpc::PeerWriteHandle<F::Body>,
		}

		impl<F: #fizyr_rpc::util::format::Format> ::core::fmt::Debug for Client<F> {
			fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
				f.debug_struct(::core::any::type_name::<Self>())
					.field("peer", &self.peer)
					.finish()
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

			/// Connect to a remote server.
			///
			/// See [`fizyr_rpc::Peer::connect`](https://docs.rs/fizyr-rpc/latest/fizyr_rpc/struct.Peer.html#method.connect) for more details.
			pub async fn connect<'a, Transport, Address>(address: Address, config: Transport::Config) -> std::io::Result<Self>
			where
				Address: 'a,
				Transport: #fizyr_rpc::transport::Transport<Body = F::Body> + #fizyr_rpc::util::Connect<'a, Address>,
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
			pub fn close_handle(&self) -> #fizyr_rpc::PeerCloseHandle<F::Body> {
				self.peer.close_handle()
			}

			#extra_impl
		}
	})
}

/// Generate a server struct.
///
/// `extra_impl` is used to add additional functions to the main `impl` block.
fn generate_server(item_tokens: &mut TokenStream, fizyr_rpc: &syn::Ident, interface: &InterfaceDefinition) {
	// Generic parameters for the `ReceivedMessage` enum.
	let mut received_msg_generics = TokenStream::new();
	// Where clauses for the ReceivedMessage struct.
	let mut received_msg_where = TokenStream::new();
	// Variants for the `ReceivedMessage` enum.
	let mut received_msg_variants = TokenStream::new();
	// Match arms for the `ReceivedMessage` debug implementation.
	let mut received_msg_debug_arms = TokenStream::new();
	// Where clause for the `recv_message` function.
	let mut recv_message_where = TokenStream::new();
	// Match arms for decoding a request message.
	let mut decode_request_arms = TokenStream::new();
	// Match arms for the receive_msg function.
	let mut recv_message_arms = TokenStream::new();

	if !interface.streams().is_empty() {
		recv_message_where.extend(quote! {
			StreamMessage: #fizyr_rpc::util::format::FromMessage<F>,
		});
		received_msg_variants.extend(quote! {
			/// A stream message.
			Stream(StreamMessage),
		});
		received_msg_debug_arms.extend(quote! {
			Self::Stream(x) => ::core::write!(f, "Stream({:?})", x),
		});
		recv_message_arms.extend(quote! {
			#fizyr_rpc::ReceivedMessage::Stream(raw) => {
				Ok(ReceivedMessage::Stream(F::decode_message(raw)?))
			},
		});
	} else {
		recv_message_arms.extend(quote! {
			#fizyr_rpc::ReceivedMessage::Stream(raw) => {
				let service_id = raw.header.service_id;
				Err(#fizyr_rpc::error::UnexpectedServiceId { service_id }.into())
			},
		});
	}

	for service in interface.services() {
		let service_id = service.service_id();
		let service_name = service.name();
		let variant_name = syn::Ident::new(&to_upper_camel_case(&service_name.to_string()), Span::call_site());
		let request_type = service.request_type();
		recv_message_where.extend(quote! {
			F: #fizyr_rpc::util::format::DecodeBody<#request_type>,
		});
		decode_request_arms.extend(quote! {
			#service_id =>  {
				let body = F::decode_body(body).map_err(#fizyr_rpc::error::RecvMessageError::DecodeBody)?;
				let request = #service_name::ReceivedRequest { request };
				Ok(ReceivedMessage::Request(ReceivedRequest::#variant_name(request, body)))
			},
		});
	}

	if !interface.services().is_empty() {
		received_msg_generics.extend(quote!(F));
		received_msg_where.extend(quote! {
			F: #fizyr_rpc::util::format::Format,
		});
		received_msg_variants.extend(quote! {
			/// A request message.
			Request(ReceivedRequest<F>),
		});
		received_msg_debug_arms.extend(quote! {
			Self::Request(x) => ::core::write!(f, "Request({:?})", x),
		});
		recv_message_arms.extend(quote! {
			#fizyr_rpc::ReceivedMessage::Request(request, body) => {
				match request.service_id() {
					#decode_request_arms
					service_id => Err(#fizyr_rpc::error::UnexpectedServiceId { service_id }.into())
				}
			},
		});
	} else {
		recv_message_arms.extend(quote! {
			#fizyr_rpc::ReceivedMessage::Request(request, body) => {
				let service_id = request.service_id();
				Err(#fizyr_rpc::error::UnexpectedServiceId { service_id }.into())
			},
		});
	}

	let server_doc = format!("RPC server for the {} interface.", interface.name());
	item_tokens.extend(quote! {
		#[doc = #server_doc]
		pub struct Server<F: #fizyr_rpc::util::format::Format> {
			peer: #fizyr_rpc::PeerReadHandle<F::Body>,
		}

		impl<F: #fizyr_rpc::util::format::Format> ::core::fmt::Debug for Server<F> {
			fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
				f.debug_struct(::core::any::type_name::<Self>())
					.field("peer", &self.peer)
					.finish()
			}
		}

		impl<F: #fizyr_rpc::util::format::Format> Server<F> {
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
			pub async fn recv_message(&mut self) -> Result<ReceivedMessage<#received_msg_generics>, #fizyr_rpc::error::RecvMessageError>
			where
				#recv_message_where
			{
				match self.peer.recv_message().await? {
					#recv_message_arms
				}
			}
		}

		impl<F: #fizyr_rpc::util::format::Format> ::core::convert::From<#fizyr_rpc::PeerReadHandle<F::Body>> for Server<F> {
			fn from(other: #fizyr_rpc::PeerReadHandle<F::Body>) -> Self {
				Self::new(other)
			}
		}

		impl<F: #fizyr_rpc::util::format::Format> ::core::convert::From<#fizyr_rpc::PeerHandle<F::Body>> for Server<F> {
			fn from(other: #fizyr_rpc::PeerHandle<F::Body>) -> Self {
				let (read, _write) = other.split();
				Self::new(read)
			}
		}

		/// An incoming message from a remote peer.
		pub enum ReceivedMessage<#received_msg_generics>
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

/// Generate the support types and function definitions for each service.
fn generate_services(item_tokens: &mut TokenStream, client_impl_tokens: &mut TokenStream, fizyr_rpc: &syn::Ident, interface: &InterfaceDefinition) {
	for service in interface.services() {
		generate_service(item_tokens, client_impl_tokens, fizyr_rpc, service);
	}
}

/// Generate the support types and function definitions for each service.
fn generate_service(item_tokens: &mut TokenStream, client_impl_tokens: &mut TokenStream, fizyr_rpc: &syn::Ident, service: &ServiceDefinition) {
	let service_name = service.name();
	let service_doc = to_doc_attrs(&service.doc());
	let service_id = service.service_id();

	let request_type = service.request_type();
	let request_param;
	let request_body;
	if is_unit_type(request_type) {
		request_param = None;
		request_body = quote!(F::encode_body(()))
	} else {
		request_param = Some(quote!(request: #request_type));
		request_body = quote!(F::encode_body(request))
	}

	let response_type = service.response_type();
	let mut service_item_tokens = TokenStream::new();

	// Service without updates, so directly return the response (asynchronously).
	if service.request_updates().is_empty() && service.response_updates().is_empty() {
		client_impl_tokens.extend(quote! {
			#service_doc
			pub async fn #service_name(&self, #request_param) -> Result<#response_type, #fizyr_rpc::error::ServiceCallError>
			where
				F: #fizyr_rpc::util::format::EncodeBody<#request_type>,
				F: #fizyr_rpc::util::format::DecodeBody<#response_type>,
			{
				let request_body = #request_body.map_err(#fizyr_rpc::error::SendRequestError::EncodeBody)?;
				let mut request = self.peer.send_request(#service_id, request_body).await?;
				let response = request.recv_response().await?;
				let decoded = F::decode_body(response.body).map_err(#fizyr_rpc::error::RecvMessageError::DecodeBody)?;
				Ok(decoded)
			}
		})
	} else {
		generate_sent_request(&mut service_item_tokens, &fizyr_rpc, service);
		client_impl_tokens.extend(quote! {
			#service_doc
			pub async fn #service_name(&self, #request_param) -> Result<#service_name::SentRequest<F>, #fizyr_rpc::error::SendRequestError>
			where
				F: #fizyr_rpc::util::format::EncodeBody<#request_type>,
				F: #fizyr_rpc::util::format::DecodeBody<#response_type>,
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
	let service_name = service.name();
	let mut impl_tokens = TokenStream::new();

	if !service.request_updates().is_empty() {
		generate_message_enum(
			item_tokens,
			fizyr_rpc,
			service.request_updates(),
			&syn::Ident::new("RequestUpdate", Span::call_site()),
			&format!("A request update for the {} service", service.name()),
		);
		generate_send_update_functions(&mut impl_tokens, fizyr_rpc, &quote!(#service_name::RequestUpdate), service.request_updates());
	}

	if !service.response_updates().is_empty() {
		generate_message_enum(
			item_tokens,
			fizyr_rpc,
			service.response_updates(),
			&syn::Ident::new("ResponseUpdate", Span::call_site()),
			&format!("A response update for the {} service", service.name()),
		);
		generate_recv_update_functions(&mut impl_tokens, fizyr_rpc, &quote!(#service_name::ResponseUpdate), UpdateKind::ResponseUpdate);
	}

	let response_type = service.response_type();
	let struct_doc = format!("A sent request for the {} service.", service.name());
	let doc_recv_update = match service.response_updates().is_empty() {
		true => quote! {
			/// This service call does not support update messages, so there is no way to retrieve it.
		},
		false => quote! {
			/// You can still receive the update message by calling [`Self::recv_update`].
			/// This function will keep returning an error until the update message is received.
		},
	};

	item_tokens.extend(quote! {
		#[doc = #struct_doc]
		pub struct SentRequest<F: #fizyr_rpc::util::format::Format> {
			pub(super) request: #fizyr_rpc::SentRequest<F::Body>,
		}

		impl<F: #fizyr_rpc::util::format::Format> ::core::fmt::Debug for SentRequest<F> {
			fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
				f.debug_struct(::core::any::type_name::<Self>())
					.field("request_id", &self.request_id())
					.field("service_id", &self.service_id())
					// TODO: use finish_non_exhaustive when it hits stable
					.finish()
			}
		}

		impl<F: #fizyr_rpc::util::format::Format> SentRequest<F> {
			/// Receive the final response.
			///
			/// If an update message is received instead of the final response an error is returned.
			/// The update message will remain in the message queue and must be read before the response can be received.
			#doc_recv_update
			pub async fn recv_response(&mut self) -> Result<#response_type, #fizyr_rpc::error::RecvMessageError>
			where
				F: #fizyr_rpc::util::format::DecodeBody<#response_type>,
			{
				let response = self.request.recv_response().await?;
				let decoded = F::decode_body(response.body).map_err(#fizyr_rpc::error::RecvMessageError::DecodeBody)?;
				Ok(decoded)
			}

			/// Get the raw request.
			pub fn inner(&self) -> &#fizyr_rpc::SentRequest<F::Body> {
				&self.request
			}

			/// Get an exclusive reference to the raw request.
			pub fn inner_mut(&self) -> &#fizyr_rpc::SentRequest<F::Body> {
				&self.request
			}

			/// Consume this object to get the raw request.
			pub fn into_inner(self) -> #fizyr_rpc::SentRequest<F::Body> {
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

			#impl_tokens
		}
	});
}

#[derive(Debug, Eq, PartialEq)]
enum UpdateKind {
	RequestUpdate,
	ResponseUpdate,
}

fn generate_send_update_functions(impl_tokens: &mut TokenStream, fizyr_rpc: &syn::Ident, enum_type: &TokenStream, updates: &[UpdateDefinition]) {
	quote! {
		/// Send a request update to the remote peer.
		pub async fn send_update(&self, update: #enum_type) -> Result<(), #fizyr_rpc::error::SendMessageError>
		where
			#enum_type: #fizyr_rpc::util::format::ToMessage<F>,
		{
			let (service_id, body) = F::encode_message(update).map_err(#fizyr_rpc::error::SendMessageError::EncodeBody)?;
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
			pub async fn #function_name(&self, #body_arg) -> Result<(), #fizyr_rpc::error::SendMessageError>
			where
				F: #fizyr_rpc::util::format::EncodeBody<#body_type>,
			{
				let body = F::encode_body(#body_val).map_err(#fizyr_rpc::error::SendMessageError::EncodeBody)?;
				self.request.send_update(#service_id, body).await?;
				Ok(())
			}
		})
	}
}

fn generate_recv_update_functions(impl_tokens: &mut TokenStream, fizyr_rpc: &syn::Ident, enum_type: &TokenStream, kind: UpdateKind) {
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
		pub async fn recv_update(&mut self) -> Result<Option<#enum_type>, #fizyr_rpc::error::RecvMessageError>
		where
			#enum_type: #fizyr_rpc::util::format::FromMessage<F>,
		{
			use #fizyr_rpc::util::format::FromMessage;
			match self.request.recv_update().await {
				Some(x) => Ok(Some(F::decode_message(x)?)),
				None => Ok(None),
			}
		}
	});
}

fn generate_streams(item_tokens: &mut TokenStream, client_impl_tokens: &mut TokenStream, fizyr_rpc: &syn::Ident, interface: &InterfaceDefinition) {
	if !interface.streams().is_empty() {
		generate_message_enum(
			item_tokens,
			fizyr_rpc,
			&interface.streams(),
			&syn::Ident::new("StreamMessage", Span::call_site()),
			&format!("A stream message for the {} interface.", interface.name()),
		);
	}
	for stream in interface.streams() {
		let service_id = stream.service_id();
		let fn_name = syn::Ident::new(&format!("send_{}", stream.name()), Span::call_site());
		let fn_doc = format!("Send a {} stream message to the remote peer.", stream.name());
		let body_arg;
		let body_val;
		let body_type = stream.body_type();
		if is_unit_type(body_type) {
			body_arg = None;
			body_val = quote!(());
		} else {
			body_arg = Some(quote!(body: #body_type));
			body_val = quote!(body);
		}
		client_impl_tokens.extend(quote! {
			#[doc = #fn_doc]
			pub async fn #fn_name(&self, #body_arg) -> Result<(), #fizyr_rpc::error::SendMessageError>
			where
				F: #fizyr_rpc::util::format::EncodeBody<#body_type>,
			{
				let encoded = F::encode_body(#body_val).map_err(#fizyr_rpc::error::SendMessageError::EncodeBody)?;
				self.peer.send_stream(#service_id, encoded).await?;
				Ok(())
			}
		})
	}
}

trait MessageDefinition {
	fn service_id(&self) -> &WithSpan<i32>;
	fn name(&self) -> &syn::Ident;
	fn doc(&self) -> &[WithSpan<String>];
	fn body_type(&self) -> &syn::Type;
}

impl MessageDefinition for UpdateDefinition {
	fn service_id(&self) -> &WithSpan<i32> {
		self.service_id()
	}

	fn name(&self) -> &syn::Ident {
		self.name()
	}

	fn doc(&self) -> &[WithSpan<String>] {
		self.doc()
	}

	fn body_type(&self) -> &syn::Type {
		self.body_type()
	}
}

impl MessageDefinition for StreamDefinition {
	fn service_id(&self) -> &WithSpan<i32> {
		self.service_id()
	}

	fn name(&self) -> &syn::Ident {
		self.name()
	}

	fn doc(&self) -> &[WithSpan<String>] {
		self.doc()
	}

	fn body_type(&self) -> &syn::Type {
		self.body_type()
	}
}

/// Generate an enum with all possible body types for a message.
fn generate_message_enum(item_tokens: &mut TokenStream, fizyr_rpc: &syn::Ident, messages: &[impl MessageDefinition], enum_name: &syn::Ident, enum_doc: &str) {
	let mut variants = TokenStream::new();
	let mut from_message = TokenStream::new();
	let mut into_message = TokenStream::new();
	let mut service_id_arms = TokenStream::new();
	let mut decode_all = TokenStream::new();
	let mut encode_all = TokenStream::new();
	let mut impl_tokens = TokenStream::new();
	for message in messages {
		let variant_name = to_upper_camel_case(&message.name().to_string());
		let variant_name = syn::Ident::new(&variant_name, message.name().span());
		let variant_doc = to_doc_attrs(message.doc());
		let body_type = message.body_type();

		let service_id = message.service_id();
		variants.extend(quote! {
			#variant_doc
			#variant_name(#body_type),
		});

		from_message.extend(quote! {
			#service_id => Ok(Self::#variant_name(F::decode_body(message.body).map_err(#fizyr_rpc::error::FromMessageError::DecodeBody)?)),
		});

		decode_all.extend(quote! {
			F: #fizyr_rpc::util::format::DecodeBody<#body_type>,
		});

		into_message.extend(quote! {
			Self::#variant_name(message) => Ok((#service_id, F::encode_body(message)?)),
		});

		service_id_arms.extend(quote! {
			Self::#variant_name(_) => #service_id,
		});

		encode_all.extend(quote! {
			F: #fizyr_rpc::util::format::EncodeBody<#body_type>,
		});

		let is_fn_name = syn::Ident::new(&format!("is_{}", message.name()), Span::call_site());
		let is_fn_doc = format!("Check if the message is a [`Self::{}`].", variant_name);

		let as_fn_name = syn::Ident::new(&format!("as_{}", message.name()), Span::call_site());
		let as_fn_doc = format!("Get the message as [`Self::{}`] by reference.", variant_name);

		let into_fn_name = syn::Ident::new(&format!("into_{}", message.name()), Span::call_site());
		let into_fn_doc = format!("Get the message as [`Self::{}`] by value.", variant_name);

		impl_tokens.extend(quote! {
			#[doc = #is_fn_doc]
			pub fn #is_fn_name(&self) -> bool {
				if let Self::#variant_name(_) = self {
					true
				} else {
					false
				}
			}

			#[doc = #as_fn_doc]
			pub fn #as_fn_name(&self) -> Option<&#body_type> {
				if let Self::#variant_name(x) = self {
					Some(x)
				} else {
					None
				}
			}

			#[doc = #into_fn_doc]
			pub fn #into_fn_name(self) -> Result<#body_type, #fizyr_rpc::error::UnexpectedServiceId> {
				let service_id = self.service_id();
				if let Self::#variant_name(x) = self {
					Ok(x)
				} else {
					Err(#fizyr_rpc::error::UnexpectedServiceId { service_id })
				}
			}
		})
	}

	item_tokens.extend(quote! {
		#[doc = #enum_doc]
		#[derive(Debug)]
		pub enum #enum_name {
			#variants
		}

		impl #enum_name {
			/// Get the service ID of the message.
			fn service_id(&self) -> i32 {
				match self {
					#service_id_arms
				}
			}

			#impl_tokens
		}

		impl<F: #fizyr_rpc::util::format::Format> #fizyr_rpc::util::format::FromMessage<F> for #enum_name
		where
			#decode_all
		{
			fn from_message(message: #fizyr_rpc::Message<F::Body>) -> Result<Self, #fizyr_rpc::error::FromMessageError> {
				match message.header.service_id {
					#from_message
					service_id => Err(#fizyr_rpc::error::UnexpectedServiceId { service_id }.into()),
				}
			}
		}

		impl<F: #fizyr_rpc::util::format::Format> #fizyr_rpc::util::format::IntoMessage<F> for #enum_name
		where
			#encode_all
		{
			fn into_message(self) -> Result<(i32, F::Body), Box<dyn std::error::Error + Send>> {
				match self {
					#into_message
				}
			}
		}
	})
}

fn generate_received_request(item_tokens: &mut TokenStream, fizyr_rpc: &syn::Ident, service: &ServiceDefinition) {
	let response_type = service.response_type();
	let service_name = service.name();
	let service_id = service.service_id();

	let mut impl_tokens = TokenStream::new();
	if !service.response_updates().is_empty() {
		generate_send_update_functions(&mut impl_tokens, fizyr_rpc, &quote!(#service_name::ResponseUpdate), service.response_updates());
	}
	if !service.request_updates().is_empty() {
		generate_recv_update_functions(&mut impl_tokens, fizyr_rpc, &quote!(#service_name::RequestUpdate), UpdateKind::RequestUpdate);
	}

	item_tokens.extend(quote! {
		pub struct ReceivedRequest<F: #fizyr_rpc::util::format::Format> {
			pub(super) request: #fizyr_rpc::ReceivedRequest<F::Body>,
		}

		impl<F: #fizyr_rpc::util::format::Format> ::core::fmt::Debug for ReceivedRequest<F> {
			fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
				f.debug_struct(::core::any::type_name::<Self>())
					.field("request_id", &self.request_id())
					.field("service_id", &self.service_id())
					// TODO: use finish_non_exhaustive when it hits stable
					.finish()
			}
		}

		impl<F: #fizyr_rpc::util::format::Format> ReceivedRequest<F> {
			/// Get the raw request.
			///
			/// Note that the request body has been consumed when it was parsed.
			/// As a result, the raw request always has an empty body.
			pub fn inner(&self) -> &#fizyr_rpc::ReceivedRequest<F::Body> {
				&self.request
			}

			/// Get an exclusive reference to the raw request.
			///
			/// Note that the request body has been consumed when it was parsed.
			/// As a result, the raw request always has an empty body.
			pub fn inner_mut(&self) -> &#fizyr_rpc::ReceivedRequest<F::Body> {
				&self.request
			}

			/// Consume this object to get the raw request.
			///
			/// Note that the request body has been consumed when it was parsed.
			/// As a result, the raw request always has an empty body.
			pub fn into_inner(self) -> #fizyr_rpc::ReceivedRequest<F::Body> {
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
			pub async fn send_response(self, response: #response_type) -> Result<(), #fizyr_rpc::error::SendMessageError>
			where
				F: #fizyr_rpc::util::format::EncodeBody<#response_type>,
			{
				let encoded = F::encode_body(response).map_err(#fizyr_rpc::error::SendMessageError::EncodeBody)?;
				let response = self.request.send_response(#service_id, encoded).await?;
				Ok(())
			}

			#impl_tokens
		}
	})
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
			#variant_name(#service_name::ReceivedRequest<F>, #request_type),
		});
		debug_tokens.extend(quote! {
			Self::#variant_name(request, _body) => ::core::write!(f, "{}({:?})", #variant_name_string, request),
		});
	}

	let enum_doc = format!("Enum for all possible incoming requests of the {} interface.", interface.name());
	item_tokens.extend(quote! {
		#[doc = #enum_doc]
		pub enum ReceivedRequest<F: #fizyr_rpc::util::format::Format> {
			#variant_tokens
		}

		impl<F: #fizyr_rpc::util::format::Format> ::core::fmt::Debug for ReceivedRequest<F> {
			fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
				match self {
					#debug_tokens
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
