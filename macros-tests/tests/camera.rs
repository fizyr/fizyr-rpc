use assert2::{let_assert, assert};
use fizyr_rpc::{UnixStreamPeer, UnixStreamTransport};
use fizyr_rpc::util::format::Format;

use macros_tests::{camera, Json};

fn client_server_pair<F: fizyr_rpc::util::format::Format<Body = fizyr_rpc::StreamBody>>() -> std::io::Result<(camera::Client<F>, camera::Server<F>)> {
	let (client, server) = tokio::net::UnixStream::pair()?;
	let client = UnixStreamPeer::spawn(UnixStreamTransport::new(client, Default::default()));
	let server = UnixStreamPeer::spawn(UnixStreamTransport::new(server, Default::default()));
	Ok((client.into(), server.into()))
}

#[tokio::test]
async fn ping() {
	let_assert!(Ok((client, mut server)) = client_server_pair::<Json>());

	let server = tokio::spawn(async move {
		let_assert!(Ok(camera::ReceivedMessage::Request(camera::ReceivedRequestHandle::Ping(request, ()))) = server.recv_message().await);
		assert!(let Ok(()) = request.send_response(&()).await);
		let_assert!(Err(fizyr_rpc::RecvMessageError::Other(e)) = server.recv_message().await);
		assert!(e.is_connection_aborted());
	});

	assert!(let Ok(()) = client.ping().await);
	drop(client);

	assert!(let Ok(()) = server.await);
}

#[tokio::test]
async fn record() {
	let_assert!(Ok((client, mut server)) = client_server_pair::<Json>());

	let server = tokio::spawn(async move {
		let_assert!(Ok(camera::ReceivedMessage::Request(camera::ReceivedRequestHandle::Record(request, body))) = server.recv_message().await);
		assert!(body.color == true);
		assert!(body.cloud == false);
		assert!(let Ok(()) = request.send_state_update(&camera::RecordState::Recording).await);
		assert!(let Ok(()) = request.send_state_update(&camera::RecordState::Processing).await);
		assert!(let Ok(()) = request.send_image_update(&camera::Image {
			format: 1,
			width: 2,
			height: 3,
			data: vec![0, 1, 2, 3, 4, 5],
		}).await);
		assert!(let Ok(()) = request.send_state_update(&camera::RecordState::Done).await);
		assert!(let Ok(()) = request.send_response(&()).await);
		let_assert!(Err(fizyr_rpc::RecvMessageError::Other(e)) = server.recv_message().await);
		assert!(e.is_connection_aborted());
	});

	let_assert!(Ok(mut sent_request) = client.record(&camera::RecordRequest { color: true, cloud: false }).await);

	let_assert!(Some(Ok(update)) = sent_request.recv_update().await);
	assert!(update.is_state() == true);
	assert!(update.is_image() == false);
	assert!(let Some(camera::RecordState::Recording) = update.as_state());
	assert!(let None = update.as_image());
	assert!(let Ok(camera::RecordState::Recording) = update.into_state());

	let_assert!(Some(Ok(update)) = sent_request.recv_update().await);
	assert!(let Ok(camera::RecordState::Processing) = update.into_state());

	let_assert!(Some(Ok(update)) = sent_request.recv_update().await);
	let_assert!(Ok(image) = update.into_image());
	assert!(image.format == 1);
	assert!(image.width == 2);
	assert!(image.height == 3);
	assert!(image.data == &[0, 1, 2, 3, 4, 5]);

	let_assert!(Some(Ok(update)) = sent_request.recv_update().await);
	assert!(let Ok(camera::RecordState::Done) = update.into_state());

	assert!(let None = sent_request.recv_update().await);

	assert!(let Ok(()) = sent_request.recv_response().await);
	drop(client);

	assert!(let Ok(()) = server.await);
}

#[tokio::test]
async fn record_state() {
	use camera::camera_events;

	fn client_server_pair<F: fizyr_rpc::util::format::Format<Body = fizyr_rpc::StreamBody>>() -> std::io::Result<(camera_events::Client<F>, camera_events::Server<F>)> {
		let (client, server) = tokio::net::UnixStream::pair()?;
		let client = UnixStreamPeer::spawn(UnixStreamTransport::new(client, Default::default()));
		let server = UnixStreamPeer::spawn(UnixStreamTransport::new(server, Default::default()));
		Ok((client.into(), server.into()))
	}

	let_assert!(Ok((client, mut server)) = client_server_pair::<Json>());

	let server = tokio::spawn(async move {
		let_assert!(Ok(camera_events::ReceivedMessage::Stream(msg)) = server.recv_message().await);
		let_assert!(camera_events::StreamMessage::RecordState(state) = msg);
		assert!(state == camera::RecordState::Recording);

		let_assert!(Ok(camera_events::ReceivedMessage::Stream(msg)) = server.recv_message().await);
		let_assert!(camera_events::StreamMessage::RecordState(state) = msg);
		assert!(state == camera::RecordState::Processing);

		let_assert!(Ok(camera_events::ReceivedMessage::Stream(msg)) = server.recv_message().await);
		let_assert!(camera_events::StreamMessage::RecordState(state) = msg);
		assert!(state == camera::RecordState::Done);

		let_assert!(Err(fizyr_rpc::RecvMessageError::Other(e)) = server.recv_message().await);
		assert!(e.is_connection_aborted());
	});

	assert!(let Ok(()) = client.send_record_state(&camera::RecordState::Recording).await);
	assert!(let Ok(()) = client.send_record_state(&camera::RecordState::Processing).await);
	assert!(let Ok(()) = client.send_record_state(&camera::RecordState::Done).await);

	drop(client);
	assert!(let Ok(()) = server.await);
}

#[allow(dead_code, clippy::all)]
fn assert_client_clone<F: Format>(camera: camera::Client<F>) {
	let _ = camera.clone();
}

#[allow(dead_code, clippy::all)]
fn assert_received_request_write_handle_clone<F: Format>(handle: camera::record::ReceivedRequestWriteHandle<F>) {
	let _ = handle.clone();
}

#[allow(dead_code, clippy::all)]
fn assert_sent_request_write_handle_clone<F: Format>(handle: camera::record::SentRequestWriteHandle<F>) {
	let _ = handle.clone();
}

#[test]
fn interface_introspection_camera() {
	let interface = camera::Interface::definition::<Json>();
	assert!(interface.name == "Camera");
	assert!(interface.doc == concat!(
		"Interface to a camera server.\n",
		"\n",
		"A camera server can represent many different types of cameras,\n",
		"like a simple 2D camera, a 3D camera with or without RGB data,\n",
		"or even a line scanner.\n",
	));

	assert!(interface.services.len() == 2);

	assert!(interface.services[0].name == "ping");
	assert!(interface.services[0].service_id == 0);
	assert!(interface.services[0].doc == concat!(
		"Ping the server.\n",
		"\n",
		"A succesful ping indicates that the server is running,\n",
		"but it does not guarantee that it is connected to a camera.\n",
	));
	assert!(interface.services[0].request_body == "()");
	assert!(interface.services[0].response_body == "()");
	assert!(interface.services[0].request_updates.len() == 0);
	assert!(interface.services[0].response_updates.len() == 0);

	assert!(interface.services[1].name == "record");
	assert!(interface.services[1].service_id == 1);
	assert!(interface.services[1].doc == "Record an image.\n");
	assert!(interface.services[1].request_body == "macros_tests::camera::RecordRequest");
	assert!(interface.services[1].response_body == "()");
	assert!(interface.services[1].request_updates.len() == 1);
	assert!(interface.services[1].request_updates[0].name == "cancel");
	assert!(interface.services[1].request_updates[0].doc == "Cancel the recording prematurely.\n");
	assert!(interface.services[1].request_updates[0].service_id == 10);
	assert!(interface.services[1].request_updates[0].body == "macros_tests::camera::CancelReason");

	assert!(interface.services[1].response_updates.len() == 2);
	assert!(interface.services[1].response_updates[0].name == "state");
	assert!(interface.services[1].response_updates[0].doc == concat!(
		"Update sent by the server to notify the client about recording progress.\n",
		"\n",
		"When the record state goes to `RecordState::Processing`,\n",
		"the camera field of view may be obstructed by a robot again.\n",
	));
	assert!(interface.services[1].response_updates[0].service_id == 11);
	assert!(interface.services[1].response_updates[0].body == "macros_tests::camera::RecordState");

	assert!(interface.services[1].response_updates[1].name == "image");
	assert!(interface.services[1].response_updates[1].doc == concat!(
			"Update sent by the server when an image is available.\n",
			"\n",
			"The camera may send multiple image updates depending on the configuration.\n",
	));
	assert!(interface.services[1].response_updates[1].service_id == 12);
	assert!(interface.services[1].response_updates[1].body == "macros_tests::camera::Image");
}

#[test]
fn interface_introspection_camera_events() {
	use camera::camera_events;

	let interface = camera_events::Interface::definition::<Json>();
	assert!(interface.name == "CameraEvents");
	assert!(interface.doc == "");
	assert!(interface.services.len() == 0);
	assert!(interface.streams.len() == 1);

	assert!(interface.streams[0].name == "record_state");
	assert!(interface.streams[0].doc == "Notifications whenever the camera changes record state.\n");
	assert!(interface.streams[0].service_id == 11);
	assert!(interface.streams[0].body == "macros_tests::camera::RecordState");
}
