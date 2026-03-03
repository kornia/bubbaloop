//! Integration tests for the MCP server.
//!
//! Uses `MockPlatform` behind a real MCP client-server transport (in-process duplex)
//! to exercise tool routing, RBAC, serialization, and response formatting end-to-end.
//!
//! Run with: `cargo test --features test-harness --test integration_mcp`
#![cfg(feature = "test-harness")]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use bubbaloop::mcp::platform::mock::MockPlatform;
use bubbaloop::mcp::platform::NodeInfo;
use bubbaloop::mcp::BubbaLoopMcpServer;

use rmcp::model::{CallToolRequestParams, ClientInfo};
use rmcp::{ClientHandler, ServiceExt};

// ── Dummy client handler (required by rmcp) ──────────────────────────

#[derive(Debug, Clone, Default)]
struct TestClientHandler;

impl ClientHandler for TestClientHandler {
    fn get_info(&self) -> ClientInfo {
        ClientInfo::default()
    }
}

// ── Test harness ─────────────────────────────────────────────────────

/// Harness that spins up a `BubbaLoopMcpServer<MockPlatform>` on an in-process
/// duplex transport and returns an MCP client that can call tools.
struct TestHarness {
    client: rmcp::service::RunningService<rmcp::RoleClient, TestClientHandler>,
    _server_handle: tokio::task::JoinHandle<anyhow::Result<()>>,
}

impl TestHarness {
    /// Create a harness with the default MockPlatform (one "test-node").
    async fn new() -> Self {
        Self::with_mock(MockPlatform::new()).await
    }

    /// Create a harness with a custom MockPlatform.
    async fn with_mock(mock: MockPlatform) -> Self {
        let platform = Arc::new(mock);
        let server = BubbaLoopMcpServer::new(
            platform,
            None, // no auth token
            "test".to_string(),
            "test-machine".to_string(),
        );

        let (server_transport, client_transport) = tokio::io::duplex(65536);

        let server_handle = tokio::spawn(async move {
            server.serve(server_transport).await?.waiting().await?;
            anyhow::Ok(())
        });

        let client = TestClientHandler
            .serve(client_transport)
            .await
            .expect("client setup failed");

        Self {
            client,
            _server_handle: server_handle,
        }
    }

    /// Call a tool with no arguments.
    async fn call(
        &self,
        tool_name: &str,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ServiceError> {
        self.client
            .call_tool(CallToolRequestParams {
                meta: None,
                name: tool_name.to_string().into(),
                arguments: None,
                task: None,
            })
            .await
    }

    /// Call a tool with JSON arguments.
    async fn call_with_args(
        &self,
        tool_name: &str,
        args: serde_json::Value,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ServiceError> {
        self.client
            .call_tool(CallToolRequestParams {
                meta: None,
                name: tool_name.to_string().into(),
                arguments: Some(
                    args.as_object()
                        .expect("call_with_args requires a JSON object")
                        .clone(),
                ),
                task: None,
            })
            .await
    }

    /// Shut down the client and server.
    async fn shutdown(self) -> anyhow::Result<()> {
        self.client.cancel().await?;
        self._server_handle.await??;
        Ok(())
    }
}

/// Extract the text content from a CallToolResult.
fn result_text(result: &rmcp::model::CallToolResult) -> String {
    result
        .content
        .first()
        .and_then(|c| c.raw.as_text())
        .map(|t| t.text.clone())
        .unwrap_or_default()
}

/// Parse the text content as JSON.
fn result_json(result: &rmcp::model::CallToolResult) -> serde_json::Value {
    let text = result_text(result);
    match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => serde_json::Value::String(text),
    }
}

// ── Helper: build a MockPlatform with custom nodes ───────────────────

fn mock_with_nodes(nodes: Vec<NodeInfo>) -> MockPlatform {
    MockPlatform {
        nodes: Mutex::new(nodes),
        configs: Mutex::new(HashMap::new()),
        manifests: Mutex::new(Vec::new()),
    }
}

// ════════════════════════════════════════════════════════════════════════
// Integration tests
// ════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn list_tools_returns_expected_tools() {
    let h = TestHarness::new().await;
    let tools = h.client.list_tools(None).await.expect("list_tools failed");

    let tool_names: Vec<String> = tools.tools.iter().map(|t| t.name.to_string()).collect();

    // Verify key tools exist
    assert!(tool_names.contains(&"list_nodes".to_string()));
    assert!(tool_names.contains(&"get_system_status".to_string()));
    assert!(tool_names.contains(&"get_machine_info".to_string()));
    assert!(tool_names.contains(&"get_node_health".to_string()));
    assert!(tool_names.contains(&"start_node".to_string()));
    assert!(tool_names.contains(&"stop_node".to_string()));
    assert!(tool_names.contains(&"query_zenoh".to_string()));
    assert!(tool_names.contains(&"install_node".to_string()));
    assert!(tool_names.contains(&"remove_node".to_string()));

    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn list_nodes_default_mock() {
    let h = TestHarness::new().await;
    let result = h.call("list_nodes").await.unwrap();
    let json = result_json(&result);

    let nodes = json.as_array().expect("expected array");
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0]["name"], "test-node");
    assert_eq!(nodes[0]["status"], "Running");
    assert_eq!(nodes[0]["health"], "Healthy");
    assert_eq!(nodes[0]["node_type"], "rust");
    assert_eq!(nodes[0]["installed"], true);
    assert_eq!(nodes[0]["is_built"], true);

    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn list_nodes_empty() {
    let h = TestHarness::with_mock(mock_with_nodes(vec![])).await;
    let result = h.call("list_nodes").await.unwrap();
    let json = result_json(&result);

    let nodes = json.as_array().expect("expected array");
    assert!(nodes.is_empty());

    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn list_nodes_multiple() {
    let nodes = vec![
        NodeInfo {
            name: "camera".to_string(),
            status: "Running".to_string(),
            health: "Healthy".to_string(),
            node_type: "python".to_string(),
            installed: true,
            is_built: true,
        },
        NodeInfo {
            name: "detector".to_string(),
            status: "Stopped".to_string(),
            health: "Unknown".to_string(),
            node_type: "rust".to_string(),
            installed: true,
            is_built: false,
        },
    ];
    let h = TestHarness::with_mock(mock_with_nodes(nodes)).await;
    let result = h.call("list_nodes").await.unwrap();
    let json = result_json(&result);

    let arr = json.as_array().expect("expected array");
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["name"], "camera");
    assert_eq!(arr[1]["name"], "detector");
    assert_eq!(arr[1]["status"], "Stopped");

    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn get_system_status() {
    let h = TestHarness::new().await;
    let result = h.call("get_system_status").await.unwrap();
    let json = result_json(&result);

    assert_eq!(json["scope"], "test");
    assert_eq!(json["machine_id"], "test-machine");
    assert_eq!(json["nodes_total"], 1);
    assert_eq!(json["nodes_running"], 1);
    assert_eq!(json["nodes_healthy"], 1);
    assert_eq!(json["mcp_server"], "running");

    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn get_system_status_no_nodes() {
    let h = TestHarness::with_mock(mock_with_nodes(vec![])).await;
    let result = h.call("get_system_status").await.unwrap();
    let json = result_json(&result);

    assert_eq!(json["nodes_total"], 0);
    assert_eq!(json["nodes_running"], 0);
    assert_eq!(json["nodes_healthy"], 0);

    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn get_machine_info() {
    let h = TestHarness::new().await;
    let result = h.call("get_machine_info").await.unwrap();
    let json = result_json(&result);

    assert_eq!(json["machine_id"], "test-machine");
    assert_eq!(json["scope"], "test");
    // arch and os are runtime values, just verify they exist
    assert!(json["arch"].is_string());
    assert!(json["os"].is_string());
    assert!(json["hostname"].is_string());

    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn get_node_health_existing() {
    let h = TestHarness::new().await;
    let result = h
        .call_with_args(
            "get_node_health",
            serde_json::json!({"node_name": "test-node"}),
        )
        .await
        .unwrap();
    let json = result_json(&result);

    assert_eq!(json["name"], "test-node");
    assert_eq!(json["status"], "Running");

    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn get_node_health_missing() {
    let h = TestHarness::new().await;
    let result = h
        .call_with_args(
            "get_node_health",
            serde_json::json!({"node_name": "nonexistent"}),
        )
        .await
        .unwrap();
    let text = result_text(&result);

    assert!(
        text.contains("not found"),
        "Expected 'not found' in: {}",
        text
    );

    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn get_node_health_invalid_name() {
    let h = TestHarness::new().await;
    let result = h
        .call_with_args(
            "get_node_health",
            serde_json::json!({"node_name": "../etc/passwd"}),
        )
        .await
        .unwrap();
    let text = result_text(&result);

    // Validation should reject path traversal (message mentions allowed character set)
    assert!(
        text.contains("alphanumeric")
            || text.contains("1-64 characters")
            || text.contains("Invalid"),
        "Expected validation error in: {}",
        text
    );

    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn start_node_existing() {
    let h = TestHarness::new().await;
    let result = h
        .call_with_args("start_node", serde_json::json!({"node_name": "test-node"}))
        .await
        .unwrap();
    let text = result_text(&result);

    assert_eq!(text, "mock: Start executed");

    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn stop_node_existing() {
    let h = TestHarness::new().await;
    let result = h
        .call_with_args("stop_node", serde_json::json!({"node_name": "test-node"}))
        .await
        .unwrap();
    let text = result_text(&result);

    assert_eq!(text, "mock: Stop executed");

    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn restart_node_existing() {
    let h = TestHarness::new().await;
    let result = h
        .call_with_args(
            "restart_node",
            serde_json::json!({"node_name": "test-node"}),
        )
        .await
        .unwrap();
    let text = result_text(&result);

    assert_eq!(text, "mock: Restart executed");

    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn get_stream_info() {
    let h = TestHarness::new().await;
    let result = h
        .call_with_args(
            "get_stream_info",
            serde_json::json!({"node_name": "test-node"}),
        )
        .await
        .unwrap();
    let json = result_json(&result);

    assert!(json["zenoh_topic"].as_str().unwrap().contains("test-node"));
    assert_eq!(json["encoding"], "protobuf");
    assert_eq!(json["endpoint"], "tcp/localhost:7447");

    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn discover_nodes() {
    let h = TestHarness::new().await;
    let result = h.call("discover_nodes").await.unwrap();
    let text = result_text(&result);

    // MockPlatform returns "mock: query bubbaloop/**/manifest"
    assert!(text.contains("mock: query"));
    assert!(text.contains("manifest"));

    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn query_zenoh_valid() {
    let h = TestHarness::new().await;
    let result = h
        .call_with_args(
            "query_zenoh",
            serde_json::json!({"key_expr": "bubbaloop/local/jetson1/openmeteo/status"}),
        )
        .await
        .unwrap();
    let text = result_text(&result);

    assert!(text.contains("mock: query"));
    assert!(text.contains("bubbaloop/local/jetson1/openmeteo/status"));

    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn send_command_existing_node() {
    let h = TestHarness::new().await;
    let result = h
        .call_with_args(
            "send_command",
            serde_json::json!({
                "node_name": "test-node",
                "command": "capture_frame",
                "params": {"resolution": "1080p"}
            }),
        )
        .await
        .unwrap();
    let text = result_text(&result);

    // MockPlatform's send_zenoh_query returns "mock: zenoh_query <key_expr>"
    assert!(
        text.contains("mock: zenoh_query"),
        "Expected mock zenoh_query response, got: {}",
        text
    );

    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn build_node_existing() {
    let h = TestHarness::new().await;
    let result = h
        .call_with_args("build_node", serde_json::json!({"node_name": "test-node"}))
        .await
        .unwrap();
    let text = result_text(&result);

    assert_eq!(text, "mock: Build executed");

    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn get_node_logs_existing() {
    let h = TestHarness::new().await;
    let result = h
        .call_with_args(
            "get_node_logs",
            serde_json::json!({"node_name": "test-node"}),
        )
        .await
        .unwrap();
    let text = result_text(&result);

    assert_eq!(text, "mock: GetLogs executed");

    h.shutdown().await.unwrap();
}

// ════════════════════════════════════════════════════════════════════════
// Error / negative-path tests
// ════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn start_node_nonexistent() {
    let h = TestHarness::new().await;
    let result = h
        .call_with_args("start_node", serde_json::json!({"node_name": "ghost-node"}))
        .await
        .unwrap();
    let text = result_text(&result);

    assert!(
        text.contains("Error:") && text.contains("not found"),
        "Expected 'Error:' + 'not found' in: {}",
        text
    );

    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn stop_node_nonexistent() {
    let h = TestHarness::new().await;
    let result = h
        .call_with_args("stop_node", serde_json::json!({"node_name": "ghost-node"}))
        .await
        .unwrap();
    let text = result_text(&result);

    assert!(
        text.contains("Error:") && text.contains("not found"),
        "Expected 'Error:' + 'not found' in: {}",
        text
    );

    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn restart_node_nonexistent() {
    let h = TestHarness::new().await;
    let result = h
        .call_with_args(
            "restart_node",
            serde_json::json!({"node_name": "ghost-node"}),
        )
        .await
        .unwrap();
    let text = result_text(&result);

    assert!(
        text.contains("Error:") && text.contains("not found"),
        "Expected 'Error:' + 'not found' in: {}",
        text
    );

    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn build_node_nonexistent() {
    let h = TestHarness::new().await;
    let result = h
        .call_with_args("build_node", serde_json::json!({"node_name": "ghost-node"}))
        .await
        .unwrap();
    let text = result_text(&result);

    assert!(
        text.contains("Error:") && text.contains("not found"),
        "Expected 'Error:' + 'not found' in: {}",
        text
    );

    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn query_zenoh_invalid_key() {
    let h = TestHarness::new().await;
    let result = h
        .call_with_args(
            "query_zenoh",
            serde_json::json!({"key_expr": "other/not-bubbaloop/path"}),
        )
        .await
        .unwrap();
    let text = result_text(&result);

    assert!(
        text.contains("Validation error") && text.contains("bubbaloop/"),
        "Expected validation error mentioning 'bubbaloop/' prefix in: {}",
        text
    );

    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn query_zenoh_wildcard_only_rejected() {
    let h = TestHarness::new().await;
    let result = h
        .call_with_args(
            "query_zenoh",
            serde_json::json!({"key_expr": "bubbaloop/**"}),
        )
        .await
        .unwrap();
    let text = result_text(&result);

    assert!(
        text.contains("Validation error") && text.contains("too broad"),
        "Expected validation error about overly broad query in: {}",
        text
    );

    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn get_node_config_missing_config() {
    // Default mock has "test-node" but no config for it
    let h = TestHarness::new().await;
    let result = h
        .call_with_args(
            "get_node_config",
            serde_json::json!({"node_name": "test-node"}),
        )
        .await
        .unwrap();
    let text = result_text(&result);

    assert!(
        text.contains("Error:") && text.contains("not found"),
        "Expected 'Error:' + 'not found' when config is absent in: {}",
        text
    );

    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn get_node_manifest_existing() {
    let h = TestHarness::new().await;
    let result = h
        .call_with_args(
            "get_node_manifest",
            serde_json::json!({"node_name": "test-node"}),
        )
        .await
        .unwrap();
    let text = result_text(&result);

    // get_node_manifest now reads from cached manifests via get_manifests()
    assert!(
        text.contains("test-node") && text.contains("sensor"),
        "Expected manifest with node name and capability in: {}",
        text
    );

    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn list_commands_no_commands_available() {
    let h = TestHarness::new().await;
    let result = h
        .call_with_args(
            "list_commands",
            serde_json::json!({"node_name": "test-node"}),
        )
        .await
        .unwrap();
    let text = result_text(&result);

    // MockPlatform returns non-JSON manifest, so list_commands parses nothing
    assert!(
        text.contains("No commands available"),
        "Expected 'No commands available' when manifest has no commands in: {}",
        text
    );

    h.shutdown().await.unwrap();
}

// ── install_node tests ──────────────────────────────────────────────────

#[tokio::test]
async fn install_node_valid_source() {
    let h = TestHarness::new().await;
    let result = h
        .call_with_args(
            "install_node",
            serde_json::json!({"source": "/path/to/my-node"}),
        )
        .await
        .unwrap();
    let text = result_text(&result);

    assert!(
        text.contains("installed") && text.contains("/path/to/my-node"),
        "Expected mock install success in: {}",
        text
    );

    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn install_node_github_shorthand() {
    let h = TestHarness::new().await;
    let result = h
        .call_with_args(
            "install_node",
            serde_json::json!({"source": "kornia/bubbaloop-nodes-official"}),
        )
        .await
        .unwrap();
    let text = result_text(&result);

    assert!(
        text.contains("installed"),
        "Expected mock install success for GitHub shorthand in: {}",
        text
    );

    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn install_node_empty_source_rejected() {
    let h = TestHarness::new().await;
    let result = h
        .call_with_args("install_node", serde_json::json!({"source": ""}))
        .await
        .unwrap();
    let text = result_text(&result);

    assert!(
        text.contains("Error") && text.contains("empty"),
        "Expected empty source error in: {}",
        text
    );

    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn install_node_shell_metacharacters_rejected() {
    let h = TestHarness::new().await;

    for bad_source in &["foo;rm -rf /", "foo|bar", "foo&bar"] {
        let result = h
            .call_with_args("install_node", serde_json::json!({"source": bad_source}))
            .await
            .unwrap();
        let text = result_text(&result);

        assert!(
            text.contains("Error") && text.contains("invalid characters"),
            "Expected invalid characters error for '{}' in: {}",
            bad_source,
            text
        );
    }

    h.shutdown().await.unwrap();
}

// ── remove_node tests ───────────────────────────────────────────────────

#[tokio::test]
async fn remove_node_existing() {
    let h = TestHarness::new().await;

    // Default mock has "test-node" — remove it
    let result = h
        .call_with_args("remove_node", serde_json::json!({"node_name": "test-node"}))
        .await
        .unwrap();
    let text = result_text(&result);

    assert!(
        text.contains("removed") && text.contains("test-node"),
        "Expected mock remove success in: {}",
        text
    );

    // Verify node is actually gone from the mock
    let list_result = h.call("list_nodes").await.unwrap();
    let json = result_json(&list_result);
    let nodes = json.as_array().expect("expected array");
    assert!(nodes.is_empty(), "Expected no nodes after removal");

    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn remove_node_nonexistent() {
    let h = TestHarness::new().await;
    let result = h
        .call_with_args(
            "remove_node",
            serde_json::json!({"node_name": "no-such-node"}),
        )
        .await
        .unwrap();
    let text = result_text(&result);

    assert!(
        text.contains("Error"),
        "Expected error for nonexistent node in: {}",
        text
    );

    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn remove_node_invalid_name_rejected() {
    let h = TestHarness::new().await;
    let result = h
        .call_with_args(
            "remove_node",
            serde_json::json!({"node_name": "../etc/passwd"}),
        )
        .await
        .unwrap();
    let text = result_text(&result);

    assert!(
        text.contains("Node name may only contain")
            || text.contains("Invalid")
            || text.contains("Error"),
        "Expected validation error for invalid node name in: {}",
        text
    );

    h.shutdown().await.unwrap();
}

// ── uninstall_node tests ─────────────────────────────────────────────

#[tokio::test]
async fn uninstall_node_existing() {
    let h = TestHarness::new().await;
    let result = h
        .call_with_args(
            "uninstall_node",
            serde_json::json!({"node_name": "test-node"}),
        )
        .await
        .unwrap();
    let text = result_text(&result);

    assert_eq!(text, "mock: Uninstall executed");

    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn uninstall_node_nonexistent() {
    let h = TestHarness::new().await;
    let result = h
        .call_with_args(
            "uninstall_node",
            serde_json::json!({"node_name": "ghost-node"}),
        )
        .await
        .unwrap();
    let text = result_text(&result);

    assert!(
        text.contains("Error:") && text.contains("not found"),
        "Expected error for nonexistent node in: {}",
        text
    );

    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn uninstall_node_invalid_name_rejected() {
    let h = TestHarness::new().await;
    let result = h
        .call_with_args(
            "uninstall_node",
            serde_json::json!({"node_name": "../etc/passwd"}),
        )
        .await
        .unwrap();
    let text = result_text(&result);

    assert!(
        text.contains("alphanumeric") || text.contains("Invalid") || text.contains("Error"),
        "Expected validation error in: {}",
        text
    );

    h.shutdown().await.unwrap();
}

// ── clean_node tests ─────────────────────────────────────────────────

#[tokio::test]
async fn clean_node_existing() {
    let h = TestHarness::new().await;
    let result = h
        .call_with_args("clean_node", serde_json::json!({"node_name": "test-node"}))
        .await
        .unwrap();
    let text = result_text(&result);

    assert_eq!(text, "mock: Clean executed");

    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn clean_node_nonexistent() {
    let h = TestHarness::new().await;
    let result = h
        .call_with_args("clean_node", serde_json::json!({"node_name": "ghost-node"}))
        .await
        .unwrap();
    let text = result_text(&result);

    assert!(
        text.contains("Error:") && text.contains("not found"),
        "Expected error for nonexistent node in: {}",
        text
    );

    h.shutdown().await.unwrap();
}

// ── enable_autostart tests ───────────────────────────────────────────

#[tokio::test]
async fn enable_autostart_existing() {
    let h = TestHarness::new().await;
    let result = h
        .call_with_args(
            "enable_autostart",
            serde_json::json!({"node_name": "test-node"}),
        )
        .await
        .unwrap();
    let text = result_text(&result);

    assert_eq!(text, "mock: EnableAutostart executed");

    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn enable_autostart_nonexistent() {
    let h = TestHarness::new().await;
    let result = h
        .call_with_args(
            "enable_autostart",
            serde_json::json!({"node_name": "ghost-node"}),
        )
        .await
        .unwrap();
    let text = result_text(&result);

    assert!(
        text.contains("Error:") && text.contains("not found"),
        "Expected error for nonexistent node in: {}",
        text
    );

    h.shutdown().await.unwrap();
}

// ── disable_autostart tests ──────────────────────────────────────────

#[tokio::test]
async fn disable_autostart_existing() {
    let h = TestHarness::new().await;
    let result = h
        .call_with_args(
            "disable_autostart",
            serde_json::json!({"node_name": "test-node"}),
        )
        .await
        .unwrap();
    let text = result_text(&result);

    assert_eq!(text, "mock: DisableAutostart executed");

    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn disable_autostart_nonexistent() {
    let h = TestHarness::new().await;
    let result = h
        .call_with_args(
            "disable_autostart",
            serde_json::json!({"node_name": "ghost-node"}),
        )
        .await
        .unwrap();
    let text = result_text(&result);

    assert!(
        text.contains("Error:") && text.contains("not found"),
        "Expected error for nonexistent node in: {}",
        text
    );

    h.shutdown().await.unwrap();
}

// ── install_node marketplace routing tests ───────────────────────────

#[tokio::test]
async fn install_node_marketplace_name() {
    let h = TestHarness::new().await;
    let result = h
        .call_with_args("install_node", serde_json::json!({"source": "rtsp-camera"}))
        .await
        .unwrap();
    let text = result_text(&result);

    // MockPlatform.install_from_marketplace returns "mock: installed 'rtsp-camera' from marketplace"
    assert!(
        text.contains("marketplace") && text.contains("rtsp-camera"),
        "Expected marketplace install for simple name in: {}",
        text
    );

    h.shutdown().await.unwrap();
}

#[tokio::test]
async fn install_node_path_still_works() {
    let h = TestHarness::new().await;
    let result = h
        .call_with_args(
            "install_node",
            serde_json::json!({"source": "/path/to/my-node"}),
        )
        .await
        .unwrap();
    let text = result_text(&result);

    // MockPlatform.install_node returns "mock: installed node from /path/to/my-node"
    assert!(
        text.contains("installed") && text.contains("/path/to/my-node"),
        "Expected path install for absolute path in: {}",
        text
    );

    h.shutdown().await.unwrap();
}

// ── list_tools includes new tools ────────────────────────────────────

#[tokio::test]
async fn list_tools_includes_new_tools() {
    let h = TestHarness::new().await;
    let tools = h.client.list_tools(None).await.expect("list_tools failed");

    let tool_names: Vec<String> = tools.tools.iter().map(|t| t.name.to_string()).collect();

    assert!(
        tool_names.contains(&"uninstall_node".to_string()),
        "Missing uninstall_node in: {:?}",
        tool_names
    );
    assert!(
        tool_names.contains(&"clean_node".to_string()),
        "Missing clean_node in: {:?}",
        tool_names
    );
    assert!(
        tool_names.contains(&"enable_autostart".to_string()),
        "Missing enable_autostart in: {:?}",
        tool_names
    );
    assert!(
        tool_names.contains(&"disable_autostart".to_string()),
        "Missing disable_autostart in: {:?}",
        tool_names
    );

    h.shutdown().await.unwrap();
}
