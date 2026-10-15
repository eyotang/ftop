//! Metra-style locale resolution, four catalogs, English fallback and live switching.
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    #[default]
    #[serde(rename = "system")]
    System,
    #[serde(rename = "zh-CN")]
    Chinese,
    #[serde(rename = "ja")]
    Japanese,
    #[serde(rename = "ko")]
    Korean,
    #[serde(rename = "en", other)]
    English,
}
impl Language {
    pub const ALL: [Self; 5] = [
        Self::System,
        Self::English,
        Self::Chinese,
        Self::Japanese,
        Self::Korean,
    ];
    pub fn resolve(input: &str) -> Self {
        match input
            .split(['-', '_', '.'])
            .next()
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "zh" => Self::Chinese,
            "ja" => Self::Japanese,
            "ko" => Self::Korean,
            _ => Self::English,
        }
    }
    fn index(self) -> usize {
        let resolved = if self == Self::System {
            static SYSTEM: OnceLock<Language> = OnceLock::new();
            *SYSTEM.get_or_init(|| Self::resolve(&sys_locale::get_locale().unwrap_or_default()))
        } else {
            self
        };
        match resolved {
            Self::Chinese => 1,
            Self::Japanese => 2,
            Self::Korean => 3,
            _ => 0,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::English => "English",
            Self::Chinese => "简体中文",
            Self::Japanese => "日本語",
            Self::Korean => "한국어",
        }
    }
    pub fn text(self, key: &str) -> String {
        let index = self.index();
        // Translate application-owned messages only. Names, units, API identifiers,
        // and unknown driver/OS errors pass through unchanged.
        let rows = catalog();
        if let Some(row) = rows.iter().find(|r| r[0] == key || r[1] == key) {
            return if row[index].is_empty() {
                row[0].clone()
            } else {
                row[index].clone()
            };
        }
        let mut result = String::with_capacity(key.len());
        let mut rest = key;
        while !rest.is_empty() {
            if let Some(row) = rows
                .iter()
                .find(|r| rest.starts_with(&r[0]) || rest.starts_with(&r[1]))
            {
                let len = if rest.starts_with(&row[0]) {
                    row[0].len()
                } else {
                    row[1].len()
                };
                result.push_str(if row[index].is_empty() {
                    &row[0]
                } else {
                    &row[index]
                });
                rest = &rest[len..];
            } else {
                let c = rest.chars().next().unwrap();
                result.push(c);
                rest = &rest[c.len_utf8()..];
            }
        }
        result
    }
}
fn catalog() -> &'static [[String; 4]] {
    static CATALOG: OnceLock<Vec<[String; 4]>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let mut rows: Vec<[String; 4]> = serde_json::from_str(include_str!("locales.json"))
            .expect("validated translation catalog");
        rows.sort_by_key(|r| std::cmp::Reverse(r[0].len().max(r[1].len())));
        rows
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn locale_aliases_and_fallback() {
        for input in ["zh_CN.UTF-8", "zh-Hans-CN", "ZH-tw"] {
            assert_eq!(Language::resolve(input), Language::Chinese);
        }
        assert_eq!(Language::resolve("ja_JP"), Language::Japanese);
        assert_eq!(Language::resolve("ko-KR"), Language::Korean);
        for input in ["de-DE", "", "C.UTF-8"] {
            assert_eq!(Language::resolve(input), Language::English);
        }
    }
    #[test]
    fn application_sensor_messages_have_english_coverage() {
        let sources = [
            include_str!("main.rs"),
            include_str!("metrics/platform/linux.rs"),
            include_str!("metrics/platform/linux_gpu.rs"),
            include_str!("metrics/platform/macos.rs"),
            include_str!("metrics/platform/windows.rs"),
            include_str!("metrics/platform/windows_gpu.rs"),
            include_str!("metrics/platform/nvidia.rs"),
        ];
        let has_han = |text: &str| text.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c));
        for source in sources {
            for literal in source.split('"').skip(1).step_by(2).filter(|s| has_han(s)) {
                assert!(
                    !has_han(&Language::English.text(literal)),
                    "missing translation: {literal}"
                );
            }
        }
    }
    #[test]
    fn catalogs_preserve_values_and_technical_identifiers() {
        for row in catalog() {
            assert!(row.iter().all(|value| !value.is_empty()));
        }
        assert_eq!(Language::Japanese.text("CPU usage 17%"), "CPU 使用率 17%");
        assert_eq!(
            Language::English.text("GPU 内存 1.4 GB"),
            "GPU memory 1.4 GB"
        );
        assert_eq!(Language::Korean.text("Check for updates"), "업데이트 확인");
        assert_eq!(
            Language::English.text("unknown driver 0x8000"),
            "unknown driver 0x8000"
        );
    }
}
