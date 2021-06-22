use proc_macro2::{Span, TokenStream};
use quote::ToTokens;
use syn::parse::{Parse, ParseStream};

/// A span and a value.
///
/// Mainly used to keep spans around for generating errors later on.
#[derive(Debug, Clone)]
pub struct WithSpan<T> {
	pub span: Span,
	pub value: T,
}

impl<T> WithSpan<T> {
	/// Create a new `WithSpan` from a [`Span`] and a value.
	pub fn new(span: Span, value: T) -> Self {
		Self { span, value }
	}
}

impl<T: ToTokens> ToTokens for WithSpan<T> {
	fn to_tokens(&self, tokens: &mut TokenStream) {
		let value = &self.value;
		quote::quote_spanned!(self.span => #value).to_tokens(tokens);
	}
}

/// Keep parsing a type until the stream is depleted.
///
/// The type is parsed as-is without expecting any intermediate punctuation.
pub fn parse_repeated<T: Parse>(input: ParseStream) -> syn::Result<Vec<T>> {
	let mut result = Vec::new();
	while !input.is_empty() {
		result.push(input.parse()?);
	}
	Ok(result)
}

/// Helper struct to parse `= T` from a token stream.
struct EqAttrContents<T> {
	_eq_token: syn::token::Eq,
	value: T,
}

impl<T: Parse> Parse for EqAttrContents<T> {
	fn parse(input: ParseStream) -> syn::Result<Self> {
		Ok(Self {
			_eq_token: input.parse()?,
			value: input.parse()?,
		})
	}
}

/// Parse the input tokens as `= T`.
///
/// This is useful for parsing `#[attr = value]` style attributes.
pub fn parse_eq_attr_contents<T: Parse>(input: TokenStream) -> syn::Result<T> {
	let parsed: EqAttrContents<T> = syn::parse2(input)?;
	Ok(parsed.value)
}

/// Parse the string value of a doc attribute.
pub fn parse_doc_attr_contents(input: TokenStream) -> syn::Result<WithSpan<String>> {
	let doc: syn::LitStr = parse_eq_attr_contents(input)?;
	Ok(WithSpan::new(doc.span(), doc.value()))
}
