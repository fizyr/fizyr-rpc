use fizyr_rpc::UnixSeqpacketPeer;

use std::path::PathBuf;

#[derive(clap::Parser)]
#[clap(setting = clap::AppSettings::DeriveDisplayOrder)]
struct Options {
	socket: PathBuf,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
	if let Err(e) = do_main(&clap::Parser::parse()).await {
		eprintln!("Error: {}", e);
		std::process::exit(1);
	}
}

async fn do_main(options: &Options) -> Result<(), String> {
	// Connect to a remote server.
	let (peer, info) = UnixSeqpacketPeer::connect(&options.socket, Default::default()).await
		.map_err(|e| format!("failed to connect to {}: {}", options.socket.display(), e))?;
	if let Some(pid) = info.process_id() {
		eprintln!("Connected to process {} of user {}", pid, info.user_id());
	} else {
		eprintln!("Connected to process of user {}", info.user_id());
	}

	// Send a request to the remote peer.
	let mut request = peer
		.send_request(1, &b"Hello World!"[..])
		.await
		.map_err(|e| format!("failed to send request: {}", e))?;

	while let Some(update) = request.recv_update().await {
		let message = std::str::from_utf8(&update.body.data).map_err(|_| "invalid UTF-8 in update")?;
		eprintln!("Received update: {}", message);
	}

	let response = request
		.recv_response()
		.await
		.map_err(|e| format!("failed to read message: {}", e))?;
	// Parse the message body as UTF-8, print it and exit the loop.
	let message = std::str::from_utf8(&response.body.data).map_err(|_| "invalid UTF-8 in response")?;
	eprintln!("Received response: {}", message);

	Ok(())
}
