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
  - [x] **7.1** Serialize the published shapes.
    - Done: `Tick`, `ProcessMetrics`, `SystemSummary` and `GpuTelemetry` now derive
      `Serialize` following the convention Task 5 already set for the rows — camelCase
      fields, snake_case enum variants. `ProcessSnapshot`, `SignatureStatus` and
      `CriticalReason` already had it, so the plan's claim that nothing was wire-ready was
      wrong and is corrected. `GpuTelemetry` is the shape that carries data, so it
      serializes as a `kind`-tagged union (`{"kind":"sampling","enginesSampled":307,...}`)
      that a TypeScript reader can discriminate without inferring absence from a missing
      field. Two tests prove the key names and that an unmeasured metric arrives as `null`,
      not `0` — D2 carried across the wire.
    - Measured: the harness serializes each tick outside the tick's own timing, and a
      90-second release record gives **104–111 KB, mean 106 KB per tick** at 274–289 rows —
      about 6.4 MB a minute into the webview at 1 Hz. Same record: first tick 11.5 s, mean
      tick 222.8 ms (22.3 % duty), own CPU mean 0.884 % with 24 of 89 ticks at or over the
      1 % budget and a 3.289 % peak, 20 MB peak RAM. 105 tests pass.
    - Files: `src-tauri/src/monitor/types.rs`, `src-tauri/examples/metrics.rs`,
      `tasks/plan.md`.
  - [x] **7.2** Wire the loop to the window.
    - Done: `commands.rs` became `commands/mod.rs` plus `commands/processes.rs`, and the
      move is its own commit (`7391211`) so the wiring stays reviewable. `Scheduler::spawn`
      now takes a sink and hands it every tick it publishes before waiting for the next
      interval — the app emits from inside the loop rather than polling the shared slot, so
      no tick can be missed between polls. `Metrics::start(app.handle())` owns the loop in
      managed state and a loop that cannot start becomes
      `{"state":"failed","reason":"…"}` instead of aborting the app: the other pages still
      open and the table is told why it has no numbers. Dropping that state drops the
      control channel's sender, which is how the loop stops at exit. The state holds the
      loop in a `Mutex` because `RunningScheduler` is `Send` but not `Sync` (an
      `mpsc::Sender` is not `Sync`), so it cannot be managed bare. `latest_tick()` answers
      the first frame with the newest published tick, and `SamplingState` serializes with
      the same convention as the rows: `state` tag, camelCase fields, `tick: null` meaning
      "alive, nothing measured yet" — not an error. `lib.rs` registers the commands in two
      `#[cfg]`-gated `generate_handler!` chains because that macro cannot hold a cfg'd
      entry, so the sampling command exists only where the sampler does.
    - Measured: proven in the running app, with `latest_tick` and a `metrics://tick`
      listener mounted temporarily on the Processes page and read over WebView2's CDP port
      (the patch was reverted; nothing of it is committed). The pull answered
      `state=running, tick=present` **113 ms** after mount, so the table can be populated
      on first paint instead of one tick later. Pushes arrived **whole** — 303–311 rows in
      every payload, no truncation of the 106 KB figure — and the page saw **47 distinct
      ticks in 60 s**, i.e. **1.25–1.6 s apart** in a debug build. That is the Task 6.5
      overrun showing up at the consumer: the UI must treat the stream as "as fast as the
      loop manages", never as a metronome. 110 Rust tests pass, clippy clean.
    - Review found two things worth carrying, both recorded here rather than deferred: the
      372 ms release figure does **not** cover this path (the harness never emits, and the
      app now serializes and pushes ~106 KB on the tick thread — the release cost with emit
      is unmeasured, and that is the number Task 11's budget work must start from), and a
      pushed tick's `selectable` flag is a display figure from up to a second ago, so Task 9
      must re-resolve identity and re-run the denylist at action time. That is now written
      into Task 9's acceptance and into the `SamplingState::Running` doc.
    - One already-committed Task 6 test broke under this change rather than because of it:
      `a_gpu_report_is_stamped_with_the_tick_that_read_it_and_carried_after` asserted that
      the tick after a GPU read still carried that read's stamp, which only holds while a
      tick is shorter than the 1.4 s GPU interval. Two more live loops in the suite made the
      preceding tick run longer than that, so the next tick legally read the counter again
      and the stamp moved. It now accepts the reading tick's stamp or its own and still
      rejects a stamp no recent tick produced: the wall-clock assumption was the bug, and the
      repo already records that a tick can overrun its interval four times.
    - Files: `src-tauri/src/commands/processes.rs`, `src-tauri/src/commands/mod.rs`,
      `src-tauri/src/monitor/scheduler.rs`, `src-tauri/src/lib.rs`.
  - [x] **7.3** `ProcessTable`: sortable headers on CPU/RAM/disk/GPU/name, name+PID filter,
    em dash for `None` sorting last, the stale-GPU marker from `gpuSequence`, and
    empty/loading/error states. Real buttons with `aria-sort`.
    - Done: `src/lib/metrics.ts` mirrors the wire shapes T7.1 froze — declarations only,
      with a note that the Rust tests are the authority on the key names. The table reads a
      tick and nothing else: sort and filter are `useMemo`s over `tick.processes`, so
      re-sorting costs no IPC, and each row is keyed by `pid:startTime` (T4) — the test that
      feeds two rows with PID 4712 and different start times gets two rows, not a React
      key collision. Every metric is `formatPercent`/`formatBytes`/`formatBytesPerSec`,
      which answer an em dash for `null` and never a `0` (T6), and `compareRows` parks a
      `null` at the end whichever way the column runs — a numeric `0` still sorts as a
      real measurement, because only the missing path, not a quiet process, produces `null`.
      The default sort is CPU descending, so the heaviest user is on top without a click.
      A `null` `cpuPercent` is counted by `summary.unmeasuredProcesses` and said in plain
      words above the table, because on this machine about half the rows are unmeasured and
      a wall of em dashes otherwise looks like a broken read. The GPU column carries its
      staleness in the header: a visual `#12` with the sentence in a `visually-hidden`
      span, shown only when `gpuSequence < sequence`, and the `warming`/`unavailable`
      states get their own line — `unavailable` passes the Rust reason straight through.
      Loading, error and no-match states live in the page and the table respectively; the
      page keeps a numbered-tick rule (`apply` ignores a tick older than the one on screen)
      so the pull cannot overwrite an event that arrived first.
    - Not measured: no virtualisation, on the plan's arithmetic — 274–311 rows is a
      `useMemo`'d sort and a re-render, not an IPC round trip, and the harness already
      records the tick arriving 42 ms to 4.1 s apart, so the table never has to keep up
      with 1 Hz by itself. Whether 300 rows actually paint inside a tick is a question for
      the live window, which has not been opened.
    - Files: `src/lib/metrics.ts`, `src/components/ProcessTable/*`, `src/pages/Processes.tsx`.
    - Outstanding: the manual pass in `npm run tauri dev` — the live window, not jsdom — has
      not been done, and the scroll region's `max-height: calc(100vh - 260px)` is an
      untuned constant that pass should replace with whatever the shell actually leaves.
  - [ ] **7.4** Sparklines: webview ring buffer keyed by `pid`+`startTime`, last 60 samples
    per metric, inline SVG marked hidden from assistive tech with the number in text.
  - [x] **7.5** UI test stack: Vitest + Testing Library + axe as dev dependencies, an
    `npm test` script wired into `npm run verify`, and table tests for sorting, the `None`
    em dash, keyboard access and recycled-PID row identity.
    - Done: vitest 5.0.5 + jsdom 30.1.3 + Testing Library React 16.3.3 + axe-core 4.14.0,
      `vitest.config.ts` with `globals: true` (Testing Library registers its teardown
      through the global `afterEach`) and `include: src/**/*.test.{ts,tsx}`, so Rust-only
      code is not picked up. `npm test` runs `vitest run` and `npm run verify` now calls it
      between `typecheck` and `build` — the gap the plan recorded as an open question is
      closed, and the verify line is the one thing CI and a human both run. 37 tests across
      4 files: formatting, the comparator, the table, and the page's subscription to the
      loop. The table suite covers all four behaviours the task named plus `aria-sort` on
      every column, case-insensitive name and PID filters, the clear-filter path, both GPU
      note states, the stale marker on and off, and one `axe.run` over the rendered table
      with a stale GPU header — it asserts an empty violation list, so a regression fails
      with the rule and the offending HTML rather than a score.
    - Files: `package.json`, `package-lock.json`, `vitest.config.ts`, `tsconfig.json`,
      `tsconfig.node.json`.
  - Files: `src/pages/Processes/*`, `src/components/ProcessTable/*`,
    `src-tauri/src/commands/processes.rs`, `src-tauri/src/monitor/types.rs`.

- [ ] **Task 8: Process risk score**
  - Acceptance: Score explains reasons; no critical process can be marked safe; user can ignore a process.
  - Verify: Unit tests for scoring; manual review of top 20.
  - Files: `src-tauri/src/risk/process_score.rs`, `src/components/RiskBadge/*`, tests.

- [ ] **Task 9: Graceful close + audit**
  - Acceptance: WM_CLOSE first; wait; report result; force close only after confirmation; critical blocked; audit event.
  - Acceptance (from 7.2): the close path re-resolves PID + creation time + path + signature and re-runs
    `denylist::assess` at action time. A tick is a display figure and can be a second old — in that
    time a PID can exit and be recycled — so the `selectable` flag in a pushed tick is never
    sufficient authority to act.
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
