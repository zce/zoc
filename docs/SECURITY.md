# Security model

ZOC is a client: the OpenCode Server is authoritative and the app only exposes the transports and desktop APIs it needs.

## Trust boundaries

- **Trusted page:** the embedded official Web UI at `mygo://localhost`. It may call bound Go methods.
- **OpenCode Server:** configured by the user; holds session data and credentials.
- **Untrusted web content:** external links are opened in the system browser, never embedded as trusted pages. The Web UI's own previews stay upstream's responsibility.

## Transport allowlist

Native fetch and WebSocket transports are not general proxies.

- Only origins derived from persisted server profiles are allowed. For a loopback profile, `localhost`, `127.0.0.1` and `::1` on the same scheme and port are the same origin.
- `http`, `https`, `ws` and `wss` are supported; all other schemes are refused.
- URLs with userinfo are rejected so credentials never appear in profile URLs or logs.
- Redirects: at most five, target must be configured, origin must not change.
- WebSocket handshakes do not forward the app's `mygo://localhost` Origin. OpenCode accepts non-browser clients without an Origin header.
- The frontend uses MyGo's fetch only for server requests. `blob:`, `data:`, `mygo:` and unrelated external requests use the WebView's native fetch.

Adding a remote server requires opening **Servers & Inbox** and saving it. Typing an arbitrary URL into the official connect screen cannot widen the Go transport allowlist.

## Credentials

- ZOC server profiles store display names and base URLs only, mode `0600`.
- Passwords entered in the native inbox stay in process memory and are not written to disk.
- The official Web UI handles its own authentication and persistence upstream.
- ZOC never logs authorization headers, pairing tokens, ticket URLs or passwords. Request errors name method and path, not query strings.

## Desktop APIs

Bound methods are callable only by trusted app pages. `OpenExternal` accepts `http`, `https` and `mailto`; `OpenLocalFile` accepts `file:`; path operations are explicit desktop actions. Native pickers and save dialogs are parented to the calling window.

The Electron GUI plugin host, WSL/SSH plugin servers and sidecar lifecycle are intentionally not implemented. No Electron, Chromium or Node runtime is introduced.

## Release credential isolation

The release workflow separates the untrusted upstream build from signing and publishing:

- `verify` has no signing key and no write-capable token. It installs upstream dependencies, builds the official Web UI with Vite, runs tests, builds unsigned artifacts and runs the WebKitGTK smoke.
- `sign` holds `MYGO_UPDATER_PRIVATE_KEY` but contains no upstream checkout, dependency installation or untrusted preparation. It builds a fresh tree from `git archive HEAD`, receives the verified frontend, copies the pinned MyGo CLI only after checking its SHA-256, and runs `scripts/build-signed.sh` in a separate container. The signing container has no untrusted process to read its environment. `build-signed.sh` re-checks the frontend and toolchain hashes, skips the upstream `buildCommand`, and uses MyGo's supported signing path.
- `publish` holds a write token but no signing key. It verifies checksums, runs the final smoke on the exact artifact, then tags and publishes.

Signing continuity is fail-closed: once a stable release has a signed update manifest, later releases require both keys and the same public key recorded in the previous release's `upstream.json`. A missing or changed key stops the workflow before a build. Delta updates are disabled so the signing job needs no GitHub credential.

## Updates

- Archive installs may use MyGo's signed updater only when a key pair is provisioned.
- The private key lives only in GitHub Actions secret `MYGO_UPDATER_PRIVATE_KEY`; never commit or print it.
- RPM installs are updated by DNF, not the in-app updater.
- A failed sync publishes nothing; the last known-good release remains.
- Release tags use `zoc-v*`, separate from other release streams.

## Reporting

Report security issues privately to the repository owner rather than in a public issue. Do not include tokens, pairing URLs or server credentials in reports.
