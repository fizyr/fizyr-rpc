fizyr_rpc::interface! {
	pub camera {
		#[service_id = 0]
		/// Ping the server.
		fn ping();

		#[service_id = 1]
		/// Record an image.
		fn record() {
			#[service_id = 10]
			#[request_update]
			nevermind: (),

			#[service_id = 11]
			#[response_update]
			state: RecordState,
		}
	}
}

pub enum RecordState {
}

