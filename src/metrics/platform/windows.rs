//! Windows: native per-CPU/PSAPI process counters via sysinfo; Win32 memory/commit accounting.
//! Persistent PDH and optional NVML sensors; no WMI polling or guessed readings.
use super::common::Portable;
use crate::metrics::Snapshot;
#[repr(C)]
#[derive(Default)]
struct MemoryStatus {
    length: u32,
    load: u32,
    total_phys: u64,
    avail_phys: u64,
    total_page_file: u64,
    avail_page_file: u64,
    total_virtual: u64,
    avail_virtual: u64,
    avail_extended_virtual: u64,
}
#[repr(C)]
#[derive(Default)]
struct PerformanceInfo {
    size: u32,
    commit_total: usize,
    commit_limit: usize,
    commit_peak: usize,
    physical_total: usize,
    physical_available: usize,
    system_cache: usize,
    kernel_total: usize,
    kernel_paged: usize,
    kernel_nonpaged: usize,
    page_size: usize,
    handle_count: u32,
    process_count: u32,
    thread_count: u32,
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GlobalMemoryStatusEx(status: *mut MemoryStatus) -> i32;
    fn GetSystemCpuSetInformation(
        buffer: *mut std::ffi::c_void,
        size: u32,
        required: *mut u32,
        process: isize,
        flags: u32,
    ) -> i32;
}
#[link(name = "psapi")]
unsafe extern "system" {
    fn GetPerformanceInfo(info: *mut PerformanceInfo, size: u32) -> i32;
}
#[repr(C)]
#[derive(Default)]
struct BatteryState {
    ac: u8,
    present: u8,
    charging: u8,
    discharging: u8,
    reserved: [u8; 4],
    max_capacity: u32,
    remaining: u32,
    rate: u32,
    estimated: u32,
    alert1: u32,
    alert2: u32,
}
#[link(name = "powrprof")]
unsafe extern "system" {
    fn CallNtPowerInformation(
        level: i32,
        input: *const std::ffi::c_void,
        input_size: u32,
        output: *mut std::ffi::c_void,
        output_size: u32,
    ) -> i32;
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ProcessorPower {
    number: u32,
    max_mhz: u32,
    current_mhz: u32,
    limit_mhz: u32,
    max_idle: u32,
    current_idle: u32,
}
fn cpu_classes() -> std::collections::HashMap<u32, u8> {
    let mut result = std::collections::HashMap::new();
    // SAFETY: first request obtains required byte count; allocation is aligned,
    // variable-size records are parsed only after bounds/type checks.
    unsafe {
        let mut needed = 0;
        let _ = GetSystemCpuSetInformation(std::ptr::null_mut(), 0, &mut needed, 0, 0);
        if needed == 0 || needed > 1024 * 1024 {
            return result;
        }
        let mut buffer = vec![0u64; (needed as usize).div_ceil(8)];
        if GetSystemCpuSetInformation(buffer.as_mut_ptr().cast(), needed, &mut needed, 0, 0) == 0 {
            return result;
        }
        let bytes = std::slice::from_raw_parts(buffer.as_ptr().cast::<u8>(), needed as usize);
        let mut offset = 0;
        while offset + 8 <= bytes.len() {
            let record = &bytes[offset..];
            let size = u32::from_le_bytes(record[..4].try_into().unwrap()) as usize;
            let kind = u32::from_le_bytes(record[4..8].try_into().unwrap());
            if size < 8 || offset + size > bytes.len() {
                break;
            }
            if kind == 0 && size >= 32 {
                let group = u16::from_le_bytes(record[12..14].try_into().unwrap()) as u32;
                result.insert(group * 64 + u32::from(record[14]), record[18]);
            }
            offset += size;
        }
    }
    result
}
pub struct Backend {
    portable: Portable,
    classes: std::collections::HashMap<u32, u8>,
    gpu: super::windows_gpu::Counters,
    nvidia: Option<super::nvidia::Nvidia>,
}
impl Backend {
    pub fn new() -> Self {
        Self {
            portable: Portable::new(),
            classes: cpu_classes(),
            gpu: super::windows_gpu::Counters::new(),
            nvidia: super::nvidia::Nvidia::new(),
        }
    }
    pub fn sample(&mut self) -> Snapshot {
        let mut snapshot = self.portable.sample(false);
        let mut memory = MemoryStatus {
            length: std::mem::size_of::<MemoryStatus>() as u32,
            ..Default::default()
        };
        let mut performance = PerformanceInfo {
            size: std::mem::size_of::<PerformanceInfo>() as u32,
            ..Default::default()
        };
        // SAFETY: Win32 receives initialized repr(C) structures with their exact byte sizes.
        unsafe {
            if GlobalMemoryStatusEx(&mut memory) != 0 {
                snapshot.total_memory = memory.total_phys;
                snapshot.memory = memory.total_phys.saturating_sub(memory.avail_phys);
            } else {
                snapshot.error = Some("GlobalMemoryStatusEx failed".into());
            }
            if GetPerformanceInfo(
                &mut performance,
                std::mem::size_of::<PerformanceInfo>() as u32,
            ) != 0
            {
                // Windows commit is not bytes resident in the page file: label it truthfully.
                snapshot.swap = performance
                    .commit_total
                    .saturating_mul(performance.page_size) as u64;
                snapshot.total_swap = performance
                    .commit_limit
                    .saturating_mul(performance.page_size)
                    as u64;
            }
        }
        snapshot.swap_label = "Commit";
        self.gpu.sample(&mut snapshot);
        let mut power = vec![ProcessorPower::default(); snapshot.cores.len()];
        // SAFETY: contiguous repr(C) processor array with exact size.
        if !power.is_empty()
            && unsafe {
                CallNtPowerInformation(
                    11,
                    std::ptr::null(),
                    0,
                    power.as_mut_ptr().cast(),
                    std::mem::size_of_val(power.as_slice()) as u32,
                )
            } == 0
        {
            for info in power {
                if let Some(core) = snapshot.cores.iter_mut().find(|c| c.id == info.number) {
                    core.max_mhz = u64::from(info.max_mhz);
                    if core.mhz == 0 {
                        core.mhz = u64::from(info.current_mhz);
                    }
                }
            }
        }
        let mut classes = self.classes.values().copied().collect::<Vec<_>>();
        classes.sort_unstable_by(|a, b| b.cmp(a));
        classes.dedup();
        for cpu in &mut snapshot.cores {
            cpu.group = self
                .classes
                .get(&cpu.id)
                .and_then(|class| classes.iter().position(|v| v == class))
                .unwrap_or(0);
        }
        snapshot.cores.sort_by_key(|c| (c.group, c.id));
        if let Some(gpu) = self.nvidia.as_mut().and_then(|n| n.sample()) {
            // Keep WDDM's cross-vendor busiest-engine total when available.
            snapshot.gpu_usage = snapshot.gpu_usage.or(gpu.gpu_usage);
            snapshot.gpu_memory = snapshot.gpu_memory.or(gpu.gpu_memory);
            snapshot.gpu_power = gpu.gpu_power.or(snapshot.gpu_power);
            snapshot.gpu_frequency = gpu.gpu_frequency;
            snapshot.gpu_temperature = gpu.gpu_temperature;
            if snapshot.temperature.is_none() {
                snapshot.temperature = gpu.gpu_temperature.map(|v| ("NVIDIA GPU 温度".into(), v));
            }
            if gpu.gpu_power.is_some() {
                snapshot.gpu_info =
                    Some("WDDM/PDH 占用与显存；NVIDIA NVML 功耗/频率/温度（NVIDIA 最忙卡）".into());
            }
        }
        let mut battery = BatteryState::default();
        // SAFETY: SYSTEM_BATTERY_STATE is initialized, repr(C), with its exact size.
        if unsafe {
            CallNtPowerInformation(
                5,
                std::ptr::null(),
                0,
                (&mut battery as *mut BatteryState).cast(),
                std::mem::size_of::<BatteryState>() as u32,
            )
        } == 0
            && battery.present != 0
            && battery.discharging != 0
            && battery.rate != u32::MAX
        {
            snapshot.power = Some((battery.rate as i32).unsigned_abs() as f32 / 1000.);
            snapshot.power_label = Some("电池放电功率（Windows 电池驱动）".into());
        }
        if snapshot.total_memory > 0 {
            snapshot.memory_pressure_info = Some(format!(
                "Windows 物理内存负载 {}%（不是 macOS 内存压力等级）",
                memory.load
            ));
        }
        snapshot.capabilities.extend([
            (
                "CPU 频率".into(),
                "Windows PDH 驱动报告值，不保证是每核瞬时有效频率".into(),
            ),
            (
                "温度".into(),
                "ACPI thermal zone 或 NVIDIA GPU；不把 ACPI 温度标为 CPU 核心温度".into(),
            ),
            (
                "整机功耗".into(),
                "优先电池放电功率，否则尝试 Energy Meter 的 CPU/APU/GPU 命名域；局部功耗不作为整机总功耗"
                    .into(),
            ),
            (
                "压缩内存".into(),
                "Windows 没有已接入的公开压缩存储 API；不使用未公开内核结构".into(),
            ),
        ]);
        snapshot
    }
}

#[cfg(test)]
mod tests {
    use super::{MemoryStatus, PerformanceInfo};
    #[test]
    fn win32_layout_matches_pointer_width() {
        assert_eq!(std::mem::size_of::<MemoryStatus>(), 64);
        assert_eq!(std::mem::size_of::<super::BatteryState>(), 32);
        assert_eq!(std::mem::size_of::<super::ProcessorPower>(), 24);
        #[cfg(target_pointer_width = "64")]
        assert_eq!(std::mem::size_of::<PerformanceInfo>(), 104);
        #[cfg(target_pointer_width = "32")]
        assert_eq!(std::mem::size_of::<PerformanceInfo>(), 56);
    }
}
