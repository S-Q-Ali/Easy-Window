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
  - Verify: `cargo test` (78 tests; 62 of them cover `monitor` and `risk`); compare against
    Task Manager with `cargo run --manifest-path src-tauri/Cargo.toml --example processes`.
  - Files: `src-tauri/src/monitor/process.rs`, `src-tauri/src/risk/denylist.rs`, tests.

- [x] **Task 6: Per-process telemetry (CPU, disk, GPU)**
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

  - [x] **6.3** `monitor/gpu.rs`: instance-name parser and per-PID aggregation.
    - Acceptance: parses PID, LUID, engine index and engine type, including an empty
      `engtype_` token; aggregation follows the plan's GPU decision.
    - Verify: unit tests over captured real instance names from this machine.
    - Done: the parser is anchored token by token, so the engine type is the verbatim
      tail — `3D`, `GDI Render` (a space), `Security` and the empty string all survive,
      and LUID case is not assumed. Aggregation sums every sampled engine of a PID
      across LUIDs (the Task Manager model the user chose), keeps the per-engine
      breakdown for the tooltip, counts unsampled engines, and reports a process whose
      engines are all unsampled as `None` rather than zero. Names that do not parse
      land in `GpuReport::unparsed` instead of disappearing. 14 tests run against
      names captured from this machine; the `pdh` example parsed all 307 live
      instances with zero unparsed, ~1 ms to aggregate per tick.
    - Files: `src-tauri/src/monitor/gpu.rs`, `src-tauri/examples/pdh.rs`.

  - [x] **6.4** `monitor/types.rs` + `monitor/scheduler.rs`: tick loop and system totals.
    - Acceptance: a background tick produces `ProcessMetrics` + `SystemSummary` on the
      configured interval; GPU may tick slower than CPU/disk; the scheduler owns the
      sampler and the signature cache; stopping the loop leaves no thread behind.
    - Verify: `cargo test`; run for 60 s and report the app's own CPU.
    - Done: the scheduler owns both samplers and the one signature cache, snapshots
      processes every second and GPU engines every two, and publishes a whole `Tick` into
      a shared slot, so a reader clones the newest tick instead of waiting on a sample.
      GPU figures carry the sequence of the tick that read them: a two-second-old engine
      number is never presented as a one-second-old one, `Warming` keeps the previous
      report and its stamp, and a machine that stops answering loses the GPU column
      rather than showing numbers next to "no GPU counter". `spawn` runs the loop on one
      thread and `stop` joins it, so no thread outlives the handle. `ProcessSampler` grew
      `system_cpu_percent()` for the total, `None` until `sysinfo` has two windows to
      compare. 103 tests pass, 8 of them here. What the loop costs to run is measured in
      6.5, not claimed here.
      One measured fact to carry into Task 7: about half the table reports no CPU because
      Windows would not give those processes' paths — 131 of 274 rows on average, confirmed
      by the 6.5 record, not the ~75 of 280 `AGENTS.md` originally recorded.
    - Files: `src-tauri/src/monitor/types.rs`, `src-tauri/src/monitor/scheduler.rs`,
      `src-tauri/src/monitor/process.rs`, `src-tauri/src/monitor/gpu.rs`.

  - [x] **6.5** Overhead harness and documentation.
    - Acceptance: `cargo run --example metrics` prints per-tick timing and self CPU; the
      `AGENTS.md` GPU aggregation note matches what the code does.
    - Verify: 10-minute run recorded in the commit message; `npm run verify`.
    - Done: `src-tauri/examples/metrics.rs` drives `Scheduler::tick()` at the loop's own
      cadence — read from `DEFAULT_PROCESS_INTERVAL` rather than a number copied into the
      harness — and separates the cold first tick, which verifies every signature, from the
      steady state. It prints per-tick wall time (min/median/mean/max), the sampling
      process's own CPU and peak RAM against the budget, duty cycle, the GPU verdict
      counts, and the min/mean/max of the row count and the unmeasured rows, because a
      peak alone hid that 271-of-292 was a one-tick spike.
    - Measured over 600 s on this machine (2026-11-02): first tick 8.26 s, then 599 ticks
      at min 41.9 ms / median 259.9 ms / mean 372.0 ms / max 4.07 s — a 37.2 % duty on a
      one-second interval. Own CPU mean 0.963 %, median 0.916 %, peak 5.769 %, with 230 of
      599 ticks at or over the 1 % budget: the average is inside it, individual ticks are
      not, so Task 11 cannot treat this number as settled. Peak RAM 19 MB against 80 MB.
      Rows 260/274/302 (min/mean/max); rows with no CPU figure 129/131/286, which confirms
      the 6.4 reading — the 286 is the first tick, which has no window to measure over.
      GPU: 599 ticks collected, one did not (the first, `Warming`), peak 318 engines
      sampled, 11 unsampled, none unparsed.
    - Two facts to carry forward: a tick can overrun its own interval by four times, so the
      UI must not assume 1 Hz, and the harness prints the cadence from the scheduler so the
      two cannot drift apart.
    - Files: `src-tauri/examples/metrics.rs`, `src-tauri/src/monitor/scheduler.rs`,
      `AGENTS.md`.

- [ ] **Task 7: Process table UI**
  - Design and decisions: `tasks/plan.md` → "Task 7 design" (T1–T9 approved 2026-11-02).
  - Acceptance: Sort/filter by CPU/RAM/disk/GPU; sparklines; keyboard accessible; empty/error/loading states.
  - Verify: Manual UI pass in the running app; `npm test` (Vitest + Testing Library + axe on the table); `npm run verify`.
  - Facts this task must design around: 260–302 rows, mean 274; about half of them (131)
    have no CPU or disk figure and must not render as zero; a tick arrives anywhere between
    42 ms and 4.1 s after the last one, so nothing may assume 1 Hz; the cold first tick
    takes 8.3 s, which is the loading state.
  - [ ] **7.1** Serialize the published shapes: `serde` derives with camelCase keys on
    `Tick`, `ProcessMetrics`, `ProcessSnapshot`, `SystemSummary`, `GpuTelemetry` and the two
    enums, a round-trip test on the JSON keys, and the metrics harness printing bytes per
    tick. Rust only.
  - [ ] **7.2** Wire the loop to the window: `commands.rs` becomes `commands/mod.rs` plus
    `commands/processes.rs` with `latest_tick()`; `setup()` spawns the scheduler into
    managed state and each tick emits `metrics://tick`. Prove the emit reaches the webview
    from the sampling thread.
  - [ ] **7.3** `ProcessTable`: sortable headers on CPU/RAM/disk/GPU/name, name+PID filter,
    em dash for `None` sorting last, the stale-GPU marker from `gpuSequence`, and
    empty/loading/error states. Real buttons with `aria-sort`.
  - [ ] **7.4** Sparklines: webview ring buffer keyed by `pid`+`startTime`, last 60 samples
    per metric, inline SVG marked hidden from assistive tech with the number in text.
  - [ ] **7.5** UI test stack: Vitest + Testing Library + axe as dev dependencies, an
    `npm test` script wired into `npm run verify`, and table tests for sorting, the `None`
    em dash, keyboard access and recycled-PID row identity.
  - Files: `src/pages/Processes/*`, `src/components/ProcessTable/*`,
    `src-tauri/src/commands/processes.rs`, `src-tauri/src/monitor/types.rs`.

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
