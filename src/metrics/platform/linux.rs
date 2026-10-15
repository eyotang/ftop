//! Linux collector: procfs counters and sysfs sensors; no shell, thread enumeration or root.
use crate::metrics::{Core, Process, Snapshot};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Read,
    path::PathBuf,
    time::Instant,
};

#[derive(Clone, Copy, Default)]
struct Ticks {
    total: u64,
    idle: u64,
}
fn cpu_ticks(line: &str) -> Option<Ticks> {
    let values: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .take(8)
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    (values.len() >= 4).then(|| Ticks {
        total: values.iter().sum(),
        idle: values[3] + values.get(4).copied().unwrap_or(0),
    })
}
fn utilization(previous: Ticks, current: Ticks) -> f32 {
    let total = current.total.saturating_sub(previous.total);
    if total == 0 {
        return 0.;
    }
    (100. * (total.saturating_sub(current.idle.saturating_sub(previous.idle))) as f32
        / total as f32)
        .clamp(0., 100.)
}
struct Proc<'a> {
    name: &'a str,
    ticks: u64,
    start: u64,
    rss: u64,
}
fn process_stat(text: &str, pages: u64) -> Option<Proc<'_>> {
    let start = text.find('(')?;
    let end = text.rfind(')')?;
    let mut fields = text.get(end + 1..)?.split_whitespace();
    let user = fields.nth(11)?.parse::<u64>().ok()?;
    let system = fields.next()?.parse::<u64>().ok()?;
    let started = fields.nth(6)?.parse::<u64>().ok()?;
    let rss = fields.nth(1)?.parse::<i64>().ok()?.max(0) as u64;
    Some(Proc {
        name: text.get(start + 1..end)?,
        ticks: user.saturating_add(system),
        start: started,
        rss: rss.saturating_mul(pages),
    })
}
fn memory(text: &str) -> HashMap<&str, u64> {
    text.lines()
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            Some((
                key,
                value.split_whitespace().next()?.parse::<u64>().ok()? * 1024,
            ))
        })
        .collect()
}
fn number(path: impl AsRef<std::path::Path>) -> Option<u64> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}
struct CpuMeta {
    id: u32,
    capacity: u64,
    frequency: PathBuf,
    max_mhz: u64,
}
struct Sensor {
    path: PathBuf,
    label: String,
    cpu: bool,
}
pub struct Backend {
    cpu: HashMap<String, Ticks>,
    processes: HashMap<u32, (u64, u64)>,
    meta: Vec<CpuMeta>,
    sensors: Vec<Sensor>,
    temperature: Option<(String, f32)>,
    network: (u64, u64),
    physical_interfaces: HashSet<String>,
    last: Instant,
    ticks_per_second: f64,
    page_size: u64,
    samples: u64,
}
impl Backend {
    pub fn new() -> Self {
        let stat = fs::read_to_string("/proc/stat").unwrap_or_default();
        let mut meta = Vec::new();
        for line in stat.lines().filter(|line| line.starts_with("cpu")) {
            let Some(id) = line
                .split_whitespace()
                .next()
                .and_then(|s| s.strip_prefix("cpu"))
                .and_then(|s| s.parse::<u32>().ok())
            else {
                continue;
            };
            let path = PathBuf::from(format!("/sys/devices/system/cpu/cpu{id}"));
            meta.push(CpuMeta {
                id,
                capacity: number(path.join("cpu_capacity")).unwrap_or(0),
                frequency: path.join("cpufreq/scaling_cur_freq"),
                max_mhz: number(path.join("cpufreq/cpuinfo_max_freq")).unwrap_or(0) / 1000,
            });
        }
        meta.sort_by_key(|cpu| (std::cmp::Reverse(cpu.capacity), cpu.id));
        let physical_interfaces = fs::read_dir("/sys/class/net")
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().join("device").exists())
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        let mut this = Self {
            cpu: HashMap::new(),
            processes: HashMap::new(),
            meta,
            sensors: Vec::new(),
            temperature: None,
            network: (0, 0),
            physical_interfaces,
            last: Instant::now(),
            ticks_per_second: 100.,
            page_size: 4096,
            samples: 0,
        };
        // SAFETY: sysconf has no pointer arguments or ownership effects.
        unsafe {
            this.ticks_per_second = libc::sysconf(libc::_SC_CLK_TCK).max(1) as f64;
            this.page_size = libc::sysconf(libc::_SC_PAGESIZE).max(1) as u64;
        }
        for line in stat.lines().filter(|line| line.starts_with("cpu")) {
            if let (Some(name), Some(ticks)) = (line.split_whitespace().next(), cpu_ticks(line)) {
                this.cpu.insert(name.into(), ticks);
            }
        }
        this.discover_sensors();
        this.network = this.network_totals();
        this.read_processes(1.); // Prime counters before the first visible sample.
        this.last = Instant::now();
        this
    }
    fn discover_sensors(&mut self) {
        for device in fs::read_dir("/sys/class/hwmon")
            .into_iter()
            .flatten()
            .flatten()
        {
            let name = fs::read_to_string(device.path().join("name"))
                .unwrap_or_default()
                .trim()
                .to_owned();
            for file in fs::read_dir(device.path()).into_iter().flatten().flatten() {
                let file_name = file.file_name().to_string_lossy().into_owned();
                if !file_name.starts_with("temp") || !file_name.ends_with("_input") {
                    continue;
                }
                let label = fs::read_to_string(
                    file.path()
                        .with_file_name(file_name.replace("_input", "_label")),
                )
                .unwrap_or_else(|_| file_name.clone());
                let cpu = matches!(
                    name.as_str(),
                    "coretemp" | "k10temp" | "zenpower" | "cpu_thermal" | "soc_thermal"
                );
                self.sensors.push(Sensor {
                    path: file.path(),
                    label: format!("{name} {}", label.trim()),
                    cpu,
                });
            }
        }
        if self.sensors.iter().any(|s| s.cpu) {
            self.sensors.retain(|s| s.cpu);
        }
    }
    fn network_totals(&self) -> (u64, u64) {
        fs::read_to_string("/proc/net/dev")
            .unwrap_or_default()
            .lines()
            .filter_map(|line| {
                let (name, counters) = line.split_once(':')?;
                let name = name.trim();
                if name == "lo"
                    || (!self.physical_interfaces.is_empty()
                        && !self.physical_interfaces.contains(name))
                {
                    return None;
                }
                let values: Vec<&str> = counters.split_whitespace().collect();
                Some((
                    values.first()?.parse::<u64>().ok()?,
                    values.get(8)?.parse::<u64>().ok()?,
                ))
            })
            .fold((0, 0), |sum, v| {
                (sum.0.saturating_add(v.0), sum.1.saturating_add(v.1))
            })
    }
    fn read_processes(&mut self, elapsed: f64) -> Vec<Process> {
        let mut groups: HashMap<String, Process> = HashMap::with_capacity(128);
        let mut counters = HashMap::with_capacity(self.processes.len());
        let mut buffer = [0u8; 4096];
        let cpu_scale = 100. / self.ticks_per_second / elapsed / self.meta.len().max(1) as f64;
        for entry in fs::read_dir("/proc").into_iter().flatten().flatten() {
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|s| s.parse::<u32>().ok())
            else {
                continue;
            };
            // procfs stat is one bounded record. One read avoids metadata/EOF probes
            // and a fresh allocation for every process on every sample.
            let Ok(mut file) = fs::File::open(entry.path().join("stat")) else {
                continue;
            };
            let Ok(length) = file.read(&mut buffer) else {
                continue;
            };
            if length == buffer.len() {
                continue;
            }
            let Some(p) = std::str::from_utf8(&buffer[..length])
                .ok()
                .and_then(|text| process_stat(text, self.page_size))
            else {
                continue;
            };
            let ticks = self
                .processes
                .get(&pid)
                .filter(|(start, _)| *start == p.start)
                .map(|(_, ticks)| p.ticks.saturating_sub(*ticks))
                .unwrap_or(0);
            counters.insert(pid, (p.start, p.ticks));
            let cpu = (ticks as f64 * cpu_scale) as f32;
            let group = groups.entry(p.name.to_owned()).or_insert_with(|| Process {
                name: p.name.to_owned(),
                cpu: 0.,
                memory: 0,
                pids: Vec::new(),
            });
            group.cpu += cpu;
            group.memory = group.memory.saturating_add(p.rss);
            group.pids.push(pid);
        }
        self.processes = counters;
        let mut groups: Vec<_> = groups.into_values().collect();
        groups.sort_by(|a, b| b.cpu.total_cmp(&a.cpu).then_with(|| a.name.cmp(&b.name)));
        groups
    }
    pub fn sample(&mut self) -> Snapshot {
        let started = Instant::now();
        let elapsed = started.duration_since(self.last).as_secs_f64().max(0.001);
        self.last = started;
        let (Ok(stat), Ok(mem)) = (
            fs::read_to_string("/proc/stat"),
            fs::read_to_string("/proc/meminfo"),
        ) else {
            return Snapshot {
                error: Some("Cannot read Linux CPU/memory counters".into()),
                ..Snapshot::default()
            };
        };
        let mut usage = HashMap::new();
        for line in stat.lines().filter(|line| line.starts_with("cpu")) {
            if let (Some(name), Some(current)) = (line.split_whitespace().next(), cpu_ticks(line)) {
                let previous = self.cpu.insert(name.into(), current).unwrap_or(current);
                usage.insert(name.to_owned(), utilization(previous, current));
            }
        }
        let mut capacities: Vec<_> = self.meta.iter().map(|c| c.capacity).collect();
        capacities.sort_unstable_by(|a, b| b.cmp(a));
        capacities.dedup();
        let cores = self
            .meta
            .iter()
            .map(|c| Core {
                id: c.id,
                usage: usage.get(&format!("cpu{}", c.id)).copied().unwrap_or(0.),
                mhz: number(&c.frequency).unwrap_or(0) / 1000,
                max_mhz: c.max_mhz,
                group: capacities
                    .iter()
                    .position(|v| *v == c.capacity)
                    .unwrap_or(0),
            })
            .collect();
        let values = memory(&mem);
        let total = *values.get("MemTotal").unwrap_or(&0);
        let available = *values.get("MemAvailable").unwrap_or(&0);
        let total_swap = *values.get("SwapTotal").unwrap_or(&0);
        let swap = total_swap.saturating_sub(*values.get("SwapFree").unwrap_or(&0));
        let network = self.network_totals();
        let download = network.0.saturating_sub(self.network.0) as f64 / elapsed;
        let upload = network.1.saturating_sub(self.network.1) as f64 / elapsed;
        self.network = network;
        if self.samples % 5 == 0 {
            self.temperature = self
                .sensors
                .iter()
                .filter_map(|s| {
                    let temperature = fs::read_to_string(&s.path)
                        .ok()?
                        .trim()
                        .parse::<f32>()
                        .ok()?
                        / 1000.;
                    ((-50. ..=200.).contains(&temperature)).then(|| (s.label.clone(), temperature))
                })
                .max_by(|a, b| a.1.total_cmp(&b.1));
        }
        self.samples += 1;
        let processes = self.read_processes(elapsed);
        Snapshot {
            cpu: usage.get("cpu").copied().unwrap_or(0.),
            cores,
            memory: total.saturating_sub(available),
            total_memory: total,
            swap,
            total_swap,
            swap_label: "Swap",
            download,
            upload,
            processes,
            temperature: self.temperature.clone(),
            ..Snapshot::default()
        }
    }
}
#[cfg(test)]
mod tests {
    use super::{Ticks, cpu_ticks, memory, process_stat, utilization};
    #[test]
    fn kernel_counters_exclude_guest_double_counting() {
        let ticks = cpu_ticks("cpu0 100 0 50 700 10 5 5 0 40 0").unwrap();
        assert_eq!(ticks.total, 870);
        assert_eq!(ticks.idle, 710);
        assert_eq!(
            utilization(
                Ticks {
                    total: 100,
                    idle: 50
                },
                Ticks {
                    total: 200,
                    idle: 130
                }
            ),
            20.
        );
    }
    #[test]
    fn parses_memory_as_bytes() {
        let m = memory("MemTotal: 100 kB\nMemAvailable: 40 kB\n");
        assert_eq!(m["MemTotal"], 102400);
        assert_eq!(m["MemAvailable"], 40960);
    }
    #[test]
    fn process_names_with_spaces_and_parentheses_are_not_field_separators() {
        let mut fields = vec!["0"; 22];
        fields[0] = "R";
        fields[11] = "12";
        fields[12] = "8";
        fields[19] = "1234";
        fields[21] = "25";
        let text = format!("42 (worker (a) b) {}", fields.join(" "));
        let p = process_stat(&text, 4096).unwrap();
        assert_eq!(p.name, "worker (a) b");
        assert_eq!(p.ticks, 20);
        assert_eq!(p.start, 1234);
        assert_eq!(p.rss, 102400);
    }
}
