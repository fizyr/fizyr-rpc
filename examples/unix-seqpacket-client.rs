use fizyr_rpc::IntoTransport;
use fizyr_rpc::Peer;

use std::path::PathBuf;
use structopt::StructOpt;
use tokio_seqpacket::UnixSeqpacket;

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
	// Connect a socket to the server.
	let socket = UnixSeqpacket::connect(&options.socket)
		.await
		.map_err(|e| format!("failed to connect to {}: {}", options.socket.display(), e))?;

	// Wrap the socket in a transport, and create a peer from the transport.
	let mut peer = Peer::spawn(socket.into_transport_default());

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
			let message = std::str::from_utf8(&message.body.data).map_err(|_| "invalid UTF-8 in response")?;
			eprintln!("Received response: {}", message);
			break;
		}
	}

	Ok(())
}
