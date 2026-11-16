#!/usr/bin/env bash
# Write reproducible release metadata and checksums next to the packaged
# Linux artifacts. Runs inside the pinned Podman image after `mygo build`.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${ROOT}/build/linux-amd64"
[[ -d "${OUT}" ]] || { echo "error: ${OUT} not found" >&2; exit 1; }

VERSION="${ZOC_VERSION:-$(cat "${ROOT}/VERSION")}"
COMMIT="${ZOC_UPSTREAM_COMMIT:-$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["commit"])' "${ROOT}/upstream/upstream.json")}"
BRANCH="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["branch"])' "${ROOT}/upstream/upstream.json")"
PACKAGE="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["package"])' "${ROOT}/upstream/upstream.json")"
PACKAGE_VERSION="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["packageVersion"])' "${ROOT}/upstream/upstream.json")"
MYGO_VERSION="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["mygoVersion"])' "${ROOT}/upstream/upstream.json")"
GO_VERSION="$(go env GOVERSION | sed 's/^go//')"
BUN_VERSION="$(bun --version)"
IMAGE="${ZOC_BUILD_IMAGE:-localhost/zoc-build:2026-11-16}"
BUILT_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

FRONTEND_HASH="$(python3 - <<'PY'
import hashlib, pathlib, sys
root = pathlib.Path('frontend')
h = hashlib.sha256()
for p in sorted(root.rglob('*')):
    if p.is_file():
        h.update(str(p.relative_to(root)).encode())
        h.update(b'\0')
        h.update(p.read_bytes())
print(h.hexdigest())
PY
)"

PATCHES_JSON="$(python3 - <<'PY'
import hashlib, json, pathlib
items = []
for p in sorted(pathlib.Path('upstream/patches').glob('*.patch')):
    items.append({"id": p.stem, "file": str(p), "sha256": hashlib.sha256(p.read_bytes()).hexdigest()})
for p in [pathlib.Path('adapter/src/platform.ts'), pathlib.Path('adapter/vite.zoc.config.ts')]:
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
  "serviceWorker": False,
}
json.dump(data, open(out, "w"), indent=2)
open(out, "a").write("\n")
PY

(
  cd "${OUT}"
  rm -f SHA256SUMS
  find . -maxdepth 1 -type f ! -name SHA256SUMS ! -name upstream.json ! -name .mygo-files -printf '%P\n' | sort | while read -r f; do
    sha256sum "$f"
  done > SHA256SUMS
  sha256sum upstream.json >> SHA256SUMS
)

cp "${OUT}/upstream.json" "${ROOT}/build/upstream.json"
echo "metadata written to ${OUT}/upstream.json"
cat "${OUT}/upstream.json"
