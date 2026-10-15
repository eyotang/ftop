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
pub struct Backend {
    portable: Portable,
    gpu: super::windows_gpu::Counters,
    nvidia: Option<super::nvidia::Nvidia>,
}
impl Backend {
    pub fn new() -> Self {
        Self {
            portable: Portable::new(),
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
        if let Some(gpu) = self.nvidia.as_ref().and_then(|n| n.sample()) {
            // Keep WDDM's cross-vendor busiest-engine total when available.
            snapshot.gpu_usage = snapshot.gpu_usage.or(gpu.gpu_usage);
            snapshot.gpu_memory = snapshot.gpu_memory.or(gpu.gpu_memory);
            snapshot.gpu_power = gpu.gpu_power;
            snapshot.gpu_frequency = gpu.gpu_frequency;
            snapshot.gpu_temperature = gpu.gpu_temperature;
            if snapshot.temperature.is_none() {
                snapshot.temperature = gpu.gpu_temperature.map(|v| ("NVIDIA GPU 温度".into(), v));
            }
            if snapshot.gpu_power.is_some() {
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
        snapshot.memory_pressure_info = Some(format!(
            "Windows 物理内存负载 {}%（不是 macOS 内存压力等级）",
            memory.load
        ));
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
                "电池放电功率可读时显示；接通电源或无电池时无通用整机实时功率；GPU 功率取决于驱动"
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
        #[cfg(target_pointer_width = "64")]
        assert_eq!(std::mem::size_of::<PerformanceInfo>(), 104);
        #[cfg(target_pointer_width = "32")]
        assert_eq!(std::mem::size_of::<PerformanceInfo>(), 56);
    }
}
