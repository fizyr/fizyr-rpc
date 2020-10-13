use fizyr_rpc::Server;
use std::path::PathBuf;
use structopt::StructOpt;
use tokio_seqpacket::UnixSeqpacketListener;

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
	// Create a listening socket for the server.
	let socket = UnixSeqpacketListener::bind(&options.socket).map_err(|e| format!("failed to bind to {}: {}", options.socket.display(), e))?;

	// Wrap the socket in an RPC server.
	let mut server = Server::new(socket, Default::default());
	eprintln!("listening on {}", options.socket.display());

	// Run the accept loop.
	// The lambda returns a future that will be spawned in a new task for each peer.
	let result = server.run(|peer| async {
		if let Err(e) = handle_peer(peer).await {
			eprintln!("Error: {}", e);
		}
	});

	// Pass up errors from the accept loop.
	result.await.map_err(|e| format!("error in accept loop: {}", e))?;

	Ok(())
}

/// Handle communication with a single peer.
async fn handle_peer(mut peer: fizyr_rpc::PeerHandle<fizyr_rpc::UnixBody>) -> Result<(), String> {
	eprintln!("new connection accepted");
	loop {
		// Receive the next incoming message.
		let incoming = match peer.next_message().await {
			Ok(x) => x,
			Err(e) => {
				if e.is_connection_aborted() {
					// Log aborted connections but return Ok(()).
					eprintln!("connection closed by peer");
					return Ok(());
				} else {
					// Pass other errors up to the caller.
					return Err(format!("failed to receive message from peer: {}", e));
				}
			},
		};

		// Handle the incoming message.
		match incoming {
			fizyr_rpc::Incoming::Stream(msg) => eprintln!("unspported stream message received: {:?}", msg),
			fizyr_rpc::Incoming::Request(request) => match request.service_id() {
				1 => handle_hello(request).await?,
				n => request
					.send_error_response(&format!("unknown service ID: {}", n))
					.await
					.map_err(|e| format!("failed to send error response message: {}", e))?,
			},
		}
	}
}

async fn handle_hello(request: fizyr_rpc::ReceivedRequest<fizyr_rpc::UnixBody>) -> Result<(), String> {
	// Parse the request body as UTF-8 and print it.
	let message = std::str::from_utf8(&request.body().data).map_err(|_| "invalid UTF-8 in hello message")?;
	eprintln!("received hello request: {}", message);

	// Send a goodbye response.
	request
		.send_response(1, &b"Goodbye!"[..])
		.await
		.map_err(|e| format!("failed to send goodbye response: {}", e))?;

	Ok(())
}
