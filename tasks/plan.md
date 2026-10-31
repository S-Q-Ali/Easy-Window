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

- [ ] Task 1: Scaffold Tauri 2 + React/TS + Vite + Tailwind/Radix; configure lint, typecheck, tests.
- [ ] Task 2: Add `AGENTS.md`, design tokens, app shell (sidebar/topbar), empty pages.
- [ ] Task 3: Implement Rust `core` store, typed IPC, audit log, migrations.
- [ ] Task 4: Add the `verify` script and CI-ready local checks.

### Checkpoint 0

- [ ] `npm run tauri dev` opens the app.
- [ ] `npm run verify` passes.
- [ ] No secrets, no network calls, no cookie values.

### Phase 1 — Windows Performance Guard

- [ ] Task 5: Process snapshot with `sysinfo`, publisher/signature/path, critical-process denylist.
- [ ] Task 6: PDH sampling for CPU, GPU, disk; system summary; sampling scheduler.
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
