#!/usr/bin/env bash
# Check out the pinned upstream OpenCode monorepo, apply the ZOC adapter
# patch set, and copy ZOC-owned adapter sources into the app package.
#
# This script is intended to run inside the Podman image. It only touches
# .upstream/ (a generated, ignored directory), never a user's checkout.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
META="${ROOT}/upstream/upstream.json"
UPSTREAM_DIR="${ZOC_UPSTREAM_DIR:-${ROOT}/.upstream/opencode}"

read_json() {
  python3 -c "import json,sys; print(json.load(open(sys.argv[1]))[sys.argv[2]])" "${META}" "$1"
}

REPO="$(read_json repository)"
COMMIT="${ZOC_UPSTREAM_COMMIT:-$(read_json commit)}"
if [[ ! "${COMMIT}" =~ ^[0-9a-f]{40}$ ]]; then
  echo "error: upstream commit must be a full 40-character SHA" >&2
  exit 1
fi

if [[ ! -d "${UPSTREAM_DIR}/.git" ]]; then
  mkdir -p "$(dirname "${UPSTREAM_DIR}")"
  git init -q "${UPSTREAM_DIR}"
  git -C "${UPSTREAM_DIR}" remote add origin "https://github.com/${REPO}.git"
fi

git -C "${UPSTREAM_DIR}" remote set-url origin "https://github.com/${REPO}.git"
if ! git -C "${UPSTREAM_DIR}" cat-file -e "${COMMIT}^{commit}" 2>/dev/null; then
  git -C "${UPSTREAM_DIR}" fetch --depth 1 origin "${COMMIT}"
fi
# The directory is a generated build cache, not a user working tree.
git -C "${UPSTREAM_DIR}" checkout -q -f "${COMMIT}"
git -C "${UPSTREAM_DIR}" clean -qfdx
test "$(git -C "${UPSTREAM_DIR}" rev-parse HEAD)" = "${COMMIT}"

for patch in "${ROOT}"/upstream/patches/*.patch; do
  echo "Applying $(basename "${patch}")"
  git -C "${UPSTREAM_DIR}" apply --check "${patch}"
  git -C "${UPSTREAM_DIR}" apply "${patch}"
done

mkdir -p "${UPSTREAM_DIR}/packages/app/src/zoc"
install -m 0644 "${ROOT}/adapter/src/platform.ts" "${UPSTREAM_DIR}/packages/app/src/zoc/platform.ts"
install -m 0644 "${ROOT}/adapter/vite.zoc.config.ts" "${UPSTREAM_DIR}/packages/app/vite.zoc.config.ts"

echo "Upstream ready at ${COMMIT}"
