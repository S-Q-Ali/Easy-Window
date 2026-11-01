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
  - Verify: `cargo test` (44 tests; 28 of them cover `monitor` and `risk`); compare against
    Task Manager with `cargo run --manifest-path src-tauri/Cargo.toml --example processes`.
  - Files: `src-tauri/src/monitor/process.rs`, `src-tauri/src/risk/denylist.rs`, tests.

- [ ] **Task 6: PDH telemetry**
  - Acceptance: CPU %, GPU %, disk B/s sampled per process; system totals; graceful handling of missing counters.
  - Verify: Manual run on Windows 11; GPU samples map to PIDs; 10-minute overhead check.
  - Files: `src-tauri/src/monitor/pdh.rs`, `src-tauri/src/monitor/types.rs`, `src-tauri/src/monitor/scheduler.rs`.

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
