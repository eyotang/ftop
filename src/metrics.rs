mod platform;
pub use platform::Backend as Collector;
#[derive(Clone, Debug)]
pub struct Core {
    pub usage: f32,
    pub mhz: u64,
    pub max_mhz: u64,
    pub id: u32,
    pub group: usize,
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
    pub compressed: Option<u64>,
    pub pressure: Option<u32>,
    pub memory_pressure_info: Option<String>,
    pub gpu_usage: Option<f32>,
    pub gpu_info: Option<String>,
    pub gpu_temperature: Option<f32>,
    pub power_label: Option<String>,
    pub compressed_label: Option<String>,
    pub capabilities: Vec<(String, String)>,
    pub gpu_memory: Option<u64>,
    pub gpu_frequency: Option<(u64, u64)>,
    pub gpu_power: Option<f32>,
    pub power: Option<f32>,
    pub swap_label: &'static str,
    pub error: Option<String>,
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
