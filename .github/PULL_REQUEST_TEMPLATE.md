<!--
One logical change per pull request. Formatting and behaviour do not mix in
the same PR. If the change is a feature, link the task from `tasks/todo.md`.
-->

## What changes

- 

## Why

<!-- The reason belongs here, not only in the diff. -->

## Verification

- [ ] `npm run verify` passes (typecheck, build, `cargo fmt`, clippy `-D warnings`, `cargo test`)
- [ ] CI is green on this branch
- [ ] Manual check run, described below (if the change is user-visible)

<!--
For UI changes, say what you clicked and what you saw. A green suite does not
prove the screen works. For telemetry changes, state the measured numbers —
"it felt faster" is not verification.
-->

## Privacy and safety

- [ ] No cookie values, passwords, browsing-history content, or process command lines are read, stored, or logged
- [ ] No new network calls, no elevation, no service or registry changes
- [ ] Destructive actions still require preview, confirmation, and an audit event
- [ ] Third-party code added or adapted has a license that allows it (see `AGENTS.md`)

## Scope

<!-- Name what you deliberately did not touch, so review stays on this change. -->
