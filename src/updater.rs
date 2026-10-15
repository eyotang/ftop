//! Metra-style bounded release checks. Unsigned/portable builds only open a release page.
use serde::Deserialize;
use std::{io::Read, time::Duration};
const REPOSITORY: &str = "eyotang/ftop";
const LIMIT: u64 = 64 * 1024;
pub const STARTUP_DELAY: Duration = Duration::from_secs(20);
pub const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
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
