# ZOC

**ZOC** is a planned lightweight, native Linux desktop client for an existing [OpenCode](https://github.com/anomalyco/opencode) service.

The first prototype targets **Fedora GNOME / Wayland on x86_64**, using **Rust + GPUI Kit**. ZOC is a client only: it will use OpenCode's official HTTP and SSE APIs rather than bundling an agent runtime, server, browser, or JavaScript runtime.

## Goals for v0.1

- Connect to an existing OpenCode service, with XDG service discovery, manual connection and authentication.
- Provide an Inbox-style project/session navigator and real agent conversations.
- Stream messages and tool progress; handle permission requests, questions, errors and cancellation.
- Render Markdown and code, and preview files and Git changes inside the native window.
- Stay responsive during long sessions and publish downloadable Linux test builds through GitHub Actions Artifacts.

## Current status

**Bootstrap only.** This README does not imply the application has been implemented. Development and the full acceptance checklist are tracked in the initial Draft Pull Request.

## Development principles

Favor small, cohesive Rust modules and existing GPUI Kit components. Follow current OpenCode upstream contracts; do not recreate its server-side state. Do not add WebView, Electron, Node.js or Bun. Optimize for usable core workflows before optional features.

## References

- [OpenCode](https://github.com/anomalyco/opencode)
- [GPUI Kit](https://github.com/longbridge/gpui-kit)
- [MSGA CI reference](https://github.com/zce/make-slack-great-again/blob/dev/.github/workflows/ci.yml)
