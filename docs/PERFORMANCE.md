# Performance and footprint

These are measured baselines, not targets. The first baseline is the official Web UI hosted by MyGo in the pinned container under Xvfb. A real Fedora/GNOME/Wayland measurement is still required user-side.

## Environment

| Field | Value |
|---|---|
| Host CPU | Intel Core i7-9700K @ 3.60 GHz, 8 cores |
| Container OS | Fedora Linux 45 (Container Image) |
| Kernel | 7.3.3-300.fc45.x86_64 |
| Display | Xvfb 1280×900, scale 1, software GL (`LIBGL_ALWAYS_SOFTWARE=1`) |
| WebKitGTK | 2.54.2 (`webkit2gtk4.1-2.54.2-1.fc45`) |
| GTK | 3.24.52 (`gtk3-3.24.52-6.fc45`) |
| MyGo | 0.9.2 |
| OpenCode | `e4606463e7d18fd126ecfac3ec04602f1e44814b` (`dev`) |
| Scenario | Official Web UI home screen, one fixture server, SSE open, no real session |
| Method | `scripts/container.sh ./scripts/perf.sh`; process tree sampled every 2 s |
| Raw 10-minute summary | `docs/performance-10min.json` |

## Package and installed footprint

| Metric | Value |
|---|---|
| `zoc` executable | 110,461,191 bytes (~105.3 MiB) |
| Embedded Web UI (`frontend/`) | 94,126,200 bytes (~89.8 MiB, including the gate manifest) |
| RPM | 40,335,501 bytes (~38.5 MiB) |
| Archive (`tar.gz`) | 40,329,515 bytes (~38.5 MiB) |
| Installed payload | executable + icon + desktop entry; no Node/Chromium/Electron |

The binary is large because the official Web UI, its lazy Office/WASM assets, fonts and terminal emulator are embedded. Compression reduces the distributed artifact to about 38.5 MiB.

## Cold start

| Metric | Value |
|---|---|
| Process start → `OnDidFinishLoad` | **237–340 ms** across runs (340 ms in the 619 s run) |
| Process start → bridge/storage/clipboard probe complete | 4.26–4.36 s (includes a deliberate 4 s settle) |
| First usable UI | Web UI mounted at `OnDidFinishLoad`; fixture health/SSE then connect asynchronously |

These figures are container/Xvfb with a local fixture and a warm page cache. They measure WebKit page load, not human-perceived readiness after a real server probe.

## Idle memory and CPU

| Metric | Value |
|---|---|
| Run duration | **619.3 s** (10-minute stability run) |
| Samples | 304 (every 2 s) |
| App + WebKit RSS start | 474,272 KiB (~463 MiB) |
| App + WebKit RSS end | 462,308 KiB (~451 MiB) |
| App + WebKit peak RSS | 474,272 KiB (~463 MiB) |
| RSS trend | decreased ~12 MiB over 10 minutes; no upward growth |
| App process CPU over 619 s | 0.13 s (~0.02% of one core) |
| App process RSS at probe (smoke, no native inbox) | 168,228–169,852 KiB |

WebKitGTK renders and networks in separate processes; the app process alone understates the client footprint. The run was the official Web UI home with one fixture server and its SSE stream open, not an active agent session. DMA-BUF rendering was left at MyGo's default (disabled in this environment); it was not force-enabled.

## What is not measured yet

- A 10–30 minute **real session** with streaming, scrolling and tool output. The recorded run is a 10-minute stable idle home screen; `scripts/perf.sh` supports `ZOC_PERF_SECONDS=1800` for a longer run.
- A real OpenCode Server and real terminal latency.
- Long transcript scroll smoothness.
- Terminal startup/input/output latency.
- Fedora GNOME/Wayland/NVIDIA rendering and memory.

Do not treat the container figures as a Fedora desktop claim. Use the manual checklist in [ACCEPTANCE.md](ACCEPTANCE.md) and record the environment.

## Reproduce

```sh
./scripts/container.sh ./scripts/perf.sh                     # 120 s default
ZOC_PERF_SECONDS=1800 ./scripts/container.sh ./scripts/perf.sh  # 30 min stability
```

Artifacts are written to `.cache/perf/`: `summary.json`, `samples.tsv`, `smoke.json`, `fixture.log`, `app.log`.
