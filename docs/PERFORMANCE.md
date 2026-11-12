# Performance baseline

Status: **No measured benchmark values yet.** No figures in the task description should be reported as achieved.

## Targets

| Measurement | Initial target |
|---|---:|
| Release executable | Prefer < 50 MiB |
| Idle RSS | Prefer < 120 MiB |
| Typical session RSS | Prefer < 250 MiB |
| Idle CPU | Effectively zero |
| Transcript streaming | No visible full-list refresh |
| Scrolling | Smooth through 1,000+ messages |

## Implementation notes

- GPUI Kit `MessageScroller` owns virtualized message row layout, tail-following and scroll anchor semantics.
- Streaming text updates only the affected cached GPUI `TextViewState` and the affected row measurement.
- The background API worker handles HTTP operations; SSE uses a separate blocking reader thread.
- Local read/unread and draft state is stored in XDG configuration, with no OpenCode DB writes.
- No token polling loop should be used while SSE is healthy.

## Reproducible measurement plan

Collect executable bytes via `stat -c %s target/release/zoc` and dependencies via `ldd`. Capture RSS and CPU with `ps -o rss,pcpu` and wall-time sampling on an idle native Wayland session with a working Vulkan driver. Exercise a 1,000-message history, multi-thousand-line Markdown, 50+ tools, rapid streamed deltas, 20 session switches and 10-minute idle periods.

When available, record host CPU, RAM, GPU, display resolution/scaling, Fedora build, session size and exact OpenCode version alongside each measurement. CI binary size is not a substitute for native runtime measurements.
