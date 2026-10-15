#[cfg(any(target_os = "macos", target_os = "windows"))]
mod common;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "linux")]
pub use linux::Backend;
#[cfg(target_os = "macos")]
pub use macos::Backend;
#[cfg(target_os = "windows")]
pub use windows::Backend;
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
compile_error!("ftop supports Ubuntu/Linux, macOS and Windows");
