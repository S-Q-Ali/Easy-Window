# Contributing to Easy Window

## Before you write code

1. Read `docs/SPEC.md` (scope and threat model), `AGENTS.md` (invariants and
   gotchas), and `tasks/todo.md` (what is scheduled).
2. Open or comment on an issue describing the change. A feature that has no
   task and no approved plan will not be merged — the plan is the review.
3. Branch from `main`. Keep branches short-lived and small enough to review in
   one sitting.

## Setup

The app targets Windows 11; the Rust side uses the `windows` crate behind
`cfg(windows)`, so a Windows machine is required to run what you build.

```bash
npm install
npm run tauri dev     # the desktop app with hot reload
npm run verify        # typecheck + build + cargo fmt + clippy -D warnings + cargo test
```

Prerequisites: a current Node, Rust stable with the MSVC toolchain, and the
WebView2 runtime that ships with Windows 11. `docs/INSTALL.md` will document the
installer path once packaging exists.

## The rules that are not negotiable

Repeated from `AGENTS.md`, because these are the ones a well-meaning change
breaks:

- Never read or store cookie values, passwords, browsing-history content, or
  process command lines. Metadata only, and redact what gets logged.
- Never bypass Chrome's App-Bound Encryption or use CDP against the default
  profile. `chrome.cookies` and `chrome.browsingData` are the sanctioned path.
- Never auto-kill. Destructive actions need preview, confirmation, and an audit
  event, and a process whose identity cannot be verified is not a candidate.
- No network, no elevation, and no new dependencies that are GPL, MPL, or
  unlicensed. MIT and Apache-2.0 are fine to depend on; still do not copy code
  from a repo without a license file, even when the idea is what you wanted.
- The app must stay light: idle under 1% CPU and 80 MB RAM, sampling paused when
  the window is hidden.

## Commits and pull requests

- One logical change per commit, with a message that explains *why*. `fix:`,
  `feat:`, `refactor:`, `test:`, `docs:`, `chore:`.
- Do not mix formatting with behaviour, and do not bundle a dependency upgrade
  with a feature.
- CI must be green. If a gate fails, fix the code — do not disable the rule or
  skip the test.
- For UI work, say in the PR what you clicked and what you saw. A green suite
  proves the code compiles, not that the screen works.
- For telemetry work, include measured numbers.

## Tests

Rust tests run with `cargo test --manifest-path src-tauri/Cargo.toml`. Tests
that describe live Windows behaviour are marked and allowed to skip on a machine
that cannot provide it — but a test must never skip silently while pretending to
assert. If a fixture is missing, fail with the path in the message.

JavaScript tests and axe checks arrive with the first shared browser logic;
until then, the UI gate is typecheck and build.
