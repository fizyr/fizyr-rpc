fizyr_rpc::interface! {
	interface camera {
		/// Ping the server.
		service 0 ping: () -> (),

		/// Record an image.
		service 1 record: () -> () {
			request_update 10 cancel: CancelReason,
			response_update 11 state: RecordState,
			response_update 12 image: Image,
		}
	}
}

fizyr_rpc::interface! {
	interface camera_state {
		/// Notification of the record state of the camera.
		stream 100 record_state: RecordState,
	}
}

fizyr_rpc::interface! {
	interface empty {
	}
}

pub enum RecordState {
	Recording,
	Processing,
	Done,
}

pub enum CancelReason {
	BecauseISaidSo,
	SomeDoofusObscuredTheCameraView,
}

pub struct Image {
	pub width: u32,
	pub height: u32,
	pub format: u32,
	pub data: Vec<u8>,
}

struct Json;

impl fizyr_rpc::util::format::Format for Json {
	type Body = fizyr_rpc::StreamBody;
}

impl<T: serde::Serialize> fizyr_rpc::util::format::EncodeBody<T> for Json {
	fn encode_body(value: T) -> Result<fizyr_rpc::StreamBody, Box<dyn std::error::Error + Send>> {
		serde_json::to_vec(&value)
			.map(fizyr_rpc::StreamBody::from)
			.map_err(|e| Box::new(e) as _)
	}
}

impl<T: serde::de::DeserializeOwned> fizyr_rpc::util::format::DecodeBody<T> for Json {
	fn decode_body(body: Self::Body) -> Result<T, Box<dyn std::error::Error + Send>> {
		serde_json::from_slice(&body.data)
			.map_err(|e| Box::new(e) as _)
	}
}
