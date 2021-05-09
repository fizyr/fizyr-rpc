pub fn generate_interface(tokens: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
	let raw: raw::InterfaceDefinition = match syn::parse2(tokens) {
		Ok(x) => x,
		Err(e) => return e.into_compile_error(),
	};

	let mut errors = Vec::new();
	let interface = cooked::InterfaceDefinition::from_raw(&mut errors, raw);
	if !errors.is_empty() {
		let mut error_tokens = proc_macro2::TokenStream::new();
		for error in errors {
			error_tokens.extend(error.into_compile_error());
		}
		return error_tokens;
	}

	eprintln!("{:#?}", interface);
	proc_macro2::TokenStream::new()
}

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
mod cooked {
	use crate::util::{parse_doc_attr_contents, parse_eq_attr_contents, WithSpan};

	#[derive(Debug)]
	pub struct InterfaceDefinition {
		pub name: syn::Ident,
		pub doc: Vec<WithSpan<String>>,
		pub services: Vec<ServiceDefinition>,
		pub request_enum: Option<syn::Ident>,
		pub client_struct: Option<syn::Ident>,
		pub server_struct: Option<syn::Ident>,
	}

	#[derive(Debug)]
	pub struct InterfaceAttributes {
		doc: Vec<WithSpan<String>>,
		request_enum: Option<syn::Ident>,
		client_struct: Option<syn::Ident>,
		server_struct: Option<syn::Ident>,
	}

	#[derive(Debug)]
	pub struct ServiceDefinition {
		pub service_id: WithSpan<i32>,
		pub name: syn::Ident,
		pub doc: Vec<WithSpan<String>>,
		pub request_type: Option<Box<syn::Type>>,
		pub response_type: Option<Box<syn::Type>>,
		pub request_updates: Vec<UpdateDefinition>,
		pub response_updates: Vec<UpdateDefinition>,
	}

	#[derive(Debug)]
	struct ServiceAttributes {
		service_id: WithSpan<i32>,
		doc: Vec<WithSpan<String>>,
		request_updates: Vec<UpdateDefinition>,
		response_updates: Vec<UpdateDefinition>,
	}

	#[derive(Debug)]
	pub struct UpdateDefinition {
		pub service_id: WithSpan<i32>,
		pub body_type: Box<syn::Type>,
	}

	impl InterfaceDefinition {
		pub fn from_raw(errors: &mut Vec<syn::Error>, raw: super::raw::InterfaceDefinition) -> Self {
			let attrs = InterfaceAttributes::from_raw(errors, raw.attrs);
			let services = raw.services.into_iter().map(|raw| ServiceDefinition::from_raw(errors, raw)).collect();
			Self {
				name: raw.name,
				doc: attrs.doc,
				services,
				request_enum: attrs.request_enum,
				client_struct: attrs.client_struct,
				server_struct: attrs.server_struct,
			}
		}
	}

	impl InterfaceAttributes {
		fn from_raw(errors: &mut Vec<syn::Error>, attrs: Vec<syn::Attribute>) -> Self {
			let mut doc = Vec::new();
			let mut request_enum = None;
			let mut client_struct = None;
			let mut server_struct = None;

			for attr in attrs {
				if attr.path.is_ident("doc") {
					match parse_doc_attr_contents(attr.tokens) {
						Ok(x) => doc.push(x),
						Err(e) => errors.push(e),
					}
				} else if attr.path.is_ident("request_enum") {
					match parse_eq_attr_contents::<syn::Ident>(attr.tokens) {
						Err(e) => errors.push(e),
						Ok(ident) => {
							if request_enum.is_some() {
								errors.push(syn::Error::new_spanned(
									&attr.path,
									format!("duplicate `{}' attribute", attr.path.segments[0].ident),
								))
							} else {
								request_enum = Some(ident);
							}
						},
					}
				} else if attr.path.is_ident("client_struct") {
					match parse_eq_attr_contents::<syn::Ident>(attr.tokens) {
						Err(e) => errors.push(e),
						Ok(ident) => {
							if client_struct.is_some() {
								errors.push(syn::Error::new_spanned(
									&attr.path,
									format!("duplicate `{}' attribute", attr.path.segments[0].ident),
								))
							} else {
								client_struct = Some(ident);
							}
						},
					}
				} else if attr.path.is_ident("server_struct") {
					match parse_eq_attr_contents::<syn::Ident>(attr.tokens) {
						Err(e) => errors.push(e),
						Ok(ident) => {
							if server_struct.is_some() {
								errors.push(syn::Error::new_spanned(
									&attr.path,
									format!("duplicate `{}' attribute", attr.path.segments[0].ident),
								))
							} else {
								server_struct = Some(ident);
							}
						},
					}
				} else {
					errors.push(syn::Error::new_spanned(attr.path, "unknown attribute"));
				}
			}

			Self {
				doc,
				request_enum,
				client_struct,
				server_struct,
			}
		}
	}

	impl ServiceDefinition {
		fn from_raw(errors: &mut Vec<syn::Error>, raw: super::raw::ServiceDefinition) -> Self {
			let attrs = ServiceAttributes::from_raw(errors, raw.name.span(), raw.attrs);
			Self {
				service_id: attrs.service_id,
				name: raw.name,
				doc: attrs.doc,
				request_type: raw.request_type.ty,
				response_type: raw.response_type.map(|x| x.ty),
				request_updates: attrs.request_updates,
				response_updates: attrs.response_updates,
			}
		}
	}

	impl ServiceAttributes {
		fn from_raw(errors: &mut Vec<syn::Error>, name_span: proc_macro2::Span, attrs: Vec<syn::Attribute>) -> Self {
			let mut service_id = None;
			let mut doc = Vec::new();
			let mut request_updates: Vec<UpdateDefinition> = Vec::new();
			let mut response_updates: Vec<UpdateDefinition> = Vec::new();

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
				} else if attr.path.is_ident("request_update") {
					match parse_update_attr(attr.tokens) {
						Err(e) => errors.push(e),
						Ok(update) => {
							if request_updates.iter().any(|x| x.service_id.value == update.service_id.value) {
								errors.push(syn::Error::new(update.service_id.span, "duplicate service ID for request update"));
							} else {
								request_updates.push(update)
							}
						},
					}
				} else if attr.path.is_ident("response_update") {
					match parse_update_attr(attr.tokens) {
						Err(e) => errors.push(e),
						Ok(update) => {
							if response_updates.iter().any(|x| x.service_id.value == update.service_id.value) {
								errors.push(syn::Error::new(update.service_id.span, "duplicate service ID for response update"));
							} else {
								response_updates.push(update)
							}
						},
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
				request_updates,
				response_updates,
			}
		}
	}

	fn parse_i32_attr_contents(tokens: proc_macro2::TokenStream) -> syn::Result<WithSpan<i32>> {
		let int: syn::LitInt = parse_eq_attr_contents(tokens)?;
		Ok(WithSpan::new(int.span(), int.base10_parse()?))
	}

	fn parse_update_attr(tokens: proc_macro2::TokenStream) -> syn::Result<UpdateDefinition> {
		struct UpdateAttr {
			_paren_token: syn::token::Paren,
			service_id: syn::LitInt,
			_comma_token: syn::token::Comma,
			body_type: Box<syn::Type>,
		}

		impl syn::parse::Parse for UpdateAttr {
			#[allow(clippy::eval_order_dependence)]
			fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
				let contents;
				Ok(Self {
					_paren_token: syn::parenthesized!(contents in input),
					service_id: contents.parse()?,
					_comma_token: contents.parse()?,
					body_type: contents.parse()?,
				})
			}
		}

		let parsed: UpdateAttr = syn::parse2(tokens)?;
		let service_id = WithSpan::new(parsed.service_id.span(), parsed.service_id.base10_parse()?);
		let body_type = parsed.body_type;
		Ok(UpdateDefinition { service_id, body_type })
	}
}

/// First stage parsing types.
///
/// The types in this modules still contain potentially invalid data.
/// We want to fully parse this raw form before continuing to more detailed error checking.
mod raw {
	#[derive(Debug)]
	pub struct InterfaceDefinition {
		pub attrs: Vec<syn::Attribute>,
		pub visibility: syn::Visibility,
		pub name: syn::Ident,
		pub _brace: syn::token::Brace,
		pub services: Vec<ServiceDefinition>,
	}

	#[derive(Debug)]
	pub struct ServiceDefinition {
		pub attrs: Vec<syn::Attribute>,
		pub _fn_token: syn::token::Fn,
		pub name: syn::Ident,
		pub request_type: RequestType,
		pub response_type: Option<ResponseType>,
		pub _semi_token: syn::token::Semi,
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

	impl syn::parse::Parse for InterfaceDefinition {
		#[allow(clippy::eval_order_dependence)]
		fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
			let services;
			Ok(Self {
				attrs: input.call(syn::Attribute::parse_outer)?,
				visibility: input.parse()?,
				name: input.parse()?,
				_brace: syn::braced!(services in input),
				services: crate::util::parse_repeated(&services)?,
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
				_semi_token: input.parse()?,
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
