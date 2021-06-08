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
