# Easy Window

[![CI](https://github.com/S-Q-Ali/Easy-Window/actions/workflows/ci.yml/badge.svg)](https://github.com/S-Q-Ali/Easy-Window/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
![Platform](https://img.shields.io/badge/platform-Windows%2011-blue)

A lightweight Windows 11 desktop utility with two jobs:

1. **Chrome Session Guard** — find the websites that still have a live login
   session, and clear them in one click or automatically.
2. **Windows Performance Guard** — find background processes wasting CPU, RAM,
   disk, or GPU, and close or throttle them safely.

Closing a browser window is not a logout. The session cookie stays in the
profile. Easy Window makes those lingering sessions visible and lets you end
them for real — without ever decrypting cookie values or passwords.

## Status

Under active development, and there is no released build yet — you would be
building from source.

What exists today: the desktop shell with its six pages, the local SQLite store
with a redacted audit log, and a read-only process snapshot that identifies each
process by PID, start time, executable path and signature, and refuses to treat
a critical or unverifiable process as something it may close. The telemetry
numbers behind the process table, the Chrome extension, and the packaging work
are still ahead; `tasks/todo.md` is the honest record of that.

## Try it

Building requires Windows 11, a current Node, and Rust with the MSVC toolchain.

```bash
npm install
npm run tauri dev     # the desktop app with hot reload
npm run verify        # typecheck + build + fmt + clippy + cargo test
cargo run --manifest-path src-tauri/Cargo.toml --example processes   # snapshot dump
```

## Stack

- **Desktop:** Tauri + Rust, with a React + TypeScript + Vite front end
- **Browser:** Chrome MV3 extension (built with WXT) and a native messaging host
- **Store:** SQLite, local only

Exact dependency versions live in `package.json` and `src-tauri/Cargo.toml`.

## Principles

- **Local only.** No account, no cloud, no telemetry by default.
- **No secrets.** Cookie metadata only — never cookie values, passwords, or
  browsing history content.
- **No bypasses.** Uses the official `chrome.cookies` and `chrome.browsingData`
  APIs. Chrome's App-Bound Encryption stays intact.
- **No auto-kill.** Destructive actions need a preview, a confirmation, and an
  audit event. Critical Windows processes are never touched.
- **Light by design.** The app must not become the problem: idle under 1% CPU
  and 80 MB RAM.

## Documentation

| File | Contents |
|---|---|
| [`docs/SPEC.md`](docs/SPEC.md) | MVP scope, threat model, success criteria |
| [`docs/CAPABILITY_MAP.md`](docs/CAPABILITY_MAP.md) | Module boundaries and build order |
| [`docs/RESEARCH.md`](docs/RESEARCH.md) | Prior art: which repos to depend on, adapt, or avoid |
| [`tasks/plan.md`](tasks/plan.md) | Phased implementation plan |
| [`tasks/todo.md`](tasks/todo.md) | Task breakdown with acceptance criteria |
| [`AGENTS.md`](AGENTS.md) | Invariants and gotchas for contributors |
| [`CONTRIBUTING.md`](CONTRIBUTING.md) | Setup, rules that are not negotiable, review expectations |
| [`SECURITY.md`](SECURITY.md) | How to report, what the project treats as a security requirement |
| [`CHANGELOG.md`](CHANGELOG.md) | What has landed on `main` |

## License

MIT. See [`LICENSE`](LICENSE).
