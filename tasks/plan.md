# Implementation Plan: Easy Window MVP

Status: Draft for approval
Last updated: 2026-11-01
Spec: `docs/SPEC.md`
Task list: `tasks/todo.md`

## Overview

Build a Windows 11 desktop utility with two vertical slices:

1. **Windows Performance Guard** — live per-process CPU/RAM/disk/GPU telemetry, safe close/throttle actions, startup audit.
2. **Chrome Session Guard** — MV3 extension that finds persistent logged-in sessions and clears them on demand or automatically.

The desktop app is a Tauri 2 shell with a React/TypeScript UI. A separate Rust native-messaging host bridges Chrome to the desktop app. The extension works standalone for enforcement; the desktop app is the control panel and system monitor.

## Architecture decisions

- **Tauri 2 + Rust** for a small installer and low idle footprint. A performance tool must not be heavy.
- **React 19 + TypeScript + Vite** for a fast, accessible dashboard. Radix primitives for dialogs/tooltips/tabs; TanStack Table for dense process/session tables.
- **Chrome MV3 extension** is the only safe way to read cookie metadata and clear site data. No cookie decryption, no CDP on the default profile.
- **Separate native host binary** (`easy-window-host.exe`) so Chrome can launch it over stdio without starting the GUI. The host talks to the GUI over a current-user named pipe.
- **SQLite local store** for settings and audit. No cookie values, no passwords, no command lines.
- **No admin by default.** Elevated actions are out of MVP scope.
- **No network by default.** Telemetry is off; updates are the only optional network path.

## Task list

### Phase 0 — Foundation

- [x] Task 1: Scaffold Tauri 2 + React/TS + Vite; configure typecheck and build. ESLint and JS tests arrive with the first shared logic.
- [x] Task 2: Add `AGENTS.md`, design tokens, app shell, and honest empty pages.
- [x] Task 3: Implement Rust `core` store, typed IPC, audit log, migrations.
- [x] Task 4: Add the `verify` script and CI-ready local checks.

### Checkpoint 0

- [x] `npm run tauri dev` opens the app.
- [x] `npm run verify` passes.
- [x] No secrets, no network calls, no cookie values.

### Phase 1 — Windows Performance Guard

- [x] Task 5: Process snapshot with `sysinfo`, publisher/signature/path, critical-process denylist.
- [x] Task 6: PDH sampling for GPU, CPU and disk rates from `sysinfo`; system summary; sampling scheduler. Design in "Task 6 design" below.
- [ ] Task 7: Tauri commands + process table UI with sorting, filtering, sparklines.
- [ ] Task 8: Process risk scoring, "safe candidate" explanations, user feedback.
- [ ] Task 9: Graceful close action, confirmation dialog, audit event.
- [ ] Task 10: Startup audit read + disable/enable with backup; no services.
- [ ] Task 11: Self-overhead budget, pause when hidden, EcoQoS for self.

### Checkpoint 1

- [ ] Top 20 processes visible with live CPU/RAM/disk/GPU.
- [ ] Critical processes cannot be closed.
- [ ] Test app closes gracefully and action is audited.
- [ ] Startup test entry can be disabled and re-enabled.
- [ ] Idle overhead under 1% CPU and 80 MB RAM over 10 minutes.

### Phase 2 — Chrome Session Guard

- [ ] Task 12: Extension scaffold (MV3), permissions, popup/options shell, build script.
- [ ] Task 13: Cookie metadata scan, PSL grouping, risk scoring, unit tests.
- [ ] Task 14: Session list in extension options + desktop Sessions page.
- [ ] Task 15: Clear site data + close tabs + audit.
- [ ] Task 16: Auto-logout on tab close, idle, browser-startup badge.
- [ ] Task 17: Panic logout, whitelist, SSO warnings, confirmation phrase.
- [ ] Task 18: Native-messaging config sync, connection status, HKCU registration.

### Checkpoint 2

- [ ] Extension loads unpacked in Chrome 156.
- [ ] Test site logout clears cookies + localStorage + IndexedDB.
- [ ] Auto-logout on tab close works; reopening requires login.
- [ ] Panic logout clears non-whitelisted sites only.
- [ ] No cookie values read, logged, or transmitted.
- [ ] Unauthorized extension origin is rejected.

### Phase 3 — Integration and polish

- [ ] Task 19: Unified dashboard, onboarding, loading/empty/error states.
- [ ] Task 20: Tray icon, keyboard shortcuts, settings persistence.
- [ ] Task 21: Security review, IPC fuzzing, permission audit, privacy policy draft.
- [ ] Task 22: Playwright extension E2E suite + accessibility checks.

### Checkpoint 3

- [ ] First-run onboarding explains tab close vs logout.
- [ ] All destructive actions require confirmation.
- [ ] Keyboard-only pass and axe checks pass.
- [ ] E2E suite passes on Windows 11 + Chrome 156.

### Phase 4 — Packaging and release

- [ ] Task 23: Installer, native-host registration, uninstall cleanup.
- [ ] Task 24: Signed updater, release checklist, extension store assets.

### Checkpoint 4

- [ ] Clean Windows VM install/uninstall succeeds without admin.
- [ ] Update from previous version verifies signatures.
- [ ] Uninstall removes the native-host registry key and local data.
- [ ] Release checklist complete.

## Task 6 design: per-process telemetry

Status: proposed, waiting for approval. Sub-tasks are in `tasks/todo.md`.

### Measured facts (this machine, 2026-11-01)

- Windows 11 build 26300, 8 logical processors, ~290 processes.
- `\Process(*)\% Processor Time`, `\Process(*)\IO Read Bytes/sec` and
  `\Process(*)\IO Write Bytes/sec` exist; 273 `Process` instances enumerate in 26 ms.
- `\GPU Engine(*)\Utilization Percentage` exists with **336** instances. Names look like
  `\\HOST\GPU Engine(pid_1076_luid_0x00000000_0x0000cd43_phys_0_eng_1_engtype_videodecode)\utilization percentage`
  and the `engtype_` token is sometimes empty (`eng_10_engtype_` on this box).
- A PowerShell wildcard sample of both objects costs 1.8–2.9 s per call, but that
  recreates the counter set every call. A reused PDH query is the documented
  low-cost path and its real cost is the first thing sub-task 6.2 measures.
- `sysinfo` 0.40 `refresh_processes()` already requests memory, **cpu** and
  **disk_usage** on every call (`sysinfo-0.40.1/src/common/system.rs:316-331`), so CPU %
  and disk rates are already being collected by the Task 5 sampler and only need
  exposing. Its Windows backend uses `GetProcessTimes` + `GetSystemTimes` deltas and
  `GetProcessIoCounters` (`sysinfo-0.40.1/src/windows/process.rs:48-49`).
- `windows` 0.62 exposes the PDH functions we need under the
  `Win32_System_Performance` feature; `PDH_COUNTER_PATH_ELEMENTS_W` is the struct name
  (there is no `_T` alias) and `PdhSetCounterTimeScale` is **not** in the bindings.

### Corrected by the documentation pass

- **There is no `PDH_CSTATUS_NEW_INSTANCE`.** The real status pair is
  `PDH_CSTATUS_VALID_DATA` (valid, possibly unchanged) and `PDH_CSTATUS_NEW_DATA`
  (valid and changed). Only those two count as a sample; every other CStatus is skipped
  per item, never propagated as an error — PDH functions return plain `u32` status codes.
- **Engine types are not a fixed list.** This box reports `3D`, `VideoDecode`,
  `VideoProcessing`, `Copy`, `LegacyOverlay`, `GDI Render` (the token **contains a
  space**), `Security`, and the empty string. The parser must take everything after
  `engtype_` verbatim, and `\GPU Process Memory` instances end at `_phys_0` with no
  `_eng_` suffix at all — that object is where per-PID VRAM (`Dedicated Usage`,
  `Shared Usage`) lives, useful for Task 7 but not required now.
- **Buffer units differ by call:** TCHAR counts for `PdhEnumObjectItemsW` and
  `PdhMakeCounterPathW`, **bytes** for `PdhGetFormattedCounterArrayW`. The array call
  also warns that a too-small input size does not reliably report the needed size, so
  the probe always starts from zero or keeps a high-water buffer.
- **`PdhEnumObjectItemsW` results are cached**, so a "GPU Engine is missing" verdict must
  come after `PdhEnumObjectsW(bRefresh = TRUE)` — otherwise a driver installed while the
  app runs stays invisible.
- **`PDH_FMT_NOCAP100` is not in the bindings.** It only matters for
  `\Process(*)\% Processor Time`, which PDH silently caps at 100 — decision D1 avoids
  that trap by taking CPU from `sysinfo` instead.
- **Cost proxy:** `typeperf`-style sampling of `\Process(*)` ran ~3.2 s and
  `\GPU Engine(*)` ~5.2 s for two samples, but those runs are startup-dominated. The
  in-app number, with a reused query and one collect, is what 6.2 measures — and 6.2
  has now measured it, below.
- **Fallback if wildcard parsing proves fragile:** `D3DKMTQueryStatistics`
  (`windows::Wdk::Graphics::Direct3D`, feature `Wdk_Graphics_Direct3D`) attributes GPU
  time straight to a PID with no name parsing, and is what System Informer uses
  (`gpumon.c`, which also **sums** running time across nodes).

### Measured by the 6.2 wrapper (same machine)

`src-tauri/src/monitor/pdh.rs` with one reused query, two English counters added once,
and five one-second ticks; run it with `cargo run --example pdh`.

- **A warm tick is cheap:** collecting `\Process(*)` (~270 instances) takes 7.0–8.1 ms
  and reading the formatted array 0.83–3.4 ms. That is ~11 ms per tick for every process
  on the machine, against the 1.8–2.9 s a recreated PowerShell sample cost — D3's reused
  query is what closes the gap.
- **The first read of a fresh wildcard answers "no data yet"** (`PDH_NO_DATA` for the
  value, `PDH_CSTATUS_INVALID_DATA` for the array) while PDH builds the instance list. A
  sampler primes with one read and uses the value from the tick after the next collect.
- **Windows recomputes performance data about once per second.** Two collects closer
  than that hand back the same snapshot, which is why the tests sleep 1 500 ms and why
  D4's 2 s GPU cadence is not a rounding choice.
- **Per-item status is load-bearing:** 2–4 instances report `PDH_CSTATUS_INVALID_DATA`
  every tick because processes disappear as the interval closes. A number without its
  status would forward those gaps as measurements.
- **`\Process(_Total)\% Processor Time` read 100.0 % on an idle-ish machine** on every
  tick — the silent cap D1 predicted, confirmed rather than assumed. CPU stays in
  `sysinfo`.
- **`object_instances()` costs 0.44–0.59 s** (278–307 GPU instances) because the ladder
  refreshes with `PdhEnumObjectsW(bRefresh = TRUE)`. It is a probe-once call, never a
  per-tick one (D5).
- **A missing object is a named outcome:** an unknown object name answers "the
  performance object is not available on this machine", not a status code.
- **Name pointers are bounded by the buffer, not by trust:** `PdhGetFormattedCounterArrayW`
  writes the instance names into the tail of the same allocation, so a name is read only
  up to that allocation's end. Without the bound, an unterminated name would be followed
  for 32 768 units past it.
- **The 6.3 parser reads every live name:** all 307 `\GPU Engine` instances on this
  machine parse, and grouping them by PID takes ~1 ms per tick (3 LUIDs, ~21 processes).
  The 2026-11-01 capture was 287 names over 20 PIDs with the strict shape
  `pid_<n>_luid_0x_-prefixed-hex_0x_-prefixed-hex_phys_<n>_eng_<n>_engtype_<verbatim>`;
  type counts varied between captures (60 empty types then, ~80 now), which is exactly
  why the type is kept as a string, never an enum.
- **The 6.4 loop, measured over 60 s:** the default clocks (1 s process, 2 s GPU) in
  release cost a mean **0.65 % CPU**, max 0.99 %, and **19 MB** peak RAM. A warm tick is
  ~101 ms of wall time against a cold first tick of **10.1 s**, which is the Task 5
  signature cost, not the sampling cost: the wall time is mostly waiting inside Windows,
  since the process's own CPU stayed under 1 % while it ran. GPU engines sampled 296,
  unsampled 0, unparsed 0; 3 of the 60 ticks were warming, as D4's first-collect rule
  predicts. Ten minutes of the same loop reads worse — see the 6.5 bullet, which
  supersedes the budget claim this one implied.
- **The 6.5 harness over 600 s is the budget number:** 599 steady ticks after an 8.26 s
  cold first tick, at min 41.9 ms / median 259.9 ms / mean **372.0 ms** / max **4.07 s** —
  37.2 % duty on a one-second interval, and one tick that overran it four times. Own CPU
  mean **0.963 %**, median 0.916 %, peak **5.769 %**, with **230 of 599 ticks at or over
  1 %**: the average is inside the budget, individual ticks are not, so Task 11's
  pause-when-hidden and EcoQoS work is what makes the target true rather than this loop.
  Peak RAM **19 MB** against 80 MB, with room to spare. Rows 260/274/302; rows without a
  CPU figure 129/131/286, where the 286 is the first tick having no window to measure
  over — the steady figure is the ~131 of 274 that Task 7 must design around. GPU: 599
  ticks collected and one that did not (the first, `Warming`), peak 318 engines sampled,
  11 unsampled, none unparsed.
- **The machine total agrees with the OS:** `sysinfo`'s figure read 88.8–90.6 % in that
  minute while `Get-Counter '\Processor(_Total)\% Processor Time'` read 95.4–97.0 % and
  `'\Processor Information(_Total)\% Processor Time'` read 86.6 %. The two OS counters
  disagree with each other on this CPU, so `sysinfo` is within the spread — the
  per-process column is not expected to add up to it either.

### Sources

PDH lifecycle, statuses and paths: `PdhOpenQuery`/`PdhAddEnglishCounterW`,
`PdhCollectQueryData`, `PdhGetFormattedCounterArrayW`, `PdhEnumObjectItemsW`,
`PdhEnumObjectsW`, `PdhMakeCounterPathW`, the PDH error-code list, and "Checking Counter
Data Status Codes" on learn.microsoft.com. GPU aggregation practice: System Informer
`plugins/ExtendedTools/gpumon.c` and TaskExplorer `GpuCounters.c`. CPU/disk alternative:
`GetProcessTimes` and `GetSystemTimes` (note kernel time includes idle).

### Decisions

- **D1 — PDH is only for GPU.** CPU % and disk B/s come from `sysinfo`, which already
  computes the two-sample delta, normalises by core count, and is what Microsoft
  recommends for CPU time. Adding a PDH CPU counter would create a second, disagreeing
  source for the same number.
- **D2 — every metric is `Option<T>`, and `None` means "not sampled".** A zero from a
  counter that has no sample yet is indistinguishable from an idle process, and the risk
  score in Task 8 must not treat the two alike.
- **D3 — one reused query per object, wildcard counter, single collect per tick.**
  Counter handles are never rebuilt per tick; that is both the documented pattern and
  the difference between ~2 ms and ~2 s.
- **D4 — GPU runs on its own slower cadence** (default 2 s vs 1 s for the process
  snapshot) because 336 engine instances is the expensive part, not the PID count.
- **D5 — availability is probed once, then reported.** If `\GPU Engine` is absent
  (no WDDM/driver support), the sampler records the reason and stops trying until the
  next long retry, instead of failing on every tick.

### Decisions taken by the user (2026-11-01)

- **GPU aggregation: sum per PID** — the Task Manager model, normalised by the sample
  interval, with the per-engine breakdown kept for the tooltip. `AGENTS.md` said "max,
  not a sum"; it now says sum, because that is the number the user will compare against.
  Process Hacker's max-per-PID stays as the explanation for why other tools read lower.
- **Cadence: process snapshot (CPU + disk) every 1 s, GPU every 2 s.** 336 engine
  instances is the expensive part on this machine, not the ~290 PIDs. The interval stays
  a settings value by Task 20.

### Sub-tasks

1. **6.1 Expose what `sysinfo` already samples.** `cpu_percent`,
   `disk_read_bytes_per_sec`, `disk_write_bytes_per_sec` on `ProcessSnapshot`; first tick
   reports `None`. One file plus tests.
2. **6.2 `monitor/pdh.rs`.** Safe wrapper: open query, add English counters, collect,
   read the formatted array, close; `Drop`; error → status mapping; the availability
   ladder using `PdhEnumObjectItemsW`. Includes the cost measurement for D3/D4.
3. **6.3 `monitor/gpu.rs`.** Pure instance-name parser (PID, LUID, physical GPU, engine
   index, engine type including empty/unknown) with unit tests, then per-PID aggregation
   per the decision above.
4. **6.4 `monitor/types.rs` + `monitor/scheduler.rs`.** `ProcessMetrics`, `SystemSummary`,
   the tick loop, the shared latest snapshot, and the system totals; the scheduler owns
   `ProcessSampler` and `SignatureCache`, which ends the `pub(crate)` cache ownership
   question left over from Task 5.
5. **6.5 Overhead harness + docs.** An example that prints per-tick wall time and the
   app's own CPU so Task 11 can assert the <1% / <80 MB budget; correct the `AGENTS.md`
   GPU gotcha.

## Task 7 design: the process table

Task 6 puts a fresh `Tick` in a shared slot once a second, in Rust, with nobody
reading it. Task 7 moves it across the IPC boundary and renders roughly 290 rows
without adding to a sampling cost that already eats a large share of the interval.

### Measured facts (this machine, 2026-11-02)

- Rows per tick: 260–302, mean 274, over the 6.5 record, so the table is a few hundred
  rows, not tens of thousands. Virtualization is not obviously needed and must be proven
  needed, not assumed.
- The tick rate the UI would subscribe to is not 1 Hz: over 599 steady ticks the same loop
  measured min 41.9 ms / median 259.9 ms / mean 372.0 ms / max **4.07 s**, because a tick
  that overruns its interval simply delays the next one. Frames therefore arrive in bursts
  and gaps, so animation and sparkline sampling must key off `sequence`, not off the
  wall clock. The cold first tick is 8.26 s (it verifies every signature), which is why
  the table needs a loading state that is honest about taking seconds.
- Anything Task 7 adds to the tick path is measured by the harness, not estimated — the
  average tick already costs 37 % of the interval and 230 of 599 ticks put this process
  over its 1 % CPU budget on their own.
- The payload a tick is now measured, not guessed: **104–111 KB, mean 106 KB** per tick
  over 89 steady ticks at 274–289 rows, serialized by the harness outside the tick's own
  timing. One tick per second is roughly 6.4 MB a minute crossing into the webview.
- The rows were already wire-ready and the tick was not. Task 5 gave `ProcessSnapshot`
  `Serialize` with camelCase fields and snake_case enum variants, so a signature status
  arrives as `"trusted"` and a critical reason as `"identity_unavailable"`; the four
  shapes Task 6 added (`Tick`, `ProcessMetrics`, `SystemSummary`, `GpuTelemetry`) derived
  `Debug`/`Clone` only, so nothing could publish a whole tick. `ProcessSnapshot` is 16
  fields — ten `Option`, two enums.
- `core:default` in the shipped ACL manifest (`src-tauri/gen/schemas/acl-manifests.json`)
  expands to `core:event:default`, which is `allow-listen`, `allow-unlisten`,
  `allow-emit`, `allow-emit-to`. Events need no capability change.
- The frontend calls `invoke` directly in exactly two places (`Audit.tsx`,
  `AppShell.tsx`) and never calls `listen`; there is no API layer to fit into.
- Task 7's verify line in `tasks/todo.md` says `npm test` and an axe check. Neither is
  available: `npm test` is not a script, and no JS test runner, DOM library or axe
  package is a dependency. The repo-map entries `extension/`, `tests/e2e/`,
  `src-tauri/src/actions/` and `src/ipc/` do not exist either — that map is a roadmap.

### Sources

Tauri 2: `tauri::Emitter` (`AppHandle::emit`), `@tauri-apps/api/event` `listen`,
`tauri::Builder::manage` / `State`, and the generated ACL manifests for what
`core:default` actually grants. React 19: `useSyncExternalStore` for a subscription
that must not re-render on an unrelated state change; inline SVG for the sparkline,
no charting dependency.

### Decisions (approved by the user, 2026-11-02)

- **T1 — one scheduler, owned by the app.** `setup()` builds `Scheduler::new()?` and
  `spawn()`s it into managed state; the app stops it on exit. A second instance would
  open a second PDH query over the same 307 engines and double the cost.
- **T2 — pull once, then push.** `latest_tick()` answers the first frame so the table
  is populated on mount instead of one second later; every tick after that arrives as
  one `metrics://tick` event. A pure poll makes the webview's timer the clock; a pure
  push leaves the first paint empty.
- **T3 — serialize the existing shapes; no parallel view type.** Add `Serialize` to the
  four tick shapes and follow the wire convention Task 5 already set for the rows:
  camelCase fields, snake_case enum variants. `GpuTelemetry` is the one shape that carries
  data, so it serializes as a tagged union on `kind` — `{"kind":"sampling",
  "enginesSampled":307,...}` — which a TypeScript reader can discriminate without guessing
  whether a missing field means warming or absence. These were designed as the tick's
  published shapes, their fields are all metadata the UI may show, and a second struct
  mirroring them is a third thing to keep in sync with the sampler.
- **T4 — row identity is `pid` + `startTime`, never `pid` alone.** The invariant in
  `AGENTS.md` exists because Windows recycles PIDs; a sparkline keyed on PID silently
  bridges two different processes.
- **T5 — sparkline history lives in the webview.** A per-identity ring of the last 60
  samples per metric, built from the events. The sampler's RAM is the budgeted number
  (Task 11), and ~290 × 60 × 3 floats is trivial there but buys the loop nothing.
- **T6 — an unmeasured metric is an em dash that sorts last.** Same rule as D2, applied
  to pixels: a blank figure must never look like an idle process, and the header shows
  `unmeasuredProcesses` so "why is half the table blank" is answered in the UI.
- **T7 — payload size was measured before anything was trimmed.** The harness now prints
  it: 106 KB mean per tick. The decision on that number is still to send the whole tick,
  because the alternative — a top-N event plus a pull for the rest — splits one tick into
  two sources of truth for ~100 KB. If the webview shows a real cost in 7.3, the first
  thing to cut is the per-row `exePath` and `publisher` strings for rows the table is not
  displaying, and that cut gets made on a measurement, not on this figure.
- **T8 — Task 7 ships no pause-when-hidden and no EcoQoS.** Task 11 owns the budget and
  the window events; this task only starts and stops the loop with the app.
- **T9 — the UI gets a real test stack.** The chosen answer to the verification gap is
  Vitest + Testing Library + axe on the table component, so `npm test` exists and Task 7's
  verify line means what it says. Dev-only dependencies; nothing ships in the bundle.
  Sorting, the em dash for `None`, keyboard reachability and row identity are the cases
  worth proving, because those are the ones a manual pass misses.

### Sub-tasks

1. **7.1 Serialize the published shapes.** Derives, camelCase keys, a test that a `Tick`
   becomes the JSON the UI expects, and the harness prints bytes per tick. Rust only.
2. **7.2 Wire the loop to the window.** `commands.rs` becomes `commands/mod.rs` plus
   `commands/processes.rs`; `setup()` spawns the scheduler; `latest_tick()` and the
   per-tick emit. Verify that an emit from the sampling thread reaches the webview.
3. **7.3 `ProcessTable`.** Sortable headers, name/PID filter, em dash for `None`, a
   stale-GPU marker from `gpuSequence`, and the empty/loading/error states; keyboard
   accessible with `aria-sort` on real buttons.
4. **7.4 Sparklines.** The webview ring buffer keyed by `pid`+`startTime`, inline SVG,
   hidden from assistive tech with the numeric value in text beside it.
5. **7.5 The UI test stack.** Vitest + Testing Library + axe as dev dependencies, an
   `npm test` script, and table tests for sorting, the `None` em dash, keyboard access and
   row identity; `npm run verify` gains the `npm test` step so the gate covers the UI.

## Risks and mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Chrome Web Store rejects broad cookie permissions | High | Optional permissions, privacy policy, open source, enterprise/unpacked fallback |
| Process close loses unsaved data | High | No auto-kill, graceful close, confirmation, denylist |
| False positives in process scoring | Medium | Explain reasons, user feedback, conservative defaults |
| Native-host registration fails | Medium | HKCU install, diagnostics, clean uninstall |
| Chrome API changes | Medium | Feature detection, version checks, E2E tests |
| SSO domain clearing side effects | Medium | Whitelist, warnings, preview |
| App's own overhead | Medium | Sampling intervals, pause when hidden, EcoQoS for self |
| Tool becomes a security target | High | Threat model, signed builds, least privilege, no network, strict IPC |
| Rust/Tauri complexity | Medium | Vertical slices, early POCs, .NET fallback |

## Open questions

See `docs/SPEC.md` §18. Blocking before Phase 2: extension distribution model (unpacked vs Web Store) and default SSO whitelist.
