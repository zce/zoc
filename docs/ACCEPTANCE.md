# Manual acceptance checklist

Run this on Fedora GNOME/Wayland with a real OpenCode Server. Record the app version, OpenCode SHA/version, WebKitGTK version and display scale in the result. CI uses WebKitGTK under Xvfb and a fixture server; it does not replace this checklist.

## Install

- [ ] Install the archive with `install.sh` or the RPM with `dnf`.
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
- which checks passed/failed and the observed error;
- `docs/PERFORMANCE.md` updates if measurements were taken.

Do not mark a check passed based on CI alone.
