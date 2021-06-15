use serde::{Deserialize, Serialize};

fizyr_rpc::interface! {
	/// Interface to a camera server.
	///
	/// A camera server can represent many different types of cameras,
	/// like a simple 2D camera, a 3D camera with or without RGB data,
	/// or even a line scanner.
	interface camera {
		/// Ping the server.
		///
		/// A succesful ping indicates that the server is running,
		/// but it does not guarantee that it is connected to a camera.
		service 0 ping: () -> (),

		/// Record an image.
		service 1 record: RecordRequest -> () {
			/// Cancel the recording prematurely.
			request_update 10 cancel: CancelReason,

			/// Update sent by the server to notify the client about recording progress.
			///
			/// When the record state goes to `RecordState::Processing`,
			/// the camera field of view may be obstructed by a robot again.
			response_update 11 state: RecordState,

			/// Update sent by the server when an image is available.
			///
			/// The camera may send multiple image updates depending on the configuration.
			response_update 12 image: Image,
		}
	}
}
#[derive(Debug, Deserialize, Serialize)]
pub struct RecordRequest {
	pub color: bool,
	pub cloud: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub enum RecordState {
	Recording,
	Processing,
	Done,
}

#[derive(Debug, Deserialize, Serialize)]
pub enum CancelReason {
	BecauseISaidSo,
	SomeDoofusObscuredTheCameraView,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Image {
	pub width: u32,
	pub height: u32,
	pub format: u32,
	pub data: Vec<u8>,
}

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
	fn encode_body(value: T) -> Result<fizyr_rpc::StreamBody, Box<dyn std::error::Error + Send>> {
		serde_json::to_vec(&value)
			.map(fizyr_rpc::StreamBody::from)
			.map_err(|e| Box::new(e) as _)
	}
}
