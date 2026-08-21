// SPDX-License-Identifier: GPL-3.0-only

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::cursor::CursorInstallation;
use crate::fs_util::{atomic_write, read_optional, sha256_bytes, sha256_file};
use crate::translation::{ReviewStatus, TranslationCatalog};

pub const OFFICIAL_LANGUAGE_PACK_ID: &str = "MS-CEINTL.vscode-language-pack-zh-hans";

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguagePackStatus {
    pub languagepacks_json: PathBuf,
    pub locale_json: PathBuf,
    pub active_main_i18n: Option<PathBuf>,
    pub managed_overlay: Option<PathBuf>,
    pub locale: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareReport {
    pub official_main_i18n: PathBuf,
    pub overlay_main_i18n: PathBuf,
    pub applied_entries: usize,
    pub missing_entries: Vec<String>,
    pub source_mismatches: Vec<String>,
    pub ambiguous_entries: Vec<String>,
    pub extension_files: BTreeMap<String, PathBuf>,
    pub missing_extension_entries: Vec<String>,
    pub extension_source_mismatches: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreReport {
    pub restored: Vec<PathBuf>,
    pub skipped_changed: Vec<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct LanguagePackManager {
    cursor_user_data: PathBuf,
    launcher_data: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct OfficialMainI18n {
    version: String,
    contents: BTreeMap<String, BTreeMap<String, String>>,
}

#[derive(Clone, Debug, Default)]
struct NlsSourceIndex {
    values: HashMap<(String, String), BTreeSet<String>>,
}

#[derive(Debug, Deserialize)]
struct ExtensionPackage {
    publisher: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct LanguagePackPackage {
    version: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackupManifest {
    schema_version: u32,
    files: Vec<ManagedFile>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManagedFile {
    label: String,
    path: PathBuf,
    original_existed: bool,
    original_sha256: Option<String>,
    backup_path: Option<PathBuf>,
    last_written_sha256: Option<String>,
}

impl LanguagePackManager {
    pub fn new(cursor_user_data: impl Into<PathBuf>, launcher_data: impl Into<PathBuf>) -> Self {
        Self {
            cursor_user_data: cursor_user_data.into(),
            launcher_data: launcher_data.into(),
        }
    }

    pub fn default_for_current_user() -> Result<Self> {
        let app_data = env::var_os("APPDATA").context("APPDATA is not set")?;
        let local_app_data = env::var_os("LOCALAPPDATA").context("LOCALAPPDATA is not set")?;
        Ok(Self::new(
            PathBuf::from(app_data).join("Cursor"),
            PathBuf::from(local_app_data).join("CursorZhLauncher"),
        ))
    }

    #[must_use]
    pub fn languagepacks_path(&self) -> PathBuf {
        self.cursor_user_data.join("languagepacks.json")
    }

    #[must_use]
    pub fn locale_path(&self) -> PathBuf {
        self.cursor_user_data.join("User").join("locale.json")
    }

    pub fn inspect(&self) -> Result<LanguagePackStatus> {
        let languagepacks_path = self.languagepacks_path();
        let languagepacks = read_optional_json(&languagepacks_path)?;
        let active_main_i18n = languagepacks
            .as_ref()
            .and_then(|value| translation_path(value).ok());
        let managed_overlay = active_main_i18n
            .as_ref()
            .filter(|path| path.starts_with(self.launcher_data.join("language-packs")))
            .cloned();
        let locale = read_optional_json(&self.locale_path())?.and_then(|value| {
            value
                .get("locale")
                .and_then(Value::as_str)
                .map(str::to_string)
        });
        Ok(LanguagePackStatus {
            languagepacks_json: languagepacks_path,
            locale_json: self.locale_path(),
            active_main_i18n,
            managed_overlay,
            locale,
        })
    }

    pub fn ensure_official_pack(&self, installation: &CursorInstallation) -> Result<PathBuf> {
        let metadata = installation.metadata()?;
        if let Ok(path) = self.current_translation_path()
            && path.is_file()
            && !path.starts_with(self.launcher_data.join("language-packs"))
            && official_pack_matches(&path, &metadata.vscode_version)
        {
            return Ok(path);
        }
        let backup = BackupManager::load(self.launcher_data.join("backups"))?;
        if let Some(bytes) = backup.original_bytes("languagepacks")? {
            let original: Value = serde_json::from_slice(&bytes)
                .context("failed to parse original languagepacks backup")?;
            if let Ok(path) = translation_path(&original)
                && path.is_file()
                && official_pack_matches(&path, &metadata.vscode_version)
            {
                return Ok(path);
            }
        }
        if let Some(path) =
            find_installed_official_pack(&metadata.data_folder_name, &metadata.vscode_version)?
        {
            return Ok(path);
        }
        let output = Command::new(&installation.executable)
            .arg("--install-extension")
            .arg(OFFICIAL_LANGUAGE_PACK_ID)
            .arg("--force")
            .arg(format!(
                "--user-data-dir={}",
                self.cursor_user_data.to_string_lossy()
            ))
            .output()
            .context("failed to start Cursor CLI for the official language pack")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!(
                "official language pack installation failed with {}: {}",
                output.status,
                stderr.trim()
            );
        }
        let path = self
            .current_translation_path()
            .ok()
            .filter(|path| {
                path.is_file()
                    && !path.starts_with(self.launcher_data.join("language-packs"))
                    && official_pack_matches(path, &metadata.vscode_version)
            })
            .or(find_installed_official_pack(
                &metadata.data_folder_name,
                &metadata.vscode_version,
            )?)
            .with_context(|| {
                format!(
                    "official language pack installed but no main.i18n.json matches VS Code {}",
                    metadata.vscode_version
                )
            })?;
        Ok(path)
    }

    pub fn prepare(
        &self,
        installation: &CursorInstallation,
        catalog: &TranslationCatalog,
    ) -> Result<PrepareReport> {
        let official_main_i18n = self.ensure_official_pack(installation)?;
        let official: OfficialMainI18n = read_json(&official_main_i18n)?;
        let metadata = installation.metadata()?;
        let source_index =
            NlsSourceIndex::load(&installation.nls_keys_json, &installation.nls_messages_json)?;
        let (composite, merge) = merge_nls(official, &source_index, catalog);
        let overlay_main_i18n = self
            .launcher_data
            .join("language-packs")
            .join(&metadata.vscode_version)
            .join("main.i18n.json");
        let mut overlay_bytes = serde_json::to_vec(&composite)?;
        overlay_bytes.push(b'\n');
        atomic_write(&overlay_main_i18n, &overlay_bytes)?;

        let extension_result = prepare_extension_overlays(
            installation,
            catalog,
            overlay_main_i18n
                .parent()
                .context("NLS overlay path has no parent")?,
        )?;

        let languagepacks_path = self.languagepacks_path();
        let mut languagepacks = read_json::<Value>(&languagepacks_path)?;
        set_translation_path(&mut languagepacks, &overlay_main_i18n)?;
        for (extension_id, path) in &extension_result.files {
            set_named_translation_path(&mut languagepacks, extension_id, path)?;
        }
        let mut languagepacks_bytes = serde_json::to_vec(&languagepacks)?;
        languagepacks_bytes.push(b'\n');

        let mut backup = BackupManager::load(self.launcher_data.join("backups"))?;
        backup.write_managed("languagepacks", &languagepacks_path, &languagepacks_bytes)?;
        backup.write_managed(
            "locale",
            &self.locale_path(),
            b"{\n  \"locale\": \"zh-cn\"\n}\n",
        )?;
        let active_path = self.current_translation_path()?;
        if active_path != overlay_main_i18n || !active_path.is_file() {
            bail!(
                "language pack overlay write verification failed: {}",
                active_path.display()
            );
        }
        let locale = read_json::<Value>(&self.locale_path())?;
        if locale.get("locale").and_then(Value::as_str) != Some("zh-cn") {
            bail!("locale write verification failed");
        }

        Ok(PrepareReport {
            official_main_i18n,
            overlay_main_i18n,
            applied_entries: merge.applied_entries,
            missing_entries: merge.missing_entries,
            source_mismatches: merge.source_mismatches,
            ambiguous_entries: merge.ambiguous_entries,
            extension_files: extension_result.files,
            missing_extension_entries: extension_result.missing_entries,
            extension_source_mismatches: extension_result.source_mismatches,
        })
    }

    pub fn restore(&self) -> Result<RestoreReport> {
        BackupManager::load(self.launcher_data.join("backups"))?.restore()
    }

    fn current_translation_path(&self) -> Result<PathBuf> {
        let languagepacks: Value = read_json(&self.languagepacks_path())?;
        translation_path(&languagepacks)
    }
}

fn official_pack_matches(path: &Path, expected_version: &str) -> bool {
    let Ok(document) = read_json::<OfficialMainI18n>(path) else {
        return false;
    };
    if document.version == expected_version {
        return true;
    }
    let Some(extension_root) = path.parent().and_then(Path::parent) else {
        return false;
    };
    read_json::<LanguagePackPackage>(&extension_root.join("package.json")).is_ok_and(|package| {
        matches!(
            (major_minor(&package.version), major_minor(expected_version)),
            (Some(actual), Some(expected)) if actual == expected
        )
    })
}

fn major_minor(version: &str) -> Option<(&str, &str)> {
    let mut parts = version.split('.');
    Some((parts.next()?, parts.next()?))
}

fn find_installed_official_pack(
    data_folder_name: &str,
    expected_version: &str,
) -> Result<Option<PathBuf>> {
    let Some(user_profile) = env::var_os("USERPROFILE") else {
        return Ok(None);
    };
    let extensions_root = PathBuf::from(user_profile)
        .join(data_folder_name)
        .join("extensions");
    if !extensions_root.is_dir() {
        return Ok(None);
    }
    let mut candidates = fs::read_dir(&extensions_root)
        .with_context(|| format!("failed to read {}", extensions_root.display()))?
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .to_ascii_lowercase()
                .starts_with("ms-ceintl.vscode-language-pack-zh-hans-")
        })
        .map(|entry| entry.path().join("translations").join("main.i18n.json"))
        .filter(|path| official_pack_matches(path, expected_version))
        .collect::<Vec<_>>();
    candidates.sort();
    Ok(candidates.pop())
}

#[derive(Default)]
struct ExtensionOverlayResult {
    files: BTreeMap<String, PathBuf>,
    missing_entries: Vec<String>,
    source_mismatches: Vec<String>,
}

fn prepare_extension_overlays(
    installation: &CursorInstallation,
    catalog: &TranslationCatalog,
    output_root: &Path,
) -> Result<ExtensionOverlayResult> {
    let mut result = ExtensionOverlayResult::default();
    let mut grouped = BTreeMap::<String, Vec<_>>::new();
    for entry in catalog
        .extensions
        .iter()
        .filter(|entry| entry.status == ReviewStatus::Reviewed)
    {
        grouped
            .entry(entry.extension_id.clone())
            .or_default()
            .push(entry);
    }
    if grouped.is_empty() {
        return Ok(result);
    }

    let extensions_root = installation.app_root.join("extensions");
    let mut installed = HashMap::<String, PathBuf>::new();
    if extensions_root.is_dir() {
        for directory in fs::read_dir(&extensions_root)
            .with_context(|| format!("failed to read {}", extensions_root.display()))?
            .flatten()
            .filter(|entry| entry.path().is_dir())
        {
            let package_path = directory.path().join("package.json");
            if let Ok(package) = read_json::<ExtensionPackage>(&package_path) {
                installed.insert(
                    format!("{}.{}", package.publisher, package.name).to_ascii_lowercase(),
                    directory.path(),
                );
            }
        }
    }

    for (extension_id, entries) in grouped {
        let Some(directory) = installed.get(&extension_id.to_ascii_lowercase()) else {
            result
                .missing_entries
                .extend(entries.iter().map(|entry| entry.id.clone()));
            continue;
        };
        let package_nls_path = directory.join("package.nls.json");
        let package_nls = read_json::<Value>(&package_nls_path)?;
        let mut contents = serde_json::Map::new();
        for entry in entries {
            match package_nls.get(&entry.key).and_then(Value::as_str) {
                Some(source) if source == entry.source => {
                    contents.insert(entry.key.clone(), Value::String(entry.target.clone()));
                }
                Some(_) => result.source_mismatches.push(entry.id.clone()),
                None => result.missing_entries.push(entry.id.clone()),
            }
        }
        if contents.is_empty() {
            continue;
        }
        let extension_file = output_root
            .join("extensions")
            .join(format!("{extension_id}.i18n.json"));
        let document = json!({
            "": ["Generated locally by Cursor 中文启动器 from reviewed clean-room translations."],
            "version": "1.0.0",
            "contents": { "package": contents }
        });
        let mut bytes = serde_json::to_vec(&document)?;
        bytes.push(b'\n');
        atomic_write(&extension_file, &bytes)?;
        result.files.insert(extension_id, extension_file);
    }
    Ok(result)
}

#[derive(Default)]
struct MergeReport {
    applied_entries: usize,
    missing_entries: Vec<String>,
    source_mismatches: Vec<String>,
    ambiguous_entries: Vec<String>,
}

fn merge_nls(
    mut official: OfficialMainI18n,
    source_index: &NlsSourceIndex,
    catalog: &TranslationCatalog,
) -> (OfficialMainI18n, MergeReport) {
    let mut report = MergeReport::default();
    for entry in catalog
        .nls
        .iter()
        .filter(|entry| entry.status == ReviewStatus::Reviewed)
    {
        let location = (entry.module.clone(), entry.key.clone());
        let Some(sources) = source_index.values.get(&location) else {
            report.missing_entries.push(entry.id.clone());
            continue;
        };
        if sources.len() != 1 {
            report.ambiguous_entries.push(entry.id.clone());
            continue;
        }
        if !sources.contains(&entry.source) {
            report.source_mismatches.push(entry.id.clone());
            continue;
        }
        official
            .contents
            .entry(entry.module.clone())
            .or_default()
            .insert(entry.key.clone(), entry.target.clone());
        report.applied_entries += 1;
    }
    (official, report)
}

impl NlsSourceIndex {
    fn load(keys_path: &Path, messages_path: &Path) -> Result<Self> {
        let groups: Vec<(String, Vec<String>)> = read_json(keys_path)?;
        let messages: Vec<String> = read_json(messages_path)?;
        let expected = groups.iter().map(|(_, keys)| keys.len()).sum::<usize>();
        if messages.len() != expected {
            bail!(
                "Cursor NLS key/message count mismatch: keys={expected}, messages={}",
                messages.len()
            );
        }
        let mut index = Self::default();
        let mut message_index = 0;
        for (module, keys) in groups {
            for key in keys {
                let source = messages[message_index].clone();
                message_index += 1;
                index
                    .values
                    .entry((module.clone(), key))
                    .or_default()
                    .insert(source);
            }
        }
        Ok(index)
    }
}

fn translation_path(languagepacks: &Value) -> Result<PathBuf> {
    for locale in ["zh-cn", "zh-CN"] {
        if let Some(path) = languagepacks
            .get(locale)
            .and_then(|value| value.get("translations"))
            .and_then(|value| value.get("vscode"))
            .and_then(Value::as_str)
        {
            return Ok(PathBuf::from(path));
        }
    }
    bail!("zh-cn translations.vscode is missing from languagepacks.json")
}

fn set_translation_path(languagepacks: &mut Value, path: &Path) -> Result<()> {
    set_named_translation_path(languagepacks, "vscode", path)
}

fn set_named_translation_path(languagepacks: &mut Value, id: &str, path: &Path) -> Result<()> {
    let locale_key = if languagepacks.get("zh-cn").is_some() {
        "zh-cn"
    } else if languagepacks.get("zh-CN").is_some() {
        "zh-CN"
    } else {
        bail!("zh-cn entry is missing from languagepacks.json");
    };
    let translations = languagepacks
        .get_mut(locale_key)
        .and_then(|value| value.get_mut("translations"))
        .and_then(Value::as_object_mut)
        .context("zh-cn translations is not an object")?;
    translations.insert(
        id.to_string(),
        Value::String(path.to_string_lossy().to_string()),
    );
    Ok(())
}

fn read_json<T>(path: &Path) -> Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("failed to parse {}", path.display()))
}

fn read_optional_json(path: &Path) -> Result<Option<Value>> {
    read_optional(path)?
        .map(|bytes| {
            serde_json::from_slice(&bytes)
                .with_context(|| format!("failed to parse {}", path.display()))
        })
        .transpose()
}

struct BackupManager {
    root: PathBuf,
    manifest_path: PathBuf,
    manifest: BackupManifest,
}

impl BackupManager {
    fn load(root: PathBuf) -> Result<Self> {
        let manifest_path = root.join("language-state.json");
        let manifest = match read_optional(&manifest_path)? {
            Some(bytes) => serde_json::from_slice(&bytes)
                .with_context(|| format!("failed to parse {}", manifest_path.display()))?,
            None => BackupManifest {
                schema_version: 1,
                files: Vec::new(),
            },
        };
        Ok(Self {
            root,
            manifest_path,
            manifest,
        })
    }

    fn write_managed(&mut self, label: &str, path: &Path, bytes: &[u8]) -> Result<()> {
        let index = self.ensure_original(label, path)?;
        self.manifest.files[index].last_written_sha256 = Some(sha256_bytes(bytes));
        self.save()?;
        atomic_write(path, bytes)
    }

    fn original_bytes(&self, label: &str) -> Result<Option<Vec<u8>>> {
        let Some(file) = self.manifest.files.iter().find(|file| file.label == label) else {
            return Ok(None);
        };
        if !file.original_existed {
            return Ok(None);
        }
        let backup_path = file
            .backup_path
            .as_deref()
            .context("managed file backup path is missing")?;
        let bytes = fs::read(backup_path)
            .with_context(|| format!("failed to read managed backup {}", backup_path.display()))?;
        if Some(sha256_bytes(&bytes)) != file.original_sha256 {
            bail!("managed backup hash mismatch: {}", backup_path.display());
        }
        Ok(Some(bytes))
    }

    fn ensure_original(&mut self, label: &str, path: &Path) -> Result<usize> {
        if let Some(index) = self
            .manifest
            .files
            .iter()
            .position(|file| file.path == path)
        {
            return Ok(index);
        }
        let original = read_optional(path)?;
        let (original_sha256, backup_path) = if let Some(bytes) = original.as_deref() {
            let hash = sha256_bytes(bytes);
            let backup_path = self.root.join(format!("{label}.original.{hash}.json"));
            if !backup_path.exists() {
                atomic_write(&backup_path, bytes)?;
            }
            (Some(hash), Some(backup_path))
        } else {
            (None, None)
        };
        self.manifest.files.push(ManagedFile {
            label: label.to_string(),
            path: path.to_path_buf(),
            original_existed: original.is_some(),
            original_sha256,
            backup_path,
            last_written_sha256: None,
        });
        self.save()?;
        Ok(self.manifest.files.len() - 1)
    }

    fn save(&self) -> Result<()> {
        if self.manifest.schema_version != 1 {
            bail!("unsupported language backup manifest schema");
        }
        let mut bytes = serde_json::to_vec_pretty(&self.manifest)?;
        bytes.push(b'\n');
        atomic_write(&self.manifest_path, &bytes)
    }

    fn restore(mut self) -> Result<RestoreReport> {
        let mut report = RestoreReport {
            restored: Vec::new(),
            skipped_changed: Vec::new(),
        };
        for file in &mut self.manifest.files {
            let current_hash = if file.path.is_file() {
                Some(sha256_file(&file.path)?)
            } else {
                None
            };
            if current_hash != file.last_written_sha256 {
                report.skipped_changed.push(file.path.clone());
                continue;
            }
            if file.original_existed {
                let backup_path = file
                    .backup_path
                    .as_deref()
                    .context("managed file backup path is missing")?;
                let bytes = fs::read(backup_path).with_context(|| {
                    format!("failed to read managed backup {}", backup_path.display())
                })?;
                if Some(sha256_bytes(&bytes)) != file.original_sha256 {
                    bail!("managed backup hash mismatch: {}", backup_path.display());
                }
                atomic_write(&file.path, &bytes)?;
            } else if file.path.exists() {
                fs::remove_file(&file.path).with_context(|| {
                    format!("failed to remove managed file {}", file.path.display())
                })?;
            }
            file.last_written_sha256 = None;
            report.restored.push(file.path.clone());
        }
        self.save()?;
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::translation::{ExtensionEntry, NlsEntry, RuntimeEntry, RuntimeSurface};

    fn catalog() -> TranslationCatalog {
        TranslationCatalog {
            schema_version: 1,
            locale: "zh-CN".to_string(),
            glossary_version: 1,
            nls: vec![NlsEntry {
                id: "nls.open".to_string(),
                module: "vs/example".to_string(),
                key: "open".to_string(),
                source: "Open {0}".to_string(),
                target: "打开 {0}".to_string(),
                context: "test".to_string(),
                status: ReviewStatus::Reviewed,
            }],
            runtime: vec![RuntimeEntry {
                id: "runtime.login".to_string(),
                surface: RuntimeSurface::Renderer,
                source: "Log In".to_string(),
                target: "登录".to_string(),
                context: "test".to_string(),
                status: ReviewStatus::Reviewed,
                profile_ids: Vec::new(),
                scopes: Vec::new(),
            }],
            extensions: vec![ExtensionEntry {
                id: "extension.test.display-name".to_string(),
                extension_id: "test.sample".to_string(),
                key: "displayName".to_string(),
                source: "Sample Extension".to_string(),
                target: "示例扩展".to_string(),
                context: "test".to_string(),
                status: ReviewStatus::Reviewed,
            }],
        }
    }

    fn fake_installation(root: &Path) -> CursorInstallation {
        let executable = root.join("Cursor.exe");
        let app_root = root.join("resources/app");
        fs::create_dir_all(app_root.join("out")).unwrap();
        fs::write(&executable, b"fake").unwrap();
        fs::write(
            app_root.join("product.json"),
            br#"{"commit":"abc","vscodeVersion":"1.2.3","applicationName":"cursor","dataFolderName":".cursor"}"#,
        )
        .unwrap();
        fs::write(app_root.join("package.json"), br#"{"version":"9.8.7"}"#).unwrap();
        fs::write(
            app_root.join("out/nls.keys.json"),
            br#"[["vs/example",["open"]]]"#,
        )
        .unwrap();
        fs::write(app_root.join("out/nls.messages.json"), br#"["Open {0}"]"#).unwrap();
        let extension = app_root.join("extensions/sample");
        fs::create_dir_all(&extension).unwrap();
        fs::write(
            extension.join("package.json"),
            br#"{"publisher":"test","name":"sample","version":"1.0.0"}"#,
        )
        .unwrap();
        fs::write(
            extension.join("package.nls.json"),
            br#"{"displayName":"Sample Extension"}"#,
        )
        .unwrap();
        CursorInstallation::from_executable(executable).unwrap()
    }

    fn setup_language_pack(cursor_data: &Path) -> PathBuf {
        let official = cursor_data.join("official/main.i18n.json");
        fs::create_dir_all(official.parent().unwrap()).unwrap();
        fs::write(
            &official,
            r#"{"version":"1.2.3","contents":{"vs/base":{"ok":"确定"}}}"#.as_bytes(),
        )
        .unwrap();
        let languagepacks = serde_json::json!({
            "zh-cn": {
                "translations": { "vscode": official },
                "label": "中文(简体)"
            }
        });
        fs::create_dir_all(cursor_data).unwrap();
        fs::write(
            cursor_data.join("languagepacks.json"),
            serde_json::to_vec(&languagepacks).unwrap(),
        )
        .unwrap();
        official
    }

    #[test]
    fn prepares_overlay_and_restores_user_state() {
        let directory = tempfile::tempdir().unwrap();
        let install = fake_installation(&directory.path().join("install"));
        let cursor_data = directory.path().join("cursor-data");
        let launcher_data = directory.path().join("launcher-data");
        let official = setup_language_pack(&cursor_data);
        let original_languagepacks = fs::read(cursor_data.join("languagepacks.json")).unwrap();
        let manager = LanguagePackManager::new(&cursor_data, &launcher_data);
        let report = manager.prepare(&install, &catalog()).unwrap();
        assert_eq!(report.official_main_i18n, official);
        assert_eq!(report.applied_entries, 1);
        assert_eq!(report.extension_files.len(), 1);
        assert!(report.missing_extension_entries.is_empty());
        assert!(report.extension_source_mismatches.is_empty());
        let extension_path = &report.extension_files["test.sample"];
        let extension_document: Value = read_json(extension_path).unwrap();
        assert_eq!(
            extension_document["contents"]["package"]["displayName"],
            "示例扩展"
        );
        let overlay: OfficialMainI18n = read_json(&report.overlay_main_i18n).unwrap();
        assert_eq!(overlay.contents["vs/example"]["open"], "打开 {0}");
        assert_eq!(manager.inspect().unwrap().locale.as_deref(), Some("zh-cn"));

        let repeated = manager.prepare(&install, &catalog()).unwrap();
        assert_eq!(repeated.official_main_i18n, official);
        assert_eq!(repeated.applied_entries, 1);

        let restore = manager.restore().unwrap();
        assert_eq!(restore.restored.len(), 2);
        assert_eq!(
            fs::read(cursor_data.join("languagepacks.json")).unwrap(),
            original_languagepacks
        );
        assert!(!cursor_data.join("User/locale.json").exists());
    }

    #[test]
    fn restore_preserves_user_changes_after_management() {
        let directory = tempfile::tempdir().unwrap();
        let install = fake_installation(&directory.path().join("install"));
        let cursor_data = directory.path().join("cursor-data");
        let launcher_data = directory.path().join("launcher-data");
        setup_language_pack(&cursor_data);
        let manager = LanguagePackManager::new(&cursor_data, &launcher_data);
        manager.prepare(&install, &catalog()).unwrap();
        fs::write(cursor_data.join("User/locale.json"), b"{\"locale\":\"en\"}").unwrap();
        let restore = manager.restore().unwrap();
        assert_eq!(
            restore.skipped_changed,
            [cursor_data.join("User/locale.json")]
        );
        assert_eq!(
            fs::read_to_string(cursor_data.join("User/locale.json")).unwrap(),
            "{\"locale\":\"en\"}"
        );
    }

    #[test]
    fn rejects_an_official_pack_for_another_vscode_version() {
        let directory = tempfile::tempdir().unwrap();
        let extension = directory.path().join("language-pack");
        let path = extension.join("translations").join("main.i18n.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            r#"{"version":"1.0.0","contents":{"vs/base":{"ok":"确定"}}}"#.as_bytes(),
        )
        .unwrap();
        fs::write(extension.join("package.json"), br#"{"version":"1.127.9"}"#).unwrap();
        assert!(!official_pack_matches(&path, "1.128.0"));
        assert!(official_pack_matches(&path, "1.127.0"));
        assert!(!official_pack_matches(&path, "invalid"));
    }
}
