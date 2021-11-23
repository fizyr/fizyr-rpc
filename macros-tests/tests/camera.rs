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
