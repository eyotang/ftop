use std::{collections::HashMap, time::Instant};
use sysinfo::{Components, Networks, ProcessesToUpdate, System};

#[derive(Clone, Debug)]
pub struct Core {
    pub usage: f32,
    pub mhz: u64,
}
#[derive(Clone, Debug)]
pub struct Process {
    pub name: String,
    pub cpu: f32,
    pub memory: u64,
    pub pids: Vec<u32>,
}
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub cpu: f32,
    pub cores: Vec<Core>,
    pub memory: u64,
    pub total_memory: u64,
    pub swap: u64,
    pub total_swap: u64,
    pub download: f64,
    pub upload: f64,
    pub processes: Vec<Process>,
    pub temperature: Option<(String, f32)>,
}
pub struct Collector {
    system: System,
    networks: Networks,
    last: Instant,
    components: Components,
}
impl Collector {
    pub fn new() -> Self {
        Self {
            system: System::new_all(),
            networks: Networks::new_with_refreshed_list(),
            last: Instant::now(),
            components: Components::new_with_refreshed_list(),
        }
    }
    pub fn sample(&mut self) -> Snapshot {
        self.system.refresh_cpu_all();
        self.system.refresh_memory();
        self.components.refresh(true);
        self.system.refresh_processes(ProcessesToUpdate::All, true);
        self.networks.refresh(true);
        let elapsed = self.last.elapsed().as_secs_f64().max(0.001);
        self.last = Instant::now();
        let cores: Vec<_> = self
            .system
            .cpus()
            .iter()
            .map(|c| Core {
                usage: c.cpu_usage(),
                mhz: c.frequency(),
            })
            .collect();
        let mut groups: HashMap<String, Process> = HashMap::new();
        for (pid, p) in self.system.processes() {
            let name = p.name().to_string_lossy().into_owned();
            let group = groups.entry(name.clone()).or_insert(Process {
                name,
                cpu: 0.,
                memory: 0,
                pids: Vec::new(),
            });
            group.cpu += p.cpu_usage() / cores.len().max(1) as f32;
            group.memory += p.memory();
            group.pids.push(pid.as_u32());
        }
        let mut processes: Vec<_> = groups.into_values().collect();
        processes.sort_by(|a, b| b.cpu.total_cmp(&a.cpu).then_with(|| a.name.cmp(&b.name)));
        Snapshot {
            cpu: self.system.global_cpu_usage(),
            cores,
            memory: self.system.used_memory(),
            total_memory: self.system.total_memory(),
            swap: self.system.used_swap(),
            total_swap: self.system.total_swap(),
            download: self
                .networks
                .values()
                .map(|n| n.received() as f64)
                .sum::<f64>()
                / elapsed,
            upload: self
                .networks
                .values()
                .map(|n| n.transmitted() as f64)
                .sum::<f64>()
                / elapsed,
            processes,
            temperature: self
                .components
                .iter()
                .filter_map(|c| c.temperature().map(|t| (c.label().to_string(), t)))
                .filter(|(_, t)| t.is_finite())
                .max_by(|a, b| a.1.total_cmp(&b.1)),
        }
    }
}
pub fn gb(bytes: u64) -> String {
    format!("{:.1}", bytes as f64 / 1_073_741_824.)
}
pub fn rate(bytes: f64) -> String {
    if bytes >= 1_048_576. {
        format!("{:.1} MB/s", bytes / 1_048_576.)
    } else {
        format!("{:.0} KB/s", bytes / 1024.)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rates_keep_units() {
        assert_eq!(rate(1048576.), "1.0 MB/s");
        assert_eq!(rate(2048.), "2 KB/s");
    }
    #[test]
    fn sample_is_real_and_bounded() {
        let mut c = Collector::new();
        std::thread::sleep(std::time::Duration::from_millis(250));
        let s = c.sample();
        assert!(!s.cores.is_empty());
        assert!(s.total_memory > 0 && s.memory <= s.total_memory);
        assert!(s.cpu >= 0. && s.cpu <= 100.);
    }
}
