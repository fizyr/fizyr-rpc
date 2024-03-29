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
	use syn::spanned::Spanned;
	use super::raw;

	/// Marker to indicate an item should be hidden.
	#[derive(Copy, Clone)]
	pub struct Hidden {
		/// The span of the #[hidden] attribute.
		#[allow(unused)]
		span: Span,
	}

	/// A parsed interface definition.
	pub struct InterfaceDefinition {
		/// The visiblity to use for all generated items.
		visibility: syn::Visibility,

		/// The name of the interface.
		name: syn::Ident,

		/// The doc comments of the interface.
		doc: Vec<WithSpan<String>>,

		/// If set, the interface should be hidden from documentation.
		hidden: Option<Hidden>,

		/// The services in the interface.
		services: Vec<ServiceDefinition>,

		/// The stream messages in the interface.
		streams: Vec<StreamDefinition>,
	}

	/// A parsed service definition.
	pub struct ServiceDefinition {
		/// The ID of the service.
		service_id: WithSpan<i32>,

		/// The name of the service.
		name: syn::Ident,

		/// The doc comments of the service.
		doc: Vec<WithSpan<String>>,

		/// If set, the service should be hidden from documentation.
		hidden: Option<Hidden>,

		/// The type of the request body.
		request_type: Box<syn::Type>,

		/// The type of the response body.
		response_type: Box<syn::Type>,

		/// The updates that can be sent by the request initiator ("client").
		request_updates: Vec<UpdateDefinition>,

		/// The updates that can be sent by the request handler ("server").
		response_updates: Vec<UpdateDefinition>,
	}

	/// A parsed definition of an update message.
	pub struct UpdateDefinition {
		/// The service ID of the update.
		service_id: WithSpan<i32>,

		/// The name of the update.
		name: syn::Ident,

		/// The doc comments of the update.
		doc: Vec<WithSpan<String>>,

		/// If set, the update should be hidden from documentation.
		hidden: Option<Hidden>,

		/// The body type of the update.
		body_type: Box<syn::Type>,
	}

	/// A parsed definition of a stream message.
	pub struct StreamDefinition {
		/// The service ID of the stream.
		service_id: WithSpan<i32>,

		/// The name of the stream.
		name: syn::Ident,

		/// The doc comments of the stream.
		doc: Vec<WithSpan<String>>,

		/// If set, the stream should be hidden from documentation.
		hidden: Option<Hidden>,

		/// The body type of the stream message.
		body_type: Box<syn::Type>,
	}

	/// Trait for message definitions.
	///
	/// This is implemented for update messages and stream messages.
	pub trait MessageDefinition {
		/// The service ID used for the message.
		fn service_id(&self) -> &WithSpan<i32>;

		/// The name of the message.
		fn name(&self) -> &syn::Ident;

		/// The doc comments of the message.
		fn doc(&self) -> &[WithSpan<String>];

		/// Check if the message should be hidden from generated documentation.
		fn hidden(&self) -> Option<Hidden>;

		/// The type of the message body.
		fn body_type(&self) -> &syn::Type;
	}

	/// Attributes that include only doc comments.
	struct Attributes {
		doc: Vec<WithSpan<String>>,
		hidden: Option<Hidden>,
	}

	impl InterfaceDefinition {
		/// Get the visiblity to use for all generated top-level items.
		pub fn visibility(&self) -> &syn::Visibility {
			&self.visibility
		}

		/// Get the name of the interface.
		pub fn name(&self) -> &syn::Ident {
			&self.name
		}

		/// Get the doc comments of the interface.
		pub fn doc(&self) -> &[WithSpan<String>] {
			&self.doc
		}

		/// Check if the interface should be hidden from generated documentation.
		pub fn hidden(&self) -> Option<Hidden> {
			self.hidden
		}

		/// Get the list of services in the interface.
		pub fn services(&self) -> &[ServiceDefinition] {
			&self.services
		}

		/// Get the list of streams in the interface.
		pub fn streams(&self) -> &[StreamDefinition] {
			&self.streams
		}

		/// Process a raw interface definition into a cooked one.
		pub fn from_raw(errors: &mut Vec<syn::Error>, raw: raw::InterfaceDefinition) -> Self {
			let attrs = Attributes::from_raw(errors, raw.attrs);
			let mut services = Vec::new();
			let mut streams = Vec::new();
			for item in raw.items {
				match item {
					raw::InterfaceItem::Service(raw) => services.push(ServiceDefinition::from_raw(errors, raw)),
					raw::InterfaceItem::Stream(raw) => streams.push(StreamDefinition::from_raw(errors, raw)),
				}
			}

			// Remove things with duplicate names, because they'll cause a lot more compile errors.
			// Duplicate IDs we just generate though, because they don't cause duplicate type or functions name.
			let mut remove_services = Vec::new();
			let mut remove_streams = Vec::new();

			for (a_i, a) in services.iter().enumerate() {
				for (b_i, b) in services.iter().enumerate().skip(a_i + 1) {
					if a.service_id.value == b.service_id.value {
						errors.push(syn::Error::new(b.service_id.span, "duplicate service ID"));
					}
					if a.name() == b.name() {
						errors.push(syn::Error::new(b.name().span(), "duplicate service name"));
						remove_services.push(b_i);
					}
				}
			}

			for (a_i, a) in streams.iter().enumerate() {
				for (b_i, b) in streams.iter().enumerate().skip(a_i + 1) {
					if a.service_id.value == b.service_id.value {
						errors.push(syn::Error::new(b.service_id.span, "duplicate service ID"));
					}
					if a.name() == b.name() {
						errors.push(syn::Error::new(b.name().span(), "duplicate stream name"));
						remove_streams.push(b_i);
					}
				}
			}

			for i in remove_services.into_iter().rev() {
				services.remove(i);
			}

			for i in remove_streams.into_iter().rev() {
				streams.remove(i);
			}

			Self {
				visibility: raw.visibility,
				name: raw.name,
				doc: attrs.doc,
				hidden: attrs.hidden,
				services,
				streams,
			}
		}
	}

	impl ServiceDefinition {
		/// Get the service ID of the service.
		pub fn service_id(&self) -> WithSpan<i32> {
			self.service_id.clone()
		}

		/// Get the name of the service.
		pub fn name(&self) -> &syn::Ident {
			&self.name
		}

		/// Get the doc comments of the service.
		pub fn doc(&self) -> &[WithSpan<String>] {
			&self.doc
		}

		/// Check if the service should be hidden from generated documentation.
		pub fn hidden(&self) -> Option<Hidden> {
			self.hidden
		}

		/// Get the type of the request body.
		pub fn request_type(&self) -> &syn::Type {
			self.request_type.as_ref()
		}

		/// Get the type of the response body.
		pub fn response_type(&self) -> &syn::Type {
			self.response_type.as_ref()
		}

		/// Get the updates that the request initiator can send.
		pub fn request_updates(&self) -> &[UpdateDefinition] {
			&self.request_updates
		}

		/// Get the updates that the request handler can send.
		pub fn response_updates(&self) -> &[UpdateDefinition] {
			&self.response_updates
		}

		/// Process a raw service definition into a cooked one.
		fn from_raw(errors: &mut Vec<syn::Error>, raw: raw::ServiceDefinition) -> Self {
			let attrs = Attributes::from_raw(errors, raw.attrs);
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

			// Remove things with duplicate names, because they'll cause a lot more compile errors.
			// Duplicate IDs we just generate though, because they don't cause duplicate type or functions name.
			let mut remove_request_updates = Vec::new();
			let mut remove_response_updates = Vec::new();

			for (a_i, a) in request_updates.iter().enumerate() {
				for (b_i, b) in request_updates.iter().enumerate().skip(a_i + 1) {
					if a.service_id.value == b.service_id.value {
						errors.push(syn::Error::new(b.service_id.span, "duplicate service ID"));
					}
					if a.name() == b.name() {
						errors.push(syn::Error::new(b.name().span(), "duplicate request update name"));
						remove_request_updates.push(b_i);
					}
				}
			}

			for (a_i, a) in response_updates.iter().enumerate() {
				for (b_i, b) in response_updates.iter().enumerate().skip(a_i + 1) {
					if a.service_id.value == b.service_id.value {
						errors.push(syn::Error::new(b.service_id.span, "duplicate service ID"));
					}
					if a.name() == b.name() {
						errors.push(syn::Error::new(b.name().span(), "duplicate response update name"));
						remove_response_updates.push(b_i);
					}
				}
			}

			for i in remove_request_updates.into_iter().rev() {
				request_updates.remove(i);
			}

			for i in remove_response_updates.into_iter().rev() {
				response_updates.remove(i);
			}

			Self {
				service_id: parse_i32(errors, raw.service_id),
				name: raw.name,
				doc: attrs.doc,
				hidden: attrs.hidden,
				request_type: raw.request_type,
				response_type: raw.response_type,
				request_updates,
				response_updates,
			}
		}
	}

	impl UpdateDefinition {
		/// Get the service ID of the update.
		pub fn service_id(&self) -> &WithSpan<i32> {
			&self.service_id
		}

		/// Get the name of the update.
		pub fn name(&self) -> &syn::Ident {
			&self.name
		}

		/// Get the doc comments of the update.
		pub fn doc(&self) -> &[WithSpan<String>] {
			&self.doc
		}

		/// Check if the update should be hidden from generated documentation.
		pub fn hidden(&self) -> Option<Hidden> {
			self.hidden
		}

		/// Get the type of the update body.
		pub fn body_type(&self) -> &syn::Type {
			&self.body_type
		}

		/// Process a raw update definition into a cooked one.
		fn from_raw(errors: &mut Vec<syn::Error>, raw: raw::UpdateDefinition) -> (raw::UpdateKind, Self) {
			let attrs = Attributes::from_raw(errors, raw.attrs);

			(raw.kind, Self {
				service_id: parse_i32(errors, raw.service_id),
				name: raw.name,
				doc: attrs.doc,
				hidden: attrs.hidden,
				body_type: raw.body_type,
			})
		}
	}

	impl StreamDefinition {
		/// Get the service ID of the stream.
		pub fn service_id(&self) -> &WithSpan<i32> {
			&self.service_id
		}

		/// Get the name of the stream.
		pub fn name(&self) -> &syn::Ident {
			&self.name
		}

		/// Get the doc comments of the stream.
		pub fn doc(&self) -> &[WithSpan<String>] {
			&self.doc
		}

		/// Check if the stream should be hidden from generated documentation.
		pub fn hidden(&self) -> Option<Hidden> {
			self.hidden
		}

		/// Get the type of the stream body.
		pub fn body_type(&self) -> &syn::Type {
			self.body_type.as_ref()
		}

		/// Process a raw stream definition into a cooked one.
		fn from_raw(errors: &mut Vec<syn::Error>, raw: raw::StreamDefinition) -> Self {
			let attrs = Attributes::from_raw(errors, raw.attrs);

			Self {
				service_id: parse_i32(errors, raw.service_id),
				name: raw.name,
				doc: attrs.doc,
				hidden: attrs.hidden,
				body_type: raw.body_type,
			}
		}
	}


	impl Attributes {
		/// Process raw attributes into cooked DocOnlyAttributes.
		fn from_raw(errors: &mut Vec<syn::Error>, attrs: Vec<syn::Attribute>) -> Self {
			let mut doc = Vec::new();
			let mut hidden = None;

			for attr in attrs {
				if attr.path().is_ident("doc") {
					match parse_doc_attr_contents(attr) {
						Ok(x) => doc.push(x),
						Err(e) => errors.push(e),
					}
				} else if attr.path().is_ident("hidden") {
					if let Err(e) = attr.meta.require_path_only() {
						errors.push(e);
					} else {
						hidden = Some(Hidden { span: attr.path().span() });
					}
				} else {
					errors.push(syn::Error::new_spanned(attr.path(), "unknown attribute"));
				}
			}

			Self { doc, hidden }
		}
	}

	/// Parse an integer literal into an i32.
	fn parse_i32(errors: &mut Vec<syn::Error>, literal: syn::LitInt) -> WithSpan<i32> {
		match literal.base10_parse() {
			Ok(x) => WithSpan::new(literal.span(), x),
			Err(e) => {
				errors.push(e);
				WithSpan::new(Span::call_site(), 0)
			}
		}
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

		fn hidden(&self) -> Option<Hidden> {
			self.hidden
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

		fn hidden(&self) -> Option<Hidden> {
			self.hidden
		}

		fn body_type(&self) -> &syn::Type {
			self.body_type()
		}
	}
}

/// First stage parsing types.
///
/// The types in this modules still contain potentially invalid data.
/// We want to fully parse this raw form before continuing to more detailed error checking.
pub mod raw {
	pub mod keyword {
		syn::custom_keyword!(interface);
		syn::custom_keyword!(service);
		syn::custom_keyword!(request_update);
		syn::custom_keyword!(response_update);
		syn::custom_keyword!(stream);
		syn::custom_keyword!(hidden);
	}

	pub struct InterfaceInput {
		pub fizyr_rpc: syn::Ident,
		pub _semi_token: syn::token::Semi,
		pub interface: InterfaceDefinition,
	}

	pub struct InterfaceDefinition {
		pub attrs: Vec<syn::Attribute>,
		pub visibility: syn::Visibility,
		pub _interface: keyword::interface,
		pub name: syn::Ident,
		pub _brace_token: syn::token::Brace,
		pub items: Vec<InterfaceItem>,
	}

	pub enum InterfaceItem {
		Service(ServiceDefinition),
		Stream(StreamDefinition),
	}

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

	pub enum MaybeServiceBody {
		NoBody(syn::token::Comma),
		Body(ServiceBody, Option<syn::token::Comma>),
	}

	pub struct ServiceBody {
		pub _brace_token: syn::token::Brace,
		pub updates: syn::punctuated::Punctuated<UpdateDefinition, syn::token::Comma>,
	}

	pub struct UpdateDefinition {
		pub attrs: Vec<syn::Attribute>,
		pub kind: UpdateKind,
		pub service_id: syn::LitInt,
		pub name: syn::Ident,
		pub _colon_token: syn::token::Colon,
		pub body_type: Box<syn::Type>,
	}

	pub enum UpdateKind {
		RequestUpdate(keyword::request_update),
		ResponseUpdate(keyword::response_update),
	}

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
		fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
			let body;
			Ok(Self {
				attrs: input.call(syn::Attribute::parse_outer)?,
				visibility: input.parse()?,
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
				Err(input.error("expected `service' or `stream'"))
			}
		}
	}

	impl syn::parse::Parse for MaybeServiceBody {
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
