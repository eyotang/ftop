use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub language: crate::i18n::Language,
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
            language: crate::i18n::Language::System,
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn language_defaults_for_old_settings_and_round_trips() {
        let old: Settings = serde_json::from_str(r#"{"palette":2,"motion":false}"#).unwrap();
        assert_eq!(old.language, crate::i18n::Language::System);
        assert_eq!(old.palette, 2);
        let unsupported: Settings =
            serde_json::from_str(r#"{"language":"fr","palette":2}"#).unwrap();
        assert_eq!(unsupported.language, crate::i18n::Language::English);
        assert_eq!(unsupported.palette, 2);
        for language in crate::i18n::Language::ALL {
            let settings = Settings {
                language,
                ..old.clone()
            };
            let restored: Settings =
                serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
            assert_eq!(restored.language, language);
            assert_eq!(restored.palette, 2);
        }
    }
}
