pub mod camera;

pub struct Json;

impl fizyr_rpc::util::format::Format for Json {
	type Body = fizyr_rpc::StreamBody;
}

impl<T: serde::de::DeserializeOwned> fizyr_rpc::util::format::DecodeBody<T> for Json {
	fn decode_body(body: Self::Body) -> Result<T, Box<dyn std::error::Error + Send>> {
		serde_json::from_slice(&body.data)
			.map_err(|e| Box::new(e) as _)
	}
}

impl<T: serde::Serialize> fizyr_rpc::util::format::EncodeBody<T> for Json {
	fn encode_body(value: &T) -> Result<fizyr_rpc::StreamBody, Box<dyn std::error::Error + Send>> {
		serde_json::to_vec(value)
			.map(fizyr_rpc::StreamBody::from)
			.map_err(|e| Box::new(e) as _)
	}
}
