#!/usr/bin/env bash
# Trusted signing and packaging stage.
#
# This script runs in a signing workspace created from a git archive of the
# reviewed ZOC commit. It contains the verified static frontend and a pinned,
# hash-verified MyGo CLI binary. It must not run the upstream frontend build
# command, install dependencies, or execute any code from the upstream
# repository. Only ZOC's Go host and MyGo's packaging/signing path run while
# the update signing key is present.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}"

: "${ZOC_VERSION:?ZOC_VERSION is required}"
: "${ZOC_UPSTREAM_COMMIT:?ZOC_UPSTREAM_COMMIT is required}"
: "${ZOC_FRONTEND_SHA:?ZOC_FRONTEND_SHA is required}"
: "${ZOC_UPSTREAM_PACKAGE:?ZOC_UPSTREAM_PACKAGE is required}"
: "${ZOC_UPSTREAM_PACKAGE_VERSION:?ZOC_UPSTREAM_PACKAGE_VERSION is required}"
: "${ZOC_UPDATER_PUBLIC_KEY:?ZOC_UPDATER_PUBLIC_KEY is required}"
: "${MYGO_UPDATER_PRIVATE_KEY:?MYGO_UPDATER_PRIVATE_KEY is required}"

if [[ ! "${ZOC_UPSTREAM_COMMIT}" =~ ^[0-9a-f]{40}$ ]]; then
  echo "error: ZOC_UPSTREAM_COMMIT must be a full 40-character SHA" >&2
  exit 1
fi

actual_frontend="$(python3 "${ROOT}/scripts/frontend-hash.py" "${ROOT}/frontend")"
if [[ "${actual_frontend}" != "${ZOC_FRONTEND_SHA}" ]]; then
  echo "error: frontend hash ${actual_frontend} does not match verified ${ZOC_FRONTEND_SHA}" >&2
  exit 1
fi

# The signing workspace carries only this pinned MyGo binary. Refuse to run
# if it is missing or is not the expected release build for the locked MyGo
# version.
# shellcheck disable=SC1091
source "${ROOT}/scripts/mygo-cli.env"
[[ -x "${ROOT}/mygo" ]] || { echo "error: pinned MyGo CLI is missing" >&2; exit 1; }
actual_mygo="$(sha256sum "${ROOT}/mygo" | awk '{print $1}')"
if [[ "${actual_mygo}" != "${MYGO_CLI_LINUX_X64_SHA256}" ]]; then
  echo "error: MyGo CLI ${actual_mygo} does not match pinned ${MYGO_CLI_LINUX_X64_SHA256}" >&2
  exit 1
fi

echo "==> Signed MyGo build (upstream build command skipped)"
"${ROOT}/mygo" build -skip-build-command

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
