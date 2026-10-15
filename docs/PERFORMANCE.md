# Performance investigation · 2026-10-15

Upstream `scripts/perf.sh` budgets **under 1% of one core**, not 1% of the whole machine. Its README reports under 30–52 MB on Apple Silicon depending on layout. These are reference targets, not claimed results for this GPUI implementation or this Ubuntu host.

Measured on the local Ubuntu 26.04 x86_64, 12 logical CPUs, X11/XWayland, a visible native window. Each run warms up for 5 seconds, then measures process CPU time for 15 seconds through `/proc/<pid>/stat`; all threads count. UI data remains live. Raw records stay local in `docs/evidence/`.

| Version / behavior | One-core CPU | Whole-machine CPU | RSS |
| --- | ---: | ---: | ---: |
| Debug, previous continuous animation | 77.665% | 6.472% | 160.73 MiB |
| Release, previous continuous animation | 26.933% | 2.244% | 125.18 MiB |
| Debug, event-driven drawing + restricted sysinfo sampling | 10.533% | 0.878% | 154.70 MiB |
| Release, event-driven drawing + native Linux collector | 2.467% | 0.206% | 116.09 MiB |

These runs are separate snapshots on a live desktop, not simultaneous controlled laboratory tests. The last result is materially better but **still exceeds the upstream CPU and memory targets**. Do not describe it as matching the original benchmark.

Changes: remove the permanent 33 ms UI poll/rebuild loop; deliver samples through an async channel; make motion opt-in and bounded; pause collection using native window visibility notifications; avoid thread enumeration, executable/disk reads and WMI temperature polling; use Linux procfs/sysfs directly; refresh temperature every fifth sample; paint gauge arcs as vector paths rather than generate unbounded unique SVG assets.

The benchmark harness can compare `--motion on` and `--motion off` and reports the busiest application threads. macOS and Windows performance still need their own native runs. CI compilation/tests do not establish performance. Use macOS `top -l 6 -s 3 -pid PID -stats cpu,mem`, and Windows process CPU time divided by wall time (then multiply by 100) for the same single-core convention.
