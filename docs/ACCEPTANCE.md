# Manual acceptance checklist

Run this on Fedora GNOME/Wayland with a real OpenCode Server. Record the app version, OpenCode SHA/version, WebKitGTK version and display scale in the result. CI uses WebKitGTK under Xvfb and a fixture server; it does not replace this checklist.

## Install

- [ ] Install the archive with `sh install.sh zoc-<version>-linux-amd64.tar.gz` (both files in one directory), or the RPM with `dnf`.
- [ ] Launch ZOC from GNOME. A window opens with the official OpenCode Web UI.
- [ ] Confirm no Node.js, Bun, Electron, Chromium or OpenCode process is started by ZOC.

## Connection

- [ ] Local server at `127.0.0.1:4096`: the seeded **Local OpenCode** profile connects.
- [ ] Open **File → Servers & Inbox…** (`Ctrl+Shift+I`), add a remote HTTPS server, save, and open it from the Web UI.
- [ ] Wrong password shows the upstream auth flow; correct password connects.
- [ ] Disconnect the network; the UI reports reconnecting and recovers when it returns.
- [ ] Switch between two configured servers and back.
- [ ] Attempt to enter an unconfigured remote URL in the Web UI: it is refused by ZOC's origin policy; add it in Servers & Inbox first.

## Sessions

- [ ] Browse projects and existing sessions; open one and restore its normal persisted state.
- [ ] Create a session, choose agent/model, send a prompt.
- [ ] See streamed assistant output and tool progress incrementally.
- [ ] Stop an active run; the server confirms the stop.
- [ ] Trigger a permission request and a question; answer both. Confirm nothing was auto-approved before the server response.
- [ ] Render Markdown, fenced code, images, file references and an Office/Markdown preview if available.

## Attachments

- [ ] Attach a file with the native picker.
- [ ] Paste an image from the clipboard.
- [ ] Drag and drop files onto the Web UI.
- [ ] Save a file with a native save dialog if the workflow offers it.

## Terminal

- [ ] Open the terminal for a session (`Ctrl+\`` or the terminal control).
- [ ] Run `printf 'hello\n'`; output appears.
- [ ] Type Chinese text; composition and commit work.
- [ ] Resize the window; the PTY receives the new size.
- [ ] Close and reopen the terminal; the persistent PTY restores output.
- [ ] Kill the server connection; the terminal reports a failure and reconnects after the server returns.
- [ ] Close the window; no WebSocket or PTY process remains on the server.

## Keyboard, clipboard, CJK

- [ ] Navigate the Web UI without a mouse.
- [ ] `Ctrl+Shift+I` opens Servers & Inbox.
- [ ] Copy/paste text and images between ZOC and another GNOME app.
- [ ] Compose Chinese/Japanese/Korean text in the composer and terminal.
- [ ] Zoom in/out and toggle fullscreen from the menu.

## Window chrome and context menu

- [ ] Drag the window by the empty titlebar; double-click the titlebar toggles maximize.
- [ ] Minimize, maximize/restore (the icon changes) and close from the top-right controls.
- [ ] The in-page Window menu's minimize, maximize and close actions do the same as the controls.
- [ ] Fullscreen hides the controls and their reserved titlebar space and drops the window's 8px corners; maximizing also squares them. Leaving fullscreen or maximize restores the controls, space and corners.
- [ ] Vertical tabs: the Linux titlebar row remains, the controls stay aligned and the drag region works.
- [ ] Right-click in the composer, on a link, on an image and on blank page space: the native menu appears. Editing, spelling suggestions and Inspect Element stay; Back, Forward and Reload are absent. Open/Copy Link, Image and Media actions work.
- [ ] The Window menu's **New Window** is a known no-op.

## IME and rendering

- [ ] Compose Chinese/Japanese/Korean text in the composer at the top of a session and after scrolling a long transcript; the candidate popup stays stable and never appears outside the window. A fixed platform offset is recorded separately; jitter is a failure.
- [ ] Compose in the terminal as well; candidate placement stays stable around the terminal editor.
- [ ] With `ZOC_IME_CALC=1`, record whether the caret workaround aligns the popup without jitter; it is not the default.
- [ ] The startup log reports `dmabuf=enabled` on Wayland with Intel/AMD Mesa or NVIDIA 600-series, or `dmabuf=disabled (CPU)` on older NVIDIA, a VM or a container.
- [ ] Scroll a long transcript; record smoothness with the active renderer. If it is disabled, record the GPU/session and try `ZOC_DMABUF=on` (or `WEBKIT_DISABLE_DMABUF_RENDERER=0`) only for diagnosis.

## Native inbox

- [ ] The native window lists the configured servers.
- [ ] Add a server, test the connection, edit its name/URL, remove it.
- [ ] Select a server; projects and sessions load.
- [ ] Filter sessions by project and text.
- [ ] Open a session from Native UI; the Web UI navigates to it.
- [ ] **Official UI** returns to the Web UI home.
- [ ] Restart ZOC; the server profiles and window position persist. Passwords are not persisted.

## Notifications, links, lifecycle

- [ ] Trigger an upstream notification; the GNOME notification appears and clicking it focuses ZOC.
- [ ] Click an external link; it opens in the default browser, not inside ZOC.
- [ ] Open `zoc://open?url=http%3A%2F%2F127.0.0.1%3A4096`; the existing instance focuses.
- [ ] Close the window; the app exits and the single-instance lock is released.

## Stability

- [ ] Run a 10–30 minute session with streaming and tool output.
- [ ] Record idle and typical session RSS/CPU and whether it trends upward.
- [ ] Scroll a long transcript; no blanking or persistent jank.
- [ ] Terminal input/output stays under ~100 ms locally.
- [ ] No blank page, broken SSE, or unbounded memory growth.

## Record

Report:

- ZOC version and `upstream.json` commit;
- Fedora version, GNOME/Wayland, GPU and scale;
- WebKitGTK version;
- whether the startup log reports DMA-BUF enabled or disabled;
- which checks passed/failed and the observed error;
- `docs/PERFORMANCE.md` updates if measurements were taken.

Do not mark a check passed based on CI alone.

## Partial environment verification (2026-11-16)

The published `zoc-v0.1.1` artifact was launched on the real target desktop and connected to the locally running OpenCode Server. This is partial evidence, not a completed checklist:

- **Environment:** Fedora Linux 45 Workstation, GNOME Shell 51.1, Wayland (`XDG_SESSION_TYPE=wayland`), GTK 3.24.52, WebKitGTK 2.54.2.
- **Server:** real `opencode serve` on `127.0.0.1:4096`, version 2.3.0; authentication required and was supplied from the user's local service configuration into a temporary app HOME. The credential was never printed or committed; the temporary HOME was removed after the run.
- **Passed:** app launch and native-window rendering; real-server authentication; project and session list loaded; a real session opened and rendered its existing transcript and composer; the running session showed working state and a Stop control; the MyGo Native UI **Servers & Inbox** rendered against the real desktop (see `docs/assets/native-inbox-fedora.png`); a real persistent PTY was created through the terminal command palette and the server reported a running `zsh` prompt in `~/Coding/zoc`.
- **Not completed:** terminal keystroke injection into the WebKit terminal surface (the PTY existed, but automated input did not reach it); CJK composition (the test X11/IBus path inserted literal pinyin, not committed characters); permission/question prompts; real attachments; native dialogs and notifications; a new model prompt and live stream; a full 10–30 minute real-session stability run.
- **Method:** AT-SPI for accessibility inspection and actions, XTest for pointer/keyboard injection under XWayland, and read-only/local OpenCode API calls for verification. No host packages, services, system settings or global configuration were changed.
