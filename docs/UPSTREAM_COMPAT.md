# Upstream compatibility baseline

Recorded November 12, 2026.

## Pinned references

- OpenCode: `anomalyco/opencode` `dev` at `ec263d62b6e3bfd7df6e5a256d1148b0b277bc1e`
- GPUI Kit: stable release `v0.9.1`, Cargo requirement `=0.9.1`
- CI reference: `zce/make-slack-great-again` `dev` `.github/workflows/ci.yml`

## Protocol

ZOC uses the V2 service API, not the older OpenCode V1 endpoints.

| Feature | Current API |
|---|---|
| Verify server | `GET /api/info` (version, pid) |
| Project list | `GET /api/project` |
| Sessions | `GET/POST /api/session`, cursor paging, project filter |
| Update session | `PATCH /api/session/:sessionID` |
| Transcript | `GET /api/session/:sessionID/message`, cursor paging |
| Prompt | `POST /api/session/:sessionID/prompt` |
| Agent/model | `GET /api/agent`, `GET /api/model`, per-session switch endpoints |
| Interrupt | `POST /api/session/:sessionID/interrupt` |
| Approvals | `GET /api/session/:sessionID/permission`; reply on request-specific endpoint |
| Forms | `GET /api/session/:sessionID/form`; form reply with keyed answer |
| Global events | `GET /api/event` (volatile SSE) |
| File preview | `GET /api/fs/read/*` with location query |
| Git changes/diffs | `GET /api/vcs/status`, `GET /api/vcs/diff` |

Location-scoped operations use `location[directory]` as a deep-object query parameter. A Project ID, the canonical directory and the Session's current Location are **different identities**; this distinction is preserved in routing.

Native message objects have user `text` and assistant `content` arrays rather than historical V1 message-part transport. SSE has events such as `session.text.delta`, `session.text.ended`, `session.reasoning.delta`, `session.tool.called`, `session.tool.success`, `session.execution.started`, `permission.asked` and `form.created`. The event stream is volatile, so reconnects need snapshot reconciliation.

The app uses GPUI Kit's `Textarea`, `TextView` and `MessageScroller`, avoiding a WebView.

## Contract evidence

Sources: the OpenCode baseline's `packages/protocol/src/groups/{session,message,permission,form,event,model,agent,fs,vcs,location}.ts`, `packages/schema/src/{session,session-message,session-event,permission,form,model,project,location}.ts`, and GPUI Kit v0.9.1 component docs/examples.

Compatibility is scoped to the recorded baseline. Builds against newer OpenCode releases require protocol regression tests and should not be assumed compatible without verification.
