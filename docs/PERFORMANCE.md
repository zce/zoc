# Performance baseline

Status: **Measured in a disposable Fedora 45 container with Xvfb and the
llvmpipe Vulkan driver.** The native Fedora GNOME/Wayland numbers on the target
machine are still unverified, and llvmpipe's own mappings dominate part of the
RSS below.

## Targets

| Measurement | Initial target | User aspiration |
|---|---:|---:|
| Release executable | Prefer < 50 MiB | As small as practical |
| Idle RSS | Prefer < 120 MiB | ~50 MiB |
| Typical session RSS | Prefer < 250 MiB | ~100–150 MiB |
| Idle CPU | Effectively zero | Effectively zero |
| Transcript streaming | No visible full-list refresh | No visible full-list refresh |
| Scrolling | Smooth through 1,000+ messages | Smooth through 1,000+ messages |

The 50 MiB idle figure is treated as an aspiration, not an achieved value. A
native GPUI application still maps its executable, the GPU driver, font data,
and the window-system stack; those costs are below the control of the
application code. The measured container run also includes roughly 48 MiB of
llvmpipe/LLVM resident memory that a hardware Vulkan driver would not load.

## Current measurements

Environment:

- Fedora 45 x86_64 container, rootless Podman
- Xvfb 21.1.26, Openbox 3.6.1, 1600×1000
- Mesa llvmpipe (`LIBGL_ALWAYS_SOFTWARE=1`), Vulkan software rasterizer
- Rust 1.99.1, `x86_64-unknown-linux-gnu`
- 60-session mock service; one transcript with 1,000 messages

| State | RSS | PSS | Threads | Notes |
|---|---:|---:|---:|---|
| No service, connection screen | 121.8 MiB | 118.8 MiB | 37 | idle, no polling |
| 60 Inbox sessions | 138.3 MiB | — | 38 | Home / Inbox |
| 1,000-message transcript | 186.5 MiB | — | 38 | first open |
| 5 sessions opened | 193.4 MiB | — | 38 | bounded after eviction |
| 15+ sessions opened rapidly | ≈195.8 MiB | — | 38 | no further growth observed |
| Idle CPU | 0 ticks / 5 s | — | — | no repaint loop while idle |

For comparison, a default-size blank GPUI Kit window built with the same
`opt-level = "z"` and fat-LTO profile measured **111.2 MiB RSS / 82.0 MiB PSS**
under the same Xvfb + llvmpipe setup, with a 13.6 MiB executable. ZOC's connection screen is
only about 11 MiB above that framework floor, so further application tuning
cannot bring idle RSS near 50 MiB while keeping GPUI and wgpu.

Release executable: **27,354,208 bytes (26.1 MiB)**, 37% smaller than the
previous 43,413,280-byte build. A deflate-9 zip of the executable is about
11.3 MiB. CI now compresses the artifact instead of uploading it uncompressed.

## What changed

- Release profile uses `opt-level = "z"`, fat LTO, one codegen unit,
  symbol stripping and `panic = "abort"`.
- Switching sessions evicts the previous transcript through the official
  `opencode-store` `Data::evict` API. The client also drops its JSON snapshots,
  permission/form caches and rendered `TextViewState` cache for the previous
  session instead of retaining every visited session.
- Project switches, archived-view switches, deletes and opening Home clear the
  same caches. The current row stays pinned on Home and reloads when opened.
- A background run that rebuilds its transcript from SSE events is evicted when
  its status leaves `Running`; the open session is never evicted this way.
- Manual reconnects abort the previous store watcher and event connection before
  opening the new endpoint, so a silent old stream cannot retain its store.
- Pending inbox items are re-read with the transcript, because store eviction
  drops admitted items that were not in the local outbox.
- The CI artifact is compressed at level 9.

The old client kept every visited transcript and every rendered text state.
Opening five 1,000-message sessions reached roughly 412 MiB RSS. With the
eviction path, the same sequence stays near 193 MiB and subsequent switches
stay flat.

## Remaining floor and gaps

- Real Fedora GNOME/Wayland with the NVIDIA GTX 1660 Ti has not been measured.
  The hardware driver may use less resident memory than llvmpipe, but that is
  an expectation, not a verified result.
- The 50 MiB idle aspiration is unlikely to be reached with GPUI + wgpu +
  font shaping while the executable alone maps roughly 18 MiB of resident code
  and data. Reaching it would require a different rendering stack or a much
  smaller framework, not just application tuning.
- Large conversations with many tool cards and diffs still need a real-service
  endurance run. The 1,000-message fixture contains text parts only.
- The 1,000-message fixture was read from a local mock over loopback; HTTP
  serialization and network-backed store updates were not stressed.
- After a large transcript, RSS can remain at the allocator high-water mark even
  after Home releases the data. Reopening another session reuses that memory
  instead of growing further; a fresh launch on Home is 138.3 MiB in the same
  setup.

## Reproducible measurement plan

Collect executable bytes via `stat -c %s target/release/zoc` and dependencies
via `ldd`. Capture RSS and CPU with `/proc/<pid>/status`, `smaps_rollup` and
`/proc/<pid>/stat` on an idle native Wayland session with a working Vulkan
driver. Exercise a 1,000-message history, multi-thousand-line Markdown, 50+
tools, rapid streamed deltas, 20 session switches and 10-minute idle periods.

When available, record host CPU, RAM, GPU, display resolution/scaling, Fedora
build, session size and exact OpenCode version alongside each measurement. CI
binary size is not a substitute for native runtime measurements.
