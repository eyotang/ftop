# Platform metric coverage

Sources are sampled off the UI thread. A successful build does not prove availability on every GPU model. Values are optional only when an API, driver, hardware sensor or permission is unavailable; tooltips expose the source and collection status explains limitations. No external sampling command, WMI polling, privileged helper or demo data is used.

| Metric | macOS | Windows | Ubuntu |
| --- | --- | --- | --- |
| CPU/process usage | Native sysinfo counters | Native sysinfo counters | procfs tick deltas, root processes only |
| CPU frequency | Upstream IOReport bridge, supported hardware | PDH Processor Frequency with ProcessorPowerInformation fallback/max; native CPU-set performance classes; not a guaranteed instantaneous effective clock | cpufreq scaling_cur_freq, driver-reported |
| Physical memory | Mach original used-memory formula | GlobalMemoryStatusEx | MemTotal minus MemAvailable |
| Swap / commit | vm.swapusage | GetPerformanceInfo; explicitly labeled Commit | procfs SwapTotal minus SwapFree |
| Compression | Mach compressor physical pages | No supported public compression-store reader implemented; explicitly unavailable | Zswap compressed pool plus Zram compressed data, if exposed |
| Memory pressure | Native macOS pressure level | Physical memory load (not macOS pressure) | PSI memory waiting, with native units |
| GPU utilization | Upstream IOKit/IOReport | WDDM PDH: sum process shares per physical engine, select busiest engine; NVML fallback | AMD gpu_busy_percent; Intel PMU busiest engine, fallback accessible-client DRM fdinfo; NVIDIA NVML |
| GPU memory | Upstream IOKit | PDH dedicated + shared adapter usage; NVML fallback | AMD VRAM used; NVIDIA NVML; Intel accessible-client resident memory estimate |
| GPU frequency | Upstream IOReport where supported | NVIDIA NVML where available | Intel sysfs / AMD active DPM state / NVIDIA NVML |
| GPU temperature/power | Upstream energy/SMC APIs where supported | NVIDIA NVML where available | AMD/Intel hwmon / NVIDIA NVML where available |
| Temperature badge | Original CPU/PMU sensors | ACPI thermal zone, GPU fallback if exposed; never labeled CPU core temperature | hwmon CPU sensor preferred, GPU fallback |
| Power badge | SMC system PSTR | Battery discharge only when supported and discharging; no universal wall-power API | Battery discharge preferred; otherwise RAPL CPU packages, explicitly not whole-machine power |
| Network throughput | Native sysinfo interfaces | Native sysinfo interfaces | Physical sysfs interfaces + procfs byte deltas |

## Important boundaries

- Windows requires a WDDM driver exposing GPU performance counters. The first sample primes rate counters. English PDH API paths avoid assumptions about the system language. All counters share one persistent query.
- Optional NVML is dynamically loaded from the vendor runtime. It adds no SDK requirement and no `nvidia-smi` subprocess. NVIDIA-specific sensor readings refer to its busiest visible NVIDIA device; WDDM aggregate usage may refer to a different adapter, which the tooltip states.
- Linux GPU discovery is per DRM card. Multiple cards select the busiest available sample rather than adding unrelated engine percentages. Intel fdinfo fallback only covers accessible processes: it is explicitly an estimate, not a claim of full system coverage. Client IDs deduplicate duplicated descriptors. Client resident memory may double-count shared buffers, so the tooltip labels this estimate. Descriptor discovery is cached for ten seconds; counters use actual elapsed time.
- Intel PMU and RAPL are subject to kernel security settings. ftop does not change kernel permissions or request administrator access. Unsupported devices, restricted procfs, driver errors and missing sensors are shown explicitly, never fabricated as zero.
- Generic Windows APIs do not expose arbitrary vendor CPU core temperatures, signed low-level sensor drivers or compression-store internals. These remain explicitly unsupported rather than reading undocumented kernel layouts or installing a privileged driver.
- Local Intel DRM utilization and clocks, coretemp, compression and PSI were read on Ubuntu. Windows/macOS driver readings still require native hardware validation. CI checks ABI layouts, parsing and aggregation, not vendor sensor availability.

References: [Microsoft PDH array API](https://learn.microsoft.com/en-us/windows/win32/api/pdh/nf-pdh-pdhgetformattedcounterarrayw), [Windows battery state](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-system_battery_state), [DRM client stats](https://kernel.org/doc/html/latest/gpu/drm-usage-stats.html), [AMD hwmon/sysfs](https://kernel.org/doc/html/latest/gpu/amdgpu/thermal.html), [NVIDIA NVML](https://docs.nvidia.com/deploy/nvml-api/).
