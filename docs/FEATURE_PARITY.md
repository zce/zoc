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
| Official Web UI embedded | Web | Built at pinned SHA; 3,085 files, 4 WASM assets; no service worker. |
| Connect to existing server | Web + adapter | MyGo fetch through origin allowlist; fixture connection and a real local OpenCode Server (authenticated) verified. |
| Authentication | Web + adapter | Basic password from Web UI; native inbox keeps ephemeral password in memory. Real server accepted the credential in a temporary app HOME. |
| Reconnect / disconnect / switch | Web + adapter | Upstream registry and health machine; profiles seeded from ZOC. |
| Browse projects/sessions | Native + Web | Native **Servers & Inbox** reads `/api/project` and `/api/session`; unit/integration tests and a real-server session list pass. Root sessions paginate with the server cursor and a **Load more sessions** action. Filtering applies to loaded pages; the pagination control remains available when a page has no matches. |
| Open existing session | Native + Web | Native computes the official session route and navigates the Web UI; a real session transcript was opened. |
| Create session / prompt / stream | Web | Official Web UI; SSE fixture and a real server event stream connection verified. A new real model response was not measured. |
| Stop active run | Web | Official Web UI. |
| Permissions/questions | Web | Official Web UI; never auto-approved by ZOC. Manual real-server test required. |
| Markdown/code/images/previews | Web | Upstream lazy assets retained; 4 WASM assets in build. |
| Attachments | Web + adapter | Native attachment picker returns file bytes through the bridge; paste/drop remains upstream Web UI. |
| Integrated terminal | Web + adapter | PTY `openSocket` patched to MyGo WebSocket; WebSocket open + `attached` frame verified in WebKit smoke. A real persistent PTY was created and showed a `zsh` prompt, but automated keystroke delivery into the terminal surface was not verified. |
| Clipboard | Web + adapter | `Desktop.WriteClipboardText`/`ReadClipboardText` verified in WebKit smoke; image read implemented. |
| Keyboard navigation/shortcuts | Web + native | Upstream Web UI; native menu accelerators and `Ctrl+Shift+I`. |
| Window chrome and controls | Native + Web | GTK's title bar is hidden on Linux; the official 36px title bar stays, with in-page minimize/maximize/close buttons, drag regions and double-click maximize. The Web UI's Window menu calls the same Go bindings. Fullscreen hides the controls and their reserved space; fullscreen and maximized windows have square corners. |
| Native context menu | Native | WebKitGTK's menu is customized: navigation items removed; Open/Copy Link, Image and Media items added; editing, spelling and Inspect Element remain WebKitGTK's. |
| IME caret positioning | Partial | Native WebKitGTK/IBus placement: stable on the tested Fedora host but with a fixed offset. A correct fix needs upstream WebKitGTK/GTK work. |
| Rendering backend | Native | MyGo DMA-BUF with a ZOC policy: GPU compositing on Intel/AMD Wayland and NVIDIA 600-series Wayland; CPU fallback on older NVIDIA, VMs and containers. On NVIDIA Wayland, an opaque square webview and reduced compositing effects are selected automatically because the transparent rounded page is CPU-heavy there. The startup log reports the choices. |
| Chinese input composition | Web + adapter | Upstream Web UI and native WebKitGTK/IBus placement; real Fedora IME verification required. |
| Menus | Native + Web | MyGo menu roles, the native app menu and the Web UI's in-page menu; window, view and edit actions cross the Go bindings. |
| Notifications | Native | MyGo Notification; click correlation event. No notification daemon in CI. |
| File/directory pickers, save | Native | MyGo Dialog; not interaction-tested in headless CI. |
| External links/local paths | Native | MyGo Shell with scheme checks. |
| Deep links | Native | `zoc://add`, `zoc://open`; single-instance lock. |
| Window state/fullscreen/zoom | Native | MyGo `StateKey`, fullscreen events, `SetZoomFactor`; the in-page controls and web menu use the same bindings. |
| Persistent settings/drafts | Native | `Platform.storage` backed by `PathUserData/storage/*.json`; smoke verified storage round trip. Text drafts persist. Failed writes reject to the caller and leave the cache at the last confirmed value. |
| Draft image blobs across restart | Partial | The upstream `platform.draftStore` blob hook is not implemented; image attachment bytes are not guaranteed across a restart. |
| Server profiles | Native | JSON, mode 0600, URL only; no secrets. |
| Native Servers & Inbox | Native | Complete route with add/edit/remove/test/session navigation and Web UI fallback; rendered on Fedora 45/GNOME 51/Wayland. |
| Signed in-app updates | Conditional | MyGo updater wired for archive installs when keys are provisioned. Build and signing run in separate jobs; the signing key is absent from upstream build scripts and publishing. Not provisioned/claimed yet. |
| RPM updates | Package manager | DNF owns updates. |
| Electron/Sidecar/OpenCode management | Not implemented | Explicitly out of scope. |
| GUI plugin host parity | Not implemented | Optional Electron-only feature; absent bridge hides server plugin GUIs. |
| WSL/SSH plugin servers | Not implemented | Desktop-only upstream feature; out of initial Linux scope. |
| Native transcript/composer | Not implemented | Stage 3 candidate; Web UI remains authoritative. |
| Fedora GNOME/Wayland acceptance | Partial | Published 0.1.1 launched and rendered on Fedora 45/GNOME 51/Wayland; full checklist remains. |
| Real OpenCode Server workflows | Partial | Auth, project/session list, session open and PTY creation verified; permissions/questions, attachments, terminal input and CJK remain. |

## Automated proof in CI

- `go test ./...` — transport policy, profiles, native snapshot, storage, session route.
- TypeScript typecheck of the adapter against upstream types.
- Frontend gates — root mount, assets, WASM, terminal chunk, no service worker.
- WebKitGTK smoke — official UI mounts, bridge works, storage/clipboard round trip, custom window controls present, `/api/info` and `/api/event` hit.
- Reproducible packaging — RPM, archive, `upstream.json`, `SHA256SUMS`.

## Deliberate gaps

- No second SSE connection for Native UI; snapshots are on-demand.
- Native state is not persisted as server data.
- A ZOC profile and the official registry can show the same server; removing a ZOC profile does not remove the official registry copy.
- **New Window** in the Web UI's app menu is a no-op; ZOC owns one main window.
- WebKitGTK's spellcheck suggestions work, but the Electron app's Spellcheck toggle and language picker are not wired.
- Real terminal, CJK composition, file pickers and notifications need user-side Fedora verification.
