# Changelog

All notable changes to Easy Window are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

This project has no released build yet. Everything below is what has landed on
`main`; the first tagged release will give it a version number.

## [Unreleased]

### Added

- Tauri 2 desktop shell with the six navigation pages (Overview, Sessions,
  Processes, Startup, Settings, Audit) and design tokens for colour, spacing
  and type.
- Local SQLite store with migrations, typed settings commands, and an audit log
  that redacts secret-shaped values before writing.
- Read-only process snapshot: PID, parent, name, executable path, publisher,
  Authenticode or catalog signature status, session, window title, start time
  and memory.
- The reason behind a rejected signature (`CERT_E_EXPIRED` and friends) instead
  of a bare "invalid", so an expired-but-genuine binary is explainable.
- Critical-process denylist: System Idle, the System process, anything Windows
  marks critical, protected system binaries matched on name **and** directory,
  session-0 services, and any process whose identity cannot be verified.
- Manual snapshot dump (`cargo run --manifest-path src-tauri/Cargo.toml --example processes`)
  for comparing against Task Manager.
- `npm run verify` as the single local gate: typecheck, production build,
  `cargo fmt`, clippy with `-D warnings`, and `cargo test`.
- CI on every push and pull request: UI job on Linux, Rust job on Windows, with
  dependency caching. Dependabot weekly for npm, cargo and Actions.
- Contributor, security and changelog documents, and issue and pull-request
  templates.

### Changed

- Signature results are shared across snapshots and keyed by path, size and
  modification time, which cut a warm snapshot from about 8 seconds to a
  fraction of a second on a machine with roughly 290 processes.
- Window titles are read from the window manager instead of sending
  `WM_GETTEXT` to the owning thread, so a hung process can no longer stall the
  snapshot.

### Fixed

- Catalog hash calculations rewind the file handle before every pass; the
  SHA-1 fallback previously hashed only whatever the SHA-256 pass left unread.
- An unverifiable process (no readable executable path) is now blocked rather
  than offered as a close candidate.

[Unreleased]: https://github.com/S-Q-Ali/Easy-Window/commits/main
