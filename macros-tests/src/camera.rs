use serde::{Deserialize, Serialize};

fizyr_rpc::interface! {
	/// Interface to a camera server.
	///
	/// A camera server can represent many different types of cameras,
	/// like a simple 2D camera, a 3D camera with or without RGB data,
	/// or even a line scanner.
	pub interface Camera {
		/// Ping the server.
		///
		/// A succesful ping indicates that the server is running,
		/// but it does not guarantee that it is connected to a camera.
		service 0 ping: () -> (),

		/// Record an image.
		service 1 record: RecordRequest -> () {
			/// Cancel the recording prematurely.
			request_update 10 cancel: CancelReason,

			/// Forcibly disconnect during the recording to test error condition.
			#[hidden]
			request_update 101 disconnect: (),

			/// Enable tracing for this request.
			///
			/// The server will start sending tracing updates.
			#[hidden]
			request_update 102 enable_tracing: (),

			/// Update sent by the server to notify the client about recording progress.
			///
			/// When the record state goes to `RecordState::Processing`,
			/// the camera field of view may be obstructed by a robot again.
			response_update 11 state: RecordState,

			/// Update sent by the server when an image is available.
			///
			/// The camera may send multiple image updates depending on the configuration.
			response_update 12 image: Image,

			/// Update with tracing information.
			///
			/// Only sent if you send a `enable_tracing` request update.
			#[hidden]
			response_update 202 tracing: Tracing,
		},

		#[hidden]
		service 2 hidden_service: () -> (),

		#[hidden]
		stream 3 hidden_stream: (),
	}
}

pub mod camera_events {
	fizyr_rpc::interface! {
		pub interface CameraEvents {
			/// Notifications whenever the camera changes record state.
			stream 11 record_state: super::RecordState,
		}
	}
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RecordRequest {
	pub color: bool,
	pub cloud: bool,
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Deserialize, Serialize)]
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

#[derive(Debug, Deserialize, Serialize)]
pub struct Tracing {
	pub message: String,
}
