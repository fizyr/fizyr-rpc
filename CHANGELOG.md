# Changelog

## 0.5.0-alpha4 - 2021-09-27
### Added
- Add missing `write_handle()` functions for `ReceivedRequest` and `SentRequest` in generated interfaces.

## 0.5.0-alpha3 - 2021-09-23
### Fixed
- Make write handles cloneable regardless of their generic parameters.

## 0.5.0-alpha2 - 2021-09-22
### Changed
- Implement `Clone` for the write handles of generated interfaces.

## 0.5.0-alpha1 - 2021-08-31
### Added
- Add `SentRequestWriteHandle` and `ReceivedRequestWriteHandle` to support parallel reading and writing.
- Add `Body::as_error()` and `Body::into_error()` as required functions on the `Body` trait.

### Changed
- Renamed `PeerHandle::next_message()` to `recv_message()`.
- Moved message body out of `ReceivedRequest`.
- Changed `SentRequest/ReceivedRequest::recv_update()` to return an `Option<Message>`.
- Renamed `Incoming` to `ReceivedMessage`.
- Renamed `SentRequest` and `ReceivedRequest` to `SentRequestHandle` and `ReceivedRequestHandle`.
- Switched to a single opaque error type.
- Renamed `Server` to `Listener` to avoid confusion with generated interfaces.

### Removed
- Removed unused `Outgoing` type.
- Removed all old error types and the `error` module.

### Fixed
- Fixed accepting connections with Unix stream sockets.

## v0.4.2 - 2021-05-20
### Fixed
- Fixed bug where a sent request was untracked once a response to a received request was sent.

## v0.4.1 - 2021-04-25
### Fixed
- Fixed reading messages with empty body with the `StreamTransport`.
- Fixed documentation on how to receive messages on a `SentRequest`.

## v0.4.0 - 2021-02-09
### Changed
- Renamed `ReceivedRequest::next_message()` to `recv_update()`.
- Split `SentRequest::next_message()` in `recv_update()` and `recv_response()`.
- Renamed `NextMessageError` to `RecvMessageError`.

### Removed
- Removed `error::ProcessIncomingMessageError` from public the API.

## v0.3.1 - 2021-01-27
### Changed
- Updated to tokio-seqpacket 0.5.

## v0.3.0 - 2020-12-25
### Changed
- Updated to tokio 1.0 and tokio-seqpacket 0.4.
- Removed meaningless addresses from Accept implementation for Unix sockets.

## v0.2.1 - 2020-09-03
### Fixed
- Fixed link in README.

## v0.2.0 - 2020-09-03
### Added
- Added `Peer::connect` function.
- Added `Server::bind` function.

### Changed
- Changed body date to use `Vec<u8>` instead of `Box<[u8]>`.
- Moved transport traits and implementations to `transport` module.
- Moved some traits to `util` module.
- Renamed `into_transport_default()` to `into_default_transport()`.

### Removed
- Made `RequestTracker` a private implementation detail.

## v0.1.4 - 2020-10-04
### Fixed
- Regenerate `README.md` from library documentation.

## v0.1.3 - 2020-10-13
### Fixed
- Fixed link of `docs.rs` badge in README.

## v0.1.2 - 2020-10-13
### Fixed
- Fixed links to documentation in README.

## v0.1.1 - 2020-10-13
### Fixed
- Build `docs.rs` documentation with all transports enabled.

## v0.1.0 - 2020-10-13
- Initial release.
