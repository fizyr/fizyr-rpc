mod generate;
mod parse;

pub fn generate_interface(tokens: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
	let raw: parse::raw::InterfaceInput = match syn::parse2(tokens) {
		Ok(x) => x,
		Err(e) => return e.into_compile_error(),
	};

	let mut tokens = proc_macro2::TokenStream::new();
	let mut errors = Vec::new();
	let interface = parse::cooked::InterfaceDefinition::from_raw(&mut errors, raw.interface);
	if !errors.is_empty() {
		for error in errors {
			tokens.extend(error.into_compile_error());
		}
	}

	tokens.extend(generate::generate_interface(&raw.fizyr_rpc, &interface));
	tokens
}
