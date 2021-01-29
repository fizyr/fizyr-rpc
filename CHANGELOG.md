# Changelog

## Unreleased
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
