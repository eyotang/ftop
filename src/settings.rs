use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub palette: usize,
    pub light: bool,
    pub cores: bool,
    pub memory: bool,
    pub network: bool,
    pub processes: bool,
    pub interval: u64,
    pub motion: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            palette: 0,
            light: false,
            cores: true,
            memory: true,
            network: true,
            processes: true,
            interval: 1,
            motion: false,
        }
    }
}
impl Settings {
    pub fn path() -> std::path::PathBuf {
        dirs::config_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("ftop-gpui")
            .join("config.json")
    }
    pub fn load() -> Self {
        std::fs::read(Self::path())
            .ok()
            .and_then(|s| serde_json::from_slice(&s).ok())
            .unwrap_or_default()
    }
    pub fn save(&self) -> Result<(), String> {
        let path = Self::path();
        std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(
            path,
            serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())
    }
}
