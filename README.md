# ZOC

**ZOC** is a lightweight, native Linux desktop client for an existing [OpenCode](https://github.com/anomalyco/opencode) service.

The first prototype targets **Fedora GNOME / Wayland on x86_64**, using **Rust + GPUI Kit**. ZOC is a client only: it will use OpenCode's official HTTP and SSE APIs rather than bundling an agent runtime, server, browser, or JavaScript runtime.

## Goals for v0.1

- Connect to an existing OpenCode service, with XDG service discovery, manual connection and authentication.
- Provide an Inbox-style project/session navigator and real agent conversations.
- Stream messages and tool progress; handle permission requests, questions, errors and cancellation.
- Render Markdown and code, and preview files and Git changes inside the native window.
- Stay responsive during long sessions and publish downloadable Linux test builds through GitHub Actions Artifacts.

## Current status

**Experimental native prototype, not yet release-ready.** The shell, Inbox, session transcript and Composer now follow OpenCode Desktop's current design tokens, 344px sidebar proportions, 8px panel insets and Inter typography. Fedora GNOME/Wayland window behavior, streaming transcripts and a real OpenCode service still need manual verification; see [shell and Inbox notes](docs/SHELL_INBOX.md), [session and Composer notes](docs/SESSION_COMPOSER.md), the [agent handoff](docs/AGENT_HANDOFF.md) and the [feature checklist](docs/FEATURE_PARITY.md).

## Development principles

Favor small, cohesive Rust modules and existing GPUI Kit components. Follow current OpenCode upstream contracts; do not recreate its server-side state. Do not add WebView, Electron, Node.js or Bun. Optimize for usable core workflows before optional features.

## References

- [OpenCode](https://github.com/anomalyco/opencode)
- [GPUI Kit](https://github.com/longbridge/gpui-kit)
- [MSGA CI reference](https://github.com/zce/make-slack-great-again/blob/dev/.github/workflows/ci.yml)
