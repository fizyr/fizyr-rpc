fizyr_rpc::interface! {
	pub camera {
		#[service_id = 0]
		/// Ping the server.
		fn ping();

		#[service_id = 1]
		#[response_update(10, state, RecordState)]
		#[request_update(11, nevermind, ())]
		/// Record an image.
		fn record();
	}
}

pub enum RecordState {
}

