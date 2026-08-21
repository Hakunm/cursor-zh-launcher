// SPDX-License-Identifier: GPL-3.0-only

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::process::Child;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::time::sleep;

use crate::cdp::{
    InjectionReport, evaluate_expression, install_script, is_cursor_workbench_target, list_targets,
};
use crate::compatibility::CompatibilityProfile;
use crate::fs_util::atomic_write;
use crate::translation::{ReviewStatus, RuntimeSurface, TranslationCatalog};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct RendererRule<'a> {
    source: &'a str,
    target: &'a str,
    scopes: &'a [String],
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchdogTick {
    pub workbench_targets: usize,
    pub injected: Vec<InjectionReport>,
    pub failures: Vec<String>,
    pub collected_findings: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UntranslatedFinding {
    pub source: String,
    pub tag: String,
    pub role: String,
}

pub struct RendererWatchdog {
    debug_port: u16,
    script: String,
    injected_targets: HashSet<String>,
    collector_output: Option<PathBuf>,
    collected: BTreeMap<String, UntranslatedFinding>,
}

impl RendererWatchdog {
    #[must_use]
    pub fn new(debug_port: u16, script: String, collector_output: Option<PathBuf>) -> Self {
        Self {
            debug_port,
            script,
            injected_targets: HashSet::new(),
            collector_output,
            collected: BTreeMap::new(),
        }
    }

    pub async fn tick(&mut self) -> Result<WatchdogTick> {
        let targets = list_targets(self.debug_port).await?;
        let workbench = targets
            .into_iter()
            .filter(is_cursor_workbench_target)
            .collect::<Vec<_>>();
        let live_ids = workbench
            .iter()
            .map(|target| target.id.clone())
            .collect::<HashSet<_>>();
        self.injected_targets.retain(|id| live_ids.contains(id));

        let mut report = WatchdogTick {
            workbench_targets: workbench.len(),
            ..WatchdogTick::default()
        };
        for target in workbench {
            if !self.injected_targets.contains(&target.id) {
                match install_script(&target, self.debug_port, &self.script).await {
                    Ok(injection) => {
                        self.injected_targets.insert(target.id.clone());
                        report.injected.push(injection);
                    }
                    Err(error) => report.failures.push(format!("{error:#}")),
                }
            }
            if self.collector_output.is_some()
                && let Ok(Value::Array(findings)) = evaluate_expression(
                    &target,
                    self.debug_port,
                    "globalThis.__cursorZhGetUntranslated?.() ?? []",
                )
                .await
            {
                for finding in findings {
                    if let Ok(finding) = serde_json::from_value::<UntranslatedFinding>(finding) {
                        let key = format!("{}\0{}\0{}", finding.source, finding.tag, finding.role);
                        self.collected.insert(key, finding);
                    }
                }
            }
        }
        report.collected_findings = self.collected.len();
        Ok(report)
    }

    pub async fn run_until_exit(&mut self, child: &mut Child) -> Result<()> {
        let mut tick_count = 0_u32;
        loop {
            if child.try_wait()?.is_some() {
                self.flush_collector()?;
                return Ok(());
            }
            if let Err(error) = self.tick().await {
                eprintln!("Renderer CDP 尚未就绪：{error:#}");
            }
            tick_count += 1;
            let interval = if tick_count <= 10 {
                Duration::from_secs(1)
            } else {
                Duration::from_secs(5)
            };
            sleep(interval).await;
        }
    }

    pub fn flush_collector(&self) -> Result<()> {
        let Some(output) = self.collector_output.as_deref() else {
            return Ok(());
        };
        let findings = self.collected.values().collect::<Vec<_>>();
        let mut bytes = serde_json::to_vec_pretty(&findings)?;
        bytes.push(b'\n');
        atomic_write(output, &bytes)
    }
}

pub fn build_renderer_script(
    catalog: &TranslationCatalog,
    launcher_version: &str,
    collect_untranslated: bool,
) -> Result<String> {
    let rules = catalog
        .runtime
        .iter()
        .filter(|entry| {
            entry.status == ReviewStatus::Reviewed
                && entry.surface == RuntimeSurface::Renderer
                && !entry.scopes.is_empty()
        })
        .map(|entry| RendererRule {
            source: &entry.source,
            target: &entry.target,
            scopes: &entry.scopes,
        })
        .collect::<Vec<_>>();
    let rules = serde_json::to_string(&rules)?;
    let version = serde_json::to_string(launcher_version)?;
    let collect_untranslated = serde_json::to_string(&collect_untranslated)?;
    let template = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/inject/renderer-bootstrap.js"
    ));
    if !template.contains("__CURSOR_ZH_VERSION__")
        || !template.contains("__CURSOR_ZH_RULES__")
        || !template.contains("__CURSOR_ZH_COLLECT__")
    {
        anyhow::bail!("renderer bootstrap placeholders are missing");
    }
    let script = template
        .replacen("__CURSOR_ZH_VERSION__", &version, 1)
        .replacen("__CURSOR_ZH_RULES__", &rules, 1)
        .replacen("__CURSOR_ZH_COLLECT__", &collect_untranslated, 1);
    if script.contains("__CURSOR_ZH_") {
        anyhow::bail!("renderer bootstrap contains unresolved placeholders");
    }
    Ok(script)
}

pub async fn validate_profile_signatures(
    debug_port: u16,
    profile: &CompatibilityProfile,
    attempts: usize,
) -> Result<bool> {
    if profile.renderer_signatures.is_empty() {
        return Ok(false);
    }
    let signatures = serde_json::to_string(&profile.renderer_signatures)?;
    let expression = format!(
        r#"(() => {{
  const signatures = {signatures};
  const normalize = (value) => String(value || "").replace(/\s+/g, " ").trim();
  return signatures.every((signature) => {{
    const elements = [...document.querySelectorAll(signature.selector)];
    if (!elements.length) return false;
    if (signature.expectedText == null) return true;
    return elements.some((element) => normalize(element.innerText || element.textContent) === signature.expectedText);
  }});
}})()"#
    );
    let mut last_error = None;
    for _ in 0..attempts {
        match list_targets(debug_port).await {
            Ok(targets) => {
                if let Some(target) = targets
                    .iter()
                    .find(|target| is_cursor_workbench_target(target))
                {
                    match evaluate_expression(target, debug_port, &expression).await {
                        Ok(Value::Bool(true)) => return Ok(true),
                        Ok(Value::Bool(false) | Value::Null) => {}
                        Ok(_) => return Ok(false),
                        Err(error) => last_error = Some(error),
                    }
                }
            }
            Err(error) => last_error = Some(error),
        }
        sleep(Duration::from_millis(250)).await;
    }
    if let Some(error) = last_error {
        Err(error).context("renderer compatibility signature check did not become ready")
    } else {
        Ok(false)
    }
}

pub async fn wait_for_first_injection(
    watchdog: &mut RendererWatchdog,
    attempts: usize,
) -> Result<WatchdogTick> {
    let mut last_error = None;
    for _ in 0..attempts {
        match watchdog.tick().await {
            Ok(report) if !report.injected.is_empty() || report.workbench_targets > 0 => {
                return Ok(report);
            }
            Ok(_) => {}
            Err(error) => last_error = Some(error),
        }
        sleep(Duration::from_millis(250)).await;
    }
    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("no Cursor Workbench target became ready")))
        .context("renderer injection did not become ready")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_contains_only_renderer_rules() {
        let catalog = crate::embedded_translation_catalog().unwrap();
        let script = build_renderer_script(&catalog, "0.1.0", false).unwrap();
        assert!(script.contains("Log In"));
        assert!(script.contains("登录"));
        assert!(script.contains(".monaco-editor"));
        assert!(script.contains("glass-palette-agent-workspace-label"));
        assert!(script.contains("attributeFilter: attributes"));
        assert!(script.contains("replaceCompositeLabel"));
        assert!(script.contains(".menubar-menu-title"));
        assert!(script.contains("New Terminal"));
        assert!(script.contains("新建终端"));
        assert!(script.contains("Open project"));
        assert!(script.contains("打开项目"));
        assert!(script.contains("Steer from iOS"));
        assert!(script.contains("通过 iOS 操控"));
        assert!(script.contains("Agent Stats: {accepted}/{suggested} ({percent}%)"));
        assert!(script.contains("智能体统计：{accepted}/{suggested}（{percent}%）"));
        assert!(script.contains("replaceDynamicStatus"));
        assert!(!script.contains("native-menu.file"));
        assert!(!script.contains("__CURSOR_ZH_"));
    }

    #[test]
    fn unknown_versions_keep_scoped_exact_rules() {
        let catalog = crate::embedded_translation_catalog().unwrap();
        let script = build_renderer_script(&catalog, "0.1.0", false).unwrap();
        assert!(script.contains("Log In"));
        assert!(script.contains("登录"));
        assert!(script.contains("Sign Up"));
    }
}
