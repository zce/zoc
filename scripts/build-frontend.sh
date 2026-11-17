#!/usr/bin/env bash
# Build the official OpenCode Web UI from the pinned upstream commit with
# ZOC's adapter and Vite configuration, then place the static output in
# frontend/ for `mygo build` to embed.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
UPSTREAM_DIR="${ZOC_UPSTREAM_DIR:-${ROOT}/.upstream/opencode}"
COMMIT="${ZOC_UPSTREAM_COMMIT:-$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["commit"])' "${ROOT}/upstream/upstream.json")}"

cd "${ROOT}"
"${ROOT}/scripts/prepare-upstream.sh"
if [[ ! -f "${ROOT}/adapter/src/mygo.ts" ]]; then
  echo "error: generated MyGo client is missing; run mygo generate first" >&2
  exit 1
fi
install -m 0644 "${ROOT}/adapter/src/mygo.ts" "${UPSTREAM_DIR}/packages/app/src/zoc/mygo.ts"

if [[ ! -d node_modules ]]; then
  echo "Installing ZOC frontend dependencies"
  bun install --frozen-lockfile
fi

cd "${UPSTREAM_DIR}"
echo "Installing upstream dependencies from its frozen lockfile"
bun install --frozen-lockfile

export VITE_OPENCODE_SERVER_MODE=none
export OPENCODE_CHANNEL="${OPENCODE_CHANNEL:-prod}"
echo "Building official OpenCode Web UI at ${COMMIT}"
(cd packages/app && bun x vite build --config vite.zoc.config.ts)

DIST="${UPSTREAM_DIR}/packages/app/dist"
test -f "${DIST}/index.html"

# Gates that catch an accidental service worker or missing lazy assets
# before they are embedded in a release.
if [[ -e "${DIST}/sw.js" ]]; then
  echo "error: PWA service worker was emitted" >&2
  exit 1
fi
if grep -RIl "serviceWorker.register" "${DIST}" >/dev/null 2>&1; then
  echo "error: service-worker registration survived the ZOC build" >&2
  exit 1
fi
if ! grep -q '_assets/' "${DIST}/index.html"; then
  echo "error: built index.html does not reference the upstream asset directory" >&2
  exit 1
fi

rm -rf "${ROOT}/frontend"
mkdir -p "${ROOT}/frontend"
cp -a "${DIST}/." "${ROOT}/frontend/"

cat >"${ROOT}/.cache/frontend-build.json" <<EOF
{
  "upstreamCommit": "${COMMIT}",
  "packageVersion": "$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["version"])' "${UPSTREAM_DIR}/packages/app/package.json")",
  "builtAt": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "serviceWorker": false
}
EOF

python3 "${ROOT}/scripts/verify-frontend.py" "${ROOT}/frontend"

echo "Web UI built into ${ROOT}/frontend"
