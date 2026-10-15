//! Optional driver API. No subprocess, SDK installation, or polling CLI required.
use crate::metrics::Snapshot;
use libloading::Library;
use std::{
    ffi::c_void,
    time::{Duration, Instant},
};
type Device = *mut c_void;
#[repr(C)]
#[derive(Default)]
struct Utilization {
    gpu: u32,
    memory: u32,
}
#[repr(C)]
#[derive(Default)]
struct Memory {
    total: u64,
    free: u64,
    used: u64,
}
struct DeviceState {
    handle: Device,
    index: u32,
    max_mhz: Option<u64>,
    temperature: Option<f32>,
    temperature_at: Option<Instant>,
}
pub struct Nvidia {
    library: Library,
    devices: Vec<DeviceState>,
    discovery: Instant,
}
impl Nvidia {
    pub fn new() -> Option<Self> {
        #[cfg(target_os = "linux")]
        let names = ["libnvidia-ml.so.1"];
        #[cfg(target_os = "windows")]
        let names = ["nvml.dll"];
        for name in names {
            // SAFETY: Load only the vendor runtime by its standard loader name.
            #[cfg(target_os = "linux")]
            let loaded = unsafe { Library::new(name) };
            #[cfg(target_os = "windows")]
            let loaded = unsafe {
                libloading::os::windows::Library::load_with_flags(name, 0x1000).map(Library::from)
            };
            let Ok(library) = loaded else {
                continue;
            };
            // SAFETY: NVML's documented init ABI; no borrowed pointers survive the call.
            let ok = unsafe {
                library
                    .get::<unsafe extern "C" fn() -> u32>(b"nvmlInit_v2\0")
                    .ok()
                    .is_some_and(|f| f() == 0)
            };
            if ok {
                let mut this = Self {
                    library,
                    devices: Vec::new(),
                    discovery: Instant::now(),
                };
                this.discover();
                return Some(this);
            }
        }
        None
    }
    fn discover(&mut self) {
        self.discovery = Instant::now();
        // SAFETY: Public NVML ABI; handles stay within the initialized library lifetime.
        unsafe {
            let Ok(count_fn) = self
                .library
                .get::<unsafe extern "C" fn(*mut u32) -> u32>(b"nvmlDeviceGetCount_v2\0")
            else {
                return;
            };
            let Ok(device_fn) = self
                .library
                .get::<unsafe extern "C" fn(u32, *mut Device) -> u32>(
                    b"nvmlDeviceGetHandleByIndex_v2\0",
                )
            else {
                return;
            };
            let mut count = 0;
            if count_fn(&mut count) != 0 {
                return;
            }
            let mut devices = Vec::new();
            for index in 0..count.min(64) {
                let mut handle = std::ptr::null_mut();
                if device_fn(index, &mut handle) != 0 {
                    continue;
                }
                let mut max_mhz = None;
                if let Ok(f) = self
                    .library
                    .get::<unsafe extern "C" fn(Device, u32, *mut u32) -> u32>(
                        b"nvmlDeviceGetMaxClockInfo\0",
                    )
                {
                    let mut value = 0;
                    if f(handle, 0, &mut value) == 0 {
                        max_mhz = Some(u64::from(value));
                    }
                }
                devices.push(DeviceState {
                    handle,
                    index,
                    max_mhz,
                    temperature: None,
                    temperature_at: None,
                });
            }
            self.devices = devices;
        }
    }
    pub fn sample(&mut self) -> Option<Snapshot> {
        if self.discovery.elapsed() >= Duration::from_secs(30) {
            self.discover();
        }
        // SAFETY: Checked public NVML function signatures and initialized repr(C)
        // buffers. No handle escapes its owning library. Only the selected GPU
        // receives detailed queries; temperature is cached for five seconds.
        unsafe {
            let mut selected = None;
            let mut usage = None;
            if let Ok(f) = self
                .library
                .get::<unsafe extern "C" fn(Device, *mut Utilization) -> u32>(
                    b"nvmlDeviceGetUtilizationRates\0",
                )
            {
                for (i, state) in self.devices.iter().enumerate() {
                    let mut value = Utilization::default();
                    if f(state.handle, &mut value) == 0
                        && value.gpu <= 100
                        && usage.is_none_or(|old| value.gpu > old)
                    {
                        selected = Some(i);
                        usage = Some(value.gpu);
                    }
                }
            }
            let state = self.devices.get_mut(selected.unwrap_or(0))?;
            let device = state.handle;
            let mut s = Snapshot {
                gpu_usage: usage.map(|v| v as f32),
                ..Snapshot::default()
            };
            if let Ok(f) = self
                .library
                .get::<unsafe extern "C" fn(Device, *mut Memory) -> u32>(
                    b"nvmlDeviceGetMemoryInfo\0",
                )
            {
                let mut value = Memory::default();
                if f(device, &mut value) == 0 {
                    s.gpu_memory = Some(value.used);
                }
            }
            if let Ok(f) = self
                .library
                .get::<unsafe extern "C" fn(Device, *mut u32) -> u32>(b"nvmlDeviceGetPowerUsage\0")
            {
                let mut value = 0;
                if f(device, &mut value) == 0 {
                    s.gpu_power = Some(value as f32 / 1000.);
                }
            }
            if let (Some(maximum), Ok(f)) = (
                state.max_mhz,
                self.library
                    .get::<unsafe extern "C" fn(Device, u32, *mut u32) -> u32>(
                        b"nvmlDeviceGetClockInfo\0",
                    ),
            ) {
                let mut value = 0;
                if f(device, 0, &mut value) == 0 {
                    s.gpu_frequency = Some((u64::from(value), maximum));
                }
            }
            if state
                .temperature_at
                .is_none_or(|at| at.elapsed() >= Duration::from_secs(5))
            {
                state.temperature_at = Some(Instant::now());
                state.temperature = None;
                if let Ok(f) = self
                    .library
                    .get::<unsafe extern "C" fn(Device, u32, *mut u32) -> u32>(
                        b"nvmlDeviceGetTemperature\0",
                    )
                {
                    let mut value = 0;
                    if f(device, 0, &mut value) == 0 {
                        state.temperature = Some(value as f32);
                    }
                }
            }
            s.gpu_temperature = state.temperature;
            s.gpu_info = Some(format!(
                "NVIDIA GPU {} · NVML（多卡显示最忙的一张；温度每5秒更新）",
                state.index
            ));
            Some(s)
        }
    }
}
impl Drop for Nvidia {
    fn drop(&mut self) {
        // SAFETY: Release the initialization acquired by new before unloading the library.
        unsafe {
            if let Ok(f) = self
                .library
                .get::<unsafe extern "C" fn() -> u32>(b"nvmlShutdown\0")
            {
                let _ = f();
            }
        }
    }
}
