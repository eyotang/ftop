//! macOS-specific adapter. Private hardware APIs are confined to the upstream MIT C shim.
//! CPU/memory/process/network counters use sysinfo's native Mach/libproc/sysctl backend.
use super::common::Portable;
use crate::metrics::Snapshot;
use std::{
    collections::HashMap,
    ffi::{c_char, c_void},
    time::Instant,
};
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CoreFrequency {
    channel: [c_char; 32],
    kind: c_char,
    index: i32,
    active: f64,
    mhz: f64,
    max_mhz: f64,
    steps: i32,
    tables: i32,
    fitting: i32,
}
#[repr(C)]
#[derive(Default)]
struct Temperature {
    average: f64,
    maximum: f64,
    sensor_count: i32,
    source: i32,
}
#[repr(C)]
#[derive(Default)]
struct GpuFrequency {
    active: f64,
    mhz: f64,
    max_mhz: f64,
    steps: i32,
    table_steps: i32,
    uncovered_state: i32,
    uncovered_name: [c_char; 32],
}
#[repr(C)]
#[derive(Default)]
struct GpuStats {
    utilization: f64,
    memory_bytes: f64,
}
unsafe extern "C" {
    fn ftop_cpu_sampler_create() -> *mut c_void;
    fn ftop_cpu_sampler_destroy(sampler: *mut c_void);
    fn ftop_cpu_sampler_update(sampler: *mut c_void, out: *mut CoreFrequency, capacity: i32)
    -> i32;
    fn ftop_cpu_cluster_types(out: *mut c_char, capacity: i32) -> i32;
    fn ftop_read_temperature(out: *mut Temperature) -> i32;
    fn ftop_gpu_sampler_create() -> *mut c_void;
    fn ftop_gpu_sampler_destroy(sampler: *mut c_void);
    fn ftop_gpu_sampler_update(sampler: *mut c_void, out: *mut GpuFrequency) -> i32;
    fn ftop_gpu_sampler_energy(sampler: *mut c_void) -> f64;
    fn ftop_gpu_read_stats(out: *mut GpuStats) -> i32;
    fn ftop_smc_read_float(key: *const c_char, out: *mut f64) -> i32;
    fn mach_port_deallocate(task: libc::mach_port_t, name: libc::mach_port_t) -> i32;
}
fn reading(value: f64) -> Option<f32> {
    (value.is_finite() && value >= 0.).then_some(value as f32)
}
pub struct Backend {
    portable: Portable,
    cpu_sampler: *mut c_void,
    gpu_sampler: *mut c_void,
    types: Vec<u8>,
    host: libc::mach_port_t,
    page_size: u64,
    total_memory: u64,
    last: Instant,
    temperature: Option<(String, f32)>,
    samples: u64,
}
impl Backend {
    pub fn new() -> Self {
        let mut types = [0 as c_char; 256];
        // SAFETY: the shim writes at most the supplied capacity into this owned array.
        unsafe {
            let mut total_memory = 0u64;
            let mut length = std::mem::size_of_val(&total_memory);
            libc::sysctlbyname(
                c"hw.memsize".as_ptr(),
                &mut total_memory as *mut _ as *mut c_void,
                &mut length,
                std::ptr::null_mut(),
                0,
            );
            let count = ftop_cpu_cluster_types(types.as_mut_ptr(), types.len() as i32)
                .clamp(0, types.len() as i32) as usize;
            Self {
                portable: Portable::new(),
                cpu_sampler: ftop_cpu_sampler_create(),
                gpu_sampler: ftop_gpu_sampler_create(),
                types: types[..count].iter().map(|v| *v as u8).collect(),
                host: libc::mach_host_self(),
                total_memory,
                page_size: libc::sysconf(libc::_SC_PAGESIZE).max(1) as u64,
                last: Instant::now(),
                temperature: None,
                samples: 0,
            }
        }
    }
    pub fn sample(&mut self) -> Snapshot {
        let mut snapshot = self.portable.sample(false);
        let elapsed = self.last.elapsed().as_secs_f64().max(0.001);
        self.last = Instant::now();
        // SAFETY: C samplers are created, used, and destroyed on this one worker thread.
        // Each output buffer is correctly aligned, repr(C), initialized, and capacity-bounded.
        unsafe {
            let mut raw = [CoreFrequency::default(); 256];
            let count = if self.cpu_sampler.is_null() {
                -1
            } else {
                ftop_cpu_sampler_update(self.cpu_sampler, raw.as_mut_ptr(), raw.len() as i32)
            };
            let mut frequencies: HashMap<u8, Vec<CoreFrequency>> = HashMap::new();
            for core in raw.iter().take(count.max(0) as usize) {
                frequencies.entry(core.kind as u8).or_default().push(*core);
            }
            let mut kinds = self.types.clone();
            kinds.sort_unstable();
            kinds.dedup();
            kinds.sort_by_key(|kind| match kind {
                b'P' => 0,
                b'M' => 1,
                b'E' => 2,
                _ => 3,
            });
            let mut positions: HashMap<u8, usize> = HashMap::new();
            for core in &mut snapshot.cores {
                let kind = self.types.get(core.id as usize).copied().unwrap_or(0);
                core.group = kinds.iter().position(|v| *v == kind).unwrap_or(0);
                let index = positions.entry(kind).or_default();
                // Match by cluster type and channel position only when counts agree, as upstream does.
                if let Some(list) = frequencies
                    .get(&kind)
                    .filter(|list| list.len() == self.types.iter().filter(|v| **v == kind).count())
                {
                    if let Some(frequency) = list.get(*index) {
                        core.mhz = reading(frequency.mhz).unwrap_or(0.) as u64;
                        core.max_mhz = reading(frequency.max_mhz).unwrap_or(0.) as u64;
                    }
                }
                *index += 1;
            }
            snapshot.cores.sort_by_key(|c| (c.group, c.id));
            let mut stats = GpuStats::default();
            if ftop_gpu_read_stats(&mut stats) == 0 {
                snapshot.gpu_usage = reading(stats.utilization).map(|v| (v * 100.).clamp(0., 100.));
                snapshot.gpu_memory = reading(stats.memory_bytes).map(|v| v as u64);
            }
            if !self.gpu_sampler.is_null() {
                let mut frequency = GpuFrequency::default();
                if ftop_gpu_sampler_update(self.gpu_sampler, &mut frequency) == 0 {
                    if let (Some(mhz), Some(max_mhz)) =
                        (reading(frequency.mhz), reading(frequency.max_mhz))
                    {
                        if mhz > 0. && max_mhz > 0. {
                            snapshot.gpu_frequency = Some((mhz as u64, max_mhz as u64));
                        }
                    }
                    snapshot.gpu_power =
                        reading(ftop_gpu_sampler_energy(self.gpu_sampler) / elapsed);
                }
            }
            let mut watts = 0.;
            if ftop_smc_read_float(c"PSTR".as_ptr(), &mut watts) == 0 {
                snapshot.power = reading(watts);
            }
            if self.samples % 5 == 0 {
                let mut temperature = Temperature::default();
                self.temperature = if ftop_read_temperature(&mut temperature) == 0 {
                    reading(temperature.average).map(|value| {
                        (
                            if temperature.source == 1 {
                                "CPU cluster"
                            } else {
                                "PMU die"
                            }
                            .into(),
                            value,
                        )
                    })
                } else {
                    None
                };
            }
            snapshot.temperature = self.temperature.clone();
            let mut statistics: libc::vm_statistics64 = std::mem::zeroed();
            let mut count = libc::HOST_VM_INFO64_COUNT;
            if libc::host_statistics64(
                self.host,
                libc::HOST_VM_INFO64,
                &mut statistics as *mut _ as *mut i32,
                &mut count,
            ) == libc::KERN_SUCCESS
            {
                snapshot.total_memory = self.total_memory;
                let app_pages = u64::from(statistics.internal_page_count)
                    .saturating_sub(u64::from(statistics.purgeable_count));
                snapshot.memory = (app_pages
                    + u64::from(statistics.wire_count)
                    + u64::from(statistics.compressor_page_count))
                .saturating_mul(self.page_size)
                .min(self.total_memory);
                snapshot.compressed =
                    Some(u64::from(statistics.compressor_page_count) * self.page_size);
            }
            let mut swap: libc::xsw_usage = std::mem::zeroed();
            let mut swap_size = std::mem::size_of_val(&swap);
            if libc::sysctlbyname(
                c"vm.swapusage".as_ptr(),
                &mut swap as *mut _ as *mut c_void,
                &mut swap_size,
                std::ptr::null_mut(),
                0,
            ) == 0
            {
                snapshot.swap = swap.xsu_used;
                snapshot.total_swap = swap.xsu_total;
            }
            let mut pressure = 0i32;
            let mut length = std::mem::size_of_val(&pressure);
            if libc::sysctlbyname(
                c"kern.memorystatus_vm_pressure_level".as_ptr(),
                &mut pressure as *mut _ as *mut c_void,
                &mut length,
                std::ptr::null_mut(),
                0,
            ) == 0
                && matches!(pressure, 1 | 2 | 4)
            {
                snapshot.pressure = Some(pressure as u32);
            }
        }
        self.samples += 1;
        snapshot
    }
}
impl Drop for Backend {
    fn drop(&mut self) {
        // SAFETY: each sampler and the host send right are released exactly once by their owner.
        unsafe {
            if !self.cpu_sampler.is_null() {
                ftop_cpu_sampler_destroy(self.cpu_sampler);
            }
            if !self.gpu_sampler.is_null() {
                ftop_gpu_sampler_destroy(self.gpu_sampler);
            }
            #[allow(deprecated)]
            let task = libc::mach_task_self();
            mach_port_deallocate(task, self.host);
        }
    }
}
