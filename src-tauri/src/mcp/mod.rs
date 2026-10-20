//! The built-in MCP server: lets an AI agent (Claude Code, Claude Desktop,
//! any MCP client) patch fixtures and build the node graph through the same
//! operations as the UI. Streamable HTTP on 127.0.0.1 only. Requests whose
//! Host isn't localhost are refused (DNS-rebinding protection, rmcp's
//! default), and so is anything carrying an `Origin` header: MCP clients
//! don't send one, browsers always do, so no web page can drive the lights.
//! Settings live in the app data dir, not the show.

pub mod tools;

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use serde::{Deserialize, Serialize};
use tauri::State;
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use crate::engine::manager::EngineManager;
use crate::show::service::ShowService;
use tools::LumenMcp;

pub const DEFAULT_PORT: u16 = 7410;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct McpSettings {
    pub enabled: bool,
    pub port: u16,
}

impl Default for McpSettings {
    fn default() -> Self {
        Self { enabled: true, port: DEFAULT_PORT }
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct McpStatus {
    pub settings: McpSettings,
    /// Where clients connect, while running.
    pub url: Option<String>,
    pub error: Option<String>,
}

struct Running {
    cancel: CancellationToken,
    url: String,
}

pub struct McpServer {
    show: Arc<ShowService>,
    engine: Arc<EngineManager>,
    settings_path: Option<PathBuf>,
    settings: Mutex<McpSettings>,
    running: Mutex<Option<Running>>,
    error: Mutex<Option<String>>,
}

/// Serves MCP on `listener` until `cancel` fires.
pub async fn serve(listener: tokio::net::TcpListener, show: Arc<ShowService>, engine: Arc<EngineManager>, cancel: CancellationToken) {
    let config = StreamableHttpServerConfig::default()
        .enforce_origin_validation()
        .with_cancellation_token(cancel.child_token());
    let service: StreamableHttpService<LumenMcp, LocalSessionManager> = StreamableHttpService::new(
        move || Ok(LumenMcp::new(show.clone(), engine.clone())),
        Default::default(),
        config,
    );
    let router = axum::Router::new().nest_service("/mcp", service);
    let _ = axum::serve(listener, router).with_graceful_shutdown(async move { cancel.cancelled_owned().await }).await;
}

impl McpServer {
    pub fn new(show: Arc<ShowService>, engine: Arc<EngineManager>, data_dir: Option<PathBuf>) -> Self {
        let settings_path = data_dir.map(|d| d.join("mcp.json"));
        let settings = settings_path
            .as_ref()
            .and_then(|p| fs::read_to_string(p).ok())
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Self {
            show,
            engine,
            settings_path,
            settings: Mutex::new(settings),
            running: Mutex::new(None),
            error: Mutex::new(None),
        }
    }

    /// Starts or stops the server to match the settings.
    pub fn apply(self: &Arc<Self>) {
        if let Some(running) = self.running.lock().unwrap().take() {
            running.cancel.cancel();
        }
        *self.error.lock().unwrap() = None;
        let settings = self.settings.lock().unwrap().clone();
        if !settings.enabled {
            return;
        }
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], settings.port));
        let std_listener = match std::net::TcpListener::bind(addr) {
            Ok(l) => l,
            Err(e) => {
                *self.error.lock().unwrap() = Some(format!("Can't listen on port {}: {e}", settings.port));
                return;
            }
        };
        let _ = std_listener.set_nonblocking(true);
        let cancel = CancellationToken::new();
        *self.running.lock().unwrap() = Some(Running { cancel: cancel.clone(), url: format!("http://127.0.0.1:{}/mcp", settings.port) });
        let (show, engine, me) = (self.show.clone(), self.engine.clone(), self.clone());
        tauri::async_runtime::spawn(async move {
            match tokio::net::TcpListener::from_std(std_listener) {
                Ok(listener) => serve(listener, show, engine, cancel).await,
                Err(e) => *me.error.lock().unwrap() = Some(e.to_string()),
            }
        });
    }

    pub fn status(&self) -> McpStatus {
        McpStatus {
            settings: self.settings.lock().unwrap().clone(),
            url: self.running.lock().unwrap().as_ref().map(|r| r.url.clone()),
            error: self.error.lock().unwrap().clone(),
        }
    }

    pub fn configure(self: &Arc<Self>, settings: McpSettings) {
        if let Some(path) = &self.settings_path {
            if let Some(dir) = path.parent() {
                let _ = fs::create_dir_all(dir);
            }
            if let Ok(text) = serde_json::to_string_pretty(&settings) {
                let _ = fs::write(path, text);
            }
        }
        *self.settings.lock().unwrap() = settings;
        self.apply();
    }
}

#[tauri::command]
pub fn mcp_status(server: State<Arc<McpServer>>) -> McpStatus {
    server.status()
}

#[tauri::command]
pub fn mcp_configure(server: State<Arc<McpServer>>, settings: McpSettings) -> McpStatus {
    server.configure(settings);
    server.status()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use crate::graph::compile::GraphPlan;
    use crate::output::config::DriverConfig;
    use crate::show::resolve::ResolvedPatch;
    use crate::show::service::tests::service_with;
    use crate::show::service::ShowSink;

    /// Hands the show's patch and graph to a real engine, as the app does.
    struct EngineSink(Arc<EngineManager>);

    impl ShowSink for EngineSink {
        fn patch_resolved(&self, patch: &ResolvedPatch) {
            self.0.set_patch(patch);
        }
        fn graph_compiled(&self, plan: Arc<GraphPlan>) {
            self.0.set_graph(plan);
        }
        fn controls(&self) -> HashMap<String, f32> {
            self.0.controls()
        }
        fn set_controls(&self, controls: HashMap<String, f32>) {
            self.0.set_controls(controls);
        }
        fn set_control(&self, id: &str, value: f32) {
            self.0.set_control(id, value);
        }
        fn configure_universe(&self, _id: u32, _driver: &DriverConfig) -> Result<(), String> {
            Ok(())
        }
        fn remove_universe(&self, _id: u32) {}
        fn bpm(&self) -> f32 {
            self.0.bpm()
        }
        fn set_bpm(&self, bpm: f32) {
            self.0.set_bpm(bpm);
        }
        fn changed(&self, _revision: u64) {}
    }

    /// Speaks MCP over HTTP to a running server, like a client would.
    struct Client {
        http: reqwest::Client,
        url: String,
        session: Option<String>,
        next_id: u64,
    }

    impl Client {
        async fn post(&mut self, body: serde_json::Value) -> serde_json::Value {
            let mut req = self
                .http
                .post(&self.url)
                .header("Content-Type", "application/json")
                .header("Accept", "application/json, text/event-stream")
                .body(body.to_string());
            if let Some(s) = &self.session {
                req = req.header("Mcp-Session-Id", s);
            }
            let resp = req.send().await.unwrap();
            if let Some(s) = resp.headers().get("mcp-session-id") {
                self.session = Some(s.to_str().unwrap().to_string());
            }
            let text = resp.text().await.unwrap();
            // SSE or plain JSON: take the JSON-RPC message either way (SSE
            // opens with an empty priming event).
            let payload = text
                .lines()
                .filter_map(|l| l.strip_prefix("data:").map(str::trim))
                .find(|d| !d.is_empty())
                .unwrap_or(text.trim());
            if payload.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::from_str(payload).unwrap_or_else(|e| panic!("bad response {text:?}: {e}"))
            }
        }

        async fn request(&mut self, method: &str, params: serde_json::Value) -> serde_json::Value {
            self.next_id += 1;
            self.post(serde_json::json!({ "jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params })).await
        }

        async fn call(&mut self, tool: &str, args: serde_json::Value) -> serde_json::Value {
            let resp = self.request("tools/call", serde_json::json!({ "name": tool, "arguments": args })).await;
            let result = &resp["result"];
            assert_ne!(result["isError"], true, "{tool} failed: {resp}");
            let text = result["content"][0]["text"].as_str().unwrap_or_default();
            serde_json::from_str(text).unwrap_or(serde_json::Value::String(text.to_string()))
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_agent_can_patch_and_build_a_graph_that_drives_the_output() {
        let engine = Arc::new(EngineManager::new());
        let show = Arc::new(service_with(Box::new(EngineSink(engine.clone()))));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://127.0.0.1:{}/mcp", listener.local_addr().unwrap().port());
        let cancel = CancellationToken::new();
        tokio::spawn(serve(listener, show.clone(), engine.clone(), cancel.clone()));

        let mut client = Client { http: reqwest::Client::new(), url, session: None, next_id: 0 };
        let init = client
            .request(
                "initialize",
                serde_json::json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "test", "version": "1" } }),
            )
            .await;
        assert!(init["result"]["instructions"].as_str().unwrap().contains("Fixture Output"), "{init}");
        client.post(serde_json::json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).await;

        let tools = client.request("tools/list", serde_json::json!({})).await;
        let names: Vec<&str> = tools["result"]["tools"].as_array().unwrap().iter().filter_map(|t| t["name"].as_str()).collect();
        assert!(names.contains(&"replace_graph") && names.contains(&"add_fixtures"), "{names:?}");

        client.call("add_fixtures", serde_json::json!({ "fixtureType": "par", "universe": 1, "count": 2 })).await;
        let group = client.call("create_group", serde_json::json!({ "name": "Pars", "fixture_numbers": [1, 2] })).await;
        let out = client.call("add_node", serde_json::json!({ "kind": "output", "x": 800, "params": { "group": group["id"] } })).await;
        let color = client.call("add_node", serde_json::json!({ "kind": "color", "params": { "color": "#00ff00" } })).await;
        let one = client.call("add_node", serde_json::json!({ "kind": "number" })).await;
        for (from, socket, to) in [(&color, "color", "color"), (&one, "value", "intensity")] {
            client
                .call("connect", serde_json::json!({ "fromNode": from["id"], "fromSocket": socket, "toNode": out["id"], "toSocket": to }))
                .await;
        }

        engine.step(10.0);
        let output = client.call("get_output", serde_json::json!({})).await;
        assert_eq!(output[1]["values"]["green"], 255, "{output}");
        assert_eq!(output[1]["values"]["dimmer"], 255, "{output}");

        // Errors come back as tool errors the agent can read.
        let bad = client.request("tools/call", serde_json::json!({ "name": "remove_fixtures", "arguments": { "numbers": [99] } })).await;
        assert_eq!(bad["result"]["isError"], true, "{bad}");

        assert!(show.snapshot().undo_label.is_some());

        // A web page's request (it always carries an Origin) is refused.
        let from_page = client
            .http
            .post(&client.url)
            .header("Origin", "https://evil.example")
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .body(serde_json::json!({ "jsonrpc": "2.0", "id": 99, "method": "tools/list" }).to_string())
            .send()
            .await
            .unwrap();
        assert_eq!(from_page.status(), 403);
        cancel.cancel();
    }
}
