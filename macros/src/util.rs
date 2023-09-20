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

/// Parse the string value of a doc attribute.
pub fn parse_doc_attr_contents(attribute: syn::Attribute) -> syn::Result<WithSpan<String>> {
	let meta = attribute.meta.require_name_value()?;
	let doc = match &meta.value {
		syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Str(value), .. }) => value,
		_ => return Err(syn::Error::new_spanned(&meta.value, "expected a string literal")),
	};

	Ok(WithSpan::new(doc.span(), doc.value()))
}
