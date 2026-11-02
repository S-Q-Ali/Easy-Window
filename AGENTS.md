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
audit log, and `npm run verify`. Phase 1 is in progress: Task 5 delivered the
read-only process snapshot (`src-tauri/src/monitor/process.rs`) and the
critical-process denylist (`src-tauri/src/risk/denylist.rs`); Task 6 is
complete — the PDH wrapper, the GPU engine parser and sampler, the tick shapes
in `monitor/types.rs`, and `monitor/scheduler.rs`, which ticks both clocks and
publishes each snapshot, plus `cargo run --release --example metrics`, the
overhead harness that measures what that loop costs. Task 7 is code-complete: the
loop runs inside the app (`commands/processes.rs`), `latest_tick()` answers the
first frame and every tick is emitted on `metrics://tick`, and the Processes
page now paints that tick — `src/components/ProcessTable/` sorts on any metric,
filters by name or PID, prints an unmeasured figure as an em dash, marks the
GPU column with the tick that actually read it, and draws one trend line per row
on the column being read, fed by the 60-sample buffer in `src/lib/history.ts`.
All of it runs under a Vitest + Testing Library + axe stack that `npm run verify`
now runs (58 UI tests). A real Chromium has timed a tick through the table —
~124 ms of render and layout for 300 rows in the production bundle — so the
table keeps up without virtualisation. What is left in Task 7 is not code: the
manual pass in `npm run tauri dev`, which alone can see the paint (a hidden
measurement page produces no frame) and tune the table's `max-height` constant
to the shell's real geometry. Task 8 (process risk score) is designed, not built:
`tasks/plan.md` → "Task 8 design", decisions D1–D8 awaiting approval — the score
lands in Rust on the tick (never JS, because Task 9 acts on it), a denylisted or
unverifiable process short-circuits to `Protected` before any arithmetic, and the
spec's "idle > 10 min" gets a sampler-side quiet streak because no idle time
exists in the snapshot today.

## Commands

```bash
npm install
npm run tauri dev          # dev app with HMR
npm run tauri build        # installer
npm run typecheck
npm test                   # vitest: table sorting, formatting, filters, axe on the rendered table
npm run verify             # typecheck + vitest + build + fmt + clippy + cargo test
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo run --release --manifest-path src-tauri/Cargo.toml --example metrics   # 60 s overhead record; pass seconds to lengthen it
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
  components/         shell, page header, empty state, icons, ProcessTable/
  lib/                metrics.ts (tick shapes), history.ts (60-sample per-process buffer)
  pages/              Overview, Sessions, Processes, Startup, Settings, Audit
  styles/             tokens.css + global.css
src-tauri/            Rust core + Tauri app
  migrations/         SQL, applied via PRAGMA user_version
  src/store/          SQLite settings + audit (tested)
  src/monitor/        snapshot, PDH wrapper, GPU sampler, scheduler — all live
  src/commands/       latest_tick() and the emit loop Tauri sees
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
- Without elevation, `ProcessIdToSessionId` and `QueryFullProcessImageNameW` both fail for Windows-protected processes, so session and path are missing for exactly the processes that must never be closed. The share measured here varies with what is running: ~75 of 280 in the Task 5 capture, 131 of 274 on average over the 600-second Task 6 one — plan on nearly half the table having no CPU or disk figure. `CriticalReason::IdentityUnavailable` blocks them; a missing path is never harmless.
- Signature checks dominate a snapshot: ~8 s cold, ~0.5 s warm with one shared `SignatureCache`. Extra worker threads do not help; Windows serialises `WinVerifyTrust` and catalog calls.
- Expired-but-genuine signatures exist here (Git's `bash.exe` verifies `CERT_E_EXPIRED`). Carry the reason, do not report a bare `invalid`.
- The app must stay lightweight: pause sampling when hidden, use EcoQoS for itself, keep idle CPU <1% and RAM <80 MB. Measured over 600 s in release (`--example metrics`): mean 0.96% CPU and 19 MB peak RAM, but 230 of 599 ticks land at or over 1% and the worst reads 5.8% — the budget holds on average and not per tick, which is exactly what the pause/EcoQoS work has to fix. Mean tick wall time is 372 ms of a 1 s interval, and one tick overran it by four times, so nothing downstream may assume a steady 1 Hz. Note what that figure covers: the harness calls `scheduler.tick()` and never emits, so it excludes serializing and pushing the ~106 KB tick, which since 7.2 happens on the same thread. The release cost with emit is not measured; the debug build's webview observed 47 ticks in 60 s — 1.25–1.6 s apart — which is the number a UI consumer should design around.
- UI cost is measured, and the dev bundle lies: in Chromium with the shipped production build, one tick through a 300-row `ProcessTable` costs ~79 ms of React render plus ~45 ms of layout (~124 ms, worst 168), roughly linear in rows (~0.26 ms/row) — about 10 % of a 1.25 s interval, which is why there is no virtualisation. The same table in `npm run tauri dev` costs ~614 ms per tick, five times heavier, so a sluggish table in dev is not a shipped-app defect. Paint has never been timed: the measurement ran in a hidden 0×0 viewport that produced no frame.
- Extension build uses **WXT** (MIT). CRXJS has no SPDX license — do not add it.
- `nickshui/Win-Top` has **no LICENSE file**: ideas only, never copy code or assets.
- `Moustachauve/cookie-editor` is GPL-3.0 and `sienori/Tab-Session-Manager` is MPL-2.0: do not copy code into this MIT project.
- Per-process GPU % = **sum of every engine of that PID over the sample interval** (the Task Manager model), not a max. Keep the engine type (`3D`, `VideoDecode`, `Copy`, and the empty `engtype_` case) and the per-engine breakdown for the tooltip. Process Hacker reports max-per-PID, which is why its number reads lower than Task Manager's for the same process.
