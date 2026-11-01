# Security Policy

Easy Window reads things that other tools do not: which websites hold a live
login session, and which processes are running on the machine. That is the
whole reason it exists, and the whole reason it has a threat model.

## Report a vulnerability privately

Use [GitHub private vulnerability reporting](https://github.com/S-Q-Ali/Easy-Window/security/advisories/new).
Do not open a public issue, and do not post reproduction steps in a normal
issue thread.

This project is a single-maintainer, pre-release effort with no bug-bounty
programme and no fixed SLA. You will get a reply, and credit if you want it.

## The properties this project treats as security requirements

They are stated in `docs/SPEC.md` and enforced in `AGENTS.md`. A report that any
of them is broken is a real bug, not a design complaint.

- **No cookie values, passwords, or browsing-history content** are ever read,
  stored, or logged. The Chrome side uses `chrome.cookies` metadata and
  `chrome.browsingData` only. Chrome's App-Bound Encryption is not bypassed,
  and the Cookie SQLite database is not decrypted.
- **No process command lines or environment blocks** are collected. Process
  identity is PID, creation time, executable path, and signature — never the
  name alone.
- **No network by default.** The app phones home for nothing; updates are a
  separate, signed, opt-in path.
- **No admin by default.** No service stopping, no registry cleaning, no system
  tweaks. Anything requiring elevation is out of scope for the MVP.
- **No auto-kill.** A destructive action needs a preview, an explicit
  confirmation, and an audit event. Critical Windows processes and anything
  whose identity cannot be verified are never offered as candidates.
- **IPC is untrusted input.** Every native-messaging and Tauri command message
  is validated; the native host accepts an exact `allowed_origins` list, and its
  named pipe is accessible to the current user only.

## Out of scope

- Attacks that need code already running as you, physical access to the
  machine, or a compromised Chrome profile.
- Anything that would require elevation to exploit; the app does not have
  elevation.
- Vulnerabilities in Chrome, the WebView2 runtime, or Windows itself — report
  them to the vendor, then tell us if a fix has to land here.
- Cookie or session handling on a specific website. Easy Window clears site
  data; it does not control what a server does with a session token.
- "Compromised" detection claims. The tool scores risk from metadata; it never
  claims to prove an account is safe or taken over.

## Known pre-release gaps

Nothing here is a secret, and none of it should surprise a reviewer:

- Builds are unsigned, and there is no updater, so there is nothing to verify
  a download against yet. A signed build and a signed update manifest are
  release tasks (`tasks/todo.md`, Phase 4).
- The extension is distributed unpacked. The Chrome Web Store review for broad
  cookie permissions has not been run.
- IPC fuzzing and the accessibility pass are scheduled tasks, not completed
  work.

## Hardening we already run

`npm run verify` (typecheck, production build, `cargo fmt`, clippy with
`-D warnings`, Rust tests) runs on every push and pull request, and Dependabot
opens weekly update PRs for npm, cargo, and Actions.
