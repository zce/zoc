#!/usr/bin/env bash
# Write reproducible release metadata and checksums next to the packaged
# Linux artifacts. Runs inside the pinned Podman image after `mygo build`.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${ROOT}/build/linux-amd64"
[[ -d "${OUT}" ]] || { echo "error: ${OUT} not found" >&2; exit 1; }

VERSION="${ZOC_VERSION:-$(cat "${ROOT}/VERSION")}"
if [[ -n "${MYGO_UPDATER_PRIVATE_KEY:-}" && -z "${ZOC_UPDATER_PUBLIC_KEY:-}" ]]; then
  echo "error: a private updater key needs ZOC_UPDATER_PUBLIC_KEY" >&2
  exit 1
fi
UPSTREAM_DIR="${ZOC_UPSTREAM_DIR:-${ROOT}/.upstream/opencode}"
COMMIT="${ZOC_UPSTREAM_COMMIT:-$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["commit"])' "${ROOT}/upstream/upstream.json")}"
BRANCH="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["branch"])' "${ROOT}/upstream/upstream.json")"
if [[ -n "${ZOC_UPSTREAM_PACKAGE:-}" || -n "${ZOC_UPSTREAM_PACKAGE_VERSION:-}" ]]; then
  # The isolated signing stage has no upstream checkout. It receives the
  # package identity from the credential-free build that produced and
  # verified the frontend; the pinned commit SHA remains authoritative.
  : "${ZOC_UPSTREAM_PACKAGE:?ZOC_UPSTREAM_PACKAGE is required with ZOC_UPSTREAM_PACKAGE_VERSION}"
  : "${ZOC_UPSTREAM_PACKAGE_VERSION:?ZOC_UPSTREAM_PACKAGE_VERSION is required with ZOC_UPSTREAM_PACKAGE}"
  PACKAGE="${ZOC_UPSTREAM_PACKAGE}"
  PACKAGE_VERSION="${ZOC_UPSTREAM_PACKAGE_VERSION}"
else
  PACKAGE="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["name"])' "${UPSTREAM_DIR}/packages/app/package.json")"
  PACKAGE_VERSION="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["version"])' "${UPSTREAM_DIR}/packages/app/package.json")"
  ACTUAL_COMMIT="$(git -C "${UPSTREAM_DIR}" rev-parse HEAD 2>/dev/null || true)"
  if [[ "${ACTUAL_COMMIT}" != "${COMMIT}" ]]; then
    echo "error: checked-out upstream ${ACTUAL_COMMIT:-<missing>} does not match ${COMMIT}" >&2
    exit 1
  fi
fi
MYGO_VERSION="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["mygoVersion"])' "${ROOT}/upstream/upstream.json")"
GO_VERSION="$(go env GOVERSION | sed 's/^go//')"
BUN_VERSION="$(bun --version)"
IMAGE="${ZOC_BUILD_IMAGE:-localhost/zoc-build:2026-11-16}"
BUILT_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

FRONTEND_HASH="$(python3 "${ROOT}/scripts/frontend-hash.py" "${ROOT}/frontend")"

PATCHES_JSON="$(python3 - <<'PY'
import hashlib, json, pathlib
items = []
for p in sorted(pathlib.Path('upstream/patches').glob('*.patch')):
    items.append({"id": p.stem, "file": str(p), "sha256": hashlib.sha256(p.read_bytes()).hexdigest()})
for p in [
    pathlib.Path('adapter/src/platform.ts'),
    pathlib.Path('adapter/src/storage.ts'),
    pathlib.Path('adapter/src/window-controls.ts'),
    pathlib.Path('adapter/src/zoc.css'),
    pathlib.Path('adapter/vite.zoc.config.ts'),
]:
    items.append({"id": p.stem + "-adapter", "file": str(p), "sha256": hashlib.sha256(p.read_bytes()).hexdigest()})
print(json.dumps(items, separators=(',', ':')))
PY
)"

python3 - "${OUT}/upstream.json" "${VERSION}" "${COMMIT}" <<PY
import json, sys
out, version, commit = sys.argv[1], sys.argv[2], sys.argv[3]
data = {
  "schemaVersion": 1,
  "upstreamRepository": "anomalyco/opencode",
  "upstreamBranch": ${BRANCH@Q},
  "upstreamCommit": commit,
  "upstreamPackage": ${PACKAGE@Q},
  "upstreamPackageVersion": ${PACKAGE_VERSION@Q},
  "zocVersion": version,
  "mygoVersion": ${MYGO_VERSION@Q},
  "goVersion": ${GO_VERSION@Q},
  "bunVersion": ${BUN_VERSION@Q},
  "adapterPatches": json.loads(${PATCHES_JSON@Q}),
  "frontendSha256": ${FRONTEND_HASH@Q},
  "buildImage": ${IMAGE@Q},
  "builtAt": ${BUILT_AT@Q},
  "updateSigning": "provisioned" if __import__("os").environ.get("MYGO_UPDATER_PRIVATE_KEY") else "not-provisioned",
  "updaterPublicKey": __import__("os").environ.get("ZOC_UPDATER_PUBLIC_KEY", ""),
  "serviceWorker": False,
}
json.dump(data, open(out, "w"), indent=2)
open(out, "a").write("\n")
PY

(
  cd "${OUT}"
  rm -f SHA256SUMS
  find . -maxdepth 1 -type f ! -name SHA256SUMS ! -name upstream.json ! -name .mygo-files ! -name zoc -printf '%P\n' | sort | while read -r f; do
    sha256sum "$f"
  done > SHA256SUMS
  sha256sum upstream.json >> SHA256SUMS
)

cp "${OUT}/upstream.json" "${ROOT}/build/upstream.json"
echo "metadata written to ${OUT}/upstream.json"
cat "${OUT}/upstream.json"
