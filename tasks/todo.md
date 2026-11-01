# Task List: Easy Window MVP

Status: Phase 1 in progress — Tasks 1-5 done
Plan: `tasks/plan.md`
Spec: `docs/SPEC.md`

## Phase 0 — Foundation

- [x] **Task 1: Scaffold Tauri 2 + React/TS**
  - Acceptance: `npm run tauri dev` opens a window; typecheck and build are configured. ESLint and JS tests arrive with the first shared logic (Task 13).
  - Verify: `npm run tauri dev`; `npm run verify`.
  - Files: `package.json`, `vite.config.ts`, `tsconfig*.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `src/main.tsx`.

- [x] **Task 2: App shell + design tokens + AGENTS.md**
  - Acceptance: Sidebar/topbar shell with Overview, Sessions, Processes, Startup, Settings, Audit pages; tokens for color/spacing/type; `AGENTS.md` documents commands and invariants.
  - Verify: Keyboard tab pass; pages render; `AGENTS.md` reviewed.
  - Files: `src/components/layout/*`, `src/styles/tokens.css`, `src/pages/*`, `AGENTS.md`.

- [x] **Task 3: Rust core store + IPC + audit**
  - Acceptance: SQLite migrations create settings/audit tables; typed Tauri commands round-trip; audit events redact secrets.
  - Verify: `cargo test --manifest-path src-tauri/Cargo.toml` (16 tests).
  - Files: `src-tauri/src/store/*`, `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs`, `src-tauri/migrations/*`.

- [x] **Task 4: Verification script**
  - Acceptance: One command runs UI typecheck/build plus Rust fmt/clippy/tests. The extension build joins this in Task 12.
  - Verify: `npm run verify` exits 0.
  - Files: `package.json`.

## Phase 1 — Windows Performance Guard

- [x] **Task 5: Process snapshot + denylist**
  - Acceptance: Snapshot includes PID, name, path, publisher, signature and the
    Windows reason behind a rejection, parent, session, window title, start
    time, and memory; critical processes are flagged with a reason and are
    unselectable, including the ones whose path Windows hides.
  - Verify: `cargo test` (64 tests; 48 of them cover `monitor` and `risk`); compare against
    Task Manager with `cargo run --manifest-path src-tauri/Cargo.toml --example processes`.
  - Files: `src-tauri/src/monitor/process.rs`, `src-tauri/src/risk/denylist.rs`, tests.

- [ ] **Task 6: Per-process telemetry (CPU, disk, GPU)**
  - Design and decisions: `tasks/plan.md` → "Task 6 design".
  - Acceptance: CPU %, disk B/s and GPU % per process; system totals; a metric that was
    not sampled is `None`, never a misleading zero; a machine without `\GPU Engine`
    reports why and stops retrying each tick.
  - Verify: `cargo test`; `cargo run --example metrics` shows per-tick cost and the
    app's own CPU; spot-check three processes against Task Manager.
  - Files: `src-tauri/src/monitor/*`, `src-tauri/Cargo.toml`, tests.

  - [x] **6.1** Expose the CPU and disk rates `sysinfo` already samples.
    - Acceptance: `ProcessSnapshot` carries `cpu_percent`, `disk_read_bytes_per_sec`,
      `disk_write_bytes_per_sec`; the first tick is `None`, a later tick is a number.
    - Verify: unit test on the live snapshot; Task Manager comparison.
    - Done: `cpu_percent` is a share of the machine (a saturated core on this
      8-logical-core PC reads 12.5, matching Task Manager); the `processes`
      example now samples over one second and prints CPU %, MEM, READ/s, WRITE/s.
      Processes whose path Windows hides report `None`, not zero.
    - Files: `src-tauri/src/monitor/process.rs`.

  - [x] **6.2** `monitor/pdh.rs`: reusable query wrapper and availability ladder.
    - Acceptance: one query, wildcard English counters, one collect per tick; every PDH
      status becomes a named outcome, and an absent object is reported as unavailable
      rather than an error; handles close on `Drop`.
    - Verify: example prints `\Process(_Total)\% Processor Time` and the wall time of a
      warm tick for ~290 processes; `cargo test`.
    - Done: `Query` owns an `Owned<PDH_HQUERY>` and `Counter<'q>` carries the query's
      lifetime, so both are released when the query drops. A warm tick for ~270
      instances measures 7–8 ms to collect and 0.8–3.4 ms to read, all named in
      `CounterStatus`; the first read of a fresh wildcard answers "no data yet" and
      primes the next one. `object_instances()` refreshes the object list, costs about
      half a second, and reports a missing object as `NoSuchObject` — so it is a
      probe-once call, never a per-tick one. `\Process(_Total)` reads a capped 100 %,
      confirming decision D1: CPU stays in `sysinfo`.
    - Files: `src-tauri/src/monitor/pdh.rs`, `src-tauri/src/monitor/mod.rs`,
      `src-tauri/Cargo.toml`, `src-tauri/examples/pdh.rs`.

  - [ ] **6.3** `monitor/gpu.rs`: instance-name parser and per-PID aggregation.
    - Acceptance: parses PID, LUID, engine index and engine type, including an empty
      `engtype_` token; aggregation follows the plan's GPU decision.
    - Verify: unit tests over captured real instance names from this machine.
    - Files: `src-tauri/src/monitor/gpu.rs`.

  - [ ] **6.4** `monitor/types.rs` + `monitor/scheduler.rs`: tick loop and system totals.
    - Acceptance: a background tick produces `ProcessMetrics` + `SystemSummary` on the
      configured interval; GPU may tick slower than CPU/disk; the scheduler owns the
      sampler and the signature cache; stopping the loop leaves no thread behind.
    - Verify: `cargo test`; run for 60 s and report the app's own CPU.
    - Files: `src-tauri/src/monitor/types.rs`, `src-tauri/src/monitor/scheduler.rs`.

  - [ ] **6.5** Overhead harness and documentation.
    - Acceptance: `cargo run --example metrics` prints per-tick timing and self CPU; the
      `AGENTS.md` GPU aggregation note matches what the code does.
    - Verify: 10-minute run recorded in the commit message; `npm run verify`.
    - Files: `src-tauri/examples/metrics.rs`, `AGENTS.md`.

- [ ] **Task 7: Process table UI**
  - Acceptance: Sort/filter by CPU/RAM/disk/GPU; sparklines; keyboard accessible; empty/error/loading states.
  - Verify: Manual UI pass; `npm test`; axe check.
  - Files: `src/pages/Processes/*`, `src/components/ProcessTable/*`, `src-tauri/src/commands/processes.rs`.

- [ ] **Task 8: Process risk score**
  - Acceptance: Score explains reasons; no critical process can be marked safe; user can ignore a process.
  - Verify: Unit tests for scoring; manual review of top 20.
  - Files: `src-tauri/src/risk/process_score.rs`, `src/components/RiskBadge/*`, tests.

- [ ] **Task 9: Graceful close + audit**
  - Acceptance: WM_CLOSE first; wait; report result; force close only after confirmation; critical blocked; audit event.
  - Verify: Close a test app; verify exit and audit; attempt critical close.
  - Files: `src-tauri/src/actions/close.rs`, `src-tauri/src/commands/actions.rs`, `src/components/ConfirmDialog/*`.

- [ ] **Task 10: Startup audit**
  - Acceptance: Lists HKCU/HKLM Run keys, Startup folder, scheduled tasks; disable/enable user-level entries with backup; no services.
  - Verify: Disable/re-enable a test Run key; state persists; no admin.
  - Files: `src-tauri/src/actions/startup.rs`, `src-tauri/src/commands/startup.rs`, `src/pages/Startup/*`.

- [ ] **Task 11: Self-overhead**
  - Acceptance: Sampling pauses when hidden; app uses EcoQoS when backgrounded; idle budget met.
  - Verify: 10-minute `Get-Counter` measurement; CPU <1%, RAM <80 MB.
  - Files: `src-tauri/src/monitor/scheduler.rs`, `src-tauri/src/lib.rs`, `src/pages/Settings/*`.

## Phase 2 — Chrome Session Guard

- [ ] **Task 12: Extension scaffold**
  - Acceptance: MV3 extension loads unpacked; service worker registers; popup/options render; build script works.
  - Verify: `npm run build:extension`; load `extension/dist` in Chrome 156.
  - Files: `extension/manifest.json`, `extension/src/background.ts`, `extension/src/popup/*`, `extension/src/options/*`, `package.json`.

- [ ] **Task 13: Cookie metadata scan**
  - Acceptance: Groups by registrable domain; computes risk reasons; never returns cookie values; unit tests with mocked `chrome.cookies`.
  - Verify: `npm test`; manual scan on Chrome 156; inspect output for value fields.
  - Files: `extension/src/cookies/scan.ts`, `extension/src/cookies/risk.ts`, `extension/src/lib/psl.ts`, tests.

- [ ] **Task 14: Session list UI**
  - Acceptance: Sessions page shows domain, flags, age, risk, reasons; search/sort; desktop app receives summary via native host (or shared config).
  - Verify: Manual UI pass; no values in payload/logs.
  - Files: `src/pages/Sessions/*`, `extension/src/options/Sessions.tsx`, `src-tauri/src/chrome/*`.

- [ ] **Task 15: Clear site data + close tabs**
  - Acceptance: Clears cookies + localStorage + IndexedDB + cacheStorage + serviceWorkers for selected origin; closes tabs; audit event.
  - Verify: Local test server logs out; tab closes; audit entry.
  - Files: `extension/src/actions/clear.ts`, `extension/src/actions/close-tabs.ts`, `extension/src/lib/audit.ts`, tests.

- [ ] **Task 16: Auto-logout**
  - Acceptance: Per-domain tab-close rule; idle rule; browser-startup badge count; rules persist across service-worker restarts.
  - Verify: E2E tab close and idle; reopen requires login; badge count.
  - Files: `extension/src/background.ts`, `extension/src/rules.ts`, `extension/src/idle.ts`, tests.

- [ ] **Task 17: Panic logout**
  - Acceptance: Clears all non-whitelisted domains after confirmation phrase; whitelisted remain; SSO warning; audit event.
  - Verify: E2E with two test sites; whitelist one; confirm only the other clears.
  - Files: `extension/src/options/Panic.tsx`, `extension/src/rules.ts`, `extension/src/actions/clear.ts`.

- [ ] **Task 18: Native messaging**
  - Acceptance: Host registers under HKCU; extension connects; config/audit sync; unauthorized origin and malformed messages rejected.
  - Verify: Integration test; inspect `chrome://extensions` errors; remove registry key on uninstall.
  - Files: `src-tauri/src/bin/easy-window-host.rs`, `src-tauri/src/ipc/native.rs`, `extension/src/native.ts`, `scripts/install-native-host.ps1`.

## Phase 3 — Integration and polish

- [ ] **Task 19: Dashboard + onboarding**
  - Acceptance: Overview shows security/performance scores, top actions; onboarding explains tab close vs logout and permissions.
  - Verify: First-run flow; loading/empty/error states; keyboard pass.
  - Files: `src/pages/Overview/*`, `src/pages/Onboarding/*`, `src/components/*`.

- [ ] **Task 20: Tray + shortcuts + settings**
  - Acceptance: Tray menu (pause, panic logout, open); Ctrl+K command palette; settings persist.
  - Verify: Manual tray/shortcut test; restart app; settings retained.
  - Files: `src-tauri/src/tray.rs`, `src/hooks/useHotkeys.ts`, `src/pages/Settings/*`.

- [ ] **Task 21: Security review**
  - Acceptance: IPC fuzz tests; permission audit; privacy policy draft; no secrets in logs; signed-build plan.
  - Verify: Fuzz suite; grep for secrets; review checklist.
  - Files: `tests/security/*`, `docs/PRIVACY.md`, `docs/SECURITY.md`.

- [ ] **Task 22: E2E + accessibility**
  - Acceptance: Playwright extension suite covers scan, clear, auto-logout, panic; axe checks pass.
  - Verify: `npx playwright test`.
  - Files: `tests/e2e/*`, `playwright.config.ts`.

## Phase 4 — Packaging and release

- [ ] **Task 23: Installer + uninstall**
  - Acceptance: NSIS installer; HKCU native-host registration; uninstall removes registry key and local data; no admin.
  - Verify: Clean Windows VM install/uninstall.
  - Files: `src-tauri/tauri.conf.json`, `installer/*`, `docs/INSTALL.md`.

- [ ] **Task 24: Signed updater + release**
  - Acceptance: Signed update manifest; update from previous version; release checklist; extension store assets.
  - Verify: Update test; signature verification; checklist review.
  - Files: `src-tauri/tauri.conf.json`, `docs/RELEASE.md`, `extension/store/*`.

## Checkpoints

- [x] Checkpoint 0: app shell runs; verify passes.
- [ ] Checkpoint 1: Windows monitor works; safe close and startup audit verified.
- [ ] Checkpoint 2: Chrome guard works; no cookie values; auto-logout verified.
- [ ] Checkpoint 3: integrated UX, security review, E2E pass.
- [ ] Checkpoint 4: installer, updater, uninstall verified on clean VM.
