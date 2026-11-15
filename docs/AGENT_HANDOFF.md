# ZOC — Agent Handoff: Official Desktop UI Parity

> Status: experimental baseline consolidated on 2026-11-15. This is a working prototype, **not** a finished v0.1 or a verified pixel-perfect clone. Start from the repository's **main** branch.

## Mission

Make **ZOC** a lightweight, genuinely native Linux desktop client that **looks and behaves like the official OpenCode Desktop (Electron) UI**, especially its Inbox layout. Visual fidelity is a product requirement, not optional polish. Core functionality may be intentionally limited, but what is visible should be coherent, polished, and feel official.

Do **not** redesign OpenCode, invent a new layout, or port Electron/Chromium/WebView into the client. Keep **Rust + GPUI Kit** and connect to an existing OpenCode service. Treat the upstream official app as the product/design reference, not as a requirement to duplicate its runtime implementation.

## Repository baseline and PR disposition

- Repository: https://github.com/zce/zoc
- Source of truth: **main**. The 2026-11-15 consolidation included experimental commits through `e51cd4e5c6e366227fdd57e5cdc672e627771058` by a non-forced fast-forward; subsequent commits may advance main.
- PR #1 (native prototype): **merged** into main.
- PR #2 (isolated Rust API-types spike): **closed as superseded**; its experiment-only tests were *not* independently merged.
- PR #4 (official OpenCode Store integration): **closed after its full commits were integrated into main**.
- PR #5 (native titlebar/window chrome): **closed after its full commits were integrated into main**.
- #4/#5 show "closed", not "merged", because GitHub's regular merge and base-retarget endpoints reject this stack. **Do not interpret their state as rejected code**.
- No open PRs remained at handoff. Create new, focused PRs **against main**; do not rebuild the old stack.

## Existing implementation: inspect before changing

- `Cargo.toml`: Rust 2024, `gpui-kit = "=0.9.1"`, pinned OpenCode Rust packages.
- `src/api.rs`: OpenCode client/connection and command boundary.
- `src/state.rs`: ZOC-facing projected application state plus presentation state.
- `src/main.rs`: UI and native application lifecycle; much of the UI is currently concentrated here. Reorganize only when it helps deliver a cohesive UI, not as an architectural project in itself.
- `docs/UPSTREAM_COMPAT.md`: authority/ownership boundary and pinned dependency SHA.
- `docs/FEATURE_PARITY.md`, `docs/PERFORMANCE.md`: **claims of implementation are not real-service, Fedora, or performance verification**.
- `.github/workflows/ci.yml`: PR/main validation and runnable x86_64 GitHub Actions artifacts.

### Upstream reuse boundary

Use OpenCode's pinned Rust crates: `opencode-api` for generated HTTP/API contracts, `opencode-local` for service registration, `opencode-store` for SSE lifecycle, caching, deltas, prompt admission, permission/forms and session state. The existing pins use OpenCode commit `5b9a2128fd39abd0126b856c7d0f6f9aa7c0fb5f`; keep related crates on one tested immutable commit.

ZOC owns native window, layout, navigation, local presentation preferences, and rendering. Avoid a second SSE/event reducer, hand-written replacement for official protocol/state, local agent/provider runtime, direct edits to OpenCode's database, and unnecessary new dependencies.

### Current known quality gaps

The project is usable enough to explore, but the UI lacks convincing official Desktop visual parity. The initial window lacked close controls and draggable titlebar. PR #5 switched to GPUI Kit's native `TitleBar` and requested Linux client-side decorations (CSD); **actual Fedora GNOME/Wayland window behavior still needs manual verification**. Layout, typography, spacing, Inbox rows, transcript, and Composer need substantial attention.

The previous PR #1 CI failed on an old Clippy large-enum warning; PR #4's final CI passed. PR #5 initially failed on an unused `border` binding, fixed by commit `e51cd4e`. Check the latest **main** CI after consolidation rather than assuming the new binary or native window is verified.

Some legacy docs/README language still describes "bootstrap only" or the old draft PR workflow. Refresh those statements as part of the next coherent documentation update.

## Authoritative UI references

Inspect the **current official OpenCode Desktop source** at https://github.com/anomalyco/opencode. The desktop frontend lives in the app package; check the desktop host separately for platform-specific behavior.

Start with:
- `packages/app/src/shell/titlebar/titlebar.tsx` and `titlebar.css`.
- `packages/app/src/shell/titlebar/inbox.tsx`, `inbox-model.ts`, `inbox-card.stories.tsx`, `inbox-list.stories.tsx`.
- Official session/composer/transcript components, their CSS, icons and **actual design tokens** (locate current paths; do not guess).
- `packages/desktop` for current Linux window semantics.
- GPUI Kit `TitleBar` component/examples for CSD, window controls, drag, double-click, and SSD fallbacks.

Capture **real reference screenshots** from the official application (or its maintained stories), with comparable window size, scaling and state. Do not treat the old ZOC styling, generic “dark UI” images, or generated mockups as the source of truth. Record upstream commit/ref used for comparisons. The official frontend may move ahead of the pinned Rust API; UI research and dependency upgrades are separate decisions.

## Delivery priorities

### Stage A — Native window + shell (first testable handoff)

1. Confirm latest main CI, fix genuine build/clippy issues, launch a real GNOME/Wayland window.
2. Confirm functional **close, minimize, maximize/restore**, drag from empty titlebar area, double-click maximize/restore, proper non-draggable buttons/inputs; never display duplicate controls under SSD.
3. Match official desktop shell: titlebar composition, sizes, colors, iconography, pane boundaries, sidebar width, alignment, typography, density and surface hierarchy.
4. Keep project selection, connection settings and session navigation usable. Prefer native/system-provided behavior when it is correct.

### Stage B — Inbox visual fidelity

Recreate official **Needs Attention / Unread / Recent / Working** navigation and session cards. Match grouping, empty states, status cues, badge treatment, hover/selected/focus/disabled states, truncation, spacing, sidebar actions and contextual menu details. Use the **official Inbox semantics**, not cosmetic imitations. Keep navigation and scrolling stable under updates.

### Stage C — Session + Composer

Bring transcript density, role distinction, tool execution cards, reasoning, streaming, syntax-highlighted code, Markdown, file links, pending permissions/questions, composer shape, model/agent controls and disabled/loading states in line with the official UI. Preserve correct execution behavior; never auto-approve permissions or fabricate agent output. Defer expensive features without inserting ugly fake controls.

### Later, only when justified

Native file/image/diff previews, richer menus and interactions, performance tuning. Avoid feature expansion before the main shell feels right.

## Acceptance and proof

- **Visual:** side-by-side comparison with official Desktop at matching dimensions and state; review spacing, visual hierarchy, color, fonts, icons, borders, surfaces, row density and interaction states. Explain any unavoidable native differences.
- **Window:** physical Fedora GNOME/Wayland test for controls, titlebar drag/double-click, resizing, scale factors (including 4K/150%), keyboard focus and IME.
- **Behavior:** real existing OpenCode service; browse projects/sessions, view transcript, send/stop a prompt, handle permission/question without changing protocol behavior.
- **Engineering:** `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo build --release`, startup check; compare current CI. These are **not** substitutes for manual UI validation.
- **Packaging:** GitHub Actions artifact named `zoc-linux-x86_64-pr<N>` or `zoc-linux-x86_64-main`. Prefer the established Artifact workflow; do not publish tags/Releases or merge automatically.
- **Evidence:** every handoff states commit SHA, CI run, what changed, what actually ran, what is unverified, and **one short focused test request**. Do not claim screenshots/performance/real Wayland tests unless performed.

## Working style

Work in small, vertically complete, reviewable PRs based on main. First ship a presentable window/shell, then Inbox, then session content. Compare against the real official source before coding each area. Use existing GPUI Kit components and OpenCode crates; implement a missing native widget only when needed. Fix evidenced defects and preserve existing data/state contracts, rather than doing unrelated refactors or speculative abstractions.

The success criterion is simple: opening ZOC should feel like opening the official OpenCode Desktop, not a separate app that merely borrows its colors—while remaining a lightweight native Linux client.
