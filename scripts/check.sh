#!/usr/bin/env bash
# In-container verification entry point. CI and developers run this through
# scripts/container.sh; it never touches the host toolchain.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}"

echo "==> formatting"
unformatted="$(gofmt -l $(find . \( -path ./.cache -o -path ./.upstream \) -prune -o -name '*.go' -print) | grep -v '^\.upstream/' || true)"
if [[ -n "${unformatted}" ]]; then
  echo "error: gofmt needed for:" >&2
  echo "${unformatted}" >&2
  exit 1
fi

echo "==> go vet"
go vet ./...

echo "==> go test"
go test ./...

echo "==> frontend dependencies"
bun install --frozen-lockfile

echo "==> upstream checkout and adapter"
"${ROOT}/scripts/prepare-upstream.sh"
(cd "${ROOT}/.upstream/opencode" && bun install --frozen-lockfile)

echo "==> generated MyGo client"
bun run generate

echo "==> adapter typecheck"
bun x tsc --noEmit -p adapter/tsconfig.json

echo "==> official Web UI build"
"${ROOT}/scripts/build-frontend.sh"

echo "==> Web UI asset gates"
python3 "${ROOT}/scripts/verify-frontend.py" "${ROOT}/frontend"

echo "all checks passed"
