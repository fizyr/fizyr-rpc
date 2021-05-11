use crate::interface::cooked::{InterfaceDefinition, ServiceDefinition, UpdateDefinition};
use quote::quote;
use proc_macro2::{Span, TokenStream};

/// Generate a client struct for the given interface.
pub fn generate_client(fizyr_rpc: &syn::Ident, interface: &InterfaceDefinition) -> TokenStream {
	let client_name = interface.client_struct
		.clone()
		.unwrap_or(syn::Ident::new(&format!("{}Client", interface.name), Span::call_site()));
	let mut item_tokens = TokenStream::new();
	let mut impl_tokens = TokenStream::new();
	generate_services(&mut item_tokens, &mut impl_tokens, fizyr_rpc, interface);
	let tokens = quote! {
		pub struct #client_name<P: #fizyr_rpc::macros::Protocol> {
			peer: #fizyr_rpc::PeerWriteHandle<P::Body>,
		}

		impl<P: #fizyr_rpc::macros::Protocol> #client_name<P> {
			fn new(peer: #fizyr_rpc::PeerWriteHandle<P::Body>) -> Self {
				Self { peer }
			}

			#impl_tokens
		}

		impl<P: #fizyr_rpc::macros::Protocol> ::core::convert::From<#fizyr_rpc::PeerWriteHandle<P::Body>> for #client_name<P> {
			fn from(other: #fizyr_rpc::PeerWriteHandle<P::Body>) -> Self {
				Self::new(other)
			}
		}

		impl<P: #fizyr_rpc::macros::Protocol> ::core::convert::From<#fizyr_rpc::PeerHandle<P::Body>> for #client_name<P> {
			fn from(other: #fizyr_rpc::PeerHandle<P::Body>) -> Self {
				let (_read, write) = other.split();
				Self::new(write)
			}
		}

		#item_tokens
	};

	eprintln!("{}", tokens);
	tokens
}

/// Generate the support types and function definitions for each service.
fn generate_services(item_tokens: &mut TokenStream, impl_tokens: &mut TokenStream, fizyr_rpc: &syn::Ident, interface: &InterfaceDefinition) {
	for service in &interface.services {
		let service_name = &service.name;
		let service_id = &service.service_id;

		let request_param = service.request_type.as_ref().map(|x| quote!(request: #x));
		let request_type = service.request_type.as_ref().map(|x| quote!(#x)).unwrap_or_else(|| quote!(()));
		let request_body = if service.request_type.is_some() {
			quote!(P::encode(request))
		} else {
			quote!(P::encode(()))
		};

		let response_type = service.response_type.as_ref().map(|x| quote!(#x)).unwrap_or_else(|| quote!(()));

		// Service without updates, so directly return the response (asynchronously).
		if service.request_updates.is_empty() && service.response_updates.is_empty() {
			impl_tokens.extend(quote! {
				pub async fn #service_name(&self, #request_param) -> Result<#response_type, #fizyr_rpc::macros::error::ServiceCallError<P::EncodeError, P::DecodeError>>
				where
					#request_type: #fizyr_rpc::macros::Encode<P>,
					#response_type: #fizyr_rpc::macros::Decode<P>,
				{
					let request_body = #request_body.map_err(#fizyr_rpc::macros::error::SendMessageError::EncodeBody)?;
					let mut request = self.peer.send_request(#service_id, request_body).await?;
					let response = request.recv_response().await?;
					P::decode(response.body).map_err(|e| #fizyr_rpc::macros::error::FromMessageError::DecodeBody(e).into())
				}
			})
		} else {
			let request_name = to_upper_camel_case(&interface.name.to_string()) + "Client" + &to_upper_camel_case(&service.name.to_string()) + "Request";
			let request_name = syn::Ident::new(&request_name, Span::call_site());
			generate_sent_request(item_tokens, &fizyr_rpc, &interface.name, service, &request_name);
			impl_tokens.extend(quote! {
				pub async fn #service_name(&self, #request_param) -> Result<#request_name<P>, #fizyr_rpc::macros::error::SendMessageError<P::EncodeError>>
				where
					#request_type: #fizyr_rpc::macros::Encode<P>,
					#response_type: #fizyr_rpc::macros::Decode<P>,
				{
					let request_body = #request_body.map_err(#fizyr_rpc::macros::error::SendMessageError::EncodeBody)?;
					let mut request = self.peer.send_request(#service_id, request_body).await?;
					Ok(#request_name { request })
				}
			})
		}
	}
}

/// Generate a type for the sent request for a specific service.
///
/// Only used for service calls that have update messages.
/// Otherwise, the return type of a service call will simply the the response message.
fn generate_sent_request(item_tokens: &mut TokenStream, fizyr_rpc: &syn::Ident, interface: &syn::Ident, service: &ServiceDefinition, request_name: &syn::Ident) {
	let response_type = service.response_type.as_ref().map(|x| quote!(#x)).unwrap_or_else(|| quote!(()));
	item_tokens.extend(quote! {
		pub struct #request_name<P: #fizyr_rpc::macros::Protocol> {
			request: #fizyr_rpc::SentRequest<P::Body>,
		}

		impl<P: #fizyr_rpc::macros::Protocol> #request_name<P> {
			pub async fn recv_response(&mut self) -> Result<#response_type, #fizyr_rpc::macros::error::RecvMessageError<P::DecodeError>>
			where
				#response_type: #fizyr_rpc::macros::Decode<P>,
			{
				let response = self.request.recv_response().await?;
				let decoded = P::decode(response.body).map_err(#fizyr_rpc::macros::error::FromMessageError::DecodeBody)?;
				Ok(decoded)
			}
		}
	});

	if !service.request_updates.is_empty() {
		let request_update_name = to_upper_camel_case(&interface.to_string()) + "Client" + &to_upper_camel_case(&service.name.to_string()) + "RequestUpdate";
		let request_update_name = syn::Ident::new(&request_update_name, Span::call_site());
		generate_message_enum(item_tokens, fizyr_rpc, &service.request_updates, &request_update_name);
		item_tokens.extend(quote! {
			impl<P: #fizyr_rpc::macros::Protocol> #request_name<P> {
				pub async fn send_update(&self, update: #request_update_name) -> Result<(), #fizyr_rpc::macros::error::SendMessageError<P::EncodeError>>
				where
					#request_update_name: #fizyr_rpc::macros::ToMessage<P>,
				{
					let (service_id, body) = update.to_message::<P>().map_err(#fizyr_rpc::macros::error::SendMessageError::EncodeBody)?;
					self.request.send_update(service_id, body).await?;
					Ok(())
				}
			}
		});
	}

	if !service.response_updates.is_empty() {
		let response_update_name = to_upper_camel_case(&interface.to_string()) + "Client" + &to_upper_camel_case(&service.name.to_string()) + "ResponseUpdate";
		let response_update_name = syn::Ident::new(&response_update_name, Span::call_site());
		generate_message_enum(item_tokens, fizyr_rpc, &service.response_updates, &response_update_name);
		item_tokens.extend(quote! {
			impl<P: #fizyr_rpc::macros::Protocol> #request_name<P> {
				pub async fn recv_update(&mut self, timeout: std::time::Duration) -> Result<Option<#response_update_name>, #fizyr_rpc::macros::error::RecvMessageError<P::DecodeError>>
				where
					#response_update_name: #fizyr_rpc::macros::FromMessage<P>,
				{
					use #fizyr_rpc::macros::FromMessage;
					let update = match self.request.recv_update().await? {
						Some(x) => x,
						None => return Ok(None),
					};
					Ok(Some(#response_update_name::from_message(update)?))
				}
			}
		});
	}
}

/// Generate an enum with all possible body types for a message.
fn generate_message_enum(item_tokens: &mut TokenStream, fizyr_rpc: &syn::Ident, updates: &[UpdateDefinition], enum_name: &syn::Ident) {
	let mut variants = TokenStream::new();
	let mut from_message = TokenStream::new();
	let mut to_message = TokenStream::new();
	let mut decode_all = TokenStream::new();
	let mut encode_all = TokenStream::new();
	for update in updates {
		let variant_name = to_upper_camel_case(&update.name.to_string());
		let variant_name = syn::Ident::new(&variant_name, update.name.span());
		let body_type = &update.body_type;
		let service_id = &update.service_id;
		variants.extend(quote!{
			#variant_name(#body_type),
		});

		from_message.extend(quote! {
			#service_id => Ok(Self::#variant_name(P::decode::<#body_type>(message.body).map_err(#fizyr_rpc::macros::error::FromMessageError::DecodeBody)?)),
		});

		decode_all.extend(quote!(
			#body_type: #fizyr_rpc::macros::Decode<P>,
		));

		to_message.extend(quote! {
			Self::#variant_name(update) => Ok((#service_id, P::encode(update)?)),
		});

		encode_all.extend(quote!(
			#body_type: #fizyr_rpc::macros::Encode<P>,
		));
	}

	item_tokens.extend(quote! {
		pub enum #enum_name {
			#variants
		}

		impl<P: #fizyr_rpc::macros::Protocol> #fizyr_rpc::macros::FromMessage<P> for #enum_name
		where
			#decode_all
		{
			fn from_message(message: #fizyr_rpc::Message<P::Body>) -> Result<Self, #fizyr_rpc::macros::error::FromMessageError<P::DecodeError>> {
				match message.header.service_id {
					#from_message
					service_id => Err(#fizyr_rpc::macros::error::UnknownServiceId { service_id }.into()),
				}
			}
		}

		impl<P: #fizyr_rpc::macros::Protocol> #fizyr_rpc::macros::ToMessage<P> for #enum_name
		where
			#encode_all
		{
			fn to_message(self) -> Result<(i32, P::Body), P::EncodeError> {
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
