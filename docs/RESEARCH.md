# GitHub Research: Easy Window

Status: Complete
Date: 2026-11-01
Purpose: identify reusable code, libraries, and reference projects for the two modules.

## How to read this

- **Depend** — add as a package dependency.
- **Adapt** — copy/port code; license permits it, attribution required.
- **Reference** — read for design/API patterns only; do not copy code.
- **Avoid** — license or security reasons.

Star counts and push dates were captured 2026-11-01.

---

## 1. Chrome Session Guard

### 1.1 Directly useful

| Repo | Stars | License | Last push | Verdict |
|---|---|---|---|---|
| [Cookie-AutoDelete/Cookie-AutoDelete](https://github.com/Cookie-AutoDelete/Cookie-AutoDelete) | 2016 | MIT | 2025-12 | **Reference** |
| [VasilisPlavos/Cookie-AutoDeleteV3](https://github.com/VasilisPlavos/Cookie-AutoDeleteV3) | 37 | MIT | 2026-10 | **Adapt** |
| [wxt-dev/wxt](https://github.com/wxt-dev/wxt) | 10682 | MIT | active | **Depend** |
| [remusao/tldts](https://github.com/remusao/tldts) | 773 | MIT | active | **Depend** |
| [GoogleChrome/chrome-extensions-samples](https://github.com/GoogleChrome/chrome-extensions-samples) | 17831 | Apache-2.0 | active | **Reference** |

**Cookie-AutoDelete (original)** — the mature project, but the mainline is still the
WebExtension/MV2 generation. Useful for the *product model*: per-domain
auto-delete rules, tab-close triggers, greylist/whitelist semantics, and how to
explain cleanup to a non-expert. Not a code source for an MV3 build.

**Cookie-AutoDeleteV3** — the MV3 fork, actively maintained, and the single most
relevant codebase for our extension. Concrete files worth reading before writing ours:

- `src/services/CleanupService.ts` (~40 KB) — the cleanup engine; how it batches
  cookie removal, handles `browsingData` limits, and reports partial failures.
- `src/services/TabEvents.ts` (~12 KB) — tab close/update/replace handling and the
  race between "tab closed" and "new tab opened to same site".
- `src/services/AlarmScheduler.ts` — surviving service-worker suspension with `chrome.alarms`.
- `src/services/BrowserActionService.ts` — badge/count state.
- `docs/chrome-vs-extensions-api.md` — documents that `browsingData` is
  **host-scoped only, not partition-aware**, while `cookies.remove` supports
  `partitionKey`. This directly shapes our clear-site-data design.
- `AGENTS.md` — their own invariants; useful cross-check for ours.

**wxt** — the extension framework. MV3, TypeScript, HMR, multi-browser build,
manifest generation, and a sane service-worker entry model. Better fit than
Plasmo for a single focused extension. Avoids hand-rolling a webpack config like
Cookie-AutoDeleteV3 does.

**tldts** — Public Suffix List parsing in TypeScript. Needed to group
`mail.google.com` and `accounts.google.com` under `google.com` correctly, and to
avoid the classic `co.uk` mistake. Do not hand-roll this.

### 1.2 Reference only, or avoid

| Repo | Stars | License | Verdict | Why |
|---|---|---|---|---|
| [sienori/Tab-Session-Manager](https://github.com/sienori/Tab-Session-Manager) | 2484 | MPL-2.0 | **Avoid** | Saves/restores tab sessions; does not log out. MPL-2.0 is file-level copyleft. Low relevance. |
| [Moustachauve/cookie-editor](https://github.com/Moustachauve/cookie-editor) | 1763 | GPL-3.0 | **Avoid code** | GPL-3.0 is viral; cannot copy into an MIT project. UI ideas only. |
| [crxjs/chrome-extension-tools](https://github.com/crxjs/chrome-extension-tools) | 4181 | no SPDX | **Avoid** | No clear license file → legally unsafe to depend on. |
| [PlasmoHQ/plasmo](https://github.com/PlasmoHQ/plasmo) | 13169 | MIT | Reference | Heavier and more opinionated than WXT; unnecessary for one extension. |
| `xaitax/Chrome-App-Bound-Encryption-Decryption`, `runassu/chrome_v20_decryption`, `browser_cookie3`, `pycookiecheat`, CookieKatz | — | — | **Never** | Cookie/credential decryption. These are infostealer techniques and violate our hard invariants. |

### 1.3 Gap we own

No repo does **session risk scoring** — "which of these logged-in sites is
actually dangerous to leave open". Cookie managers delete; they do not assess.
That scoring model (non-Secure, non-HttpOnly, SameSite=None, auth-like names,
stale last-access, sensitive category) is our differentiator and must be written
from scratch with tests.

---

## 2. Windows Performance Guard

### 2.1 Directly useful

| Repo | Stars | License | Last push | Verdict |
|---|---|---|---|---|
| [psmux/pstop](https://github.com/psmux/pstop) | 266 | MIT | 2026-10 | **Adapt** |
| [TX230/winproc-tui](https://github.com/TX230/winproc-tui) | 122 | MIT | active | **Reference** |
| [Waqar-743/Syslume](https://github.com/Waqar-743/Syslume) | 5 | MIT | active | **Reference (scaffold)** |
| [nickshui/Win-Top](https://github.com/nickshui/Win-Top) | 31 | **none** | active | **Ideas only** |
| [winsiderss/systeminformer](https://github.com/winsiderss/systeminformer) | 16386 | MIT | active | Reference |

**pstop** — Rust, MIT, and the cleanest small implementation of the exact PDH
counters we need. `src/system/gpu.rs` queries:

```text
\GPU Engine(pid_N_*)\Utilization Percentage        per engine per process
\GPU Process Memory(pid_N_*)\Dedicated Usage       dedicated VRAM per process
\GPU Process Memory(pid_N_*)\Shared Usage          shared VRAM per process
```

It aggregates per-process GPU as "max engine utilization across all engines",
which matches Task Manager's per-process number better than a naive sum. It also
carries the engine type (`3D`, `VideoDecode`, `Copy`) which we want for the
"what is it doing" column. `src/system/winapi.rs` (~40 KB) has process
enumeration, handles, and kill paths. MIT → we can port with attribution.

**winproc-tui** — MIT, Rust 2024, PDH + DXGI + ETW. More valuable for its
**design docs** than its code:

- `docs/architecture.md` — responsibility boundaries and data flow.
- `docs/tracking-and-history.md` — process identity across PID reuse, "ghost
  rows", live-history retention. This is the correct answer to our
  "PID + creation time + path + signature" invariant.
- `docs/process-investigation.md` — async target safety when a process exits
  mid-inspection.

**Syslume** — MIT, and the closest stack match: Tauri 2, React 19, TypeScript,
Tailwind 4, `sysinfo` 0.38, `windows` 0.62 (DXGI), `wmi`. Use it to sanity-check
our `Cargo.toml`, Tauri capability config, and Vite/Tailwind 4 setup. Note it is
a disk-space analyzer, not a process monitor, and it is a young project — copy
configuration shape, not architecture wholesale.

**Win-Top** — functionally the closest thing that exists: per-process GPU and
VRAM, startup items, process priority, ETW, Tauri + Rust. **But it has no LICENSE
file**, which means all rights reserved. We may read it to learn which Windows
APIs to call and which UX decisions work; we may not copy code, assets, or
strings. It is also Tauri 1 + Svelte, so the code would not port cleanly anyway.
Recorded here so nobody accidentally lifts from it later.

**System Informer** — the reference implementation for process trees,
privileges, and handle inspection. Far too heavy to reuse (C, kernel driver,
admin). Read for concepts only.

### 2.2 ETW, if we need it later

| Repo | Stars | License | Verdict |
|---|---|---|---|
| [microsoft/rust_win_etw](https://github.com/microsoft/rust_win_etw) | 135 | MIT | Depend if needed |
| [wuanzhuan/system_monitor](https://github.com/wuanzhuan/system_monitor) | 32 | Apache-2.0 | Reference |

ETW requires admin for real-time kernel events. That conflicts with our
"no admin by default" invariant, so ETW is **post-MVP** and only behind an
explicit elevation flow. PDH + `sysinfo` covers CPU/RAM/disk/GPU for the MVP.

### 2.3 Gap we own

Existing tools are monitors for technical users. None of them answer
*"which of these background apps is useless and safe to close?"* with a
conservative, explained, reversible recommendation. The risk model
(impact × idleness × safety, hard denylist, graceful close, audit) is ours.

---

## 3. Decisions

### Dependencies to add

| Layer | Package | Why |
|---|---|---|
| Extension build | `wxt` | MV3 + HMR + manifest generation, MIT |
| Domain grouping | `tldts` | Correct PSL handling, MIT |
| Process snapshot | `sysinfo` 0.40 | Cross-checked process list, memory, CPU |
| Win32 | `windows` 0.62 | Typed bindings incl. `Win32::System::Performance` (PDH) |
| GPU/disk counters | PDH via `windows` | Same source Task Manager uses |
| Local store | `rusqlite` (bundled) | Settings + audit, no server |
| Serialization | `serde` / `serde_json` | Typed IPC |
| Errors/logs | `thiserror`, `tracing` | No `unwrap` in prod, redacted logs |

### Code to adapt (MIT, attribution in `NOTICE`)

1. `pstop/src/system/gpu.rs` — PDH GPU engine + VRAM counter logic.
2. `Cookie-AutoDeleteV3` — cleanup batching, tab-event races, alarm scheduling
   patterns (rewrite in our structure, keep the hard-won edge cases).

### Never

- Any cookie/password decryption project.
- Win-Top code (no license).
- cookie-editor code (GPL-3.0).
- Tab-Session-Manager code (MPL-2.0, and irrelevant).

---

## 4. What this changes in the plan

- Extension scaffold task (Task 12) uses **WXT**, not CRXJS. Removes the
  unlicensed-dependency risk.
- Task 13 adds `tldts` instead of a hand-written suffix parser.
- Task 6 ports pstop's PDH approach, including engine-type reporting and
  max-across-engines aggregation.
- Task 5 adopts winproc-tui's identity/history model (creation time + ghost rows).
- Task 21 adds a license/NOTICE audit to the security review.
