# Incremental deliveries — original prototype plan

This records the **original v0.1 functional delivery sequence**. The initial stacked PRs have been consolidated into main; for the **current visual-parity priorities and Agent handoff**, follow [AGENT_HANDOFF.md](AGENT_HANDOFF.md). Deliver new work as focused PRs against main with inspectable GitHub Actions Artifacts, not Releases. Each handoff must name its actual CI run and commit SHA. A successful Rust build is not proof of a working native window.

| Stage | Scope | What to observe | Release gate |
|---|---|---|---|
| **01 — Connect & browse** | XDG service registration, manual endpoint/password, connection failures, project/session list, historic transcript, archived sessions, local selection restoration | Does ZOC connect to your existing Service and let you browse long histories smoothly without changing server state? | Contract tests, Clippy, release artifact, real native-window smoke; Fedora run still requires observation |
| **02 — Execute & stream** | New session, first prompt, model/agent/variant selection, real streaming, stop, continuation, stable drafts | Prompt delivery, streaming text/tools, switches during execution, no duplicate or lost messages | Real HTTP/SSE fixture tests, reconnect reconciliation, stage-01 regression |
| **03 — Approval & input** | Session/child permissions, form fields/options, pending state, safe request replies | Are approval and question requests intelligible, routed correctly, and never auto-accepted? | Server acknowledgement required; rejection and stale-form tests |
| **04 — Preview & endurance** | Native Markdown/code, file links, image/text previews, native Diff, Inbox parity, large histories, performance checks | Editing ergonomics, reading/copying code, changes review, 4K/150% Wayland scrolling and memory | Scenario tests and **measured** performance, stage-01..03 regression |

## Delivery rules

- Keep each stage narrow: fix evidenced defects in the chosen workflow; postpone unrelated polish.
- Keep upstream UI/product behavior authoritative; when an upstream feature is not implemented yet, mark it **partial** rather than inventing a substitute.
- Use the pinned upstream Rust crates for API types/transport, service registration, event lifecycle, session/transcript state and approval/form behavior. ZOC-owned state should remain presentation-only.
- An Artifact is runnable only after native startup and package checks. CI's `--check` option alone only proves the binary loads.
- No automatic GitHub release, tag or merge; user feedback may influence the **next** stage, not retrospectively redefine a verified result.
- Every stage keeps a short manual-test script and actual CI link/commit. Local Fedora GNOME/Wayland observations are explicitly user-side and must not be misrepresented as performed by Linux CI.
