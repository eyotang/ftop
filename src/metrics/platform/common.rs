use crate::metrics::{Core, Process, Snapshot};
use std::{collections::HashMap, time::Instant};
use sysinfo::{Networks, ProcessRefreshKind, ProcessesToUpdate, System};
pub struct Portable {
    system: System,
    networks: Networks,
    last: Instant,
}
impl Portable {
    pub fn new() -> Self {
        let mut system = System::new();
        system.refresh_cpu_all();
        system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .with_cpu()
                .with_memory()
                .without_tasks(),
        );
        Self {
            system,
            networks: Networks::new_with_refreshed_list(),
            last: Instant::now(),
        }
    }
    pub fn sample(&mut self, memory: bool) -> Snapshot {
        self.system.refresh_cpu_usage();
        if memory {
            self.system.refresh_memory();
        }

        self.system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .with_cpu()
                .with_memory()
                .without_tasks(),
        );
        self.networks.refresh(true);
        let elapsed = self.last.elapsed().as_secs_f64().max(0.001);
        self.last = Instant::now();
        let cores: Vec<_> = self
            .system
            .cpus()
            .iter()
            .enumerate()
            .map(|(id, c)| Core {
                usage: c.cpu_usage(),
                mhz: 0,
                max_mhz: 0,
                id: id as u32,
                group: 0,
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
            swap_label: "Swap",
            ..Snapshot::default()
        }
    }
}
