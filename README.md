# Easy Window

A lightweight Windows 11 desktop utility with two jobs:

1. **Chrome Session Guard** — find the websites that still have a live login
   session, and clear them in one click or automatically.
2. **Windows Performance Guard** — find background processes wasting CPU, RAM,
   disk, or GPU, and close or throttle them safely.

Closing a browser window is not a logout. The session cookie stays in the
profile. Easy Window makes those lingering sessions visible and lets you end
them for real — without ever decrypting cookie values or passwords.

## Status

Early development. Spec and plan are written; implementation starts with the
Windows telemetry slice.

## Stack

- **Desktop:** Tauri 2 + Rust, React 19 + TypeScript + Vite
- **Browser:** Chrome MV3 extension (WXT), native messaging host
- **Store:** SQLite (local only)

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

## Build

Commands will be finalized once the scaffold lands.

```bash
npm install
npm run tauri dev
npm run verify
```

## License

MIT. See [`LICENSE`](LICENSE).
