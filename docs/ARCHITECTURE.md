# Platform boundaries

`Monitor` renders the shared `Snapshot`. A background worker owns the compile-time selected `Collector`, sends snapshots through a bounded async channel and sleeps on a condition variable while native visibility reports the window invisible. Update checks have a separate worker and do not trigger metrics polling. Closing the final window quits the process.

| Backend | Data sources and custom behavior |
| --- | --- |
| Ubuntu/Linux | `/proc/stat` tick deltas; `/proc/meminfo` available memory; bounded `/proc/<pid>/stat` records with PID start-time checks; physical-interface counters from `/proc/net/dev`; sysfs CPU capacity/frequency/max-frequency and hwmon sensor files. Root process directories only, never `/proc/<pid>/task`. Temperatures update every fifth sample. |
| macOS | Native Mach/libproc process/CPU counters through a restricted sysinfo adapter; original C bridge for IOReport CPU frequencies and cluster identities, GPU statistics/energy, SMC system watts and CPU/PMU temperature. Mach VM statistics compute app + wired + compressed used memory as upstream does; sysctl supplies pressure and swap. Bridge ownership is confined to the sampling thread. |
| Windows | Restricted native sysinfo CPU/process/network counters; `GlobalMemoryStatusEx` physical memory and `GetPerformanceInfo` system Commit. No WMI sensor polling, no fake per-core frequencies. Unsupported GPU/power/temperature remain unavailable. |

Windows Commit uses the system-wide API and is deliberately labeled `Commit`: [Microsoft PERFORMANCE_INFORMATION](https://learn.microsoft.com/en-us/windows/win32/api/psapi/ns-psapi-performance_information). It is not page-file residency. See also [MEMORYSTATUSEX](https://learn.microsoft.com/en-us/windows/win32/api/sysinfoapi/ns-sysinfoapi-memorystatusex).

`chrome.rs` draws the translucent rounded boundary and shadow directly using Kit Base, supplies application-owned button visuals, and forwards edge gestures to native resize APIs. The styled Kit window border is intentionally rectangular and is not used for this floating panel. Core bars and gauge arcs use native canvas/vector primitives; dynamic readings do not create new SVG assets.

Optional hardware support is intentionally asymmetric. Private Apple APIs are never compiled on other targets. Additional Linux/Windows GPU vendors need their own adapters and corresponding native validation; unsupported readings do not masquerade as zero.
