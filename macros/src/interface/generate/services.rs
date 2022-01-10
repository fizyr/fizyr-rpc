use proc_macro2::{TokenStream, Span};
use quote::quote;

use crate::interface::parse::cooked::{InterfaceDefinition, ServiceDefinition, UpdateDefinition};

use super::{to_doc_attrs, is_unit_type, to_upper_camel_case, message_enum::generate_message_enum};

#[derive(Debug, Eq, PartialEq)]
enum UpdateKind {
	RequestUpdate,
	ResponseUpdate,
}

/// Generate the support types and function definitions for each service.
pub fn generate_services(item_tokens: &mut TokenStream, client_impl_tokens: &mut TokenStream, fizyr_rpc: &syn::Ident, interface: &InterfaceDefinition) {
	for service in interface.services() {
		generate_service(item_tokens, client_impl_tokens, fizyr_rpc, service, interface.visibility());
	}
}

/// Generate the support types and function definitions for each service.
fn generate_service(item_tokens: &mut TokenStream, client_impl_tokens: &mut TokenStream, fizyr_rpc: &syn::Ident, service: &ServiceDefinition, visibility: &syn::Visibility) {
	let service_name = service.name();
	let service_doc = to_doc_attrs(service.doc());
	let service_id = service.service_id();

	let request_type = service.request_type();
	let request_param;
	let request_body;
	if is_unit_type(request_type) {
		request_param = None;
		request_body = quote!(F::encode_body(&()))
	} else {
		request_param = Some(quote!(request: &#request_type));
		request_body = quote!(F::encode_body(request))
	}

	let response_type = service.response_type();
	let mut service_item_tokens = TokenStream::new();

	// Service without updates, so directly return the response (asynchronously).
	if service.request_updates().is_empty() && service.response_updates().is_empty() {
		client_impl_tokens.extend(quote! {
			#service_doc
			#[allow(clippy::ptr_arg)]
			pub async fn #service_name(&self, #request_param) -> ::core::result::Result<#response_type, #fizyr_rpc::Error>
			where
				F: #fizyr_rpc::util::format::EncodeBody<#request_type>,
				F: #fizyr_rpc::util::format::DecodeBody<#response_type>,
			{
				let request_body = #request_body.map_err(#fizyr_rpc::Error::encode_failed)?;
				let mut request = self.peer.send_request(#service_id, request_body).await?;

				let response = request.recv_response().await?;
				if response.header.service_id == #fizyr_rpc::service_id::ERROR {
					use #fizyr_rpc::Body;
					let message = response.body
						.into_error()
						.map_err(|e| #fizyr_rpc::Error::decode_failed(::std::boxed::Box::new(e)))?;
					::core::result::Result::Err(#fizyr_rpc::Error::remote_error(message))
				} else {
					let decoded = F::decode_body(response.body).map_err(#fizyr_rpc::Error::decode_failed)?;
					::core::result::Result::Ok(decoded)
				}
			}
		})
	} else {
		generate_sent_request(&mut service_item_tokens, fizyr_rpc, service);
		client_impl_tokens.extend(quote! {
			#service_doc
			#[allow(clippy::ptr_arg)]
			pub async fn #service_name(&self, #request_param) -> ::core::result::Result<#service_name::SentRequestHandle<F>, #fizyr_rpc::Error>
			where
				F: #fizyr_rpc::util::format::EncodeBody<#request_type>,
				F: #fizyr_rpc::util::format::DecodeBody<#response_type>,
			{
				let request_body = #request_body.map_err(#fizyr_rpc::Error::encode_failed)?;
				let mut request = self.peer.send_request(#service_id, request_body).await?;
				::core::result::Result::Ok(#service_name::SentRequestHandle { request })
			}
		});

	}

	generate_received_request(&mut service_item_tokens, fizyr_rpc, service);

	let mod_doc = format!("Support types for the `{}` service.", service.name());
	item_tokens.extend(quote! {
		#[doc = #mod_doc]
		#visibility mod #service_name {
			#[allow(unused_imports)]
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
	let mut read_handle_impl_tokens = TokenStream::new();
	let mut write_handle_impl_tokens = TokenStream::new();

	let response_type = service.response_type();
	let doc_recv_update = match service.response_updates().is_empty() {
		true => quote! {
			/// This service call does not support update messages, so there is no way to retrieve it.
		},
		false => quote! {
			/// You can still receive the update message by calling [`Self::recv_update`].
			/// This function will keep returning an error until the update message is received.
		},
	};

	read_handle_impl_tokens.extend(quote! {
		/// Receive the final response.
		///
		/// If an update message is received instead of the final response an error is returned.
		/// The update message will remain in the message queue and must be read before the response can be received.
		///
		#doc_recv_update
		pub async fn recv_response(&mut self) -> ::core::result::Result<#response_type, #fizyr_rpc::Error>
		where
			F: #fizyr_rpc::util::format::DecodeBody<#response_type>,
		{
			let response = self.request.recv_response().await?;
			if response.header.service_id == #fizyr_rpc::service_id::ERROR {
				use #fizyr_rpc::Body;
				let message = response.body
					.into_error()
					.map_err(|e| #fizyr_rpc::Error::decode_failed(::std::boxed::Box::new(e)))?;
				::core::result::Result::Err(#fizyr_rpc::Error::remote_error(message))
			} else {
				let decoded = F::decode_body(response.body).map_err(#fizyr_rpc::Error::decode_failed)?;
				::core::result::Result::Ok(decoded)
			}
		}
	});

	if !service.request_updates().is_empty() {
		generate_message_enum(
			item_tokens,
			fizyr_rpc,
			service.request_updates(),
			&syn::Ident::new("RequestUpdate", Span::call_site()),
			&format!("A request update for the {} service", service.name()),
		);
		generate_send_update_functions(&mut write_handle_impl_tokens, fizyr_rpc, &quote!(#service_name::RequestUpdate), service.request_updates());
	}

	if !service.response_updates().is_empty() {
		generate_message_enum(
			item_tokens,
			fizyr_rpc,
			service.response_updates(),
			&syn::Ident::new("ResponseUpdate", Span::call_site()),
			&format!("A response update for the {} service", service.name()),
		);
		generate_recv_update_function(&mut read_handle_impl_tokens, fizyr_rpc, service.response_updates(), UpdateKind::ResponseUpdate);
	}

	let handle_doc = format!("Read/write handle for a sent request for the `{}` service.", service.name());
	let write_handle_doc = format!("Write handle for a sent request for the `{}` service.", service.name());

	item_tokens.extend(quote! {
		#[doc = #handle_doc]
		pub struct SentRequestHandle<F: #fizyr_rpc::util::format::Format> {
			pub(super) request: #fizyr_rpc::SentRequestHandle<F::Body>,
		}

		#[doc = #write_handle_doc]
		pub struct SentRequestWriteHandle<F: #fizyr_rpc::util::format::Format> {
			pub(super) request: #fizyr_rpc::SentRequestWriteHandle<F::Body>,
		}

		impl<F: #fizyr_rpc::util::format::Format> ::core::fmt::Debug for SentRequestHandle<F> {
			fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
				f.debug_struct(::core::any::type_name::<Self>())
					.field("request_id", &self.request_id())
					.field("service_id", &self.service_id())
					// TODO: use finish_non_exhaustive when it hits stable
					.finish()
			}
		}

		impl<F: #fizyr_rpc::util::format::Format> ::core::fmt::Debug for SentRequestWriteHandle<F> {
			fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
				f.debug_struct(::core::any::type_name::<Self>())
					.field("request_id", &self.request_id())
					.field("service_id", &self.service_id())
					// TODO: use finish_non_exhaustive when it hits stable
					.finish()
			}
		}

		impl<F: #fizyr_rpc::util::format::Format> ::core::clone::Clone for SentRequestWriteHandle<F> {
			fn clone(&self) -> Self {
				Self {
					request: self.request.clone(),
				}
			}
		}

		impl<F: #fizyr_rpc::util::format::Format> SentRequestHandle<F> {
			/// Get the raw request.
			pub fn inner(&self) -> &#fizyr_rpc::SentRequestHandle<F::Body> {
				&self.request
			}

			/// Get an exclusive reference to the raw request.
			pub fn inner_mut(&self) -> &#fizyr_rpc::SentRequestHandle<F::Body> {
				&self.request
			}

			/// Consume this object to get the raw request.
			pub fn into_inner(self) -> #fizyr_rpc::SentRequestHandle<F::Body> {
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

			/// Get a write handle for the sent request.
			///
			/// The write handle can be cloned and sent to other threads freely,
			/// but it can not be used to receive update messages or the final response.
			pub fn write_handle(&self) -> SentRequestWriteHandle<F> {
				SentRequestWriteHandle {
					request: self.request.write_handle(),
				}
			}

			#read_handle_impl_tokens

			#write_handle_impl_tokens
		}

		impl<F: #fizyr_rpc::util::format::Format> SentRequestWriteHandle<F> {
			/// Get the raw request.
			pub fn inner(&self) -> &#fizyr_rpc::SentRequestWriteHandle<F::Body> {
				&self.request
			}

			/// Get an exclusive reference to the raw request.
			pub fn inner_mut(&self) -> &#fizyr_rpc::SentRequestWriteHandle<F::Body> {
				&self.request
			}

			/// Consume this object to get the raw request.
			pub fn into_inner(self) -> #fizyr_rpc::SentRequestWriteHandle<F::Body> {
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

			#write_handle_impl_tokens
		}
	});
}

fn generate_received_request(item_tokens: &mut TokenStream, fizyr_rpc: &syn::Ident, service: &ServiceDefinition) {
	let response_type = service.response_type();
	let service_name = service.name();
	let service_id = service.service_id();

	let mut read_handle_impl_tokens = TokenStream::new();
	let mut write_handle_impl_tokens = TokenStream::new();
	if !service.response_updates().is_empty() {
		generate_send_update_functions(&mut write_handle_impl_tokens, fizyr_rpc, &quote!(#service_name::ResponseUpdate), service.response_updates());
	}
	if !service.request_updates().is_empty() {
		generate_recv_update_function(&mut read_handle_impl_tokens, fizyr_rpc, service.request_updates(), UpdateKind::RequestUpdate);
	}

	write_handle_impl_tokens.extend(quote! {
		/// Send the final response.
		#[allow(clippy::ptr_arg)]
		pub async fn send_response(&self, response: &#response_type) -> ::core::result::Result<(), #fizyr_rpc::Error>
		where
			F: #fizyr_rpc::util::format::EncodeBody<#response_type>,
		{
			let encoded = F::encode_body(response).map_err(#fizyr_rpc::Error::encode_failed)?;
			let response = self.request.send_response(#service_id, encoded).await?;
			::core::result::Result::Ok(())
		}

		/// Send the final response.
		pub async fn send_error_response(&self, error: &str) -> ::core::result::Result<(), #fizyr_rpc::Error> {
			::core::result::Result::Ok(self.request.send_error_response(error).await?)
		}
	});

	let handle_doc = format!("Handle for a received `{}` request.", service.name());
	let write_handle_doc = format!("Write-only handle for a received `{}` request.", service.name());
	item_tokens.extend(quote! {
		#[doc = #handle_doc]
		pub struct ReceivedRequestHandle<F: #fizyr_rpc::util::format::Format> {
			pub(super) request: #fizyr_rpc::ReceivedRequestHandle<F::Body>,
		}

		#[doc = #write_handle_doc]
		pub struct ReceivedRequestWriteHandle<F: #fizyr_rpc::util::format::Format> {
			pub(super) request: #fizyr_rpc::ReceivedRequestWriteHandle<F::Body>,
		}

		impl<F: #fizyr_rpc::util::format::Format> ::core::fmt::Debug for ReceivedRequestHandle<F> {
			fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
				f.debug_struct(::core::any::type_name::<Self>())
					.field("request_id", &self.request_id())
					.field("service_id", &self.service_id())
					// TODO: use finish_non_exhaustive when it hits stable
					.finish()
			}
		}

		impl<F: #fizyr_rpc::util::format::Format> ::core::fmt::Debug for ReceivedRequestWriteHandle<F> {
			fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
				f.debug_struct(::core::any::type_name::<Self>())
					.field("request_id", &self.request_id())
					.field("service_id", &self.service_id())
					// TODO: use finish_non_exhaustive when it hits stable
					.finish()
			}
		}

		impl<F: #fizyr_rpc::util::format::Format> ::core::clone::Clone for ReceivedRequestWriteHandle<F> {
			fn clone(&self) -> Self {
				Self {
					request: self.request.clone(),
				}
			}
		}

		impl<F: #fizyr_rpc::util::format::Format> ReceivedRequestHandle<F> {
			/// Get the raw request.
			///
			/// Note that the request body has been consumed when it was parsed.
			/// As a result, the raw request always has an empty body.
			pub fn inner(&self) -> &#fizyr_rpc::ReceivedRequestHandle<F::Body> {
				&self.request
			}

			/// Get an exclusive reference to the raw request.
			///
			/// Note that the request body has been consumed when it was parsed.
			/// As a result, the raw request always has an empty body.
			pub fn inner_mut(&self) -> &#fizyr_rpc::ReceivedRequestHandle<F::Body> {
				&self.request
			}

			/// Consume this object to get the raw request.
			///
			/// Note that the request body has been consumed when it was parsed.
			/// As a result, the raw request always has an empty body.
			pub fn into_inner(self) -> #fizyr_rpc::ReceivedRequestHandle<F::Body> {
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

			/// Get a write handle for the received request.
			///
			/// The write handle can be cloned and sent to other threads freely,
			/// but it can not be used to receive update messages.
			pub fn write_handle(&self) -> ReceivedRequestWriteHandle<F> {
				ReceivedRequestWriteHandle {
					request: self.request.write_handle(),
				}
			}

			#read_handle_impl_tokens

			#write_handle_impl_tokens
		}

		impl<F: #fizyr_rpc::util::format::Format> ReceivedRequestWriteHandle<F> {
			/// Get the raw request.
			///
			/// Note that the request body has been consumed when it was parsed.
			/// As a result, the raw request always has an empty body.
			pub fn inner(&self) -> &#fizyr_rpc::ReceivedRequestWriteHandle<F::Body> {
				&self.request
			}

			/// Get an exclusive reference to the raw request.
			///
			/// Note that the request body has been consumed when it was parsed.
			/// As a result, the raw request always has an empty body.
			pub fn inner_mut(&self) -> &#fizyr_rpc::ReceivedRequestWriteHandle<F::Body> {
				&self.request
			}

			/// Consume this object to get the raw request.
			///
			/// Note that the request body has been consumed when it was parsed.
			/// As a result, the raw request always has an empty body.
			pub fn into_inner(self) -> #fizyr_rpc::ReceivedRequestWriteHandle<F::Body> {
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

			#write_handle_impl_tokens
		}
	})
}

fn generate_send_update_functions(impl_tokens: &mut TokenStream, fizyr_rpc: &syn::Ident, enum_type: &TokenStream, updates: &[UpdateDefinition]) {
	quote! {
		/// Send a request update to the remote peer.
		#[allow(clippy::ptr_arg)]
		pub async fn send_update(&self, update: &#enum_type) -> ::core::result::Result<(), #fizyr_rpc::Error>
		where
			#enum_type: #fizyr_rpc::util::format::ToMessage<F>,
		{
			let (service_id, body) = F::encode_message(update).map_err(#fizyr_rpc::Error::encode_failed)?;
			self.request.send_update(service_id, body).await?;
			::core::result::Result::Ok(())
		}
	};

	for update in updates {
		let function_name = syn::Ident::new(&format!("send_{}_update", update.name()), Span::call_site());
		let body_type = update.body_type();
		let service_id = update.service_id();
		let doc = format!("Send a `{}` update to the remote peer.", update.name());
		let body_arg;
		let body_val;
		if is_unit_type(body_type) {
			body_arg = None;
			body_val = quote!(&());
		} else {
			body_arg = Some(quote!(update: &#body_type));
			body_val = quote!(update);
		}
		impl_tokens.extend(quote! {
			#[doc = #doc]
			#[allow(clippy::ptr_arg)]
			pub async fn #function_name(&self, #body_arg) -> ::core::result::Result<(), #fizyr_rpc::Error>
			where
				F: #fizyr_rpc::util::format::EncodeBody<#body_type>,
			{
				let body = F::encode_body(#body_val).map_err(#fizyr_rpc::Error::encode_failed)?;
				self.request.send_update(#service_id, body).await?;
				::core::result::Result::Ok(())
			}
		})
	}
}

fn generate_recv_update_function(impl_tokens: &mut TokenStream, fizyr_rpc: &syn::Ident, updates: &[UpdateDefinition], kind: UpdateKind) {
	let mut doc = quote! {
		/// Receive an update from the remote peer.
		///
		/// This function only returns an error if an update message was received that could not be parsed.
		/// If an I/O error occurs or the connection is closed, this function returns `Ok(None)`.
	};
	if kind == UpdateKind::ResponseUpdate {
		doc.extend(quote! {
			///
			/// Once the final response is received,
			/// this function will keep returning `Ok(None)`.
			/// Use [`Self::recv_response`] to receive the response.
		})
	}

	let update_kind = match kind {
		UpdateKind::RequestUpdate => quote!(RequestUpdate),
		UpdateKind::ResponseUpdate => quote!(ResponseUpdate),
	};

	let mut decode_arms = TokenStream::new();
	let mut where_clause = TokenStream::new();
	for update in updates {
		let service_id = update.service_id();
		let body_type = update.body_type();
		let variant_name = syn::Ident::new(&to_upper_camel_case(&update.name().to_string()), Span::call_site());
		where_clause.extend(quote! {
			F: #fizyr_rpc::util::format::DecodeBody<#body_type>,
		});
		decode_arms.extend(quote! {
			#service_id =>  {
				match F::decode_body(update.body) {
					::core::result::Result::Ok(body) => {
						::core::result::Result::Ok(#update_kind::#variant_name(body))
					},
					::core::result::Result::Err(e) => {
						::core::result::Result::Err(#fizyr_rpc::ParseUpdateError::InvalidUpdate(update.header, e))
					},
				}
			},
		});
	}

	impl_tokens.extend(quote! {
		#doc
		pub async fn recv_update(&mut self) -> ::core::option::Option<::core::result::Result<#update_kind, #fizyr_rpc::ParseUpdateError<F::Body>>>
		where
			#where_clause
		{
			let update = self.request.recv_update().await?;
			::core::option::Option::Some(match update.header.service_id {
				#decode_arms
				_ => ::core::result::Result::Err(#fizyr_rpc::ParseUpdateError::UnknownUpdate(update))
			})
		}
	});
}
