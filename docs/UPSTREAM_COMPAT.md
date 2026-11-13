# OpenCode upstream reuse baseline

Recorded November 13, 2026.

## Pinned references

- OpenCode Rust client: `anomalyco/opencode` commit `a1834e74f515cd113cf9fbffcad672bc5ea41541` (the `oc-rust` line).
- GPUI Kit: stable release `v0.9.1`, Cargo requirement `=0.9.1`.
- CI reference: `zce/make-slack-great-again` `.github/workflows/ci.yml`.

All OpenCode Git dependencies use the same immutable commit. Do not float these dependencies to a branch head. Upgrade them together after checking generated API changes, store behavior and compatibility tests.

## Ownership boundary

ZOC is a native desktop client. It owns the desktop window, layout, navigation and presentation. OpenCode remains authoritative for protocol, service registration, session semantics, transcript state, event handling, prompt admission and approval/form behavior.

| Concern | Upstream owner | ZOC responsibility |
|---|---|---|
| HTTP API, auth, generated request/response types | `opencode-api`, `opencode-api-types` | Call generated methods and render results |
| Service registration, XDG paths, channel filenames | `opencode-local` | Offer connection override and show connection status |
| SSE lifecycle, reconnect/stall handling, event batching | `opencode-api::events`, `opencode-store::connection` | Observe connection status |
| Project/session cache and change tracking | `opencode-store` | Project snapshots into UI state |
| Transcript paging, deltas, tool updates | `opencode-store` | Render official message data |
| Pending permissions/forms and replies | `opencode-store` | Present choices and submit user decisions |
| Prompt admission and optimistic inbox state | `opencode-store` | Compose prompt and display delivery state |
| Native widgets, Markdown/code, diff and window behavior | GPUI Kit / ZOC | Native rendering and interaction |

Do not introduce OpenTUI, a second SSE consumer, a second transcript reducer, a parallel permission/form state machine, or a local provider/agent runtime. If an upstream Rust crate already owns the behavior, use it rather than porting the TypeScript implementation.

## V2 protocol

The client uses the V2 service API through generated methods. Important routes include:

- `GET /api/info`, `GET /api/project`
- `GET/POST /api/session`, `PATCH /api/session/:sessionID`
- `GET /api/session/:sessionID/message`
- `GET /api/agent`, `GET /api/model`
- `POST /api/session/:sessionID/prompt`, interrupt and model/agent switch endpoints
- Session permission/form endpoints
- `GET /api/event` via the official store connection
- File read and VCS status/diff endpoints

Location-scoped operations use the official generated `LocationQueryInput`. A Project ID, its canonical directory and a Session's current Location are different identities; do not substitute one for another.

## Validation

The isolated compatibility tests exercise official model references, unknown future transcript members, session cursor responses, generated request/auth construction and the event endpoint. Production integration is validated separately by ZOC CI.

Compatibility is scoped to the pinned commit. Upgrading OpenCode requires:
1. Review changes to the Rust crate APIs and their dependency graph.
2. Run protocol and store behavior tests.
3. Verify service registration and local/remote proxy behavior.
4. Run the full Linux build and a real native client session.

The app uses GPUI Kit's native components and does not embed a WebView.
