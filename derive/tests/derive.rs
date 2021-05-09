use fizyr_rpc_derive::interface;

interface!(
	pub Camera {
		#[service_id = 0x12]
		fn ping();

		#[service_id = 1]
		#[response_update(1001, RecordState)]
		fn record_image(RecordImageRequest) -> CameraData;
	}
);
