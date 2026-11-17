#!/usr/bin/env bash
# Measure the packaged app against the deterministic fixture in the pinned
# image. This is container/Xvfb evidence, not a Fedora GNOME/Wayland test.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}"
BIN="${ZOC_BIN:-${ROOT}/build/linux-amd64/zoc}"
SECONDS_TO_RUN="${ZOC_PERF_SECONDS:-120}"
PORT="${ZOC_PERF_PORT:-4299}"
OUT="${ZOC_PERF_DIR:-${ROOT}/.cache/perf}"
rm -rf "${OUT}"
mkdir -p "${OUT}/user-data"
: > "${OUT}/samples.tsv"
chmod 644 "${OUT}/samples.tsv"

if [[ ! -x "${BIN}" ]]; then
  echo "error: packaged binary not found at ${BIN}" >&2
  exit 1
fi

go build -o "${OUT}/zoc-fixture" ./cmd/zoc-fixture
"${OUT}/zoc-fixture" -addr "127.0.0.1:${PORT}" -log "${OUT}/fixture.log" &
FIXTURE_PID=$!
trap 'kill "${FIXTURE_PID}" 2>/dev/null || true' EXIT
for _ in $(seq 1 50); do
  curl -fsS "http://127.0.0.1:${PORT}/api/info" >/dev/null 2>&1 && break
  sleep 0.1
done

HOLD_MS=$(( (SECONDS_TO_RUN + 10) * 1000 ))
START_MS="$(date +%s%3N)"
xvfb-run -a -s "-screen 0 1280x900x24" dbus-run-session -- env \
  GDK_BACKEND=x11 \
  LIBGL_ALWAYS_SOFTWARE=1 \
  WEBKIT_DISABLE_DMABUF_RENDERER=1 \
  WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1 \
  NO_AT_BRIDGE=1 \
  ZOC_SERVER_URL="http://127.0.0.1:${PORT}" \
  ZOC_USER_DATA="${OUT}/user-data" \
  ZOC_SMOKE_OUT="${OUT}/smoke.json" \
  ZOC_SMOKE_HOLD_MS="${HOLD_MS}" \
  "${BIN}" >"${OUT}/app.log" 2>&1 &
WRAPPER_PID=$!

APP_PID=""
for _ in $(seq 1 100); do
  for candidate in $(pgrep -x zoc || true); do
    state="$(awk '/^State:/{print $2}' "/proc/${candidate}/status" 2>/dev/null || true)"
    cmdline="$(tr '\0' ' ' < "/proc/${candidate}/cmdline" 2>/dev/null || true)"
    if [[ "${state}" != "Z" && "${cmdline}" == *"${BIN}"* ]]; then
      APP_PID="${candidate}"
      break
    fi
  done
  [[ -n "${APP_PID}" && -s "${OUT}/smoke.json" ]] && break
  sleep 0.2
done
if [[ -z "${APP_PID}" ]]; then
  echo "error: live app process not found" >&2
  tail -50 "${OUT}/app.log" >&2
  exit 1
fi

clk_tck="$(getconf CLK_TCK)"
samples=0
cpu_ticks_total=0
cpu_seconds_total=0
max_rss=0
first_rss=0
last_rss=0
while kill -0 "${APP_PID}" 2>/dev/null; do
  read -r utime stime < <(awk '{print $14, $15}' "/proc/${APP_PID}/stat" 2>/dev/null || echo "0 0")
  rss="$(awk '/VmRSS/{print $2}' "/proc/${APP_PID}/status" 2>/dev/null || echo 0)"
  hwm="$(awk '/VmHWM/{print $2}' "/proc/${APP_PID}/status" 2>/dev/null || echo 0)"
  for child in $(pgrep -P "${APP_PID}" 2>/dev/null || true); do
    child_rss="$(awk '/VmRSS/{print $2}' "/proc/${child}/status" 2>/dev/null || echo 0)"
    rss=$((rss + child_rss))
  done
  [[ -n "${rss}" ]] || rss=0
  [[ -n "${hwm}" ]] || hwm=0
  if [[ ${rss} -gt ${max_rss} ]]; then max_rss=${rss}; fi
  if [[ ${first_rss} -eq 0 ]]; then first_rss=${rss}; fi
  last_rss=${rss}
  prev_ticks=${cpu_ticks_total}
  cpu_ticks_total=$((utime + stime))
  if [[ ${samples} -gt 0 ]]; then
    cpu_seconds_total=$((cpu_seconds_total + (cpu_ticks_total - prev_ticks)))
  fi
  printf '%s\t%s\t%s\t%s\n' "$(date +%s)" "${rss}" "${hwm}" "$((cpu_ticks_total))" >> "${OUT}/samples.tsv"
  samples=$((samples + 1))
  sleep 2
done
wait "${WRAPPER_PID}" 2>/dev/null || true
END_MS="$(date +%s%3N)"

if [[ ${samples} -gt 1 ]]; then
  cpu_seconds="$(python3 -c "print((${cpu_seconds_total})/${clk_tck})")"
else
  cpu_seconds=0
fi

python3 - "${OUT}" "${BIN}" "${START_MS}" "${END_MS}" "${samples}" "${first_rss}" "${last_rss}" "${max_rss}" "${cpu_seconds}" <<'PY'
import json, os, platform, subprocess, sys
out, binary, start_ms, end_ms, samples, first_rss, last_rss, max_rss, cpu_seconds = sys.argv[1:]
out, start_ms, end_ms, samples = os.path.abspath(out), int(start_ms), int(end_ms), int(samples)
smoke = json.load(open(os.path.join(out, "smoke.json")))
def cmd(*args):
    try:
        return subprocess.check_output(args, text=True, stderr=subprocess.DEVNULL).strip()
    except Exception:
        return ""
data = {
  "schemaVersion": 1,
  "scenario": "official Web UI home with fixture server, Xvfb 1280x900, scale 1, software GL",
  "environment": "Podman container, not Fedora GNOME/Wayland",
  "binaryBytes": os.path.getsize(binary),
  "rpmBytes": next((os.path.getsize(os.path.join(os.path.dirname(binary), f)) for f in os.listdir(os.path.dirname(binary)) if f.endswith(".rpm")), 0),
  "archiveBytes": next((os.path.getsize(os.path.join(os.path.dirname(binary), f)) for f in os.listdir(os.path.dirname(binary)) if f.endswith(".tar.gz")), 0),
  "frontendBytes": int(cmd("du", "-sb", "frontend").split("\t")[0] or 0),
  "startupMs": smoke.get("startupMs"),
  "probeMs": smoke.get("probeMs"),
  "rssStartKb": int(first_rss),
  "rssEndKb": int(last_rss),
  "rssMaxKb": int(max_rss),
  "cpuSecondsTotal": float(cpu_seconds),
  "durationSeconds": round((end_ms - start_ms) / 1000, 1),
  "samples": samples,
  "cpuModel": cmd("sh", "-c", "lscpu | sed -n 's/^Model name:[[:space:]]*//p' | head -1"),
  "cores": os.cpu_count(),
  "os": cmd("sh", "-c", ". /etc/os-release && echo $PRETTY_NAME"),
  "kernel": platform.release(),
  "webkitgtk": cmd("rpm", "-q", "webkit2gtk4.1"),
  "gtk3": cmd("rpm", "-q", "gtk3"),
  "mygoVersion": "0.9.2",
  "upstreamCommit": json.load(open(os.path.join(os.path.dirname(binary), "upstream.json")))["upstreamCommit"],
  "notes": "RSS sums the app and its direct child processes every 2s; WebKit may have deeper descendants. CPU is app-process CPU. WebKitGTK renders in separate processes.",
}
json.dump(data, open(os.path.join(out, "summary.json"), "w"), indent=2)
open(os.path.join(out, "summary.json"), "a").write("\n")
print(json.dumps(data, indent=2))
PY
