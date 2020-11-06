# Changelog

## v0.3.0 - unreleased
### Changed
- Update to tokio 0.3.

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

### Removed
- Made `RequestTracker` a private implementation detail.

### Changed
- Renamed `into_transport_default()` to `into_default_transport()`.

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
