#!/usr/bin/env bash
# In-container production build: official Web UI + MyGo packaging + release
# metadata and checksums.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}"

bun install --frozen-lockfile
"${ROOT}/scripts/build-frontend.sh"
python3 "${ROOT}/scripts/verify-frontend.py" "${ROOT}/frontend"

echo "==> MyGo build"
bun run build

"${ROOT}/scripts/write-release-metadata.sh"
echo "artifacts:"
find build -maxdepth 2 -type f -printf '  %p (%s bytes)\n' | sort
