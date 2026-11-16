#!/usr/bin/env bash
# Headless WebKitGTK smoke test: launch the packaged ZOC binary under Xvfb,
# connect it to a deterministic OpenCode Server fixture, and require the
# official Web UI to mount and open its SSE stream.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${ZOC_BIN:-${ROOT}/build/linux-amd64/zoc}"
PORT="${ZOC_SMOKE_PORT:-4199}"
OUT_DIR="${ZOC_SMOKE_DIR:-${ROOT}/.cache/smoke}"
rm -rf "${OUT_DIR}"
mkdir -p "${OUT_DIR}"
LOG="${OUT_DIR}/fixture.log"
SMOKE="${OUT_DIR}/smoke.json"
USER_DATA="${OUT_DIR}/user-data"
mkdir -p "${USER_DATA}"

if [[ ! -x "${BIN}" ]]; then
  echo "error: packaged binary not found at ${BIN}" >&2
  exit 1
fi

go build -o "${OUT_DIR}/zoc-fixture" ./cmd/zoc-fixture

"${OUT_DIR}/zoc-fixture" -addr "127.0.0.1:${PORT}" -log "${LOG}" &
FIXTURE_PID=$!
trap 'kill "${FIXTURE_PID}" 2>/dev/null || true' EXIT
for _ in $(seq 1 50); do
  if curl -fsS "http://127.0.0.1:${PORT}/api/info" >/dev/null 2>&1; then
    break
  fi
  sleep 0.1
done

set +e
timeout 45s xvfb-run -a -s "-screen 0 1280x900x24" dbus-run-session -- env \
  GDK_BACKEND=x11 \
  LIBGL_ALWAYS_SOFTWARE=1 \
  WEBKIT_DISABLE_DMABUF_RENDERER=1 \
  WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1 \
  NO_AT_BRIDGE=1 \
  ZOC_SERVER_URL="http://127.0.0.1:${PORT}" \
  ZOC_USER_DATA="${USER_DATA}" \
  ZOC_SMOKE_OUT="${SMOKE}" \
  "${BIN}" >"${OUT_DIR}/app.log" 2>&1
APP_STATUS=$?
set -e

if [[ ! -s "${SMOKE}" ]]; then
  echo "error: WebKit smoke result was not written" >&2
  echo "--- app log ---" >&2
  tail -100 "${OUT_DIR}/app.log" >&2
  exit 1
fi
if ! grep -q '"ok": true' "${SMOKE}"; then
  echo "error: official Web UI did not mount in WebKitGTK" >&2
  cat "${SMOKE}" >&2
  echo "--- app log ---" >&2
  tail -100 "${OUT_DIR}/app.log" >&2
  exit 1
fi
if [[ ${APP_STATUS} -ne 0 ]]; then
  echo "error: app exited with status ${APP_STATUS}" >&2
  tail -100 "${OUT_DIR}/app.log" >&2
  exit 1
fi
if ! grep -q "GET /api/info" "${LOG}"; then
  echo "error: the Web UI never probed the OpenCode Server" >&2
  cat "${LOG}" >&2
  exit 1
fi
if ! grep -q "GET /api/event" "${LOG}"; then
  echo "error: the Web UI never opened its SSE event stream" >&2
  cat "${LOG}" >&2
  exit 1
fi

echo "WebKitGTK smoke passed"
echo "  smoke:   ${SMOKE}"
echo "  fixture: ${LOG}"
echo "  app log: ${OUT_DIR}/app.log"
