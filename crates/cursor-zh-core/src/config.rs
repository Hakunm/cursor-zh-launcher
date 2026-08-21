// SPDX-License-Identifier: GPL-3.0-only

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::CONFIG_SCHEMA_VERSION;
use crate::fs_util::atomic_write;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[allow(clippy::struct_excessive_bools)]
pub struct Config {
    pub schema_version: u32,
    pub cursor_path: Option<PathBuf>,
    pub renderer_injection: bool,
    pub native_menu_injection: bool,
    pub update_check: bool,
    pub untranslated_collector: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            cursor_path: None,
            renderer_injection: true,
            native_menu_injection: true,
            update_check: true,
            untranslated_collector: false,
        }
    }
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != CONFIG_SCHEMA_VERSION {
            bail!(
                "unsupported config schema {}, expected {}",
                self.schema_version,
                CONFIG_SCHEMA_VERSION
            );
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn default_for_current_user() -> Result<Self> {
        let app_data = env::var_os("APPDATA").context("APPDATA is not set")?;
        Ok(Self::new(
            PathBuf::from(app_data)
                .join("CursorZhLauncher")
                .join("config.json"),
        ))
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load_or_default(&self) -> Result<Config> {
        if !self.path.exists() {
            return Ok(Config::default());
        }
        let bytes = fs::read(&self.path)
            .with_context(|| format!("failed to read {}", self.path.display()))?;
        let config: Config = serde_json::from_slice(&bytes)
            .with_context(|| format!("failed to parse {}", self.path.display()))?;
        config.validate()?;
        Ok(config)
    }

    pub fn save(&self, config: &Config) -> Result<()> {
        config.validate()?;
        let mut bytes = serde_json::to_vec_pretty(config)?;
        bytes.push(b'\n');
        atomic_write(&self.path, &bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_config_uses_defaults() {
        let directory = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(directory.path().join("config.json"));
        assert_eq!(store.load_or_default().unwrap(), Config::default());
    }

    #[test]
    fn config_round_trip_and_replace_are_stable() {
        let directory = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(directory.path().join("nested/config.json"));
        let mut config = Config {
            update_check: false,
            ..Config::default()
        };
        store.save(&config).unwrap();
        assert_eq!(store.load_or_default().unwrap(), config);

        config.native_menu_injection = false;
        store.save(&config).unwrap();
        assert_eq!(store.load_or_default().unwrap(), config);
    }

    #[test]
    fn rejects_future_schema() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.json");
        fs::write(&path, br#"{"schemaVersion":99}"#).unwrap();
        let error = ConfigStore::new(path).load_or_default().unwrap_err();
        assert!(error.to_string().contains("unsupported config schema"));
    }
}
