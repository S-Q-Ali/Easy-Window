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
- [ ] Task 6: PDH sampling for GPU, CPU and disk rates from `sysinfo`; system summary; sampling scheduler. Design in "Task 6 design" below.
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

### Open decisions for the user

- **GPU aggregation: sum or max per PID.** `AGENTS.md` currently records "max engine
  utilization per PID, not a sum". Task Manager **sums** all engines of a process
  (normalised by the sample interval); Process Hacker uses **max**, which is why the two
  disagree. Recommendation: sum, keep the per-engine breakdown for the tooltip, and
  correct the `AGENTS.md` gotcha in the same commit.

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
