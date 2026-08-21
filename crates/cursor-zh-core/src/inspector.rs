// SPDX-License-Identifier: GPL-3.0-only

use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde_json::{Value, json};
use tokio::time::sleep;

use crate::cdp::{CdpTarget, DevtoolsSession, validate_websocket_url};
use crate::translation::{ReviewStatus, RuntimeSurface, TranslationCatalog};

const INSPECTOR_HTTP_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MainMenuInjectionReport {
    pub target_id: String,
    pub title: String,
    pub evaluation_value: Option<Value>,
    pub resumed: bool,
}

pub fn build_main_menu_script(
    catalog: &TranslationCatalog,
    launcher_version: &str,
) -> Result<String> {
    let rules = catalog
        .runtime
        .iter()
        .filter(|entry| {
            entry.status == ReviewStatus::Reviewed && entry.surface == RuntimeSurface::NativeMenu
        })
        .map(|entry| (&entry.source, &entry.target))
        .collect::<Vec<_>>();
    let rules = serde_json::to_string(&rules)?;
    let version = serde_json::to_string(launcher_version)?;
    let template = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/inject/main-menu-bootstrap.js"
    ));
    let script = template
        .replacen("__CURSOR_ZH_VERSION__", &version, 1)
        .replacen("__CURSOR_ZH_RULES__", &rules, 1);
    if script.contains("__CURSOR_ZH_") {
        bail!("main menu bootstrap contains unresolved placeholders");
    }
    Ok(script)
}

pub async fn install_main_menu_hook(
    inspector_port: u16,
    script: &str,
    attempts: usize,
) -> Result<MainMenuInjectionReport> {
    let target = wait_for_inspector_target(inspector_port, attempts).await?;
    let websocket_url = target
        .web_socket_debugger_url
        .as_deref()
        .context("Node Inspector target has no WebSocket URL")?;
    validate_websocket_url(websocket_url, inspector_port)?;
    let mut session = DevtoolsSession::connect(websocket_url, inspector_port).await?;
    let _ = session.send("Runtime.enable", json!({})).await;
    let first_evaluation = evaluate_hook(&mut session, script).await;
    let resume = session
        .send("Runtime.runIfWaitingForDebugger", json!({}))
        .await;
    let resume = resume.context("failed to resume Cursor main process")?;
    let mut evaluation_value = first_evaluation
        .as_ref()
        .ok()
        .and_then(extract_evaluation_value);
    if !hook_installed(evaluation_value.as_ref()) {
        for _ in 0..attempts {
            sleep(Duration::from_millis(25)).await;
            if let Ok(value) = evaluate_hook(&mut session, script).await {
                evaluation_value = extract_evaluation_value(&value);
                if hook_installed(evaluation_value.as_ref()) {
                    break;
                }
            }
        }
    }
    session.close().await;
    if !hook_installed(evaluation_value.as_ref()) {
        bail!("Electron Menu/Tray APIs did not become available after main process resume");
    }
    Ok(MainMenuInjectionReport {
        target_id: target.id,
        title: target.title,
        evaluation_value,
        resumed: resume.get("result").is_some(),
    })
}

async fn evaluate_hook(session: &mut DevtoolsSession, script: &str) -> Result<Value> {
    session
        .send(
            "Runtime.evaluate",
            json!({
                "expression": script,
                "returnByValue": true,
                "awaitPromise": false
            }),
        )
        .await
}

fn extract_evaluation_value(response: &Value) -> Option<Value> {
    response
        .get("result")
        .and_then(|value| value.get("result"))
        .and_then(|value| value.get("value"))
        .cloned()
}

fn hook_installed(value: Option<&Value>) -> bool {
    value
        .and_then(|value| value.get("status"))
        .and_then(Value::as_str)
        .is_some_and(|status| matches!(status, "installed" | "already-installed"))
}

async fn wait_for_inspector_target(inspector_port: u16, attempts: usize) -> Result<CdpTarget> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(INSPECTOR_HTTP_TIMEOUT)
        .build()
        .context("failed to build loopback Inspector client")?;
    let url = format!("http://127.0.0.1:{inspector_port}/json/list");
    let mut last_error = None;
    for _ in 0..attempts {
        match client.get(&url).send().await {
            Ok(response) => match response.error_for_status() {
                Ok(response) => match response.json::<Vec<CdpTarget>>().await {
                    Ok(targets) => {
                        if let Some(target) = targets.into_iter().find(|target| {
                            target.web_socket_debugger_url.is_some()
                                && matches!(target.target_type.as_str(), "node" | "page")
                        }) {
                            return Ok(target);
                        }
                    }
                    Err(error) => last_error = Some(anyhow::Error::new(error)),
                },
                Err(error) => last_error = Some(anyhow::Error::new(error)),
            },
            Err(error) => last_error = Some(anyhow::Error::new(error)),
        }
        sleep(Duration::from_millis(250)).await;
    }
    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("no Node Inspector target became ready")))
        .context("Cursor main process Inspector did not become ready")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_script_contains_only_native_rules() {
        let catalog = crate::embedded_translation_catalog().unwrap();
        let script = build_main_menu_script(&catalog, "0.1.0").unwrap();
        assert!(script.contains("File"));
        assert!(script.contains("文件"));
        assert!(script.contains("setContextMenu"));
        assert!(!script.contains("Search Settings"));
        assert!(!script.contains("__CURSOR_ZH_"));
    }
}
