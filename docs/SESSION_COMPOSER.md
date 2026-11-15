# Session and Composer parity notes

Reference: `anomalyco/opencode` at commit
`d432a06b9c07bab0be4e8d642173b1cb14137f11`, primarily
`packages/session-ui/src/components/message-part.css`,
`packages/session-ui/src/components/basic-tool.*`,
`packages/app/src/session/timeline/message-timeline.tsx` and
`packages/app/src/composer/editor/editor.tsx`.

## What changed

- Transcript container at the official 744px timeline width with 24px side
  padding; composer and request docks stay at the 720px composer width.
- Messages are sorted chronologically before rendering. The previous store
  snapshot order put the newest message at the top of a tail-following scroller.
- User messages are right-aligned, fit-content bubbles with the official layer-02
  surface, 10px radius and 8px/12px padding.
- Markdown uses official heading sizes (17/15/14px) and a dark highlight theme;
  paragraphs use the Inter body scale. Reasoning is muted behind the existing
  Thinking toggle.
- Tool parts are compact 28px rows with a tool icon, label, subtitle/file,
  status and disclosure chevron. Clicking anywhere on the row, including the
  subtitle text, expands a layer-01 body; the separate file button opens the
  existing preview without toggling the row. `edit` tools show a red/green diff
  with old/new line-number gutters, three lines of context and added/removed
  counts; `write` shows added content; other tools show raw output text.
- Composer uses a 10px layer-01 card with 13px/20px editor text. The submit
  control is the official light 28px square, shows the stop glyph while a run is
  active, and is disabled at 40% when the prompt is empty or pending.
- Model, agent and variant pickers are compact 320px panels above the composer
  with selected check marks, a shared search field, bounded scrolling,
  provider/model/agent/variant filtering and keyboard Up/Down/Enter/Escape
  navigation. Models are grouped by provider with a letter tile; agents are
  grouped by Primary/Subagent mode. Opening one picker closes the others and
  focuses search.
- Permission requests use a neutral raised layer-01 dock with a warning icon,
  monospace resource patterns and the official action order: Reject, Always
  allow, Allow once. Form requests use the same surface with native string,
  option, boolean and multiselect controls, required-field checks and typed
  answers; the JSON answer editor is gone. Conditional fields support `eq`/`neq`
  rules, string options with `custom: true` show an Other input, and a typed
  answer is only built from currently visible fields.
- Session metadata no longer clears the selected model/agent when a session row
  omits those fields.

## Verification performed

- Podman Fedora 45: `cargo fmt --all --check`, `cargo clippy --all-targets --
  -D warnings`, 18 unit tests, `cargo build --release`, `zoc --check`.
- Xvfb + openbox with a disposable mock OpenCode transcript: inspected user and
  assistant turns, reasoning reveal, Markdown headings/lists/code, compact tool
  rows, expanded red/green diff with old/new line-number gutters, three context
  lines and added/removed counts, file preview open, permission dock, native
  question fields (including a conditional field and custom option input),
  provider-grouped model picker, agent mode grouping, keyboard Up/Down/Enter
  selection and composer states. Tool-row clicks toggled expansion at the top
  and after scrolling, including clicks on the subtitle; opening the file
  preview did not toggle the row. An SSE `session.text.delta` appended text into
  the open assistant row in place and the timeline stayed at its tail.
- This did not use the real OpenCode service and did not exercise long
  transcripts, attachments, images or real syntax coverage.

## Known gaps

- No composer attachments, image paste or file/image preview inside the
  transcript.
- Tool output is a simple unified diff for edit/write plus raw text for other
  tools; it has no syntax highlighting or dedicated read/grep/task renderers.
- Question forms support string, option, boolean, multiselect, `eq`/`neq`
  conditional fields and custom option text, but not external controls,
  server-side validation display or full official Select/Checkbox styling.
- Picker provider and model icons are letter tiles rather than official provider
  artwork.
- No token/cost/duration footer under responses or per-message action menus.
- The file preview panel is the existing generic viewer, not the official review
  pane.

## Fedora GNOME/Wayland checklist

1. Open a real session and confirm chronological transcript order, tail
   following, scrolling up without jumps, and the jump-to-latest affordance.
2. Send a prompt and confirm the transcript updates in place while streaming;
   click stop and confirm the submit control returns to send.
3. Expand a tool row, open its file subtitle, and close the preview without
   losing transcript position.
4. Open model/agent/variant pickers and confirm selection changes are reflected
   in the composer and applied through the official prompt gate.
5. Answer a permission request and a question form; confirm the request
   disappears only after the server acknowledges it.
6. Check Markdown headings, lists, tables and fenced code with dark syntax
   colours, and confirm selection/copy works at 100% and 150% scaling.
