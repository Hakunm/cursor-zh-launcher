// SPDX-License-Identifier: GPL-3.0-only

use std::net::IpAddr;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};
use url::Url;

const CDP_HTTP_TIMEOUT: Duration = Duration::from_secs(3);
const CDP_WEBSOCKET_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CdpTarget {
    pub id: String,
    #[serde(rename = "type")]
    pub target_type: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub url: String,
    #[serde(default, rename = "webSocketDebuggerUrl")]
    pub web_socket_debugger_url: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InjectionReport {
    pub target_id: String,
    pub title: String,
    pub url: String,
    pub new_document_script_id: Option<String>,
}

pub struct DevtoolsSession {
    stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
    next_id: u64,
}

impl DevtoolsSession {
    pub async fn connect(websocket_url: &str, expected_port: u16) -> Result<Self> {
        let websocket_url = validate_websocket_url(websocket_url, expected_port)?;
        let (stream, _) = timeout(CDP_WEBSOCKET_TIMEOUT, connect_async(websocket_url.as_str()))
            .await
            .context("timed out connecting to DevTools WebSocket")?
            .context("failed to connect to DevTools WebSocket")?;
        Ok(Self { stream, next_id: 1 })
    }

    pub async fn send(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        let request = json!({ "id": id, "method": method, "params": params }).to_string();
        self.stream
            .send(Message::Text(request.into()))
            .await
            .with_context(|| format!("failed to send DevTools command {method}"))?;
        loop {
            let message = timeout(CDP_WEBSOCKET_TIMEOUT, self.stream.next())
                .await
                .with_context(|| format!("timed out waiting for DevTools command {method}"))?
                .context("DevTools WebSocket closed before the command response")?
                .context("failed to receive DevTools command response")?;
            let Message::Text(text) = message else {
                continue;
            };
            let response: Value =
                serde_json::from_str(&text).context("invalid DevTools JSON response")?;
            if response.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = response.get("error") {
                bail!("DevTools command {method} failed: {error}");
            }
            if let Some(exception) = response
                .get("result")
                .and_then(|value| value.get("exceptionDetails"))
            {
                bail!("DevTools command {method} raised an exception: {exception}");
            }
            return Ok(response);
        }
    }

    pub async fn close(mut self) {
        let _ = self.stream.close(None).await;
    }
}

pub async fn list_targets(debug_port: u16) -> Result<Vec<CdpTarget>> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(CDP_HTTP_TIMEOUT)
        .build()
        .context("failed to build loopback CDP client")?;
    let url = format!("http://127.0.0.1:{debug_port}/json");
    let targets = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("failed to query {url}"))?
        .error_for_status()
        .context("CDP target query returned an error status")?
        .json::<Vec<CdpTarget>>()
        .await
        .context("failed to parse CDP target list")?;
    for target in &targets {
        if let Some(websocket_url) = target.web_socket_debugger_url.as_deref() {
            validate_websocket_url(websocket_url, debug_port)
                .with_context(|| format!("unsafe WebSocket URL for CDP target {}", target.id))?;
        }
    }
    Ok(targets)
}

#[must_use]
pub fn is_cursor_workbench_target(target: &CdpTarget) -> bool {
    if target.target_type != "page" || target.web_socket_debugger_url.is_none() {
        return false;
    }
    let normalized = target.url.trim().replace('\\', "/").to_ascii_lowercase();
    normalized.starts_with("vscode-file://vscode-app/")
        && normalized.ends_with("/vs/code/electron-sandbox/workbench/workbench.html")
}

pub fn validate_websocket_url(value: &str, expected_port: u16) -> Result<Url> {
    let url = Url::parse(value).context("invalid CDP WebSocket URL")?;
    if url.scheme() != "ws" {
        bail!("CDP WebSocket must use ws");
    }
    if !url.username().is_empty() || url.password().is_some() {
        bail!("CDP WebSocket URL must not include credentials");
    }
    let host = url
        .host_str()
        .context("CDP WebSocket URL has no host")?
        .parse::<IpAddr>()
        .context("CDP WebSocket host must be an IP address")?;
    if !host.is_loopback() {
        bail!("CDP WebSocket host must be loopback");
    }
    if url.port() != Some(expected_port) {
        bail!("CDP WebSocket port does not match the selected debug port");
    }
    Ok(url)
}

pub async fn install_script(
    target: &CdpTarget,
    debug_port: u16,
    script: &str,
) -> Result<InjectionReport> {
    if !is_cursor_workbench_target(target) {
        bail!("target {} is not a Cursor Workbench page", target.id);
    }
    let websocket = target
        .web_socket_debugger_url
        .as_deref()
        .context("CDP target has no WebSocket URL")?;
    let mut session = DevtoolsSession::connect(websocket, debug_port).await?;
    let add_script = session
        .send(
            "Page.addScriptToEvaluateOnNewDocument",
            json!({ "source": script }),
        )
        .await?;
    session
        .send(
            "Runtime.evaluate",
            json!({
                "expression": script,
                "returnByValue": true,
                "awaitPromise": false
            }),
        )
        .await?;
    session.close().await;
    Ok(InjectionReport {
        target_id: target.id.clone(),
        title: target.title.clone(),
        url: target.url.clone(),
        new_document_script_id: add_script
            .get("result")
            .and_then(|value| value.get("identifier"))
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

pub async fn evaluate_expression(
    target: &CdpTarget,
    debug_port: u16,
    expression: &str,
) -> Result<Value> {
    if !is_cursor_workbench_target(target) {
        bail!("target {} is not a Cursor Workbench page", target.id);
    }
    let websocket = target
        .web_socket_debugger_url
        .as_deref()
        .context("CDP target has no WebSocket URL")?;
    let mut session = DevtoolsSession::connect(websocket, debug_port).await?;
    let response = session
        .send(
            "Runtime.evaluate",
            json!({
                "expression": expression,
                "returnByValue": true,
                "awaitPromise": false
            }),
        )
        .await?;
    session.close().await;
    Ok(response
        .get("result")
        .and_then(|value| value.get("result"))
        .and_then(|value| value.get("value"))
        .cloned()
        .unwrap_or(Value::Null))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(url: &str) -> CdpTarget {
        CdpTarget {
            id: "target-1".to_string(),
            target_type: "page".to_string(),
            title: "Cursor".to_string(),
            url: url.to_string(),
            web_socket_debugger_url: Some(
                "ws://127.0.0.1:49173/devtools/page/target-1".to_string(),
            ),
        }
    }

    #[test]
    fn accepts_only_exact_loopback_port() {
        assert!(
            validate_websocket_url("ws://127.0.0.1:49173/devtools/page/target-1", 49173).is_ok()
        );
        assert!(
            validate_websocket_url("ws://192.168.1.5:49173/devtools/page/target-1", 49173).is_err()
        );
        assert!(
            validate_websocket_url("ws://127.0.0.1:49174/devtools/page/target-1", 49173).is_err()
        );
        assert!(
            validate_websocket_url("wss://127.0.0.1:49173/devtools/page/target-1", 49173).is_err()
        );
    }

    #[test]
    fn identifies_only_cursor_workbench_pages() {
        assert!(is_cursor_workbench_target(&target(
            "vscode-file://vscode-app/c:/Cursor/resources/app/out/vs/code/electron-sandbox/workbench/workbench.html"
        )));
        assert!(!is_cursor_workbench_target(&target("https://example.com/")));
        assert!(!is_cursor_workbench_target(&target(
            "vscode-file://vscode-app/c:/Cursor/resources/app/out/vs/code/electron-sandbox/processExplorer/processExplorer.html"
        )));
    }
}
