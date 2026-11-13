# ZOC v0.1 — Implementation and Acceptance Checklist

> This document tracks the requested implementation. Nothing is considered implemented until verified against a real OpenCode service and the pinned GPUI Kit release.

## Baselines to inspect and record

- [ ] Record exact OpenCode `dev` commit SHA and inspect protocol OpenAPI, protocol groups, client and server implementations.
- [ ] Inspect official Desktop Inbox behavior, grouping, ordering, sessions, home and runtime implementation.
- [ ] Record a fixed compatible GPUI Kit version, review its native component examples and development guidance.
- [ ] Inspect MSGA's current CI and adapt only appropriate release-artifact practices.

## Architecture

- [ ] Rust + pinned GPUI Kit + committed `Cargo.lock`, Linux x86_64 native rendering.
- [ ] Keep layers limited to API/protocol, application state, native UI, plus substantial preview/Markdown responsibilities.
- [ ] No embedded OpenCode server, agent/runtime, JS engine, browser/WebView or duplicated session database.
- [ ] XDG-compliant local settings, no writes to the user's OpenCode database/configuration.

## Must-work user journeys

- [ ] Detect XDG service registration; connect and authenticate to a verified existing local/remote service, reconnect and report diagnostics.
- [ ] List and select projects with correct directory/worktree semantics; restore last project/session.
- [ ] List, create, navigate, rename and archive/unarchive sessions as supported.
- [ ] Display **official Inbox groups**: Needs Attention, Unread, Recent, Working with correct state and working run-start ordering.
- [ ] Native dark theme, compact title bar, collapsible/resizable Inbox and optional preview pane; GNOME/Wayland input and scaling.
- [ ] Load real message transcripts: user/assistant, reasoning, tool calls, errors, attachments, fallback for unknown parts.
- [ ] Multi-line composer: send, stop, agent/model/variant pickers, draft restoration, file references and keyboard shortcuts.
- [ ] End-to-end **real** agent prompt: create/open session, select model/agent, send, see SSE text/tools, answer permission/question, interrupt, continue.
- [ ] Correct server/session-targeted permission and question replies, no automatic grants or premature success.
- [ ] Event-led synchronized state: keys for server/session/message/part, incremental updates, idempotency, stale-response protection, SSE reconnect and snapshot reconciliation.
- [ ] Native Markdown and syntax-highlighted code with selection/copy, clickable file paths and graceful fallbacks.
- [ ] Native embedded preview of text, source, Markdown, image, unified diff and a usable changes list, respecting remote file APIs.

## CI, testing and documentation

- [ ] `cargo fmt --check`, Clippy, unit/integration tests, release build, smoke validation on Ubuntu 24.04 x86_64.
- [ ] Upload runnable `zoc-linux-x86_64-pr<N>` / `zoc-linux-x86_64-main` artifacts with executable, checksums, dependencies and startup README.
- [ ] No automatic GitHub Release/tag/updater.
- [ ] Validate representative real API fixtures, out-of-order/duplicate events, disconnects and repeated navigation.
- [ ] Exercise 1000+ messages, large Markdown/code, rapid streaming, idle and DPI/IME behavior.
- [ ] Record measured executable size, RSS, idle CPU and test environment in `docs/PERFORMANCE.md`.
- [ ] Maintain `docs/UPSTREAM_COMPAT.md` and core feature-parity matrix.

## Deferred

Terminal tabs, multi-session tabs, advanced search/attachments/command palette, side-by-side review, complex theming and cross-platform support.

## Completion gate

This PR stays Draft until there is a tested, usable end-to-end native client. Scaffold, placeholder UI or simulated server activity do **not** constitute completion.
