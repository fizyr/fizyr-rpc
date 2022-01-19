use proc_macro2::{TokenStream, Span};
use quote::quote;

use crate::interface::parse::cooked::InterfaceDefinition;

use super::is_unit_type;
use super::message_enum::generate_message_enum;

pub fn generate_streams(item_tokens: &mut TokenStream, client_impl_tokens: &mut TokenStream, fizyr_rpc: &syn::Ident, interface: &InterfaceDefinition) {
	if !interface.streams().is_empty() {
		generate_message_enum(
			item_tokens,
			fizyr_rpc,
			interface.streams(),
			&syn::Ident::new("StreamMessage", Span::call_site()),
			&format!("A stream message for the {} interface.", interface.name()),
		);
	}
	for stream in interface.streams() {
		let service_id = stream.service_id();
		let fn_name = syn::Ident::new(&format!("send_{}", stream.name()), Span::call_site());
		let fn_doc = format!("Send a `{}` stream message to the remote peer.", stream.name());
		let body_arg;
		let body_val;
		let body_type = stream.body_type();
		if is_unit_type(body_type) {
			body_arg = None;
			body_val = quote!(&());
		} else {
			body_arg = Some(quote!(body: &#body_type));
			body_val = quote!(body);
		}
		client_impl_tokens.extend(quote! {
			#[doc = #fn_doc]
			#[allow(clippy::ptr_arg)]
			pub async fn #fn_name(&self, #body_arg) -> ::core::result::Result<(), #fizyr_rpc::Error>
			where
				F: #fizyr_rpc::format::EncodeBody<#body_type>,
			{
				let encoded = F::encode_body(#body_val).map_err(#fizyr_rpc::Error::encode_failed)?;
				self.peer.send_stream(#service_id, encoded).await?;
				::core::result::Result::Ok(())
			}
		})
	}
}
