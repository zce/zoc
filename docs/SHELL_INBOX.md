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
- Inbox cards share a roving keyboard cursor. Clicking a card focuses the
  Inbox; Up/Down/Home/End move the cursor through the visible Needs
  attention/Unread/Recent/Working rows, Enter opens the focused session and
  Escape clears the cursor. The focused card uses the official hover surface
  while the selected session keeps the pressed surface; both the main list and
  the pinned Working shelf scroll the focused row into view.
- Official status precedence: approval, form input, working, failed/done unread,
  ready. Working entries are ordered by run start and pinned in a shelf at the
  bottom of the sidebar. Recent is capped at ten with Show more/Show less.
- A selected session that falls outside the visible Recent limit is pinned at
  the top as an unlabelled current row, matching the official `current`
  section, and is not repeated in Recent.
- Cards show the current branch from the official store location catalog and a
  worktree marker when the session directory differs from the project canonical
  directory. The store's `vcs.branch.updated` handling refreshes the branch
  without a reconnect. Server tags remain absent for a single server, as the
  official `servers.tag` helper suppresses them when only one server is
  visible.
- Search filters the Inbox in place. Archived sessions load on demand and keep
  the archived-only view and unarchive action. The Archived link only appears
  when the service reports an archived session.
- Context menus rename, export, mark read/unread, archive/unarchive and delete.
  Export fetches the session info and all messages, writes the official
  `{info, messages}` JSON shape through the native save dialog and reports the
  result in a dismissible notice. Delete asks for confirmation, waits for the
  server's 204 response, forgets the store row and refreshes the Inbox.
  Archiving keeps the open transcript until the list confirms the change;
  failures leave it in place.
- Window shell requests native server-side decorations. When GPUI reports a
  client-decorated window, a minimal 36px titlebar with a project label,
  connection state and window controls is drawn instead.
- Home shows the project list, recent sessions, connection state and service
  settings; opening ZOC starts on Home so the Inbox is the primary navigator.

## Verification performed

- Podman Fedora 45: `cargo fmt --all --check`, `cargo clippy --all-targets --
  -D warnings`, 16 unit tests, `cargo build --release`, `zoc --check`.
- Xvfb + openbox with a disposable mock OpenCode HTTP/SSE service: inspected
  Home, grouped Inbox, hover/selection, working shelf, archived view,
  permission dock and context-menu archive flow, including a two-project
  Inbox and selecting a session from the non-active project. Sidebar drag was exercised to
  510px, the 640px maximum, the 136px minimum, and double-click reset to 344px;
  the width persisted in settings. Inbox keyboard navigation was exercised by
  clicking a card, moving with Down/Home/End, opening with Enter and confirming
  that search-field keys were not captured; in a 900x600 window the list
  scrolled the Recent section into view as the cursor advanced.
- The extended mock served `/api/vcs` and a worktree session: the branch and
  worktree markers rendered on cards. With twelve newer ready sessions the
  selected session was pinned above the sections, stayed selected and was not
  duplicated in the visible Recent rows. The context menu showed
  Rename/Export/Mark read/Archive/Delete; delete displayed the confirmation
  dialog, removed the row and kept Home open. Export invoked the native save
  dialog path and reported its unavailable-portal error without changing the
  selected session. The mock's `vcs.branch.updated` event changed the branch to
  `fix/live-refresh`, and the card tooltip updated without a reconnect. With
  nine working rows, End focused the last task and the pinned shelf scrolled it
  into view.
- The mock service was not the real OpenCode service. Server-side decorations
  were exercised under openbox; the client-decorated fallback was inspected by
  forcing its draw path because Xvfb provides no compositor.

## Known gaps

- Multiple servers are still discovered one at a time; the Inbox is global
  across projects on the connected service, not across simultaneous servers.
  Server tags are therefore intentionally absent for the single connected
  server, matching `servers.tag` when fewer than two servers are visible.
- The context menu uses the GPUI Kit popup surface; its radius/elevation still
  differs slightly from the official menu.
- Export's native save dialog, saved JSON content and delete against the real
  service were not verified. Xvfb has no XDG desktop portal, so only the
  unavailable-dialog error path was exercised.
- Attention age uses the session's idle/update time; the pinned store request
  snapshots do not expose the original request timestamp.
- Session transcript, Markdown/tool-card density and Composer controls are a
  later milestone; only the header, user bubble, tool card and composer shell
  were adjusted here.
- Fedora GNOME/Wayland still needs physical validation: native close/minimize/
  maximize, titlebar drag and double-click, resize, 100%/150%/200% scaling,
  keyboard focus and IME. Specifically, Tab into the Inbox, move with
  Up/Down/Home/End, open with Enter, clear with Escape, confirm the focused row
  stays visible while scrolling, and confirm search/composer typing is not
  intercepted. Against a real service, also confirm branch/worktree markers,
  the current row beyond Recent's ten, Export's save dialog and saved JSON, and
  Delete's confirmation plus list refresh.
