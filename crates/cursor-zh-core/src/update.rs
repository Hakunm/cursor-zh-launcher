// SPDX-License-Identifier: GPL-3.0-only

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use semver::Version;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::fs_util::atomic_write;

const CHECK_INTERVAL: Duration = Duration::from_hours(24);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(4);

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current_version: Version,
    pub latest_version: Version,
    pub tag_name: String,
    pub release_url: String,
}

#[derive(Clone, Debug)]
pub struct UpdateChecker {
    api_url: String,
    state_path: PathBuf,
    current_version: Version,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateState {
    schema_version: u32,
    last_checked_unix: u64,
    latest_tag: Option<String>,
    release_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
}

impl UpdateChecker {
    pub fn new(
        api_url: impl Into<String>,
        state_path: impl Into<PathBuf>,
        current_version: &str,
    ) -> Result<Self> {
        let api_url = api_url.into();
        validate_api_url(&api_url)?;
        Ok(Self {
            api_url,
            state_path: state_path.into(),
            current_version: Version::parse(current_version)
                .context("launcher version is not valid semver")?,
        })
    }

    pub async fn check_if_due(&self) -> Result<Option<UpdateInfo>> {
        let now = unix_now()?;
        let previous = self.load_state()?;
        if previous.schema_version == 1
            && now.saturating_sub(previous.last_checked_unix) < CHECK_INTERVAL.as_secs()
        {
            return Ok(update_from_state(&self.current_version, &previous));
        }

        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .context("failed to build update client")?;
        let release = client
            .get(&self.api_url)
            .header(
                reqwest::header::USER_AGENT,
                format!("cursor-zh-launcher/{}", self.current_version),
            )
            .send()
            .await
            .context("failed to query GitHub Releases")?
            .error_for_status()
            .context("GitHub Releases returned an error status")?
            .json::<GitHubRelease>()
            .await
            .context("failed to parse GitHub release response")?;
        validate_release_url(&release.html_url)?;
        let state = UpdateState {
            schema_version: 1,
            last_checked_unix: now,
            latest_tag: Some(release.tag_name),
            release_url: Some(release.html_url),
        };
        self.save_state(&state)?;
        Ok(update_from_state(&self.current_version, &state))
    }

    fn load_state(&self) -> Result<UpdateState> {
        match fs::read(&self.state_path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .with_context(|| format!("failed to parse {}", self.state_path.display())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(UpdateState::default())
            }
            Err(error) => {
                Err(error).with_context(|| format!("failed to read {}", self.state_path.display()))
            }
        }
    }

    fn save_state(&self, state: &UpdateState) -> Result<()> {
        let mut bytes = serde_json::to_vec_pretty(state)?;
        bytes.push(b'\n');
        atomic_write(&self.state_path, &bytes)
    }
}

#[must_use]
pub fn default_state_path(local_app_data: &Path) -> PathBuf {
    local_app_data.join("CursorZhLauncher").join("updates.json")
}

fn update_from_state(current: &Version, state: &UpdateState) -> Option<UpdateInfo> {
    let tag_name = state.latest_tag.as_deref()?;
    let release_url = state.release_url.as_deref()?;
    let latest_version = Version::parse(tag_name.trim_start_matches(['v', 'V'])).ok()?;
    (latest_version > *current).then(|| UpdateInfo {
        current_version: current.clone(),
        latest_version,
        tag_name: tag_name.to_string(),
        release_url: release_url.to_string(),
    })
}

fn validate_api_url(value: &str) -> Result<()> {
    let url = Url::parse(value).context("invalid GitHub Releases API URL")?;
    if url.scheme() != "https" || url.host_str() != Some("api.github.com") {
        bail!("update API URL must use https://api.github.com");
    }
    Ok(())
}

fn validate_release_url(value: &str) -> Result<()> {
    let url = Url::parse(value).context("invalid GitHub release URL")?;
    if url.scheme() != "https" || url.host_str() != Some("github.com") {
        bail!("release URL must use https://github.com");
    }
    Ok(())
}

fn unix_now() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before UNIX epoch")?
        .as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_github_only_urls() {
        assert!(validate_api_url("https://api.github.com/repos/a/b/releases/latest").is_ok());
        assert!(validate_api_url("http://api.github.com/repos/a/b/releases/latest").is_err());
        assert!(validate_api_url("https://example.com/releases/latest").is_err());
        assert!(validate_release_url("https://github.com/a/b/releases/tag/v1.0.0").is_ok());
        assert!(validate_release_url("https://example.com/v1.0.0").is_err());
    }

    #[test]
    fn compares_cached_semver() {
        let state = UpdateState {
            schema_version: 1,
            last_checked_unix: 1,
            latest_tag: Some("v1.2.0".to_string()),
            release_url: Some("https://github.com/a/b/releases/tag/v1.2.0".to_string()),
        };
        let update = update_from_state(&Version::new(1, 1, 0), &state).unwrap();
        assert_eq!(update.latest_version, Version::new(1, 2, 0));
        assert!(update_from_state(&Version::new(1, 2, 0), &state).is_none());
    }
}
