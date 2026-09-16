use std::{collections::BTreeMap, fs, path::PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub remote: BTreeMap<String, RemoteConfig>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RemoteConfig {
    #[serde(default = "default_kind")]
    pub kind: String,
    pub region: Option<String>,
    pub endpoint: Option<String>,
    pub access_key_id: Option<String>,
    pub secret_access_key: Option<String>,
    pub bucket: Option<String>,
    #[serde(default)]
    pub path_style: bool,
}

fn default_kind() -> String {
    "s3".into()
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = config_path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(&path)
            .with_context(|| format!("설정 파일을 읽을 수 없습니다: {}", path.display()))?;
        toml::from_str(&text).context("config.toml 형식이 올바르지 않습니다")
    }
}

pub fn config_path() -> PathBuf {
    if let Some(path) = std::env::var_os("MDIR_CONFIG") {
        return PathBuf::from(path);
    }
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("mdir")
        .join("config.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_r2_profile() {
        let cfg: Config = toml::from_str(
            r#"
            [remote.r2]
            kind = "r2"
            endpoint = "https://example.r2.cloudflarestorage.com"
            access_key_id = "id"
            secret_access_key = "secret"
            bucket = "assets"
        "#,
        )
        .unwrap();
        let r2 = &cfg.remote["r2"];
        assert_eq!(r2.kind, "r2");
        assert_eq!(r2.bucket.as_deref(), Some("assets"));
    }
}
