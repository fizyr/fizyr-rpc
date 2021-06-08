/// Second stage parsing types.
///
/// These types never contain invalid data.
/// However, parsing them from their [`raw`] counterparts
/// always gives a cooked type and a list of errors.
/// If the list of errors is non-empty,
/// input data that caused the errors was discarded from the cooked type.
///
/// The cooked type can still be used for code generation to minimize the impact
/// on code using the generated type, but the errors MUST be emitted too.
pub mod cooked {
	use crate::util::{parse_doc_attr_contents, WithSpan};
	use proc_macro2::Span;
	use super::raw;

	#[derive(Debug)]
	pub struct InterfaceDefinition {
		name: syn::Ident,
		doc: Vec<WithSpan<String>>,
		services: Vec<ServiceDefinition>,
		streams: Vec<StreamDefinition>,
	}

	#[derive(Debug)]
	pub struct ServiceDefinition {
		service_id: WithSpan<i32>,
		name: syn::Ident,
		doc: Vec<WithSpan<String>>,
		request_type: Box<syn::Type>,
		response_type: Box<syn::Type>,
		request_updates: Vec<UpdateDefinition>,
		response_updates: Vec<UpdateDefinition>,
	}

	#[derive(Debug)]
	pub struct UpdateDefinition {
		service_id: WithSpan<i32>,
		name: syn::Ident,
		doc: Vec<WithSpan<String>>,
		body_type: Box<syn::Type>,
	}

	#[derive(Debug)]
	pub struct UpdateAttributes {
		doc: Vec<WithSpan<String>>,
	}

	#[derive(Debug)]
	pub struct StreamDefinition {
		service_id: WithSpan<i32>,
		name: syn::Ident,
		doc: Vec<WithSpan<String>>,
		body_type: Box<syn::Type>,
	}

	#[derive(Debug)]
	struct DocOnlyAttributes {
		doc: Vec<WithSpan<String>>,
	}

	impl InterfaceDefinition {
		pub fn name(&self) -> &syn::Ident {
			&self.name
		}

		pub fn doc(&self) -> &[WithSpan<String>] {
			&self.doc
		}

		pub fn services(&self) -> &[ServiceDefinition] {
			&self.services
		}

		#[allow(dead_code)]
		pub fn streams(&self) -> &[StreamDefinition] {
			&self.streams
		}

		pub fn from_raw(errors: &mut Vec<syn::Error>, raw: raw::InterfaceDefinition) -> Self {
			let attrs = DocOnlyAttributes::from_raw(errors, raw.attrs);
			let mut services = Vec::new();
			let mut streams = Vec::new();
			for item in raw.items {
				match item {
					raw::InterfaceItem::Service(raw) => services.push(ServiceDefinition::from_raw(errors, raw)),
					raw::InterfaceItem::Stream(raw) => streams.push(StreamDefinition::from_raw(errors, raw)),
				}
			}
			Self {
				name: raw.name,
				doc: attrs.doc,
				services,
				streams,
			}
		}
	}

	impl ServiceDefinition {
		pub fn service_id(&self) -> WithSpan<i32> {
			self.service_id.clone()
		}

		pub fn name(&self) -> &syn::Ident {
			&self.name
		}

		pub fn doc(&self) -> &[WithSpan<String>] {
			&self.doc
		}

		pub fn request_type(&self) -> &syn::Type {
			self.request_type.as_ref()
		}

		pub fn response_type(&self) -> &syn::Type {
			self.response_type.as_ref()
		}

		pub fn request_updates(&self) -> &[UpdateDefinition] {
			&self.request_updates
		}

		pub fn response_updates(&self) -> &[UpdateDefinition] {
			&self.response_updates
		}

		fn from_raw(errors: &mut Vec<syn::Error>, raw: raw::ServiceDefinition) -> Self {
			let attrs = DocOnlyAttributes::from_raw(errors, raw.attrs);
			let mut request_updates = Vec::new();
			let mut response_updates = Vec::new();
			if let raw::MaybeServiceBody::Body(body, _) = raw.body {
				for update in body.updates {
					match UpdateDefinition::from_raw(errors, update) {
						(raw::UpdateKind::RequestUpdate(_), update) => request_updates.push(update),
						(raw::UpdateKind::ResponseUpdate(_), update) => response_updates.push(update),
					}
				}
			}

			for (i, a) in request_updates.iter().enumerate() {
				for b in &request_updates[i + 1..] {
					if a.service_id.value == b.service_id.value {
						errors.push(syn::Error::new(b.service_id.span, "duplicate service ID"));
					}
				}
			}

			for (i, a) in response_updates.iter().enumerate() {
				for b in &response_updates[i + 1..] {
					if a.service_id.value == b.service_id.value {
						errors.push(syn::Error::new(b.service_id.span, "duplicate service ID"));
					}
				}
			}

			Self {
				service_id: parse_i32(errors, raw.service_id),
				name: raw.name,
				doc: attrs.doc,
				request_type: raw.request_type,
				response_type: raw.response_type,
				request_updates,
				response_updates,
			}
		}
	}

	impl UpdateDefinition {
		pub fn service_id(&self) -> &WithSpan<i32> {
			&self.service_id
		}

		pub fn name(&self) -> &syn::Ident {
			&self.name
		}

		pub fn doc(&self) -> &[WithSpan<String>] {
			&self.doc
		}

		pub fn body_type(&self) -> &syn::Type {
			&self.body_type
		}

		fn from_raw(errors: &mut Vec<syn::Error>, raw: raw::UpdateDefinition) -> (raw::UpdateKind, Self) {
			let attrs = DocOnlyAttributes::from_raw(errors, raw.attrs);

			(raw.kind, Self {
				service_id: parse_i32(errors, raw.service_id),
				name: raw.name,
				doc: attrs.doc,
				body_type: raw.body_type,
			})
		}
	}

	#[allow(dead_code)]
	impl StreamDefinition {
		pub fn service_id(&self) -> &WithSpan<i32> {
			&self.service_id
		}

		pub fn name(&self) -> &syn::Ident {
			&self.name
		}

		pub fn doc(&self) -> &[WithSpan<String>] {
			&self.doc
		}

		pub fn body_type(&self) -> &syn::Type {
			self.body_type.as_ref()
		}

		fn from_raw(errors: &mut Vec<syn::Error>, raw: raw::StreamDefinition) -> Self {
			let attrs = DocOnlyAttributes::from_raw(errors, raw.attrs);

			Self {
				service_id: parse_i32(errors, raw.service_id),
				name: raw.name,
				doc: attrs.doc,
				body_type: raw.body_type,
			}
		}
	}


	impl DocOnlyAttributes {
		fn from_raw(errors: &mut Vec<syn::Error>, attrs: Vec<syn::Attribute>) -> Self {
			let mut doc = Vec::new();

			for attr in attrs {
				if attr.path.is_ident("doc") {
					match parse_doc_attr_contents(attr.tokens) {
						Ok(x) => doc.push(x),
						Err(e) => errors.push(e),
					}
				} else {
					errors.push(syn::Error::new_spanned(attr.path, "unknown attribute"));
				}
			}

			Self { doc }
		}
	}

	fn parse_i32(errors: &mut Vec<syn::Error>, literal: syn::LitInt) -> WithSpan<i32> {
		match literal.base10_parse() {
			Ok(x) => WithSpan::new(literal.span(), x),
			Err(e) => {
				errors.push(e);
				WithSpan::new(Span::call_site(), 0)
			}
		}
	}
}

/// First stage parsing types.
///
/// The types in this modules still contain potentially invalid data.
/// We want to fully parse this raw form before continuing to more detailed error checking.
pub mod raw {
	mod keyword {
		syn::custom_keyword!(interface);
		syn::custom_keyword!(service);
		syn::custom_keyword!(request_update);
		syn::custom_keyword!(response_update);
		syn::custom_keyword!(stream);
	}

	#[derive(Debug)]
	pub struct InterfaceInput {
		pub fizyr_rpc: syn::Ident,
		pub _semi_token: syn::token::Semi,
		pub interface: InterfaceDefinition,
	}

	#[derive(Debug)]
	pub struct InterfaceDefinition {
		pub attrs: Vec<syn::Attribute>,
		pub _interface: keyword::interface,
		pub name: syn::Ident,
		pub _brace_token: syn::token::Brace,
		pub items: Vec<InterfaceItem>,
	}

	#[derive(Debug)]
	pub enum InterfaceItem {
		Service(ServiceDefinition),
		Stream(StreamDefinition),
	}

	#[derive(Debug)]
	pub struct ServiceDefinition {
		pub attrs: Vec<syn::Attribute>,
		pub _service: keyword::service,
		pub service_id: syn::LitInt,
		pub name: syn::Ident,
		pub _colon: syn::token::Colon,
		pub request_type: Box<syn::Type>,
		pub _arrow: syn::Token![->],
		pub response_type: Box<syn::Type>,
		pub body: MaybeServiceBody,
	}

	#[derive(Debug)]
	pub enum MaybeServiceBody {
		NoBody(syn::token::Comma),
		Body(ServiceBody, Option<syn::token::Comma>),
	}

	#[derive(Debug)]
	pub struct ServiceBody {
		pub _brace_token: syn::token::Brace,
		pub updates: syn::punctuated::Punctuated<UpdateDefinition, syn::token::Comma>,
	}

	#[derive(Debug)]
	pub struct UpdateDefinition {
		pub attrs: Vec<syn::Attribute>,
		pub kind: UpdateKind,
		pub service_id: syn::LitInt,
		pub name: syn::Ident,
		pub _colon_token: syn::token::Colon,
		pub body_type: Box<syn::Type>,
	}

	#[derive(Debug)]
	pub enum UpdateKind {
		RequestUpdate(keyword::request_update),
		ResponseUpdate(keyword::response_update),
	}

	#[derive(Debug)]
	pub struct StreamDefinition {
		pub attrs: Vec<syn::Attribute>,
		pub _stream: keyword::stream,
		pub service_id: syn::LitInt,
		pub name: syn::Ident,
		pub _colon: syn::token::Colon,
		pub body_type: Box<syn::Type>,
		pub _comma: syn::token::Comma,
	}

	impl syn::parse::Parse for InterfaceInput {
		fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
			Ok(Self {
				fizyr_rpc: input.parse()?,
				_semi_token: input.parse()?,
				interface: input.parse()?,
			})
		}
	}

	impl syn::parse::Parse for InterfaceDefinition {
		#[allow(clippy::eval_order_dependence)]
		fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
			let body;
			Ok(Self {
				attrs: input.call(syn::Attribute::parse_outer)?,
				_interface: input.parse()?,
				name: input.parse()?,
				_brace_token: syn::braced!(body in input),
				items: body.call(crate::util::parse_repeated)?,
			})
		}
	}

	impl syn::parse::Parse for InterfaceItem {
		fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
			let attrs = input.call(syn::Attribute::parse_outer)?;
			if input.peek(keyword::service) {
				Ok(InterfaceItem::Service(ServiceDefinition {
					attrs,
					_service: input.parse()?,
					service_id: input.parse()?,
					name: input.parse()?,
					_colon: input.parse()?,
					request_type: input.parse()?,
					_arrow: input.parse()?,
					response_type: input.parse()?,
					body: input.parse()?,
				}))
			} else if input.peek(keyword::stream) {
				Ok(InterfaceItem::Stream(StreamDefinition {
					attrs,
					_stream: input.parse()?,
					service_id: input.parse()?,
					name: input.parse()?,
					_colon: input.parse()?,
					body_type: input.parse()?,
					_comma: input.parse()?,
				}))
			} else {
				return Err(input.error("expected `service' or `stream'"));
			}
		}
	}

	impl syn::parse::Parse for MaybeServiceBody {
		#[allow(clippy::eval_order_dependence)]
		fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
			if input.peek(syn::token::Comma) {
				Ok(Self::NoBody(input.parse()?))
			} else if input.peek(syn::token::Brace) {
				Ok(Self::Body(input.parse()?, input.parse()?))
			} else {
				Err(input.error("expected `,' or service body"))
			}
		}
	}

	impl syn::parse::Parse for ServiceBody {
		#[allow(clippy::eval_order_dependence)]
		fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
			let braced;
			Ok(Self {
				_brace_token: syn::braced!(braced in input),
				updates: braced.call(syn::punctuated::Punctuated::parse_terminated)?,
			})
		}
	}

	impl syn::parse::Parse for UpdateDefinition {
		fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
			Ok(Self {
				attrs: input.call(syn::Attribute::parse_outer)?,
				kind: input.parse()?,
				service_id: input.parse()?,
				name: input.parse()?,
				_colon_token: input.parse()?,
				body_type: input.parse()?,
			})
		}
	}

	impl syn::parse::Parse for UpdateKind {
		fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
			if input.peek(keyword::request_update) {
				Ok(Self::RequestUpdate(input.parse()?))
			} else if input.peek(keyword::response_update) {
				Ok(Self::ResponseUpdate(input.parse()?))
			} else {
				Err(input.error("expected `request_update' or `response_update'"))
			}
		}
	}
}
