#!/usr/bin/env bash
# Build (once) and run commands in ZOC's pinned Podman image.
#
# All local Go, Bun, test and packaging work must go through this script.
# No host toolchains are installed or modified.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IMAGE="${ZOC_IMAGE:-localhost/zoc-build:2026-11-16}"

if ! podman image exists "${IMAGE}"; then
  echo "Building ${IMAGE} ..." >&2
  podman build --pull=missing -f "${ROOT}/Containerfile" -t "${IMAGE}" "${ROOT}"
fi

mkdir -p "${ROOT}/.cache/go-build" "${ROOT}/.cache/go-mod" "${ROOT}/.cache/bun"

podman run --rm \
  --userns=keep-id \
  --network=host \
  -v "${ROOT}:/src:Z" \
  -v "${ROOT}/.cache/go-build:/tmp/zoc-cache/go-build:Z" \
  -v "${ROOT}/.cache/go-mod:/tmp/go-mod:Z" \
  -v "${ROOT}/.cache/bun:/tmp/zoc-cache/bun:Z" \
  -e HOME=/tmp/zoc-home \
  -e GOCACHE=/tmp/zoc-cache/go-build \
  -e GOMODCACHE=/tmp/go-mod \
  -e BUN_INSTALL_CACHE_DIR=/tmp/zoc-cache/bun \
  -e ZOC_BUILD_IMAGE="${IMAGE}" \
  -e ZOC_VERSION="${ZOC_VERSION:-$(cat "${ROOT}/VERSION")}" \
  -e ZOC_UPSTREAM_COMMIT \
  -e ZOC_UPDATER_PUBLIC_KEY \
  -e MYGO_UPDATER_PRIVATE_KEY \
  -e ZOC_SMOKE_DIR \
  -e ZOC_SMOKE_PORT \
  -e ZOC_PERF_DIR \
  -e ZOC_PERF_PORT \
  -e ZOC_PERF_SECONDS \
  -w /src \
  "${IMAGE}" "$@"
