# Performance investigation · 2026-10-15

Upstream `scripts/perf.sh` budgets **under 1% of one core**, not 1% of the whole machine. Its README reports under 30–52 MB on Apple Silicon depending on layout. These are reference targets, not claimed results for this GPUI implementation or this Ubuntu host.

Measured on the local Ubuntu 26.04 x86_64, 12 logical CPUs, X11/XWayland, a visible native window. Each run warms up for 5 seconds, then measures process CPU time for 15 seconds through `/proc/<pid>/stat`; all threads count. UI data remains live. Raw records stay local in `docs/evidence/`.

| Version / behavior | One-core CPU | Whole-machine CPU | RSS |
| --- | ---: | ---: | ---: |
| Debug, previous continuous animation | 77.665% | 6.472% | 160.73 MiB |
| Release, previous continuous animation | 26.933% | 2.244% | 125.18 MiB |
| Debug, event-driven drawing + restricted sysinfo sampling | 10.533% | 0.878% | 154.70 MiB |
| Release, event-driven drawing + native Linux collector | 2.467% | 0.206% | 116.09 MiB |
| Release, latest visible canvas/procfs revision | 2.133% | 0.178% | 115.76 MiB |
| Release, visible repeat (3572e31) | 3.667% | 0.306% | 116.05 MiB |
| Release, GPU-enabled Intel DRM collection (20 s, 298d175) | 2.900% | 0.242% | 120.89 MiB |
| Release, latest X11 minimized window (3572e31) | 0.467% | 0.039% | 115.62 MiB |

These runs are separate snapshots on a live desktop, not simultaneous controlled laboratory tests. Visible-window performance is materially better but **still exceeds the upstream CPU and memory targets**. Minimized-window measurements do not establish visible-window acceptance. Do not describe it as matching the original benchmark.

Changes: remove the permanent 33 ms UI poll/rebuild loop; deliver samples through an async channel; make motion opt-in and bounded; pause collection using native window visibility notifications; avoid thread enumeration, executable/disk reads and WMI temperature polling; use Linux procfs/sysfs directly; refresh temperature every fifth sample; paint gauge arcs as vector paths rather than generate unbounded unique SVG assets.

The benchmark harness can compare `--motion on` and `--motion off` and reports the busiest application threads. macOS and Windows performance still need their own native runs. CI compilation/tests do not establish performance. Use macOS `top -l 6 -s 3 -pid PID -stats cpu,mem`, and Windows process CPU time divided by wall time (then multiply by 100) for the same single-core convention.

## Controlled GPU comparison after cache revision

Local Ubuntu 26.04 / Intel DRM fallback / 12 logical CPUs, release build, motion off, 5-second warmup then 20-second measurement. Runs were serial, with reverse-order repetition using the same executable. Raw records remain ignored. `scripts/benchmark.py --gpu off` passes the Linux diagnostic-only `--no-gpu` flag; production collection remains enabled by default.

| Run | One-core CPU | Collector thread CPU | RSS |
| --- | ---: | ---: | ---: |
| Previous full collector | 2.95% | 1.70% | 119.90 MiB |
| Revised, GPU off | 2.35% | 1.05% | 120.12 MiB |
| Revised, GPU on | 2.85% | 1.45% | 120.65 MiB |
| Revised, GPU on repeat | 2.90% | 1.50% | 120.45 MiB |
| Revised, GPU off repeat | 2.50% | 1.10% | 120.36 MiB |
| Revised, minimized | 0.60% | No sampling work observed | 120.31 MiB |

Measured incremental GPU cost is approximately 0.4–0.5 percentage points of one core, or 0.033–0.042 points of this 12-thread machine, with about 0.1–0.5 MiB RSS difference. This is a small live-desktop comparison, not a guarantee for other drivers. The previous/revised total-process difference is small; do not claim a dramatic total CPU improvement. Visible use still exceeds the original under-1%-of-one-core target.

Linux discovery groups aliased DRM descriptors by client identity, reads one live descriptor per client, retains aliases for process exit/fd reuse, and parses only necessary fields. NVML caches device/max-clock discovery for 30 seconds, temperature for five seconds and detailed reads for the selected GPU. Windows uses one persistent PDH query and reuses a scratch buffer capped at one MiB retained. These Windows/NVIDIA changes are source-level cost controls, not native performance measurements. Sampling pauses on native hidden/minimized visibility notifications; no GPU polling timer runs independently of the sampler.
