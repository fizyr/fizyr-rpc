use proc_macro2::{TokenStream, Span};
use quote::quote;

use crate::interface::parse::cooked::MessageDefinition;

use super::{to_upper_camel_case, to_doc_attrs};

/// Generate an enum with all possible body types for a message.
pub fn generate_message_enum(item_tokens: &mut TokenStream, fizyr_rpc: &syn::Ident, messages: &[impl MessageDefinition], enum_name: &syn::Ident, enum_doc: &str) {
	let mut variants = TokenStream::new();
	let mut from_message = TokenStream::new();
	let mut to_message = TokenStream::new();
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
			#service_id => ::core::result::Result::Ok(Self::#variant_name(F::decode_body(message.body).map_err(#fizyr_rpc::Error::decode_failed)?)),
		});

		decode_all.extend(quote! {
			F: #fizyr_rpc::format::DecodeBody<#body_type>,
		});

		to_message.extend(quote! {
			Self::#variant_name(message) => ::core::result::Result::Ok((#service_id, F::encode_body(message)?)),
		});

		service_id_arms.extend(quote! {
			Self::#variant_name(_) => #service_id,
		});

		encode_all.extend(quote! {
			F: #fizyr_rpc::format::EncodeBody<#body_type>,
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
			pub fn #as_fn_name(&self) -> ::core::option::Option<&#body_type> {
				if let Self::#variant_name(x) = self {
					::core::option::Option::Some(x)
				} else {
					::core::option::Option::None
				}
			}

			#[doc = #into_fn_doc]
			pub fn #into_fn_name(self) -> ::core::result::Result<#body_type, #fizyr_rpc::Error> {
				let service_id = self.service_id();
				if let Self::#variant_name(x) = self {
					::core::result::Result::Ok(x)
				} else {
					::core::result::Result::Err(#fizyr_rpc::Error::unexpected_service_id(service_id))
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

		impl<F: #fizyr_rpc::format::Format> #fizyr_rpc::format::FromMessage<F> for #enum_name
		where
			#decode_all
		{
			fn from_message(message: #fizyr_rpc::Message<F::Body>) -> ::core::result::Result<Self, #fizyr_rpc::Error> {
				match message.header.service_id {
					#from_message
					service_id => ::core::result::Result::Err(#fizyr_rpc::Error::unexpected_service_id(service_id)),
				}
			}
		}

		impl<F: #fizyr_rpc::format::Format> #fizyr_rpc::format::ToMessage<F> for #enum_name
		where
			#encode_all
		{
			fn to_message(&self) -> ::core::result::Result<(i32, F::Body), ::std::boxed::Box<dyn ::std::error::Error + ::core::marker::Send>> {
				match self {
					#to_message
				}
			}
		}
	})
}
