# Build

All local builds, tests, formatting, linting and packaging run inside the pinned Podman image. Do not install Go, Bun, MyGo, GTK headers or WebKitGTK development packages on the host.

## Prerequisites

- Podman (rootless is fine)
- Network access to GitHub, Go module proxy and npm registry for the first build
- About 4 GB free disk for the image and caches

## Commands

```sh
./scripts/container.sh ./scripts/check.sh   # format, vet, unit tests, adapter typecheck, frontend gates
./scripts/container.sh ./scripts/build.sh   # Web UI + MyGo packaging + metadata/checksums
./scripts/container.sh ./scripts/smoke.sh   # WebKitGTK + Xvfb + fixture server
```

`scripts/container.sh` builds `localhost/zoc-build:2026-11-16` on first use.

## Image pins

| Component | Pin |
|---|---|
| Base image | `registry.fedoraproject.org/fedora@sha256:a30417bd6c285eace1605deb9fba1160cbe2606f144ee3663bc4e65ec30c5da2` |
| Go | 1.27.4, official tarball SHA-256 verified |
| Bun | 1.4.4, release ZIP SHA-256 verified |
| MyGo | 0.9.2 (`go.mod`, `package.json`) |
| OpenCode | `upstream/upstream.json` (`dev` SHA) |
| Node/Electron/Chromium | absent |

The base image is digest-pinned. Fedora repository packages (GTK/WebKitGTK/Xvfb/fonts) are not individually NVR-pinned; the build image identifier and `go version`/`bun --version` are recorded in release metadata. If long-term bit-for-bit rebuilds are required, publish the built image by digest to a registry and pin that digest.

## Frontend build

`scripts/build-frontend.sh`:

1. Verifies/checks out the exact upstream SHA into `.upstream/opencode` (generated, ignored).
2. Applies every patch in `upstream/patches/` and fails if one does not apply.
3. Copies the ZOC-owned adapter and Vite config into the app package.
4. Installs upstream dependencies with `bun install --frozen-lockfile`.
5. Runs `vite build --config vite.zoc.config.ts` with `VITE_OPENCODE_SERVER_MODE=none`.
6. Fails if a service worker was emitted, if the root mount is missing, if lazy WASM/terminal assets disappeared, or if a localhost server URL was baked in.
7. Copies the static build into `frontend/` for MyGo to embed.

The built app needs no Node/Bun and no separately installed OpenCode runtime.

## Packaging

`mygo build` writes `build/linux-amd64/`:

- `zoc` executable, `.desktop`, icon and MIME XML;
- `zoc-<version>-linux-amd64.tar.gz` with `install.sh`;
- `zoc-<version>-1.x86_64.rpm`;
- signed update archives/manifests only when `MYGO_UPDATER_PRIVATE_KEY` and a public key are configured.

`scripts/write-release-metadata.sh` appends `upstream.json` and `SHA256SUMS`.

## Development loop

The official app is built from upstream; there is no ZOC frontend fork to hot-reload. Use:

```sh
./scripts/container.sh bash -lc 'bun install --frozen-lockfile && ./scripts/build-frontend.sh && bun run dev'
```

`mygo dev` serves `frontend/` from disk and needs a display. For headless checks use `scripts/smoke.sh`.

## Troubleshooting

- `WebViewAvailable` fails or the window is blank: install the runtime packages (`gtk3`, `webkit2gtk4.1`) and check the app's stderr. In containers, WebKitGTK needs an X server (Xvfb) and often `WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1`.
- DMA-BUF blank windows: ZOC follows MyGo's default and does not force-enable WebKitGTK DMA-BUF rendering. Measure before changing it (see [Performance](PERFORMANCE.md)).
- Upstream patch failure: the pinned SHA and patch no longer agree. Do not bypass the gate; update the patch deliberately and rerun all checks.
