use proc_macro2::TokenStream;
use quote::quote;

use crate::{interface::parse::cooked::{InterfaceDefinition, ServiceDefinition, UpdateDefinition, StreamDefinition}, util::WithSpan};

/// Generate a struct representing the interface.
pub fn generate_interface_struct(item_tokens: &mut TokenStream, fizyr_rpc: &syn::Ident, interface: &InterfaceDefinition) {
	let name = interface.name().to_string();
	let doc = to_doc_string(interface.doc());
	let hidden = interface.hidden().is_some();

	let interface_doc = format!("Introspection for the {} RPC interface.", interface.name());
	let visibility = interface.visibility();

	let mut services_format_bounds = TokenStream::new();
	let mut streams_format_bounds = TokenStream::new();
	let service_definitions = service_definitions(&mut services_format_bounds, fizyr_rpc, interface.services());
	let stream_definitions = stream_definitions(&mut streams_format_bounds, fizyr_rpc, interface.streams());

	item_tokens.extend(quote! {
		#[doc = #interface_doc]
		#[derive(Debug)]
		#visibility struct Interface {
			_priv: (),
		}

		impl Interface {
			/// Get the name of the interface.
			pub const fn name() -> &'static str {
				#name
			}

			/// Get the documentation of the interface.
			///
			/// This string may contain rustdoc compatible markup.
			pub const fn doc() -> &'static str {
				#doc
			}

			/// Get the full interface definition.
			///
			/// The type information for message bodies depends on serialization format used.
			pub fn definition<F>() -> #fizyr_rpc::introspection::InterfaceDefinition<F::TypeInfo>
			where
				F: #fizyr_rpc::introspection::IntrospectableFormat,
				#services_format_bounds
				#streams_format_bounds
			{
				#fizyr_rpc::introspection::InterfaceDefinition {
					name: #name.to_string(),
					doc: #doc.to_string(),
					hidden: #hidden,
					services: Self::services::<F>(),
					streams: Self::streams::<F>(),
				}
			}

			/// Get the list of services in the interface.
			///
			/// The type information for message bodies depends on serialization format used.
			pub fn services<F>() -> ::std::vec::Vec<#fizyr_rpc::introspection::ServiceDefinition<F::TypeInfo>>
			where
				F: #fizyr_rpc::introspection::IntrospectableFormat,
				#services_format_bounds
			{
				#service_definitions
			}

			/// Get the list of streams in the interface.
			///
			/// The type information for message bodies depends on serialization format used.
			pub fn streams<F>() -> ::std::vec::Vec<#fizyr_rpc::introspection::StreamDefinition<F::TypeInfo>>
			where
				F: #fizyr_rpc::introspection::IntrospectableFormat,
				#streams_format_bounds
			{
				#stream_definitions
			}
		}
	})
}

/// Generate service definitions.
///
/// This function returns tokens that represent a vector of service definitions.
///
/// It also pushes required trait bounds to `format_bounds`.
fn service_definitions(format_bounds: &mut TokenStream, fizyr_rpc: &syn::Ident, services: &[ServiceDefinition]) -> TokenStream {
	let mut push_items = TokenStream::new();
	for service in services {
		let name = service.name().to_string();
		let doc = to_doc_string(service.doc());
		let hidden = service.hidden().is_some();
		let service_id = service.service_id().value;
		let request_type = service.request_type();
		let response_type = service.response_type();
		let request_updates = update_definitions(format_bounds, fizyr_rpc, service.request_updates());
		let response_updates = update_definitions(format_bounds, fizyr_rpc, service.response_updates());

		format_bounds.extend(quote! {
			F: #fizyr_rpc::introspection::FormatTypeInfo<#request_type>,
			F: #fizyr_rpc::introspection::FormatTypeInfo<#response_type>,
		});

		push_items.extend(quote! {
			vector.push(#fizyr_rpc::introspection::ServiceDefinition {
				name: #name.to_string(),
				doc: #doc.to_string(),
				hidden: #hidden,
				service_id: #service_id,
				request_body: <F as #fizyr_rpc::introspection::FormatTypeInfo<#request_type>>::type_info(),
				response_body: <F as #fizyr_rpc::introspection::FormatTypeInfo<#response_type>>::type_info(),
				request_updates: #request_updates,
				response_updates: #response_updates,
			});
		})
	}

	let length = services.len();
	quote!({
		let mut vector = ::std::vec::Vec::with_capacity(#length);
		#push_items
		vector
	})
}

/// Generate update definitions.
///
/// This function returns tokens that represent a vector of update definitions.
///
/// It also pushes required trait bounds to `format_bounds`.
fn update_definitions(format_bounds: &mut TokenStream, fizyr_rpc: &syn::Ident, updates: &[UpdateDefinition]) -> TokenStream {
	let mut push_items = TokenStream::new();
	for update in updates {
		let name = update.name().to_string();
		let doc = to_doc_string(update.doc());
		let hidden = update.hidden().is_some();
		let service_id = update.service_id().value;
		let body_type = update.body_type();

		format_bounds.extend(quote! {
			F: #fizyr_rpc::introspection::FormatTypeInfo<#body_type>,
		});

		push_items.extend(quote! {
			vector.push(#fizyr_rpc::introspection::UpdateDefinition {
				name: #name.to_string(),
				doc: #doc.to_string(),
				hidden: #hidden,
				service_id: #service_id,
				body: <F as #fizyr_rpc::introspection::FormatTypeInfo<#body_type>>::type_info(),
			});
		})
	}

	let length = updates.len();
	quote!({
		let mut vector = ::std::vec::Vec::with_capacity(#length);
		#push_items
		vector
	})
}

/// Generate stream definitions.
///
/// This function returns tokens that represent a vector of stream definitions.
///
/// It also pushes required trait bounds to `format_bounds`.
fn stream_definitions(format_bounds: &mut TokenStream, fizyr_rpc: &syn::Ident, streams: &[StreamDefinition]) -> TokenStream {
	let mut push_items = TokenStream::new();
	for stream in streams {
		let name = stream.name().to_string();
		let doc = to_doc_string(stream.doc());
		let hidden = stream.hidden().is_some();
		let service_id = stream.service_id().value;
		let body_type = stream.body_type();

		format_bounds.extend(quote! {
			F: #fizyr_rpc::introspection::FormatTypeInfo<#body_type>,
		});

		push_items.extend(quote! {
			vector.push(#fizyr_rpc::introspection::StreamDefinition {
				name: #name.to_string(),
				doc: #doc.to_string(),
				hidden: #hidden,
				service_id: #service_id,
				body: <F as #fizyr_rpc::introspection::FormatTypeInfo<#body_type>>::type_info(),
			});
		})
	}

	let length = streams.len();
	quote!({
		let mut vector = ::std::vec::Vec::with_capacity(#length);
		#push_items
		vector
	})
}

/// Collect the doc string lines into one string.
///
/// Common leading whitespace is stripped from each line.
/// Lines consisting of only spaces are replaced with empty lines and are ignored when counting common leading whitespace.
fn to_doc_string(attrs: &[WithSpan<String>]) -> String {
	let mut lines = Vec::new();
	let mut common_leading_spaces = usize::MAX;
	let mut string_size = 0;
	let mut empty_lines = 0;
	for line in attrs {
		let leading_spaces = line.value.as_bytes().iter().take_while(|&&c| c == b' ').count();
		if line.value.len() > leading_spaces {
			common_leading_spaces = common_leading_spaces.min(leading_spaces);
			string_size += line.value.len() + 1;
			lines.push(line.value.clone());
		} else {
			empty_lines += 1;
			lines.push("".into());
		}
	}

	let total_size = string_size - (lines.len() - empty_lines) * common_leading_spaces + empty_lines;
	let mut doc_str = String::with_capacity(total_size);
	for line in lines {
		if !line.is_empty() {
			doc_str += &line[common_leading_spaces..];
		}
		doc_str.push('\n');
	}

	doc_str
}
