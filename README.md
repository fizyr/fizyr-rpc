[![Docs.rs](https://docs.rs/fizyr-rpc/badge.svg)](https://docs.rs/crate/fizyr-rpc/)
[![Tests](https://github.com/fizyr-private/fizyr-rpc-rs/workflows/tests/badge.svg)](https://github.com/fizyr/fizyr-rpc-rs/actions?query=workflow%3Atests+branch%3Amain)

# fizyr-rpc

Rust implementation of the Fizyr RPC procotol.

The Fizyr RPC protocol is a request/response protocol,
with bi-directional feedback as long as a request is open.
Additionally, you can send individual stream messages that do not initiate a request.

## Overview

### Peer and PeerHandle

As a user of the library, you will mostly be using the [`PeerHandle`][PeerHandle] object.
The [`PeerHandle`][PeerHandle] is used to interact with a remote peer.
It is used to send and receive requests and stream messages.
It can also be split in a [`PeerReadHandle`][PeerReadHandle] and a [`PeerWriteHandle`][PeerWriteHandle],
to allow moving the handles into different tasks.
The write handle can also be cloned and used in multiple tasks.

To obtain a [`PeerHandle`][PeerHandle], you must first create a [`Peer`][Peer] object.
The [`Peer`][Peer] object is responsible for reading and writing messages with the peer,
but you can't use it for sending or receiving messages directly.
Instead, you must ensure that the [`Peer::run()`][Peer::run] future is being polled.
The easiest way to do that is by calling [`Peer::spawn(...)`][Peer::spawn],
which will run the future in a background task and returns a [`PeerHandle`][PeerHandle].

### Server

The [`Server`][Server] struct is used to accept incoming connections
and gives you a [`PeerHandle`][PeerHandle] for each incoming connection.
You can then use the handle to process incoming messages and to send messages to the peer.
Usually, you will want to spawn a task for each accepted connection that handles the communication.

### Transports

Each peer internally uses a [`Transport`][Transport].
The transport is responsible for reading and writing raw messages.
By abstracting away the message transport,
the library can expose a single generic [`Peer`][Peer] and [`Server`][Server] struct.

There are different transports for different socket types.
Different transports may also use different types as message body.
For example, the [`TcpTransport`][TcpTransport] and [`UnixStreamTransport`][UnixStreamTransport]
use messages with a [`StreamBody`][StreamBody].
This [`StreamBody`][StreamBody] body type contains raw bytes.

The [`UnixSeqpacketTransport`][UnixSeqpacketTransport] has messages with a [`UnixBody`][UnixBody],
which allows you to embed file descriptors with each message.

## Features

The library uses features to avoid unnecessarily large dependency trees.
Each feature corresponds to a different transport type.
None of the features are enabled by default.
Currently, the library has these features:

* `tcp`: for the [`TcpTransport`][TcpTransport]
* `unix-stream`: for the [`UnixStreamTransport`][UnixStreamTransport]
* `unix-seqpacket`: for the [`UnixSeqpacketTransport`][UnixSeqpacketTransport]

[Peer]: https://docs.rs/fizyr-rpc/latest/fizyr-rpc/struct.Peer.html
[Peer::run]: https://docs.rs/fizyr-rpc/latest/fizyr-rpc/struct.Peer.html#method.run
[Peer::spawn]: https://docs.rs/fizyr-rpc/latest/fizyr-rpc/struct.Peer.html#method.spawn
[PeerHandle]: https://docs.rs/fizyr-rpc/latest/fizyr-rpc/struct.PeerHandle.html
[PeerReadHandle]: https://docs.rs/fizyr-rpc/latest/fizyr-rpc/struct.PeerReadHandle.html
[PeerWriteHandle]: https://docs.rs/fizyr-rpc/latest/fizyr-rpc/struct.PeerWriteHandle.html
[Server]: https://docs.rs/fizyr-rpc/latest/fizyr-rpc/struct.Server.html

[Transport]: https://docs.rs/fizyr-rpc/latest/fizyr-rpc/trait.Transport.html
[TcpTransport]: https://docs.rs/fizyr-rpc/latest/fizyr-rpc/type.TcpTransport.html
[UnixStreamTransport]: https://docs.rs/fizyr-rpc/latest/fizyr-rpc/type.UnixStreamTransport.html
[UnixSeqpacketTransport]: https://docs.rs/fizyr-rpc/latest/fizyr-rpc/type.UnixSeqpacketTransport.html

[StreamBody]: https://docs.rs/fizyr-rpc/latest/fizyr-rpc/struct.StreamBody.html
[UnixBody]: https://docs.rs/fizyr-rpc/latest/fizyr-rpc/struct.UnixBody.html
