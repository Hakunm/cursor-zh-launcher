// SPDX-License-Identifier: GPL-3.0-only

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatibilityCatalog {
    pub schema_version: u32,
    pub profiles: Vec<CompatibilityProfile>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatibilityProfile {
    pub id: String,
    pub cursor_version: String,
    pub cursor_commit: String,
    pub vscode_version: String,
    #[serde(default)]
    pub renderer_signatures: Vec<RendererSignature>,
    #[serde(default)]
    pub native_menu_supported: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RendererSignature {
    pub name: String,
    pub selector: String,
    pub expected_text: Option<String>,
}

impl CompatibilityCatalog {
    #[must_use]
    pub fn find_by_commit(&self, commit: &str) -> Option<&CompatibilityProfile> {
        self.profiles
            .iter()
            .find(|profile| profile.cursor_commit.eq_ignore_ascii_case(commit))
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err(format!(
                "unsupported compatibility schema {}",
                self.schema_version
            ));
        }
        let mut ids = std::collections::HashSet::new();
        let mut commits = std::collections::HashSet::new();
        for profile in &self.profiles {
            if profile.id.trim().is_empty()
                || profile.cursor_version.trim().is_empty()
                || profile.cursor_commit.trim().is_empty()
                || profile.vscode_version.trim().is_empty()
            {
                return Err("compatibility profile contains an empty required field".to_string());
            }
            if !ids.insert(profile.id.to_ascii_lowercase()) {
                return Err(format!(
                    "duplicate compatibility profile id: {}",
                    profile.id
                ));
            }
            if !commits.insert(profile.cursor_commit.to_ascii_lowercase()) {
                return Err(format!(
                    "duplicate Cursor commit: {}",
                    profile.cursor_commit
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_commits_case_insensitively() {
        let catalog = CompatibilityCatalog {
            schema_version: 1,
            profiles: vec![CompatibilityProfile {
                id: "cursor-1".to_string(),
                cursor_version: "1.0".to_string(),
                cursor_commit: "AbCd".to_string(),
                vscode_version: "1.2".to_string(),
                renderer_signatures: Vec::new(),
                native_menu_supported: true,
            }],
        };
        catalog.validate().unwrap();
        assert_eq!(catalog.find_by_commit("abcd").unwrap().id, "cursor-1");
    }
}
