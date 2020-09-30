use fizyr_rpc::Server;
use std::path::PathBuf;
use structopt::StructOpt;
use tokio::net::UnixListener;

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
	let socket = UnixListener::bind(&options.socket)
		.map_err(|e| format!("failed to bind to {}: {}", options.socket.display(), e))?;

	let mut server = Server::new(socket, Default::default());
	eprintln!("listening on {}", options.socket.display());
	let result = server.run(|peer| async {
		if let Err(e) = handle_peer(peer).await {
			eprintln!("Error: {}", e);
		}
	});

	result.await.map_err(|e| format!("error in accept loop: {}", e))?;

	Ok(())
}

async fn handle_peer(mut peer: fizyr_rpc::PeerHandle<fizyr_rpc::StreamBody>) -> Result<(), String> {
	eprintln!("new connection accepted");
	loop {
		let incoming = match peer.next_message().await {
			Ok(x) => x,
			Err(e) => if e.is_connection_aborted() {
				eprintln!("connection closed by peer");
				return Ok(());
			} else {
				return Err(format!("failed to receive message from peer: {}", e));
			},
		};

		if let fizyr_rpc::Incoming::Request(request) = incoming {
			let service_id = request.service_id();
			if service_id == 1 {
				handle_hello(request).await?;
			} else {
				request
					.send_error_response(&format!("unknown service ID: {}", service_id))
					.await
					.map_err(|e| format!("failed to send error response message: {}", e))?;
			}
		}
	}
}

async fn handle_hello(request: fizyr_rpc::ReceivedRequest<fizyr_rpc::StreamBody>) -> Result<(), String> {
	let message = std::str::from_utf8(request.body()).map_err(|_| "invalid UTF-8 in hello message")?;
	eprintln!("received hello request: {}", message);

	request
		.send_response(1, &b"Goodbye!"[..])
		.await
		.map_err(|e| format!("failed to goodbye response: {}", e))?;

	Ok(())
}
