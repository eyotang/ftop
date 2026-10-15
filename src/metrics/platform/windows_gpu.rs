//! Windows WDDM counters through one persistent, locale-independent PDH query.
use crate::metrics::Snapshot;
use std::{collections::HashMap, ffi::c_void};
type Handle = isize;
#[repr(C)]
struct Value {
    status: u32,
    value: f64,
}
#[repr(C)]
struct Item {
    name: *const u16,
    value: Value,
}
#[link(name = "pdh")]
unsafe extern "system" {
    fn PdhOpenQueryW(source: *const u16, user: usize, query: *mut Handle) -> u32;
    fn PdhAddEnglishCounterW(
        query: Handle,
        path: *const u16,
        user: usize,
        counter: *mut Handle,
    ) -> u32;
    fn PdhCollectQueryData(query: Handle) -> u32;
    fn PdhGetFormattedCounterArrayW(
        counter: Handle,
        format: u32,
        bytes: *mut u32,
        count: *mut u32,
        buffer: *mut c_void,
    ) -> u32;
    fn PdhCloseQuery(query: Handle) -> u32;
}
pub struct Counters {
    query: Handle,
    engine: Handle,
    dedicated: Handle,
    shared: Handle,
    thermal: Handle,
    frequency: Handle,
}
impl Counters {
    pub fn new() -> Self {
        let mut this = Self {
            query: 0,
            engine: 0,
            dedicated: 0,
            shared: 0,
            thermal: 0,
            frequency: 0,
        };
        // SAFETY: Valid initialized handle output; all handles belong to this query.
        unsafe {
            if PdhOpenQueryW(std::ptr::null(), 0, &mut this.query) != 0 {
                return this;
            }
            for (path, output) in [
                (r"\GPU Engine(*)\Utilization Percentage", &mut this.engine),
                (
                    r"\GPU Adapter Memory(*)\Dedicated Usage",
                    &mut this.dedicated,
                ),
                (r"\GPU Adapter Memory(*)\Shared Usage", &mut this.shared),
                (
                    r"\Thermal Zone Information(*)\Temperature",
                    &mut this.thermal,
                ),
                (
                    r"\Processor Information(*)\Processor Frequency",
                    &mut this.frequency,
                ),
            ] {
                let path: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
                if PdhAddEnglishCounterW(this.query, path.as_ptr(), 0, output) != 0 {
                    *output = 0;
                }
            }
            let _ = PdhCollectQueryData(this.query);
        }
        this
    }
    fn values(&self, counter: Handle) -> Vec<(String, f64)> {
        if counter == 0 {
            return Vec::new();
        }
        // SAFETY: PDH reports required bytes. u64 allocation provides alignment for
        // PDH_FMT_COUNTERVALUE_ITEM_W. Strings point inside that live allocation.
        unsafe {
            let (mut bytes, mut count) = (0, 0);
            let _ = PdhGetFormattedCounterArrayW(
                counter,
                0x200 | 0x8000,
                &mut bytes,
                &mut count,
                std::ptr::null_mut(),
            );
            if bytes == 0 || bytes > 16 * 1024 * 1024 {
                return Vec::new();
            }
            let mut buffer = vec![0u64; (bytes as usize).div_ceil(8)];
            if PdhGetFormattedCounterArrayW(
                counter,
                0x200 | 0x8000,
                &mut bytes,
                &mut count,
                buffer.as_mut_ptr().cast(),
            ) != 0
            {
                return Vec::new();
            }
            if count as usize > buffer.len() * 8 / std::mem::size_of::<Item>() {
                return Vec::new();
            }
            let start = buffer.as_ptr() as usize;
            let end = start + buffer.len() * 8;
            std::slice::from_raw_parts(buffer.as_ptr().cast::<Item>(), count as usize)
                .iter()
                .filter_map(|item| {
                    if item.value.status > 1 || !item.value.value.is_finite() {
                        return None;
                    }
                    let address = item.name as usize;
                    if address < start || address >= end || address % 2 != 0 {
                        return None;
                    }
                    let text = std::slice::from_raw_parts(item.name, (end - address) / 2);
                    let length = text.iter().position(|v| *v == 0)?;
                    Some((String::from_utf16_lossy(&text[..length]), item.value.value))
                })
                .collect()
        }
    }
    pub fn sample(&mut self, s: &mut Snapshot) {
        // SAFETY: Persistent query handle, accessed only by collector worker.
        if self.query == 0 || unsafe { PdhCollectQueryData(self.query) } != 0 {
            s.gpu_info = Some("Windows GPU 性能计数器未开放（需要支持 WDDM 的驱动）".into());
            return;
        }
        let values = self.values(self.engine);
        s.gpu_usage = busiest_engine(values);
        let dedicated = self.values(self.dedicated);
        let shared = self.values(self.shared);
        if !dedicated.is_empty() || !shared.is_empty() {
            s.gpu_memory = Some(
                dedicated
                    .iter()
                    .chain(shared.iter())
                    .filter(|(n, _)| n != "_Total")
                    .map(|(_, v)| v.max(0.) as u64)
                    .sum(),
            );
        }
        s.gpu_info = Some(
            if s.gpu_usage.is_some() {
                "Windows WDDM · PDH · 最忙引擎占用；显存为所有适配器专用+共享用量"
            } else {
                "WDDM GPU 引擎计数器无有效读数：驱动不支持或等待第二次采样"
            }
            .into(),
        );
        if s.temperature.is_none() {
            s.temperature = self
                .values(self.thermal)
                .into_iter()
                .filter_map(|(n, v)| {
                    let c = v as f32 - 273.15;
                    (-30. ..=150.)
                        .contains(&c)
                        .then(|| (format!("ACPI thermal zone {n}"), c))
                })
                .max_by(|a, b| a.1.total_cmp(&b.1));
        }
        for (name, value) in self.values(self.frequency) {
            if let Some((group, core)) = name.split_once(',') {
                if let (Ok(group), Ok(core)) = (group.parse::<u32>(), core.parse::<u32>()) {
                    let id = group * 64 + core;
                    if let Some(cpu) = s.cores.iter_mut().find(|c| c.id == id) {
                        cpu.mhz = value.max(0.) as u64;
                    }
                }
            }
        }
    }
}
fn busiest_engine(values: Vec<(String, f64)>) -> Option<f32> {
    let mut engines = HashMap::<String, f64>::new();
    for (name, value) in values {
        if !value.is_finite() || value < 0. {
            continue;
        }
        // Each PID reports a share of the same physical engine. Sum PID shares,
        // then select the busiest engine; summing unrelated engines overcounts.
        let Some((_, engine)) = name.split_once("_luid_") else {
            continue;
        };
        *engines.entry(engine.to_owned()).or_default() += value;
    }
    engines
        .values()
        .copied()
        .reduce(f64::max)
        .map(|v| v.clamp(0., 100.) as f32)
}
impl Drop for Counters {
    fn drop(&mut self) {
        if self.query != 0 {
            unsafe {
                PdhCloseQuery(self.query);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::busiest_engine;
    #[test]
    fn aggregate_process_shares_without_summing_engines() {
        assert_eq!(
            busiest_engine(vec![
                ("pid_1_luid_a_phys_0_eng_0_engtype_3D".into(), 30.),
                ("pid_2_luid_a_phys_0_eng_0_engtype_3D".into(), 20.),
                ("pid_1_luid_a_phys_0_eng_1_engtype_Copy".into(), 40.)
            ]),
            Some(50.)
        );
        assert_eq!(busiest_engine(vec![]), None);
    }
}
