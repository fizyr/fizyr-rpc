use fizyr_rpc::TcpListener;

#[derive(clap::Parser)]
#[clap(setting = clap::AppSettings::DeriveDisplayOrder)]
struct Options {
	#[clap(default_value = "[::]:12345")]
	bind: String,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
	if let Err(e) = do_main(&clap::Parser::parse()).await {
		eprintln!("Error: {}", e);
		std::process::exit(1);
	}
}

async fn do_main(options: &Options) -> Result<(), String> {
	// Create the server.
	let mut server = TcpListener::bind(options.bind.as_str(), Default::default())
		.await
		.map_err(|e| format!("failed to bind to {}: {}", options.bind, e))?;
	eprintln!("Listening on {}", options.bind);

	// Run the accept loop.
	// The lambda returns a future that will be spawned in a new task for each peer.
	let result = server.run(|peer, info| async move {
		eprintln!("Accepted connection from: {}", info.remote_address());
		if let Err(e) = handle_peer(peer).await {
			eprintln!("Error: {}", e);
		}
	});

	// Pass up errors from the accept loop.
	result.await.map_err(|e| format!("error in accept loop: {}", e))?;

	Ok(())
}

/// Handle communication with a single peer.
async fn handle_peer(mut peer: fizyr_rpc::PeerHandle<fizyr_rpc::StreamBody>) -> Result<(), String> {
	eprintln!("new connection accepted");
	loop {
		// Receive the next incoming message.
		let incoming = match peer.recv_message().await {
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
			fizyr_rpc::ReceivedMessage::Stream(msg) => eprintln!("unspported stream message received: {:?}", msg),
			fizyr_rpc::ReceivedMessage::Request(request, body) => match request.service_id() {
				1 => handle_hello(request, body).await?,
				n => request
					.send_error_response(&format!("unknown service ID: {}", n))
					.await
					.map_err(|e| format!("failed to send error response message: {}", e))?,
			},
		}
	}
}

async fn handle_hello(request: fizyr_rpc::ReceivedRequestHandle<fizyr_rpc::StreamBody>, body: fizyr_rpc::StreamBody) -> Result<(), String> {
	// Parse the request body as UTF-8 and print it.
	let message = std::str::from_utf8(&body).map_err(|_| "invalid UTF-8 in hello message")?;
	eprintln!("received hello request: {}", message);

	// Send a goodbye response.
	request
		.send_response(1, &b"Goodbye!"[..])
		.await
		.map_err(|e| format!("failed to send goodbye response: {}", e))?;

	Ok(())
}
