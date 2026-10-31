# Spec: Easy Window (MVP)

Status: Draft for approval
Date: 2026-11-01
Working title: Easy Window

## 1. Objective

Build a Windows desktop utility that helps a non-expert user:

1. See which websites still have a logged-in Chrome session, and clear selected sessions in one click or automatically when the tab closes / the user goes idle.
2. See which background processes are wasting CPU, RAM, disk, or GPU, and close or throttle them safely with clear explanations.

Success means the user can answer **"what is still logged in?"** and **"what is wasting resources?"** in under 10 seconds, take a safe action, and see the result.

## 2. Target user

- Windows 11 user, Chrome user, not a developer.
- Cares about account security and a fast machine.
- Does not want to learn Chrome internals or Windows process trees.
- Values privacy: no cloud account, no telemetry by default.

## 3. Problem statement

Closing a Chrome window does not log the user out. Authentication cookies persist in the profile. Anyone with access to the profile, or stolen cookies, can reuse the session. Users need a simple way to see and end those sessions.

Windows also runs many background helpers. Some are necessary; some waste resources. Users need a safe way to identify and stop the wasteful ones without breaking Windows.

## 4. Assumptions (correct these before implementation)

1. Primary target is Windows 11 (Windows 10 2004+ best-effort).
2. Primary browser is Google Chrome 120+ (tested on Chrome 156). Edge/Brave/Firefox are later.
3. The user accepts a companion Chrome extension for cookie/tab actions. Without it, the app can only show metadata and guide the user.
4. The app runs as the current user, not admin, by default. Elevated actions are out of MVP scope.
5. "Session" means a website login persisted via cookies and site storage, not an open tab.
6. The app may read cookie metadata (host, flags, expiry) but never cookie values, passwords, or browsing-history content.
7. The app may close user-level processes and disable user-level startup entries with confirmation. It never stops Windows services or edits system registry in MVP.
8. The app is local-only. No cloud sync, no analytics, no remote code.
9. UI language is English for MVP; Urdu localization later.
10. The extension is distributed unpacked for MVP; Chrome Web Store publication is a release task.

## 5. Threat model (summary)

Assets: browser sessions, local process control, user data, app update channel.

Trust boundaries: Chrome extension ↔ native host; native host ↔ desktop app; desktop app ↔ OS; process metadata (untrusted).

| Threat | Control |
|---|---|
| Extension impersonation | Fixed extension ID, native host `allowed_origins`, validate every message |
| Local pipe abuse | Named-pipe ACL to current user, reject remote clients, session token, schema validation |
| Malicious process names/paths | Use PID + creation time + executable path + signature, not name alone; open with least rights |
| Command injection | No shell; typed APIs; allowlisted actions only |
| Cookie/password theft | Never read values; no network; no secrets in logs; App-Bound Encryption left intact |
| Destructive actions | Hard denylist, preview, confirmation, audit log, no auto-kill |
| Update compromise | Signed Tauri updater manifests; HTTPS; code signing |
| DoS | Message size caps, rate limits, sampling backoff |

Abuse cases to test:

- A local app connects to the pipe and tries to clear cookies.
- A malicious page sends messages to the extension.
- A process named `svchost.exe` is not the real one.
- A user tries to force-close a critical process.
- An oversized or malformed native message.

## 6. Capability map

See `docs/CAPABILITY_MAP.md`. Modules: `core`, `win-monitor`, `chrome-guard`, `desktop-shell`, `packaging`.

## 7. Tech stack

- Desktop: **Tauri 2** (Rust) + **React 19** + **TypeScript** + **Vite**.
- UI: Tailwind CSS + Radix primitives + TanStack Table + Recharts + Lucide icons. Custom design tokens; no generic AI aesthetic.
- Rust: `windows` crate for Win32, `sysinfo` for process snapshots, PDH for CPU/GPU/disk counters, `rusqlite` for local store, `serde` for IPC, `tokio` for async, `tracing` for local logs.
- Chrome: **Manifest V3** extension, TypeScript, Vite + CRXJS (or plain esbuild), using `chrome.cookies`, `chrome.browsingData`, `chrome.tabs`, `chrome.idle`, `chrome.storage`, `chrome.alarms`, `chrome.nativeMessaging`.
- Native host: separate Rust binary `easy-window-host.exe` in the same Cargo project; stdio native-messaging protocol; named-pipe bridge to the GUI.
- Packaging: Tauri NSIS installer, HKCU native-host registration, signed updater, uninstall cleanup.
- Testing: Rust unit/integration tests, Vitest for extension/UI, Playwright for extension E2E, manual Windows VM install test.

**Why Tauri:** small installer, low idle footprint, modern accessible UI, Rust safety for privileged OS actions, signed updater. **.NET 8 WPF** is the fallback if delivery speed matters more than footprint.

## 8. Architecture

```text
Chrome (MV3 extension)
    │  native messaging (stdio)
    ▼
easy-window-host.exe (Rust)
    │  named pipe (current-user ACL)
    ▼
Tauri app (Rust core + React UI)
    │
    ├── win-monitor: PDH/ETW + process actions
    ├── core: SQLite settings + audit
    └── extension config/audit sync
```

- The Chrome extension is the browser agent. It can work standalone with `chrome.storage` config and enforce auto-logout even if the desktop app is closed.
- The native host bridges extension ↔ desktop app for config sync, dashboard data, and audit. It is optional for enforcement.
- The Rust core owns telemetry, process actions, local store, and policy.
- The React UI reads via Tauri commands; no direct OS access from the webview.
- Local store: `%LOCALAPPDATA%\EasyWindow\easy-window.db` (SQLite, WAL). No cookie values.
- Named pipe: `\\.\pipe\easy-window-<user-sid-hash>` with current-user ACL.
- Extension ID fixed via manifest `key` for unpacked builds; Web Store ID for release.

## 9. MVP scope

### 9.1 Chrome Session Guard

- Scan cookie metadata for the default Chrome profile and all cookie stores. Group by registrable domain using the Public Suffix List.
- Show: domain, display name, cookie count, auth-like cookie count, Secure/HttpOnly/SameSite flags, oldest/newest session age, last access, risk score, risk reasons.
- Risk score factors: non-Secure, non-HttpOnly, SameSite=None, HTTP source, auth-like name, long expiry, stale last access, sensitive category, user-marked "protect".
- Actions:
  - Close tabs for a domain.
  - Log out / clear site data for a domain (cookies, localStorage, IndexedDB, cacheStorage, serviceWorkers).
  - Auto-logout on tab close (per domain, opt-in).
  - Auto-logout after idle (global threshold, opt-in; default 30 min).
  - Panic logout: clear all non-whitelisted domains after typing a confirmation phrase.
- Onboarding: explain tab close vs logout; recommend whitelist for SSO domains; request permissions.
- Badge: number of persistent sessions found on browser startup.
- Audit: domain, action, timestamp, result. No cookie names or values.
- Never: decrypt cookie values, read passwords, upload data, clear passwords.
- Limits: "compromised" cannot be proven, only risk scored. Clearing a registrable domain may log out sibling subdomains/SSO. Some sites use non-cookie storage; we clear the main ones.

### 9.2 Windows Performance Guard

- Process table: PID, name, publisher, signature status, path, parent, session, CPU %, RAM working set/private, disk B/s, GPU %, start time, idle time, window title, critical/safe badge.
- System summary: CPU/RAM/disk/GPU totals, top 5 consumers, before/after snapshot.
- Classification:
  - Hard denylist: critical Windows processes, security software, drivers, session-0 critical services, the app itself.
  - Safe candidate: user-level, no visible window, not Microsoft-signed, idle > 10 min, resource use above threshold, not whitelisted.
  - Score = impact × idleness × safety; show reasons.
- Actions:
  - Graceful close (WM_CLOSE), wait, report result.
  - Force close only after explicit confirmation and only for non-denylisted processes.
  - Lower priority / EcoQoS for selected background apps (reversible).
  - Startup audit: list Run keys, Startup folder, scheduled tasks; disable/enable user-level entries with backup; no services.
- Never: auto-kill, kill denylisted processes, stop services, registry cleaning, uninstall apps, disable security software.
- Audit: process name, PID, action, timestamp, result. No command lines (may contain secrets).

### 9.3 Desktop shell

- Pages: Overview, Sessions, Processes, Startup, Settings, Audit.
- Tray: status, pause monitoring, panic logout, open dashboard.
- Onboarding: privacy explanation, permissions, extension install, whitelist.
- Settings: auto-logout policy, idle minutes, sampling interval, retention, theme, extension connection, telemetry (off, opt-in only).
- Accessibility: WCAG 2.1 AA, keyboard navigation, focus rings, screen-reader labels, no color-only status.
- Design: dense, calm, Windows-native feel; Segoe UI Variable + Cascadia Code for numbers; neutral graphite palette with amber warning and teal safe accent; no purple gradients, no stock card grids.
- Performance: idle < 1% CPU, < 80 MB RAM; pause sampling when window hidden; EcoQoS for self when backgrounded.

## 10. Out of scope (MVP)

- Cookie/password decryption or bypassing App-Bound Encryption.
- Killing Windows services, registry cleaning, "system tweaks".
- Automatic process killing without confirmation.
- Edge/Brave/Firefox support.
- Cloud sync, accounts, telemetry, remote config.
- Breach/password checking (HIBP) — later, opt-in.
- Extension Web Store publication — release task, not MVP.
- macOS/Linux.
- Mobile.
- Guaranteed speed-boost claims.

## 11. Commands (planned; update after scaffold)

```bash
# Install
npm install

# Dev
npm run tauri dev

# Build
npm run tauri build

# UI quality
npm run lint
npm run typecheck
npm test

# Rust quality
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
cargo fmt --manifest-path src-tauri/Cargo.toml --check

# Chrome extension
npm run build:extension
npx playwright test

# Full verification
npm run verify
```

## 12. Project structure

```text
src/                       React UI (pages, components, hooks, styles)
src-tauri/                 Rust core + Tauri app
  src/bin/easy-window-host.rs
  src/monitor/             PDH/ETW process telemetry
  src/actions/             close, priority, startup
  src/store/               SQLite settings + audit
  src/ipc/                 named pipe + native messaging protocol
  src/risk/                session/process scoring
extension/                 Chrome MV3 extension
  src/background.ts
  src/popup/
  src/options/
  public/manifest.json
docs/                      SPEC, CAPABILITY_MAP, THREAT_MODEL
tasks/                     plan.md, todo.md
tests/e2e/                 Playwright extension tests
AGENTS.md                  agent instructions
```

## 13. Code style

- TypeScript strict; no `any`; functional components; colocated tests.
- Rust: `cargo fmt`, clippy `-D warnings`, `thiserror` for errors, no `unwrap` in production paths, typed IPC.
- Naming: `snake_case` Rust, `camelCase` TS, `PascalCase` components.
- No raw hex colors in components; use semantic tokens.
- Example:

```tsx
export function RiskBadge({ level }: { level: RiskLevel }) {
  const label = level === "high" ? "High risk" : level === "medium" ? "Review" : "Safe";
  return <span className={`badge badge--${level}`}>{label}</span>;
}
```

## 14. Testing strategy

- Rust unit tests: risk scoring, domain grouping, process denylist, path validation, native-messaging schema.
- Rust integration tests: PDH sampling, process snapshot, startup registry read/write under `HKCU\Software\EasyWindow\Test`.
- Extension unit tests: Vitest with mocked `chrome` APIs.
- Extension E2E: Playwright with a persistent Chromium context + unpacked extension + local test server that sets cookies/localStorage.
- Manual: real Chrome with a test account; Windows VM for installer.
- Performance: measure the app's own CPU/RAM over 10 minutes; assert idle budget.
- Security: fuzz native-messaging JSON; unauthorized origin; oversized message; symlink/path traversal; process-name spoofing.
- Accessibility: axe-core in Playwright; keyboard-only pass.

## 15. Boundaries

**Always**

- Validate all IPC input with serde/schema.
- Keep cookie values and passwords out of memory, logs, and storage.
- Run `npm run verify` before commit.
- Add audit events for destructive actions.
- Use least privilege; no admin by default.

**Ask first**

- Adding a new Chrome or Windows permission.
- Adding a network call.
- Adding an elevated helper.
- Changing the native-messaging protocol.
- Adding a new destructive action.

**Never**

- Decrypt Chrome cookies/passwords or bypass App-Bound Encryption.
- Kill critical Windows processes or stop services.
- Auto-kill without confirmation.
- Commit secrets.
- Send browsing or process data off-device.
- Use `innerHTML` with untrusted data.

## 16. Success criteria

1. On Windows 11 + Chrome 156, the extension scans at least 100 hosts and shows the risk list in under 2 seconds; no cookie values are read or stored (code review + network check).
2. One-click logout for a test site clears cookies + localStorage + IndexedDB; reload shows logged out.
3. Auto-logout on tab close works for a configured test site; reopening requires login.
4. Panic logout clears all non-whitelisted test sites; the whitelisted site remains logged in.
5. Desktop app lists the top 20 processes by CPU/RAM/disk/GPU with under 2% CPU overhead over 10 minutes.
6. Safe close of a test app frees resources and the app exits gracefully; critical processes cannot be selected.
7. Startup audit lists Run keys and can disable/re-enable a test entry; state persists.
8. Installer installs the app, registers the native host, and uninstall removes both; no admin required.
9. No network requests from the app or extension by default (except update check, opt-in).
10. Keyboard navigation, screen-reader labels, and contrast pass.

## 17. Risks and mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Chrome Web Store rejects broad cookie permissions | High | Optional permissions, privacy policy, open source, enterprise/unpacked fallback |
| Killing a process loses unsaved data | High | No auto-kill, graceful close, confirmation, denylist |
| False positives in "useless" classification | Medium | Explain reasons, user feedback, conservative defaults |
| Native-host registration fails | Medium | HKCU install, diagnostics, clear uninstall |
| Chrome API changes | Medium | Feature detection, version checks, E2E tests |
| SSO domain clearing logs out multiple services | Medium | Whitelist, warnings, preview |
| App's own overhead | Medium | Sampling intervals, pause when hidden, EcoQoS for self |
| Tool itself becomes a security target | High | Threat model, signed builds, least privilege, no network, strict IPC |
| Tauri/Rust complexity slows delivery | Medium | Vertical slices, early POCs, .NET fallback |

## 18. Open questions

- Auto-logout on tab close: opt-in per site, or default for auth-like sites?
- Clear on Windows lock/idle: default 30 minutes, or manual only?
- Default SSO whitelist: `google.com`, `microsoftonline.com`, `apple.com`, others?
- Is Chrome Web Store publication required, or is unpacked/personal use enough for MVP?
- Minimum Windows version: Windows 11 only, or Windows 10 2004+?
- English-only UI for MVP, or English + Urdu?
- Should the desktop app support multiple Chrome profiles from day one?
