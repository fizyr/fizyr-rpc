#[derive(Debug)]
pub enum FromMessageError<ParseError> {
	UnknownServiceId(UnknownServiceId),
	Parse(ParseError),
}

#[derive(Debug)]
pub struct UnknownServiceId {
	pub service_id: i32,
}
