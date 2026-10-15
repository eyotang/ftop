//! Windows: native per-CPU/PSAPI process counters via sysinfo; Win32 memory/commit accounting.
//! No WMI temperature polling and no guessed per-core clock or GPU readings.
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
pub struct Backend {
    portable: Portable,
}
impl Backend {
    pub fn new() -> Self {
        Self {
            portable: Portable::new(),
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
        snapshot
    }
}

#[cfg(test)]
mod tests {
    use super::{MemoryStatus, PerformanceInfo};
    #[test]
    fn win32_layout_matches_pointer_width() {
        assert_eq!(std::mem::size_of::<MemoryStatus>(), 64);
        #[cfg(target_pointer_width = "64")]
        assert_eq!(std::mem::size_of::<PerformanceInfo>(), 104);
        #[cfg(target_pointer_width = "32")]
        assert_eq!(std::mem::size_of::<PerformanceInfo>(), 56);
    }
}
