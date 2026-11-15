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
  status and disclosure chevron. Clicking the row expands a layer-01 body;
  clicking a file subtitle opens the existing preview and does not toggle the
  row. `edit` tools show removed/added lines as a red/green two-block diff;
  `write` shows added content; other tools show raw output text.
- Composer uses a 10px layer-01 card with 13px/20px editor text. The submit
  control is the official light 28px square, shows the stop glyph while a run is
  active, and is disabled at 40% when the prompt is empty or pending.
- Model, agent and variant pickers are compact 320px panels above the composer
  with selected check marks and bounded scrolling. Opening one picker closes the
  others.
- Permission requests use a neutral raised layer-01 dock with a warning icon,
  monospace resource patterns and the official action order: Reject, Always
  allow, Allow once. Form requests use the same surface with native string,
  option, boolean and multiselect controls, required-field checks and typed
  answers; the JSON answer editor is gone.
- Session metadata no longer clears the selected model/agent when a session row
  omits those fields.

## Verification performed

- Podman Fedora 45: `cargo fmt --all --check`, `cargo clippy --all-targets --
  -D warnings`, 15 unit tests, `cargo build --release`, `zoc --check`.
- Xvfb + openbox with a disposable mock OpenCode transcript: inspected user and
  assistant turns, reasoning reveal, Markdown headings/lists/code, compact tool
  rows, expanded red/green diff, file preview open, permission dock, native
  question fields, model picker and composer states.
- This did not use the real OpenCode service and did not exercise streaming
  deltas, long transcripts, attachments, images or real syntax coverage.

## Known gaps

- No composer attachments, image paste or file/image preview inside the
  transcript.
- Tool output is a simple two-block diff for edit/write; syntax-highlighted
  line numbers, context lines and dedicated read/grep/task renderers are not
  implemented.
- Question forms support string, option, boolean and multiselect fields, but
  not conditional `when` rules, custom option text, external controls or
  server-side validation display.
- Model/agent pickers have no search, provider icons or grouping, and no keyboard
  navigation.
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
