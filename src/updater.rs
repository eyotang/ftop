//! Metra-style bounded release checks. Unsigned/portable builds only open a release page.
use serde::Deserialize;
use std::{io::Read, time::Duration};
const REPOSITORY: &str = "eyotang/ftop";
const LIMIT: u64 = 64 * 1024;
pub const STARTUP_DELAY: Duration = Duration::from_secs(20);
pub const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
/// Notifications from the bounded background worker; never poll on the UI thread.
pub enum Event {
    Checking,
    Finished {
        manual: bool,
        result: Result<Option<String>, String>,
    },
}
#[derive(Default)]
pub struct Status {
    pub checking: bool,
    pub version: Option<String>,
    pub message: Option<String>,
    dismissed: Option<String>,
}
impl Status {
    pub fn begin_manual(&mut self) -> bool {
        if self.checking {
            return false;
        }
        self.checking = true;
        self.dismissed = None;
        self.message = Some("Checking…".into());
        true
    }
    pub fn apply(&mut self, event: Event) {
        match event {
            Event::Checking => self.checking = true,
            Event::Finished { manual, result } => {
                self.checking = false;
                match result {
                    Ok(Some(version)) => {
                        self.message =
                            Some(format!("Version {version} available · manual download"));
                        self.version = Some(version);
                    }
                    Ok(None) => {
                        self.message = Some("Up to date".into());
                        self.version = None;
                        self.dismissed = None;
                    }
                    Err(error) if manual => {
                        self.message = Some(format!("Update check failed: {error}"))
                    }
                    Err(_) => {} // Quiet automatic failure; retain a previously detected update.
                }
            }
        }
    }
    pub fn visible_version(&self) -> Option<&str> {
        self.version
            .as_deref()
            .filter(|version| self.dismissed.as_deref() != Some(*version))
    }
    pub fn dismiss(&mut self) {
        self.dismissed.clone_from(&self.version);
    }
}
#[derive(Debug, Deserialize)]
struct Manifest {
    version: String,
}
fn stable_version(input: &str) -> Result<[u64; 3], String> {
    let parts: Vec<_> = input.split('.').collect();
    if parts.len() != 3 {
        return Err("Expected a stable major.minor.patch version".into());
    }
    let mut version = [0; 3];
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty()
            || (part.len() > 1 && part.starts_with('0'))
            || !part.bytes().all(|b| b.is_ascii_digit())
        {
            return Err("Invalid stable version".into());
        }
        version[i] = part.parse().map_err(|_| "Invalid version number")?;
    }
    Ok(version)
}
pub fn parse(bytes: &[u8], current: &str) -> Result<Option<String>, String> {
    if bytes.len() as u64 > LIMIT {
        return Err("Update manifest exceeds 64 KiB".into());
    }
    let manifest: Manifest = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    Ok((stable_version(&manifest.version)? > stable_version(current)?).then_some(manifest.version))
}
pub fn release_page(version: &str) -> Result<String, String> {
    stable_version(version)?;
    Ok(format!(
        "https://github.com/{REPOSITORY}/releases/tag/v{version}"
    ))
}
pub fn check() -> Result<Option<String>, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .https_only(true)
        .user_agent(concat!("ftop/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| e.to_string())?;
    let response = client
        .get(format!(
            "https://github.com/{REPOSITORY}/releases/latest/download/latest.json"
        ))
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| e.to_string())?;
    if response.content_length().is_some_and(|len| len > LIMIT) {
        return Err("Update manifest exceeds 64 KiB".into());
    }
    let mut bytes = Vec::new();
    response
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    parse(&bytes, env!("CARGO_PKG_VERSION"))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn update_interaction_is_bounded_and_retains_known_updates() {
        let mut state = Status::default();
        assert!(state.begin_manual());
        assert!(!state.begin_manual());
        state.apply(Event::Checking);
        state.apply(Event::Finished {
            manual: true,
            result: Ok(Some("0.1.2".into())),
        });
        assert!(!state.checking);
        assert_eq!(state.visible_version(), Some("0.1.2"));
        state.dismiss();
        assert_eq!(state.visible_version(), None);
        state.apply(Event::Finished {
            manual: false,
            result: Err("offline".into()),
        });
        assert_eq!(state.version.as_deref(), Some("0.1.2"));
        assert!(state.begin_manual());
        assert_eq!(state.visible_version(), Some("0.1.2"));
        state.apply(Event::Finished {
            manual: true,
            result: Err("offline".into()),
        });
        assert!(!state.checking);
        assert!(state.message.as_deref().unwrap().contains("offline"));
        state.apply(Event::Finished {
            manual: false,
            result: Ok(Some("0.1.3".into())),
        });
        assert_eq!(state.visible_version(), Some("0.1.3"));
        state.apply(Event::Finished {
            manual: true,
            result: Ok(None),
        });
        assert_eq!(state.visible_version(), None);
        assert_eq!(state.message.as_deref(), Some("Up to date"));
    }
    #[test]
    fn stable_order_and_no_downgrade() {
        assert_eq!(
            parse(br#"{"version":"0.10.0"}"#, "0.9.99").unwrap(),
            Some("0.10.0".into())
        );
        assert_eq!(parse(br#"{"version":"0.1.0"}"#, "0.2.0").unwrap(), None);
    }
    #[test]
    fn rejects_untrusted_release_tags() {
        for v in [
            "1.0.0-beta",
            "01.0.0",
            "1.0.0/../../settings",
            "1.0",
            "1.0.18446744073709551616",
        ] {
            assert!(release_page(v).is_err());
        }
        assert!(parse(&vec![b' '; 65537], "0.1.0").is_err());
    }
}
