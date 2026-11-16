# Feature parity

Status values:

- **Native** — implemented in ZOC Go/MyGo Native UI and tested or directly exercised.
- **Web** — provided by the embedded official OpenCode Web UI; ZOC supplies the host and transport.
- **Web + adapter** — official Web UI behavior reaches a MyGo capability through ZOC's adapter.
- **Unverified** — code and/or upstream tests exist, but it has not been exercised against a real OpenCode Server in this environment.
- **Not implemented** — intentionally out of scope.

| Capability | Status | Evidence / notes |
|---|---|---|
| Replace Rust/GPUI | Done | No Rust sources, Cargo manifests or Cargo CI remain. |
| Official Web UI embedded | Web | Built at pinned SHA; 3,082 files, 4 WASM assets; no service worker. |
| Connect to existing server | Web + adapter | MyGo fetch through origin allowlist; fixture connection verified. |
| Authentication | Web + adapter | Basic password from Web UI; native inbox keeps ephemeral password in memory. |
| Reconnect / disconnect / switch | Web + adapter | Upstream registry and health machine; profiles seeded from ZOC. |
| Browse projects/sessions | Native + Web | Native **Servers & Inbox** reads `/api/project` and `/api/session`; unit/integration tests pass. |
| Open existing session | Native + Web | Native computes the official session route and navigates the Web UI. |
| Create session / prompt / stream | Web | Official Web UI; SSE fixture stream verified. |
| Stop active run | Web | Official Web UI. |
| Permissions/questions | Web | Official Web UI; never auto-approved by ZOC. Manual real-server test required. |
| Markdown/code/images/previews | Web | Upstream lazy assets retained; 4 WASM assets in build. |
| Attachments | Web + adapter | Native attachment picker returns file bytes through the bridge; paste/drop remains upstream Web UI. |
| Integrated terminal | Web + adapter | PTY `openSocket` patched to MyGo WebSocket; origin policy and ticket path constrained. WebSocket open + `attached` frame verified in WebKit smoke. **Terminal UI against a real server is unverified.** |
| Clipboard | Web + adapter | `Desktop.WriteClipboardText`/`ReadClipboardText` verified in WebKit smoke; image read implemented. |
| Keyboard navigation/shortcuts | Web + native | Upstream Web UI; native menu accelerators and `Ctrl+Shift+I`. |
| Chinese input composition | Web | Upstream Web UI + WebKitGTK; manual Fedora test required. |
| Menus | Native | MyGo menu roles and custom items. |
| Notifications | Native | MyGo Notification; click correlation event. No notification daemon in CI. |
| File/directory pickers, save | Native | MyGo Dialog; not interaction-tested in headless CI. |
| External links/local paths | Native | MyGo Shell with scheme checks. |
| Deep links | Native | `zoc://add`, `zoc://open`; single-instance lock. |
| Window state/fullscreen/zoom | Native | MyGo `StateKey`, fullscreen events, `SetZoomFactor`. |
| Persistent settings/drafts | Native | `Platform.storage` backed by `PathUserData/storage/*.json`; smoke verified storage round trip. |
| Server profiles | Native | JSON, mode 0600, URL only; no secrets. |
| Native Servers & Inbox | Native | Complete route with add/edit/remove/test/session navigation and Web UI fallback. |
| Signed in-app updates | Conditional | MyGo updater wired for archive installs when keys are provisioned; not provisioned/claimed. |
| RPM updates | Package manager | DNF owns updates. |
| Electron/Sidecar/OpenCode management | Not implemented | Explicitly out of scope. |
| GUI plugin host parity | Not implemented | Optional Electron-only feature; absent bridge hides server plugin GUIs. |
| WSL/SSH plugin servers | Not implemented | Desktop-only upstream feature; out of initial Linux scope. |
| Native transcript/composer | Not implemented | Stage 3 candidate; Web UI remains authoritative. |
| Fedora GNOME/Wayland acceptance | Unverified | CI runs WebKitGTK/Xvfb, not a real desktop session. |
| Real OpenCode Server workflows | Unverified | Fixture verifies HTTP/SSE and UI mount; real server acceptance is manual. |

## Automated proof in CI

- `go test ./...` — transport policy, profiles, native snapshot, storage, session route.
- TypeScript typecheck of the adapter against upstream types.
- Frontend gates — root mount, assets, WASM, terminal chunk, no service worker.
- WebKitGTK smoke — official UI mounts, bridge works, storage/clipboard round trip, `/api/info` and `/api/event` hit.
- Reproducible packaging — RPM, archive, `upstream.json`, `SHA256SUMS`.

## Deliberate gaps

- No second SSE connection for Native UI; snapshots are on-demand.
- Native state is not persisted as server data.
- A ZOC profile and the official registry can show the same server; removing a ZOC profile does not remove the official registry copy.
- Real terminal, CJK composition, file pickers and notifications need user-side Fedora verification.
