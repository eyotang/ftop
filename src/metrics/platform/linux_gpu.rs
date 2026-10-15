//! Driver-specific GPU adapters. Persistent PMU/NVML handles; cached DRM descriptors.
use crate::metrics::Snapshot;
use std::{
    collections::HashMap,
    fs,
    io::Read,
    os::fd::{AsRawFd, FromRawFd},
    path::{Path, PathBuf},
    time::Instant,
};
fn number(path: impl AsRef<Path>) -> Option<u64> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}
struct Card {
    path: PathBuf,
    driver: String,
}
struct Engine {
    file: fs::File,
    last: Option<(u64, u64, u64)>,
}
struct Client {
    key: String,
    engines: HashMap<String, u64>,
    capacities: HashMap<String, u64>,
    memory: Option<u64>,
}
fn client(text: &str) -> Option<Client> {
    let (mut driver, mut id, mut device) = (None, None, None);
    let mut engines = HashMap::with_capacity(4);
    let mut capacities = HashMap::new();
    let mut memory = None;
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match key {
            "drm-driver" => driver = Some(value),
            "drm-client-id" => id = Some(value),
            "drm-pdev" => device = Some(value),
            _ => {
                if let Some(engine) = key.strip_prefix("drm-engine-capacity-") {
                    if let Ok(value) = value.parse::<u64>() {
                        capacities.insert(engine.to_owned(), value.max(1));
                    }
                } else if let Some(engine) = key.strip_prefix("drm-engine-") {
                    let mut parts = value.split_whitespace();
                    if let (Some(Ok(value)), Some("ns")) =
                        (parts.next().map(str::parse::<u64>), parts.next())
                    {
                        engines.insert(engine.to_owned(), value);
                    }
                } else if key.starts_with("drm-resident-") {
                    let mut parts = value.split_whitespace();
                    if let (Some(Ok(value)), Some(unit)) =
                        (parts.next().map(str::parse::<u64>), parts.next())
                    {
                        let scale = match unit {
                            "KiB" => 1024,
                            "MiB" => 1024 * 1024,
                            "bytes" => 1,
                            _ => continue,
                        };
                        memory = Some(
                            memory
                                .unwrap_or(0u64)
                                .saturating_add(value.saturating_mul(scale)),
                        );
                    }
                }
            }
        }
    }
    Some(Client {
        key: format!("{}:{}", device.unwrap_or(driver?), id?),
        engines,
        capacities,
        memory,
    })
}
struct CachedClient {
    key: String,
    paths: Vec<PathBuf>,
}
impl CachedClient {
    fn read(&self, buffer: &mut [u8]) -> Option<Client> {
        self.paths
            .iter()
            .find_map(|path| read_client(path, buffer).filter(|client| client.key == self.key))
    }
}
fn read_client(path: &Path, buffer: &mut [u8]) -> Option<Client> {
    let mut file = fs::File::open(path).ok()?;
    let n = file.read(buffer).ok()?;
    if n == buffer.len() {
        return None;
    }
    client(std::str::from_utf8(&buffer[..n]).ok()?)
}
pub struct Gpu {
    enabled: bool,
    cards: Vec<Card>,
    nvidia: Option<super::nvidia::Nvidia>,
    engines: Vec<Engine>,
    clients: Vec<CachedClient>,
    previous: HashMap<(String, String), u64>,
    last: Instant,
    discovery: Instant,
    pmu_error: Option<String>,
}
impl Gpu {
    pub fn new() -> Self {
        let enabled = !std::env::args().any(|arg| arg == "--no-gpu");
        let mut cards = Vec::new();
        for e in fs::read_dir(if enabled {
            "/sys/class/drm"
        } else {
            "/nonexistent/ftop-gpu-disabled"
        })
        .into_iter()
        .flatten()
        .flatten()
        {
            let name = e.file_name().to_string_lossy().into_owned();
            if !name
                .strip_prefix("card")
                .is_some_and(|v| !v.is_empty() && v.chars().all(|c| c.is_ascii_digit()))
            {
                continue;
            }
            let path = e.path();
            let driver = fs::read_link(path.join("device/driver"))
                .ok()
                .and_then(|v| v.file_name().map(|n| n.to_string_lossy().into_owned()))
                .unwrap_or_default();
            cards.push(Card { path, driver });
        }
        let mut this = Self {
            enabled,
            cards,
            nvidia: if enabled {
                super::nvidia::Nvidia::new()
            } else {
                None
            },
            engines: Vec::new(),
            clients: Vec::new(),
            previous: HashMap::new(),
            last: Instant::now(),
            discovery: Instant::now(),
            pmu_error: None,
        };
        if !enabled {
            return this;
        }
        this.open_pmu();
        if this.engines.is_empty()
            && this
                .cards
                .iter()
                .any(|c| matches!(c.driver.as_str(), "i915" | "xe"))
        {
            this.discover_clients();
        }
        this
    }
    fn open_pmu(&mut self) {
        // Intel publishes PMU type/event encoding through sysfs; no hard-coded GPU ID.
        let root = Path::new("/sys/bus/event_source/devices/i915");
        let Some(kind) = number(root.join("type")) else {
            return;
        };
        for e in fs::read_dir(root.join("events"))
            .into_iter()
            .flatten()
            .flatten()
        {
            if !e.file_name().to_string_lossy().ends_with("-busy") {
                continue;
            }
            let Some(config) = fs::read_to_string(e.path()).ok().and_then(|v| {
                v.trim()
                    .strip_prefix("config=0x")
                    .and_then(|v| u64::from_str_radix(v, 16).ok())
            }) else {
                continue;
            };
            // perf_event_attr ABI: type,size,config,...read_format,flags. The
            // zeroed remaining fields request a counting event, not sampling.
            let mut attr = [0u64; 16];
            attr[0] = kind | ((128u64) << 32);
            attr[1] = config;
            attr[4] = 3;
            // SAFETY: Valid attr buffer; system-wide counting on CPU 0, no group.
            let fd = unsafe {
                libc::syscall(
                    libc::SYS_perf_event_open,
                    attr.as_ptr(),
                    -1i32,
                    0i32,
                    -1i32,
                    8u64,
                )
            };
            if fd < 0 {
                self.pmu_error = Some(std::io::Error::last_os_error().to_string());
                break;
            }
            // SAFETY: syscall returns a fresh owned descriptor.
            self.engines.push(Engine {
                file: unsafe { fs::File::from_raw_fd(fd as i32) },
                last: None,
            });
        }
    }
    fn discover_clients(&mut self) {
        self.discovery = Instant::now();
        let mut paths = HashMap::<String, Vec<PathBuf>>::new();
        let mut buffer = [0u8; 16384];
        for process in fs::read_dir("/proc").into_iter().flatten().flatten() {
            if process
                .file_name()
                .to_str()
                .and_then(|v| v.parse::<u32>().ok())
                .is_none()
            {
                continue;
            }
            for fd in fs::read_dir(process.path().join("fd"))
                .into_iter()
                .flatten()
                .flatten()
            {
                if fs::read_link(fd.path())
                    .ok()
                    .is_some_and(|v| v.to_string_lossy().starts_with("/dev/dri/"))
                {
                    let path = process.path().join("fdinfo").join(fd.file_name());
                    if let Some(client) = read_client(&path, &mut buffer) {
                        paths.entry(client.key).or_default().push(path);
                    }
                }
            }
        }
        self.clients = paths
            .into_iter()
            .map(|(key, paths)| CachedClient { key, paths })
            .collect();
    }
    fn pmu(&mut self, elapsed: f64) -> Option<f32> {
        let mut maximum: Option<f64> = None;
        for e in &mut self.engines {
            let mut values = [0u64; 3];
            // SAFETY: read writes at most the fixed 24-byte allocation.
            if unsafe { libc::read(e.file.as_raw_fd(), values.as_mut_ptr().cast(), 24) } != 24 {
                continue;
            }
            let (count, enabled, running) = (values[0], values[1], values[2]);
            if let Some((a, b, c)) = e.last.replace((count, enabled, running)) {
                let run = running.saturating_sub(c);
                if run == 0 {
                    continue;
                }
                let busy =
                    count.saturating_sub(a) as f64 * enabled.saturating_sub(b) as f64 / run as f64;
                let value = busy / elapsed / 1e9 * 100.;
                maximum = Some(maximum.unwrap_or(0.).max(value));
            }
        }
        maximum.map(|v| v.clamp(0., 100.) as f32)
    }
    fn fdinfo(&mut self, elapsed: f64) -> (Option<f32>, Option<u64>) {
        if self.discovery.elapsed().as_secs() >= 10 {
            self.discover_clients();
        }
        let mut current = HashMap::new();
        let mut engines = HashMap::<String, u64>::new();
        let mut memory = None;
        let mut buffer = [0u8; 16384];
        for cached in &self.clients {
            // Read one descriptor per DRM client, not every duplicated/inherited fd.
            // Keep aliases for a cheap fallback when the preferred process exits.
            let Some(c) = cached.read(&mut buffer) else {
                continue;
            };
            if let Some(bytes) = c.memory {
                memory = Some(memory.unwrap_or(0u64).saturating_add(bytes));
            }
            for (engine, value) in c.engines {
                let key = (c.key.clone(), engine.clone());
                if let Some(old) = self.previous.get(&key).filter(|old| **old <= value) {
                    // Include device identity, so independent GPUs are not summed.
                    let device = c.key.rsplit_once(':').map(|(v, _)| v).unwrap_or(&c.key);
                    *engines.entry(format!("{device}:{engine}")).or_default() +=
                        (value - old) / c.capacities.get(&engine).copied().unwrap_or(1);
                }
                current.insert(key, value);
            }
        }
        self.previous = current;
        (
            engines
                .values()
                .max()
                .map(|v| (*v as f64 / elapsed / 1e9 * 100.).clamp(0., 100.) as f32),
            memory,
        )
    }
    pub fn sample(&mut self) -> Snapshot {
        if !self.enabled {
            return Snapshot::default();
        }
        let now = Instant::now();
        let elapsed = now.duration_since(self.last).as_secs_f64().max(0.001);
        self.last = now;
        let mut selected = self.nvidia.as_mut().and_then(|n| n.sample());
        for card in &self.cards {
            let device = card.path.join("device");
            if let Some(usage) = number(device.join("gpu_busy_percent")).filter(|v| *v <= 100) {
                let mut s = Snapshot {
                    gpu_usage: Some(usage as f32),
                    gpu_memory: number(device.join("mem_info_vram_used")),
                    gpu_info: Some(format!(
                        "{} · {} · 内核 gpu_busy_percent",
                        card.driver,
                        card.path.file_name().unwrap().to_string_lossy()
                    )),
                    ..Snapshot::default()
                };
                if let Ok(text) = fs::read_to_string(device.join("pp_dpm_sclk")) {
                    let clocks: Vec<_> = text
                        .lines()
                        .filter_map(|l| {
                            let mut parts = l.split_whitespace();
                            parts.next()?;
                            let mhz = parts
                                .next()?
                                .trim_end_matches(|c: char| c.is_ascii_alphabetic())
                                .parse::<u64>()
                                .ok()?;
                            Some((mhz, l.contains('*')))
                        })
                        .collect();
                    if let (Some(current), Some(maximum)) = (
                        clocks.iter().find(|v| v.1),
                        clocks.iter().map(|v| v.0).max(),
                    ) {
                        s.gpu_frequency = Some((current.0, maximum));
                    }
                }
                for hw in fs::read_dir(device.join("hwmon"))
                    .into_iter()
                    .flatten()
                    .flatten()
                {
                    s.gpu_power = number(hw.path().join("power1_average"))
                        .or_else(|| number(hw.path().join("power1_input")))
                        .map(|v| v as f32 / 1e6);
                    s.gpu_temperature =
                        number(hw.path().join("temp1_input")).map(|v| v as f32 / 1000.);
                }
                if selected
                    .as_ref()
                    .is_none_or(|old| s.gpu_usage.unwrap_or(-1.) > old.gpu_usage.unwrap_or(-1.))
                {
                    selected = Some(s);
                }
            }
        }
        let intel = self
            .cards
            .iter()
            .find(|c| matches!(c.driver.as_str(), "i915" | "xe"))
            .map(|c| c.path.clone());
        if let Some(card) = intel {
            let mut usage = self.pmu(elapsed);
            let mut memory = None;
            let mut info = "Intel GPU · PMU 最忙引擎".to_owned();
            if usage.is_none() && self.engines.is_empty() {
                (usage, memory) = self.fdinfo(elapsed);
                info = "Intel GPU · DRM fdinfo 可访问进程范围估计；GPU 内存为客户端 resident 合计，共享缓冲可能重复".into();
            }
            let current = number(card.join("gt_cur_freq_mhz"))
                .or_else(|| number(card.join("gt/gt0/rps_cur_freq_mhz")))
                .or_else(|| number(card.join("device/tile0/gt0/freq0/cur_freq")));
            let maximum = number(card.join("gt_max_freq_mhz"))
                .or_else(|| number(card.join("gt/gt0/rps_max_freq_mhz")))
                .or_else(|| number(card.join("device/tile0/gt0/freq0/max_freq")));
            let mut gpu_power = None;
            let mut gpu_temperature = None;
            for hw in fs::read_dir(card.join("device/hwmon"))
                .into_iter()
                .flatten()
                .flatten()
            {
                gpu_power = number(hw.path().join("power1_average"))
                    .or_else(|| number(hw.path().join("power1_input")))
                    .map(|v| v as f32 / 1e6);
                gpu_temperature = number(hw.path().join("temp1_input")).map(|v| v as f32 / 1000.);
            }
            if selected
                .as_ref()
                .is_none_or(|old| usage.unwrap_or(-1.) > old.gpu_usage.unwrap_or(-1.))
            {
                selected = Some(Snapshot {
                    gpu_usage: usage,
                    gpu_memory: memory,
                    gpu_frequency: current.zip(maximum),
                    gpu_power,
                    gpu_temperature,
                    gpu_info: Some(if usage.is_some() {
                        info
                    } else if !self.engines.is_empty() {
                        "Intel PMU 等待第二次有效采样".into()
                    } else {
                        format!(
                            "Intel GPU 计数器未开放：PMU {}；DRM fdinfo 无可读计数器",
                            self.pmu_error.as_deref().unwrap_or("不可用")
                        )
                    }),
                    ..Snapshot::default()
                });
            }
        }
        selected.unwrap_or_else(|| Snapshot {
            gpu_info: Some(
                "未发现可用的 GPU 驱动计数器：需要 DRM sysfs、Intel PMU 或 NVIDIA NVML".into(),
            ),
            ..Snapshot::default()
        })
    }
}
#[cfg(test)]
mod tests {
    use super::{CachedClient, client};
    #[test]
    fn cached_client_falls_back_when_descriptor_is_reused() {
        struct Fixture(std::path::PathBuf);
        impl Drop for Fixture {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let root = std::env::temp_dir().join(format!(
            "ftop-drm-alias-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let fixture = Fixture(root);
        let a = fixture.0.join("reused");
        let b = fixture.0.join("alias");
        std::fs::write(
            &a,
            "drm-driver: i915\ndrm-client-id: 99\ndrm-engine-render: 800 ns\n",
        )
        .unwrap();
        std::fs::write(
            &b,
            "drm-driver: i915\ndrm-client-id: 7\ndrm-engine-render: 200 ns\n",
        )
        .unwrap();
        let cached = CachedClient {
            key: "i915:7".into(),
            paths: vec![a, b],
        };
        assert_eq!(
            cached.read(&mut [0u8; 4096]).unwrap().engines["render"],
            200
        );
    }
    #[test]
    fn parse_drm_units_and_identity() {
        let c=client("drm-driver: i915\ndrm-client-id: 9\ndrm-pdev: 0000:00:02.0\ndrm-engine-render: 12000000 ns\ndrm-engine-capacity-render: 2\ndrm-resident-system: 8 KiB\n").unwrap();
        assert_eq!(c.engines["render"], 12000000);
        assert_eq!(c.capacities["render"], 2);
        assert_eq!(c.memory, Some(8192));
        assert!(c.key.ends_with(":9"));
        assert!(client("pos: 0\n").is_none());
    }
}
