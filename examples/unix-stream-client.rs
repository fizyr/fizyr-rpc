use fizyr_rpc::StreamPeer;
use std::path::PathBuf;
use structopt::StructOpt;
use tokio::net::UnixStream;

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
	let socket = UnixStream::connect(&options.socket)
		.await
		.map_err(|e| format!("failed to connect to {}: {}", options.socket.display(), e))?;

	let mut peer = StreamPeer::spawn(socket, Default::default()).await;

	let mut request = peer.send_request(1, &b"Hello World!"[..])
		.await
		.map_err(|e| format!("failed to send request: {}", e))?;

	loop {
		let message = request.next_message().await.map_err(|e| format!("failed to read message: {}", e))?;
		if message.header.message_type.is_response() {
			let message = std::str::from_utf8(&message.body).map_err(|_| "invalid UTF-8 in response")?;
			eprintln!("Received response: {}", message);
			break;
		}
	}

	Ok(())
}
