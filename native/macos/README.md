# macOS hardware bridge

`CFtopSys.c` and `CFtopSys.h` are copied from [Nongfsq/ftop](https://github.com/Nongfsq/ftop), commit `4a03bf66a5ce450179fb51b1e604817678e4c87f`, under its MIT license (preserved in the repository root). They are currently unmodified.

Only macOS builds compile this bridge. The Rust adapter owns each sampler on one background thread and releases it once. It follows upstream’s cluster/channel count matching, unavailable-value behavior, GPU energy interval conversion, SMC `PSTR` machine power, and five-sample temperature cadence. IOReport, IOHID and SMC interfaces are private/undocumented and can change across macOS versions; they are never used by Windows or Ubuntu.

Compressed memory and system pressure use `host_statistics64` and `sysctlbyname`. Shared CPU/process/network counters use sysinfo’s native Mach/libproc/sysctl implementation, with only CPU and resident-memory refresh enabled. No setuid helper is installed and no privileged process access is requested.
