# Shell and Inbox parity notes

Reference: `anomalyco/opencode` at commit
`d432a06b9c07bab0be4e8d642173b1cb14137f11`. Design values come from
`packages/ui/src/styles/tokens/theme.css` and
`packages/app/src/shell/titlebar/inbox.tsx`; window semantics come from
`packages/desktop/src/main/windows/defaults.ts`.

## What changed

- Official dark tokens: `#080808` deep shell, `#161616` session/Home panel,
  `#242424` layer 01, `#2e2e2e` layer 02, white-alpha hover/pressed borders,
  muted/faint text, warning/danger/accent status dots.
- Inter is bundled from the official app and loaded before the first frame.
- Eight-pixel insets and a 10px-radius raised main panel; sidebar content at
  the official 344px width with 11px/3px/8px/8px inner padding. The divider
  drags between the official 136px and 640px bounds, persists the width in
  preferences and resets to 344px on double-click.
- Inbox header actions (Home, Search sessions), 24px section labels, 54px
  session cards with a 16px project avatar and two 16px rows. Root sessions are
  listed across every project, as the official Inbox does; each row uses the
  session's own project name and avatar colour. Home filters its session list to
  the selected project.
- Official status precedence: approval, form input, working, failed/done unread,
  ready. Working entries are ordered by run start and pinned in a shelf at the
  bottom of the sidebar. Recent is capped at ten with Show more/Show less.
- Search filters the Inbox in place. Archived sessions load on demand and keep
  the archived-only view and unarchive action. The Archived link only appears
  when the service reports an archived session.
- Context menus rename, mark read/unread and archive/unarchive. Archiving keeps
  the open transcript until the list confirms the change; failures leave it in
  place.
- Window shell requests native server-side decorations. When GPUI reports a
  client-decorated window, a minimal 36px titlebar with a project label,
  connection state and window controls is drawn instead.
- Home shows the project list, recent sessions, connection state and service
  settings; opening ZOC starts on Home so the Inbox is the primary navigator.

## Verification performed

- Podman Fedora 45: `cargo fmt --all --check`, `cargo clippy --all-targets --
  -D warnings`, 14 unit tests, `cargo build --release`, `zoc --check`.
- Xvfb + openbox with a disposable mock OpenCode HTTP/SSE service: inspected
  Home, grouped Inbox, hover/selection, working shelf, archived view,
  permission dock and context-menu archive flow, including a two-project
  Inbox and selecting a session from the non-active project. Sidebar drag was exercised to
  510px, the 640px maximum, the 136px minimum, and double-click reset to 344px;
  the width persisted in settings.
- The mock service was not the real OpenCode service. Server-side decorations
  were exercised under openbox; the client-decorated fallback was inspected by
  forcing its draw path because Xvfb provides no compositor.

## Known gaps

- Multiple servers are still discovered one at a time; the Inbox is global
  across projects on the connected service, not across simultaneous servers.
- The context menu uses the GPUI Kit popup surface; its radius/elevation still
  differs slightly from the official menu.
- Attention age uses the session's idle/update time; the pinned store request
  snapshots do not expose the original request timestamp.
- Session transcript, Markdown/tool-card density and Composer controls are a
  later milestone; only the header, user bubble, tool card and composer shell
  were adjusted here.
- Fedora GNOME/Wayland still needs physical validation: native close/minimize/
  maximize, titlebar drag and double-click, resize, 100%/150%/200% scaling,
  keyboard focus and IME.
