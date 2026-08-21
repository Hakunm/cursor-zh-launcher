// SPDX-License-Identifier: GPL-3.0-only

use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CursorInstallation {
    pub executable: PathBuf,
    pub install_root: PathBuf,
    pub app_root: PathBuf,
    pub product_json: PathBuf,
    pub package_json: PathBuf,
    pub nls_keys_json: PathBuf,
    pub nls_messages_json: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CursorMetadata {
    pub cursor_version: String,
    pub vscode_version: String,
    pub commit: String,
    pub application_name: String,
    pub data_folder_name: String,
    pub win32_app_user_model_id: Option<String>,
    pub url_protocol: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProductJson {
    #[serde(default)]
    commit: String,
    #[serde(default, rename = "vscodeVersion")]
    vscode_version: String,
    #[serde(default, rename = "applicationName")]
    application_name: String,
    #[serde(default, rename = "dataFolderName")]
    data_folder_name: String,
    #[serde(default, rename = "win32AppUserModelId")]
    win32_app_user_model_id: Option<String>,
    #[serde(default, rename = "urlProtocol")]
    url_protocol: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PackageJson {
    #[serde(default)]
    version: String,
}

impl CursorInstallation {
    pub fn from_executable(executable: impl AsRef<Path>) -> Result<Self> {
        let executable = executable.as_ref();
        if !executable.is_file() {
            bail!("Cursor executable not found: {}", executable.display());
        }
        let executable = executable
            .canonicalize()
            .with_context(|| format!("failed to resolve {}", executable.display()))?;
        let install_root = executable
            .parent()
            .context("Cursor executable has no parent directory")?
            .to_path_buf();
        let app_root = install_root.join("resources").join("app");
        let installation = Self {
            executable,
            install_root,
            product_json: app_root.join("product.json"),
            package_json: app_root.join("package.json"),
            nls_keys_json: app_root.join("out").join("nls.keys.json"),
            nls_messages_json: app_root.join("out").join("nls.messages.json"),
            app_root,
        };
        installation.validate()?;
        Ok(installation)
    }

    pub fn validate(&self) -> Result<()> {
        for (label, path) in [
            ("product.json", &self.product_json),
            ("package.json", &self.package_json),
            ("nls.keys.json", &self.nls_keys_json),
            ("nls.messages.json", &self.nls_messages_json),
        ] {
            if !path.is_file() {
                bail!("Cursor {label} not found: {}", path.display());
            }
        }
        Ok(())
    }

    pub fn metadata(&self) -> Result<CursorMetadata> {
        let product: ProductJson = read_json(&self.product_json)?;
        let package: PackageJson = read_json(&self.package_json)?;
        for (label, value) in [
            ("Cursor version", package.version.as_str()),
            ("VS Code version", product.vscode_version.as_str()),
            ("Cursor commit", product.commit.as_str()),
            ("application name", product.application_name.as_str()),
            ("data folder name", product.data_folder_name.as_str()),
        ] {
            if value.trim().is_empty() {
                bail!("{label} is missing from Cursor metadata");
            }
        }
        Ok(CursorMetadata {
            cursor_version: package.version,
            vscode_version: product.vscode_version,
            commit: product.commit,
            application_name: product.application_name,
            data_folder_name: product.data_folder_name,
            win32_app_user_model_id: product.win32_app_user_model_id,
            url_protocol: product.url_protocol,
        })
    }
}

pub fn detect_cursor(explicit: Option<&Path>) -> Result<CursorInstallation> {
    let mut candidates = Vec::new();
    if let Some(explicit) = explicit {
        candidates.push(normalize_candidate(explicit));
    }
    candidates.extend(registry_candidates());
    candidates.extend(default_candidates());
    candidates.extend(path_candidates());

    let mut seen = HashSet::new();
    let mut errors = Vec::new();
    for candidate in candidates {
        let key = candidate.to_string_lossy().to_ascii_lowercase();
        if !seen.insert(key) {
            continue;
        }
        match CursorInstallation::from_executable(&candidate) {
            Ok(installation) => return Ok(installation),
            Err(error) => errors.push(error.to_string()),
        }
    }
    bail!(
        "no valid Cursor installation found{}",
        if errors.is_empty() {
            String::new()
        } else {
            format!(": {}", errors.join("; "))
        }
    )
}

fn read_json<T>(path: &Path) -> Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("failed to parse {}", path.display()))
}

fn normalize_candidate(path: &Path) -> PathBuf {
    if path.is_dir() {
        path.join("Cursor.exe")
    } else {
        path.to_path_buf()
    }
}

fn default_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(local_app_data) = env::var_os("LOCALAPPDATA") {
        candidates.push(
            PathBuf::from(local_app_data)
                .join("Programs")
                .join("cursor")
                .join("Cursor.exe"),
        );
    }
    if let Some(program_files) = env::var_os("ProgramFiles") {
        candidates.push(
            PathBuf::from(program_files)
                .join("Cursor")
                .join("Cursor.exe"),
        );
    }
    if let Some(program_files_x86) = env::var_os("ProgramFiles(x86)") {
        candidates.push(
            PathBuf::from(program_files_x86)
                .join("Cursor")
                .join("Cursor.exe"),
        );
    }
    candidates
}

fn path_candidates() -> Vec<PathBuf> {
    env::var_os("PATH")
        .map(|path| {
            env::split_paths(&path)
                .map(|directory| directory.join("Cursor.exe"))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(windows)]
fn registry_candidates() -> Vec<PathBuf> {
    use winreg::RegKey;
    use winreg::enums::{
        HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
    };

    let roots = [
        (RegKey::predef(HKEY_CURRENT_USER), KEY_READ),
        (
            RegKey::predef(HKEY_LOCAL_MACHINE),
            KEY_READ | KEY_WOW64_64KEY,
        ),
        (
            RegKey::predef(HKEY_LOCAL_MACHINE),
            KEY_READ | KEY_WOW64_32KEY,
        ),
    ];
    let mut candidates = Vec::new();
    for (root, flags) in roots {
        let Ok(uninstall) = root.open_subkey_with_flags(
            r"Software\Microsoft\Windows\CurrentVersion\Uninstall",
            flags,
        ) else {
            continue;
        };
        for key_name in uninstall.enum_keys().flatten() {
            let Ok(key) = uninstall.open_subkey_with_flags(&key_name, KEY_READ) else {
                continue;
            };
            let display_name: String = key.get_value("DisplayName").unwrap_or_default();
            let publisher: String = key.get_value("Publisher").unwrap_or_default();
            if !display_name.starts_with("Cursor") && !publisher.eq_ignore_ascii_case("Anysphere") {
                continue;
            }
            let install_location: String = key.get_value("InstallLocation").unwrap_or_default();
            if !install_location.trim().is_empty() {
                candidates.push(PathBuf::from(install_location).join("Cursor.exe"));
            }
        }
    }
    candidates
}

#[cfg(not(windows))]
fn registry_candidates() -> Vec<PathBuf> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_installation(root: &Path) -> PathBuf {
        let executable = root.join("Cursor.exe");
        let app_root = root.join("resources").join("app");
        fs::create_dir_all(app_root.join("out")).unwrap();
        fs::write(&executable, b"fake").unwrap();
        fs::write(
            app_root.join("product.json"),
            br#"{"commit":"abc","vscodeVersion":"1.2.3","applicationName":"cursor","dataFolderName":".cursor","win32AppUserModelId":"Anysphere.Cursor","urlProtocol":"cursor"}"#,
        )
        .unwrap();
        fs::write(app_root.join("package.json"), br#"{"version":"9.8.7"}"#).unwrap();
        fs::write(app_root.join("out/nls.keys.json"), b"[]").unwrap();
        fs::write(app_root.join("out/nls.messages.json"), b"[]").unwrap();
        executable
    }

    #[test]
    fn validates_and_reads_metadata() {
        let directory = tempfile::tempdir().unwrap();
        let executable = fake_installation(directory.path());
        let installation = CursorInstallation::from_executable(executable).unwrap();
        let metadata = installation.metadata().unwrap();
        assert_eq!(metadata.cursor_version, "9.8.7");
        assert_eq!(metadata.vscode_version, "1.2.3");
        assert_eq!(metadata.commit, "abc");
    }

    #[test]
    fn rejects_incomplete_installation() {
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("Cursor.exe");
        fs::write(&executable, b"fake").unwrap();
        let error = CursorInstallation::from_executable(executable).unwrap_err();
        assert!(error.to_string().contains("product.json"));
    }
}
