# Capability Map: Easy Window

Status: Draft for approval
Last updated: 2026-11-01

## Product

Easy Window is a Windows desktop utility with two independent capabilities:

1. **Chrome Session Guard** — find persistent logged-in website sessions and clear them on demand or automatically.
2. **Windows Performance Guard** — find background processes wasting CPU/RAM/disk/GPU and close or throttle them safely.

## Modules

| Module id | Responsibility | Depends on |
|---|---|---|
| `core` | Local settings, audit log, retention, shared Rust types, native-messaging protocol, named-pipe IPC | — |
| `win-monitor` | Per-process CPU/RAM/disk/GPU telemetry (PDH/ETW), classification, safe actions, startup audit | `core` |
| `chrome-guard` | MV3 extension, cookie metadata scan, session risk scoring, clear/auto-logout actions, native host | `core` |
| `desktop-shell` | Tauri 2 shell, React/TS UI, onboarding, dashboard, settings, audit views | `core`, `win-monitor`, `chrome-guard` |
| `packaging` | Installer, native-host registration, extension install flow, signed updates, uninstall | `desktop-shell` |

## Dependency direction

```text
core ──→ win-monitor ──┐
  │                     ├──→ desktop-shell ──→ packaging
  └──→ chrome-guard ────┘
```

No cycles. `chrome-guard` works standalone in Chrome if the desktop app is not running; `core` defines the shared contract.

## Build order

1. `core` + `desktop-shell` skeleton (mock data)
2. `win-monitor` vertical slice
3. `chrome-guard` vertical slice
4. Integration + polish
5. Packaging + release

## Boundary contracts

- `core` owns the native-messaging JSON schema, local DB schema, and audit event shape.
- `win-monitor` exposes read-only telemetry first; actions are separate commands with confirmation.
- `chrome-guard` never reads or stores cookie values or passwords; only metadata and flags.
- `desktop-shell` never talks to Chrome directly; it talks to `core` / the native host.
