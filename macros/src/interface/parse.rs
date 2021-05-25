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
	use crate::util::{parse_doc_attr_contents, parse_eq_attr_contents, WithSpan};
	use proc_macro2::Span;

	#[derive(Debug)]
	pub struct InterfaceDefinition {
		name: syn::Ident,
		doc: Vec<WithSpan<String>>,
		services: Vec<ServiceDefinition>,
	}

	#[derive(Debug)]
	pub struct InterfaceAttributes {
		doc: Vec<WithSpan<String>>,
	}

	#[derive(Debug)]
	pub struct ServiceDefinition {
		service_id: WithSpan<i32>,
		name: syn::Ident,
		doc: Vec<WithSpan<String>>,
		request_type: Option<Box<syn::Type>>,
		response_type: Option<Box<syn::Type>>,
		request_updates: Vec<UpdateDefinition>,
		response_updates: Vec<UpdateDefinition>,
	}

	#[derive(Debug)]
	struct ServiceAttributes {
		service_id: WithSpan<i32>,
		doc: Vec<WithSpan<String>>,
	}

	#[derive(Debug)]
	pub struct UpdateDefinition {
		service_id: Option<WithSpan<i32>>,
		name: syn::Ident,
		doc: Vec<WithSpan<String>>,
		body_type: Box<syn::Type>,
	}

	#[derive(Debug)]
	pub struct UpdateAttributes {
		service_id: Option<WithSpan<i32>>,
		kind: Option<UpdateKind>,
		doc: Vec<WithSpan<String>>,
	}

	#[derive(Debug)]
	enum UpdateKind {
		RequestUpdate,
		ResponseUpdate,
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

		pub fn from_raw(errors: &mut Vec<syn::Error>, raw: super::raw::InterfaceDefinition) -> Self {
			let attrs = InterfaceAttributes::from_raw(errors, raw.attrs);
			let services = raw.services.into_iter().map(|raw| ServiceDefinition::from_raw(errors, raw)).collect();
			Self {
				name: raw.name,
				doc: attrs.doc,
				services,
			}
		}
	}

	impl InterfaceAttributes {
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

		pub fn request_type(&self) -> Option<&syn::Type> {
			self.request_type.as_deref()
		}

		pub fn response_type(&self) -> Option<&syn::Type> {
			self.response_type.as_deref()
		}

		pub fn request_updates(&self) -> &[UpdateDefinition] {
			&self.request_updates
		}

		pub fn response_updates(&self) -> &[UpdateDefinition] {
			&self.response_updates
		}

		fn from_raw(errors: &mut Vec<syn::Error>, raw: super::raw::ServiceDefinition) -> Self {
			let attrs = ServiceAttributes::from_raw(errors, raw.name.span(), raw.attrs);
			let mut request_updates = Vec::new();
			let mut response_updates = Vec::new();
			if let super::raw::MaybeServiceBody::Body(body) = raw.body {
				for update in body.updates {
					match UpdateDefinition::from_raw(errors, update) {
						(Some(UpdateKind::RequestUpdate), update) => request_updates.push(update),
						(Some(UpdateKind::ResponseUpdate), update) => response_updates.push(update),
						(None, _) => (),
					}
				}
			}

			for (i, a) in request_updates.iter().enumerate() {
				if let Some(id_a) = &a.service_id {
					for b in &request_updates[i + 1..] {
						if let Some(id_b) = &b.service_id {
							if id_b.value == id_a.value {
								errors.push(syn::Error::new(id_b.span, "duplicate service ID"));
							}
						}
					}
				} else {
					errors.push(syn::Error::new_spanned(&a.name, "missing `#[service_id = ...]' attribute"));
				}
			}

			for (i, a) in response_updates.iter().enumerate() {
				if let Some(id_a) = &a.service_id {
					for b in &response_updates[i + 1..] {
						if let Some(id_b) = &b.service_id {
							if id_b.value == id_a.value {
								errors.push(syn::Error::new(id_b.span, "duplicate service ID"));
							}
						}
					}
				} else {
					errors.push(syn::Error::new_spanned(&a.name, "missing `#[service_id = ...]' attribute"));
				}
			}

			Self {
				service_id: attrs.service_id,
				name: raw.name,
				doc: attrs.doc,
				request_type: raw.request_type.ty,
				response_type: raw.response_type.map(|x| x.ty),
				request_updates,
				response_updates,
			}
		}
	}

	impl ServiceAttributes {
		fn from_raw(errors: &mut Vec<syn::Error>, name_span: proc_macro2::Span, attrs: Vec<syn::Attribute>) -> Self {
			let mut service_id = None;
			let mut doc = Vec::new();

			for attr in attrs {
				if attr.path.is_ident("service_id") {
					if service_id.is_some() {
						errors.push(syn::Error::new_spanned(&attr.path, "duplicate `service_id' attribute"));
					}
					match parse_i32_attr_contents(attr.tokens) {
						Err(e) => errors.push(e),
						Ok(id) => {
							if service_id.is_none() {
								service_id = Some(id);
							}
						},
					}
				} else if attr.path.is_ident("doc") {
					match parse_doc_attr_contents(attr.tokens) {
						Err(e) => errors.push(e),
						Ok(x) => doc.push(x),
					}
				} else {
					errors.push(syn::Error::new_spanned(attr.path, "unknown attribute"));
				}
			}

			let service_id = service_id.unwrap_or_else(|| {
				errors.push(syn::Error::new(name_span, "missing `#[service_id = i32]' attribute"));
				WithSpan::new(proc_macro2::Span::call_site(), 0)
			});

			Self {
				service_id,
				doc,
			}
		}
	}

	impl UpdateDefinition {
		pub fn service_id(&self) -> WithSpan<i32> {
			self.service_id.clone().unwrap_or_else(|| WithSpan::new(Span::call_site(), 0))
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

		fn from_raw(errors: &mut Vec<syn::Error>, raw: super::raw::UpdateDefinition) -> (Option<UpdateKind>, Self) {
			let name = raw.name;
			let attrs = UpdateAttributes::from_raw(errors, raw.attrs);

			(attrs.kind, Self {
				service_id: attrs.service_id,
				name,
				doc: attrs.doc,
				body_type: raw.body_type,
			})
		}
	}

	impl UpdateAttributes {
		fn from_raw(errors: &mut Vec<syn::Error>, attrs: Vec<syn::Attribute>) -> Self {
			let mut doc = Vec::new();
			let mut kind = None;
			let mut service_id = None;

			for attr in attrs {
				if attr.path.is_ident("doc") {
					match parse_doc_attr_contents(attr.tokens) {
						Ok(x) => doc.push(x),
						Err(e) => errors.push(e),
					}
				} else if attr.path.is_ident("service_id") {
					if service_id.is_some() {
						errors.push(syn::Error::new_spanned(&attr.path, "duplicate `service_id' attribute"));
					}
					match parse_i32_attr_contents(attr.tokens) {
						Err(e) => errors.push(e),
						Ok(id) => {
							if service_id.is_none() {
								service_id = Some(id);
							}
						},
					}
				} else if attr.path.is_ident("request_update") {
					if let Some(token) = attr.tokens.into_iter().next() {
						errors.push(syn::Error::new_spanned(token, "unexpected token"));
					}
					if kind.is_some() {
						errors.push(syn::Error::new_spanned(&attr.path, "duplicate update type attribute"));
					} else {
						kind = Some(UpdateKind::RequestUpdate);
					}
				} else if attr.path.is_ident("response_update") {
					if let Some(token) = attr.tokens.into_iter().next() {
						errors.push(syn::Error::new_spanned(token, "unexpected token"));
					}
					if kind.is_some() {
						errors.push(syn::Error::new_spanned(&attr.path, "duplicate update type attribute"));
					} else {
						kind = Some(UpdateKind::ResponseUpdate);
					}
				} else {
					errors.push(syn::Error::new_spanned(attr.path, "unknown attribute"));
				}
			}

			Self {
				service_id,
				kind,
				doc,
			}
		}
	}

	fn parse_i32_attr_contents(tokens: proc_macro2::TokenStream) -> syn::Result<WithSpan<i32>> {
		let int: syn::LitInt = parse_eq_attr_contents(tokens)?;
		Ok(WithSpan::new(int.span(), int.base10_parse()?))
	}
}

/// First stage parsing types.
///
/// The types in this modules still contain potentially invalid data.
/// We want to fully parse this raw form before continuing to more detailed error checking.
pub mod raw {
	#[derive(Debug)]
	pub struct InterfaceInput {
		pub fizyr_rpc: syn::Ident,
		pub _semi_token: syn::token::Semi,
		pub interface: InterfaceDefinition,
	}

	#[derive(Debug)]
	pub struct InterfaceDefinition {
		pub attrs: Vec<syn::Attribute>,
		pub visibility: syn::Visibility,
		pub name: syn::Ident,
		pub _brace_token: syn::token::Brace,
		pub services: Vec<ServiceDefinition>,
	}

	#[derive(Debug)]
	pub struct ServiceDefinition {
		pub attrs: Vec<syn::Attribute>,
		pub _fn_token: syn::token::Fn,
		pub name: syn::Ident,
		pub request_type: RequestType,
		pub response_type: Option<ResponseType>,
		pub body: MaybeServiceBody,
	}

	#[derive(Debug)]
	pub enum MaybeServiceBody {
		NoBody(syn::token::Semi),
		Body(ServiceBody),
	}

	#[derive(Debug)]
	pub struct ServiceBody {
		pub _brace_token: syn::token::Brace,
		pub updates: syn::punctuated::Punctuated<UpdateDefinition, syn::token::Comma>,
	}

	#[derive(Debug)]
	pub struct UpdateDefinition {
		pub attrs: Vec<syn::Attribute>,
		pub name: syn::Ident,
		pub _colon_token: syn::token::Colon,
		pub body_type: Box<syn::Type>,
	}

	#[derive(Debug)]
	pub struct RequestType {
		pub paren_token: syn::token::Paren,
		pub ty: Option<Box<syn::Type>>,
	}

	#[derive(Debug)]
	pub struct ResponseType {
		pub arrow: syn::token::RArrow,
		pub ty: Box<syn::Type>,
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
			let services;
			Ok(Self {
				attrs: input.call(syn::Attribute::parse_outer)?,
				visibility: input.parse()?,
				name: input.parse()?,
				_brace_token: syn::braced!(services in input),
				services: services.call(crate::util::parse_repeated)?,
			})
		}
	}

	impl syn::parse::Parse for ServiceDefinition {
		#[allow(clippy::eval_order_dependence)]
		fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
			Ok(Self {
				attrs: input.call(syn::Attribute::parse_outer)?,
				_fn_token: input.parse()?,
				name: input.parse()?,
				request_type: input.parse()?,
				response_type: parse_response_type(input)?,
				body: input.parse()?,
			})
		}
	}

	impl syn::parse::Parse for RequestType {
		#[allow(clippy::eval_order_dependence)]
		fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
			let request_type;
			Ok(Self {
				paren_token: syn::parenthesized!(request_type in input),
				ty: if request_type.is_empty() { None } else { Some(request_type.parse()?) },
			})
		}
	}

	impl syn::parse::Parse for MaybeServiceBody {
		#[allow(clippy::eval_order_dependence)]
		fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
			if input.peek(syn::token::Semi) {
				Ok(Self::NoBody(input.parse()?))
			} else if input.peek(syn::token::Brace) {
				Ok(Self::Body(input.parse()?))
			} else {
				Err(input.error("expected semicolon or service body"))
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
		#[allow(clippy::eval_order_dependence)]
		fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
			Ok(Self {
				attrs: input.call(syn::Attribute::parse_outer)?,
				name: input.parse()?,
				_colon_token: input.parse()?,
				body_type: input.parse()?,
			})
		}
	}

	#[allow(clippy::eval_order_dependence)]
	fn parse_response_type(input: syn::parse::ParseStream) -> syn::Result<Option<ResponseType>> {
		if input.peek(syn::token::RArrow) {
			Ok(Some(ResponseType {
				arrow: input.parse()?,
				ty: input.parse()?,
			}))
		} else {
			Ok(None)
		}
	}
}
