use fizyr_rpc::UnixStreamPeer;

use std::path::PathBuf;
use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt(setting = structopt::clap::AppSettings::ColoredHelp)]
#[structopt(setting = structopt::clap::AppSettings::UnifiedHelpMessage)]
#[structopt(setting = structopt::clap::AppSettings::DeriveDisplayOrder)]
struct Options {
	socket: PathBuf,
}

#[tokio::main]
async fn main() {
	if let Err(e) = do_main(&Options::from_args()).await {
		eprintln!("Error: {}", e);
		std::process::exit(1);
	}
}

async fn do_main(options: &Options) -> Result<(), String> {
	// Connect to a remote server.
	let mut peer = UnixStreamPeer::connect(&options.socket, Default::default()).await
		.map_err(|e| format!("failed to connect to {}: {}", options.socket.display(), e))?;

	// Send a request to the remote peer.
	let mut request = peer
		.send_request(1, &b"Hello World!"[..])
		.await
		.map_err(|e| format!("failed to send request: {}", e))?;

	loop {
		// Receive the next message.
		// This could be an update or the final response message.
		let message = request.next_message().await.map_err(|e| format!("failed to read message: {}", e))?;

		// Ignore anything but the response.
		if message.header.message_type.is_response() {
			// Parse the message body as UTF-8, print it and exit the loop.
			let message = std::str::from_utf8(&message.body).map_err(|_| "invalid UTF-8 in response")?;
			eprintln!("Received response: {}", message);
			break;
		}
	}

	Ok(())
}
