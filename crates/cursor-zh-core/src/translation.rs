// SPDX-License-Identifier: GPL-3.0-only

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::TRANSLATION_SCHEMA_VERSION;

static PLACEHOLDER_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\$\{[^{}]+\}|\{[A-Za-z0-9_]+\}|%(?:\d+\$)?[sdif]|https?://[^\s)]+|</?[A-Za-z][^>]*>",
    )
    .expect("placeholder regex is valid")
});

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationCatalog {
    pub schema_version: u32,
    pub locale: String,
    pub glossary_version: u32,
    #[serde(default)]
    pub nls: Vec<NlsEntry>,
    #[serde(default)]
    pub runtime: Vec<RuntimeEntry>,
    #[serde(default)]
    pub extensions: Vec<ExtensionEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NlsEntry {
    pub id: String,
    pub module: String,
    pub key: String,
    pub source: String,
    pub target: String,
    pub context: String,
    pub status: ReviewStatus,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeEntry {
    pub id: String,
    pub surface: RuntimeSurface,
    pub source: String,
    pub target: String,
    pub context: String,
    pub status: ReviewStatus,
    #[serde(default)]
    pub profile_ids: Vec<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionEntry {
    pub id: String,
    pub extension_id: String,
    pub key: String,
    pub source: String,
    pub target: String,
    pub context: String,
    pub status: ReviewStatus,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeSurface {
    Renderer,
    NativeMenu,
    PrivateExtension,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReviewStatus {
    Draft,
    Reviewed,
    Allowlisted,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationIssue {
    pub entry_id: Option<String>,
    pub message: String,
}

impl TranslationCatalog {
    #[must_use]
    pub fn validate(&self, release: bool) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();
        if self.schema_version != TRANSLATION_SCHEMA_VERSION {
            issues.push(ValidationIssue {
                entry_id: None,
                message: format!(
                    "unsupported translation schema {}, expected {}",
                    self.schema_version, TRANSLATION_SCHEMA_VERSION
                ),
            });
        }
        if self.locale != "zh-CN" {
            issues.push(ValidationIssue {
                entry_id: None,
                message: "locale must be zh-CN".to_string(),
            });
        }

        let mut ids = HashSet::new();
        let mut nls_locations = HashSet::new();
        let mut extension_locations = HashSet::new();
        let mut runtime_contexts: HashMap<(&RuntimeSurface, &str, &str), &str> = HashMap::new();
        for entry in &self.nls {
            validate_common(
                CommonEntryRef {
                    id: &entry.id,
                    source: &entry.source,
                    target: &entry.target,
                    context: &entry.context,
                    status: entry.status,
                },
                release,
                &mut ids,
                &mut issues,
            );
            if !nls_locations.insert((entry.module.as_str(), entry.key.as_str())) {
                issues.push(issue(&entry.id, "duplicate NLS module/key"));
            }
        }
        for entry in &self.runtime {
            validate_common(
                CommonEntryRef {
                    id: &entry.id,
                    source: &entry.source,
                    target: &entry.target,
                    context: &entry.context,
                    status: entry.status,
                },
                release,
                &mut ids,
                &mut issues,
            );
            let context_key = (
                &entry.surface,
                entry.source.as_str(),
                entry.context.as_str(),
            );
            if let Some(existing) = runtime_contexts.insert(context_key, entry.target.as_str())
                && existing != entry.target
            {
                issues.push(issue(
                    &entry.id,
                    "conflicting runtime target for the same surface/source/context",
                ));
            }
        }
        for entry in &self.extensions {
            validate_common(
                CommonEntryRef {
                    id: &entry.id,
                    source: &entry.source,
                    target: &entry.target,
                    context: &entry.context,
                    status: entry.status,
                },
                release,
                &mut ids,
                &mut issues,
            );
            if entry.extension_id.trim().is_empty() || entry.key.trim().is_empty() {
                issues.push(issue(&entry.id, "extensionId and key must be non-empty"));
            }
            if !extension_locations.insert((entry.extension_id.as_str(), entry.key.as_str())) {
                issues.push(issue(&entry.id, "duplicate extensionId/key"));
            }
        }
        issues
    }
}

#[derive(Clone, Copy)]
struct CommonEntryRef<'a> {
    id: &'a str,
    source: &'a str,
    target: &'a str,
    context: &'a str,
    status: ReviewStatus,
}

fn validate_common(
    entry: CommonEntryRef<'_>,
    release: bool,
    ids: &mut HashSet<String>,
    issues: &mut Vec<ValidationIssue>,
) {
    let CommonEntryRef {
        id,
        source,
        target,
        context,
        status,
    } = entry;
    if id.trim().is_empty() {
        issues.push(issue(id, "entry id is empty"));
    } else if !ids.insert(id.to_ascii_lowercase()) {
        issues.push(issue(id, "duplicate stable id"));
    }
    if source.trim().is_empty() || target.trim().is_empty() || context.trim().is_empty() {
        issues.push(issue(id, "source, target, and context must be non-empty"));
    }
    if release && status == ReviewStatus::Draft {
        issues.push(issue(id, "draft entry is not allowed in a release catalog"));
    }
    let source_placeholders = placeholders(source);
    let target_placeholders = placeholders(target);
    if source_placeholders != target_placeholders {
        issues.push(issue(
            id,
            &format!(
                "placeholder mismatch: source={source_placeholders:?}, target={target_placeholders:?}"
            ),
        ));
    }
}

fn placeholders(value: &str) -> BTreeSet<String> {
    PLACEHOLDER_PATTERN
        .find_iter(value)
        .map(|capture| capture.as_str().to_string())
        .collect()
}

fn issue(id: &str, message: &str) -> ValidationIssue {
    ValidationIssue {
        entry_id: (!id.is_empty()).then(|| id.to_string()),
        message: message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
                context: "file command".to_string(),
                status: ReviewStatus::Reviewed,
            }],
            runtime: vec![RuntimeEntry {
                id: "renderer.login".to_string(),
                surface: RuntimeSurface::Renderer,
                source: "Log In".to_string(),
                target: "登录".to_string(),
                context: "welcome primary button".to_string(),
                status: ReviewStatus::Reviewed,
                profile_ids: Vec::new(),
                scopes: vec!["button".to_string()],
            }],
            extensions: Vec::new(),
        }
    }

    #[test]
    fn accepts_reviewed_catalog() {
        assert!(catalog().validate(true).is_empty());
    }

    #[test]
    fn rejects_placeholder_loss_and_duplicate_ids() {
        let mut value = catalog();
        value.nls[0].target = "打开".to_string();
        value.runtime[0].id = "NLS.OPEN".to_string();
        let issues = value.validate(true);
        assert!(
            issues
                .iter()
                .any(|issue| issue.message.contains("placeholder mismatch"))
        );
        assert!(
            issues
                .iter()
                .any(|issue| issue.message == "duplicate stable id")
        );
    }

    #[test]
    fn rejects_drafts_only_for_release() {
        let mut value = catalog();
        value.runtime[0].status = ReviewStatus::Draft;
        assert!(value.validate(false).is_empty());
        assert!(
            value
                .validate(true)
                .iter()
                .any(|issue| issue.message.contains("draft"))
        );
    }
}
