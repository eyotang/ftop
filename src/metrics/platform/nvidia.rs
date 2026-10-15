//! Optional driver API. No subprocess, SDK installation, or polling CLI required.
use crate::metrics::Snapshot;
use libloading::Library;
use std::ffi::c_void;
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
pub struct Nvidia {
    library: Library,
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
                return Some(Self { library });
            }
        }
        None
    }
    pub fn sample(&self) -> Option<Snapshot> {
        // SAFETY: Function signatures and repr(C) records match the public NVML ABI.
        // Device handles are owned by NVML, remain within its initialized lifetime,
        // and every optional function's status is checked before using its output.
        unsafe {
            let count_fn = self
                .library
                .get::<unsafe extern "C" fn(*mut u32) -> u32>(b"nvmlDeviceGetCount_v2\0")
                .ok()?;
            let device_fn = self
                .library
                .get::<unsafe extern "C" fn(u32, *mut Device) -> u32>(
                    b"nvmlDeviceGetHandleByIndex_v2\0",
                )
                .ok()?;
            let mut count = 0;
            if count_fn(&mut count) != 0 {
                return None;
            }
            let mut selected: Option<Snapshot> = None;
            for index in 0..count.min(64) {
                let mut device = std::ptr::null_mut();
                if device_fn(index, &mut device) != 0 {
                    continue;
                }
                let mut s = Snapshot::default();
                if let Ok(f) = self
                    .library
                    .get::<unsafe extern "C" fn(Device, *mut Utilization) -> u32>(
                        b"nvmlDeviceGetUtilizationRates\0",
                    )
                {
                    let mut v = Utilization::default();
                    if f(device, &mut v) == 0 && v.gpu <= 100 {
                        s.gpu_usage = Some(v.gpu as f32);
                    }
                }
                if let Ok(f) = self
                    .library
                    .get::<unsafe extern "C" fn(Device, *mut Memory) -> u32>(
                        b"nvmlDeviceGetMemoryInfo\0",
                    )
                {
                    let mut v = Memory::default();
                    if f(device, &mut v) == 0 {
                        s.gpu_memory = Some(v.used);
                    }
                }
                if let Ok(f) = self
                    .library
                    .get::<unsafe extern "C" fn(Device, *mut u32) -> u32>(
                        b"nvmlDeviceGetPowerUsage\0",
                    )
                {
                    let mut v = 0;
                    if f(device, &mut v) == 0 {
                        s.gpu_power = Some(v as f32 / 1000.);
                    }
                }
                if let (Ok(current), Ok(maximum)) = (
                    self.library
                        .get::<unsafe extern "C" fn(Device, u32, *mut u32) -> u32>(
                            b"nvmlDeviceGetClockInfo\0",
                        ),
                    self.library
                        .get::<unsafe extern "C" fn(Device, u32, *mut u32) -> u32>(
                            b"nvmlDeviceGetMaxClockInfo\0",
                        ),
                ) {
                    let (mut a, mut b) = (0, 0);
                    if current(device, 0, &mut a) == 0 && maximum(device, 0, &mut b) == 0 {
                        s.gpu_frequency = Some((a as u64, b as u64));
                    }
                }
                if let Ok(f) = self
                    .library
                    .get::<unsafe extern "C" fn(Device, u32, *mut u32) -> u32>(
                        b"nvmlDeviceGetTemperature\0",
                    )
                {
                    let mut v = 0;
                    if f(device, 0, &mut v) == 0 {
                        s.gpu_temperature = Some(v as f32);
                    }
                }
                s.gpu_info = Some(format!("NVIDIA GPU {index} · NVML（多卡显示最忙的一张）"));
                if selected
                    .as_ref()
                    .is_none_or(|old| s.gpu_usage.unwrap_or(-1.) > old.gpu_usage.unwrap_or(-1.))
                {
                    selected = Some(s);
                }
            }
            selected
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
