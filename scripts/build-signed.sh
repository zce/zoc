#!/usr/bin/env bash
# Trusted signing and packaging stage.
#
# This script runs after the untrusted upstream build has completed and its
# verified frontend has been handed over as an artifact. It must not run the
# upstream frontend build command or any other code from the upstream
# repository. Only ZOC's own Go host and MyGo's packaging/signing path run
# while the update signing key is present.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}"

: "${ZOC_VERSION:?ZOC_VERSION is required}"
: "${ZOC_UPSTREAM_COMMIT:?ZOC_UPSTREAM_COMMIT is required}"
: "${ZOC_FRONTEND_SHA:?ZOC_FRONTEND_SHA is required}"
: "${ZOC_UPDATER_PUBLIC_KEY:?ZOC_UPDATER_PUBLIC_KEY is required}"
: "${MYGO_UPDATER_PRIVATE_KEY:?MYGO_UPDATER_PRIVATE_KEY is required}"

actual_frontend="$(python3 "${ROOT}/scripts/frontend-hash.py" "${ROOT}/frontend")"
if [[ "${actual_frontend}" != "${ZOC_FRONTEND_SHA}" ]]; then
  echo "error: frontend hash ${actual_frontend} does not match verified ${ZOC_FRONTEND_SHA}" >&2
  exit 1
fi

"${ROOT}/scripts/prepare-upstream.sh"
actual_commit="$(git -C "${ROOT}/.upstream/opencode" rev-parse HEAD)"
if [[ "${actual_commit}" != "${ZOC_UPSTREAM_COMMIT}" ]]; then
  echo "error: upstream commit ${actual_commit} does not match ${ZOC_UPSTREAM_COMMIT}" >&2
  exit 1
fi

bun install --frozen-lockfile

echo "==> Signed MyGo build (upstream build command skipped)"
bun run build -- -skip-build-command

"${ROOT}/scripts/write-release-metadata.sh"

manifest="${ROOT}/build/linux-amd64/update-linux-amd64.json"
[[ -f "${manifest}" ]] || { echo "error: signed update manifest is missing" >&2; exit 1; }
python3 - "${manifest}" "${ROOT}/build/linux-amd64/upstream.json" <<'PY'
import json, sys
manifest = json.load(open(sys.argv[1]))
metadata = json.load(open(sys.argv[2]))
if metadata.get("updateSigning") != "provisioned":
    raise SystemExit("error: release metadata does not record signed updates")
if not metadata.get("updaterPublicKey"):
    raise SystemExit("error: release metadata has no updater public key")
if not manifest.get("version") or not manifest.get("signature"):
    raise SystemExit("error: update manifest is not signed")
PY
echo "signed artifact verified"
