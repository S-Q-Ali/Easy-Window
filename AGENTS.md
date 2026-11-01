# AGENTS.md — Easy Window

## Read first

- `docs/SPEC.md` — MVP spec, threat model, success criteria.
- `docs/CAPABILITY_MAP.md` — module boundaries and build order.
- `docs/RESEARCH.md` — which GitHub repos to depend on, adapt, reference, or avoid.
- `tasks/plan.md` and `tasks/todo.md` — current phase and next task.

## What this is

Windows 11 desktop utility with two modules:

1. **Chrome Session Guard** — MV3 extension + native host. Finds persistent logged-in website sessions and clears site data on demand or automatically.
2. **Windows Performance Guard** — Tauri/Rust app. Per-process CPU/RAM/disk/GPU telemetry, safe close/throttle, startup audit.

Stack: **Tauri 2 + Rust + React 19/TypeScript/Vite**; Chrome MV3 extension; SQLite local store.

## Current status

Phase 0 is complete: Tauri 2 shell, React UI, design tokens, SQLite store,
audit log, and `npm run verify`. Phase 1 has started: Task 5 delivers the
read-only process snapshot (`src-tauri/src/monitor/process.rs`) and the
critical-process denylist (`src-tauri/src/risk/denylist.rs`), not wired to the
UI yet. Next is Task 6 (PDH telemetry); the snapshot feeds it.

## Commands

```bash
npm install
npm run tauri dev          # dev app with HMR
npm run tauri build        # installer
npm run typecheck
npm run verify             # typecheck + build + fmt + clippy + cargo test
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path src-tauri/Cargo.toml --check
```

## Hard invariants

- **Never decrypt Chrome cookies/passwords or bypass App-Bound Encryption.** Chrome 156 uses `v20`; CDP on the default profile is blocked since Chrome 136. Use `chrome.cookies` metadata and `chrome.browsingData`.
- **Never read/store cookie values, passwords, browsing-history content, or process command lines.** Metadata only; redact logs.
- **Never kill critical Windows processes/services or auto-kill.** Graceful close first, explicit confirmation, audit event.
- **No network by default.** No telemetry. Updates only via signed manifests.
- **No admin by default.** No service stopping, registry cleaning, or system tweaks.
- `browsingData.remove({ origins })` clears the **whole registrable domain**; warn for SSO and whitelist sensitive domains.
- Process identity = **PID + creation time + executable path + signature**, never name alone.
- Validate every IPC message; native host uses exact `allowed_origins`; named pipe ACL is current-user only.
- Destructive actions require preview, confirmation, and an audit event.

## Repo map

```text
src/                  React UI
  components/         shell, page header, empty state, icons
  pages/              Overview, Sessions, Processes, Startup, Settings, Audit
  styles/             tokens.css + global.css
src-tauri/            Rust core + Tauri app
  migrations/         SQL, applied via PRAGMA user_version
  src/store/          SQLite settings + audit (tested)
  src/monitor/        process snapshot (done), PDH/ETW telemetry (next)
  src/actions/        close, priority, startup
  src/ipc/            native messaging + named pipe
  src/risk/           scoring and denylist
  src/bin/            easy-window-host.exe
extension/            Chrome MV3 extension (WXT)
docs/                 spec, capability map, research, privacy/security
tasks/                plan + task list
tests/e2e/            Playwright extension tests
```

## Gotchas

- Environment tested: Windows 11 build 26300, Chrome 156, WebView2 156, Rust 1.99, Node 24, .NET 8.
- Per-process GPU: PDH `\GPU Engine(*)\Utilization Percentage`; instance names contain `pid_<PID>`.
- PDH timing (measured, see `tasks/plan.md`): Windows refreshes performance data ~once per second, the first read of a freshly added wildcard returns "no data yet" and must be treated as a priming read, and a reused query with one `collect` per tick costs ~7 ms + ~1 ms read for ~270 process instances. `monitor::pdh::object_instances` refreshes the object list and takes ~0.5 s — probe once, never per tick.
- Chrome cookie DB: `%LOCALAPPDATA%\Google\Chrome\User Data\<profile>\Network\Cookies`. If Chrome is running, copy `Cookies`, `Cookies-wal`, and `Cookies-shm` before reading.
- Native messaging manifest: `HKCU\Software\Google\Chrome\NativeMessagingHosts\<name>` with an exact extension ID in `allowed_origins`.
- Chrome Web Store broad cookie permissions are a release risk; unpacked extension is the MVP path.
- Without elevation, `ProcessIdToSessionId` and `QueryFullProcessImageNameW` both fail for Windows-protected processes (~75 of 280 here), so session and path are missing for exactly the processes that must never be closed. `CriticalReason::IdentityUnavailable` blocks them — a missing path is never harmless.
- Signature checks dominate a snapshot: ~8 s cold, ~0.5 s warm with one shared `SignatureCache`. Extra worker threads do not help; Windows serialises `WinVerifyTrust` and catalog calls.
- Expired-but-genuine signatures exist here (Git's `bash.exe` verifies `CERT_E_EXPIRED`). Carry the reason, do not report a bare `invalid`.
- The app must stay lightweight: pause sampling when hidden, use EcoQoS for itself, keep idle CPU <1% and RAM <80 MB.
- Extension build uses **WXT** (MIT). CRXJS has no SPDX license — do not add it.
- `nickshui/Win-Top` has **no LICENSE file**: ideas only, never copy code or assets.
- `Moustachauve/cookie-editor` is GPL-3.0 and `sienori/Tab-Session-Manager` is MPL-2.0: do not copy code into this MIT project.
- Per-process GPU % = **sum of every engine of that PID over the sample interval** (the Task Manager model), not a max. Keep the engine type (`3D`, `VideoDecode`, `Copy`, and the empty `engtype_` case) and the per-engine breakdown for the tooltip. Process Hacker reports max-per-PID, which is why its number reads lower than Task Manager's for the same process.
