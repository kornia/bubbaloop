//! HTTP client for CLI → daemon REST API communication.
//!
//! Replaces the old Zenoh-based CLI → daemon path with simple HTTP requests
//! to the REST API running on the same port as MCP (default 8088).

use crate::api::{ApiCommandResponse, ApiNodeListResponse};
use serde::Deserialize;

/// Error type for daemon client operations.
#[derive(Debug, thiserror::Error)]
pub enum DaemonClientError {
    #[error("Daemon not reachable at localhost:{0}. Is it running?")]
    NotReachable(u16),
    #[error("Request failed: {0}")]
    Request(String),
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
}

pub type Result<T> = std::result::Result<T, DaemonClientError>;

/// Health response from /health endpoint.
#[derive(Debug, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub nodes_total: usize,
    #[serde(default)]
    pub nodes_running: usize,
}

/// HTTP client for daemon REST API.
pub struct DaemonClient {
    port: u16,
    client: reqwest::Client,
}

impl Default for DaemonClient {
    fn default() -> Self {
        Self::new()
    }
}

impl DaemonClient {
    /// Create a new client. Reads BUBBALOOP_MCP_PORT env var, defaults to 8088.
    pub fn new() -> Self {
        let port = std::env::var("BUBBALOOP_MCP_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(crate::mcp::MCP_PORT);
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");
        Self { port, client }
    }

    fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}/api/v1", self.port)
    }

    fn health_url(&self) -> String {
        format!("http://127.0.0.1:{}/health", self.port)
    }

    /// Check daemon health via GET /health.
    pub async fn health(&self) -> Result<HealthResponse> {
        let resp = self
            .client
            .get(self.health_url())
            .send()
            .await
            .map_err(|_| DaemonClientError::NotReachable(self.port))?;
        resp.json::<HealthResponse>()
            .await
            .map_err(|e| DaemonClientError::InvalidResponse(e.to_string()))
    }

    /// List all registered nodes via GET /api/v1/nodes.
    pub async fn list_nodes(&self) -> Result<ApiNodeListResponse> {
        let url = format!("{}/nodes", self.base_url());
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|_| DaemonClientError::NotReachable(self.port))?;
        resp.json::<ApiNodeListResponse>()
            .await
            .map_err(|e| DaemonClientError::InvalidResponse(e.to_string()))
    }

    /// Send a command to a node via POST /api/v1/nodes/{name}/command.
    pub async fn send_command(&self, name: &str, command: &str) -> Result<ApiCommandResponse> {
        let url = format!("{}/nodes/{}/command", self.base_url(), name);
        let resp = self
            .client
            .post(&url)
            .json(&serde_json::json!({ "command": command }))
            .send()
            .await
            .map_err(|_| DaemonClientError::NotReachable(self.port))?;
        resp.json::<ApiCommandResponse>()
            .await
            .map_err(|e| DaemonClientError::InvalidResponse(e.to_string()))
    }

    /// Add a node from a source path via POST /api/v1/nodes/add.
    pub async fn add_node(
        &self,
        source: &str,
        name: Option<&str>,
        config: Option<&str>,
    ) -> Result<ApiCommandResponse> {
        let url = format!("{}/nodes/add", self.base_url());
        let mut body = serde_json::json!({ "source": source });
        if let Some(n) = name {
            body["name"] = serde_json::Value::String(n.to_string());
        }
        if let Some(c) = config {
            body["config"] = serde_json::Value::String(c.to_string());
        }
        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|_| DaemonClientError::NotReachable(self.port))?;
        resp.json::<ApiCommandResponse>()
            .await
            .map_err(|e| DaemonClientError::InvalidResponse(e.to_string()))
    }

    /// Install from marketplace via POST /api/v1/nodes/install.
    pub async fn install_marketplace(&self, name: &str) -> Result<ApiCommandResponse> {
        let url = format!("{}/nodes/install", self.base_url());
        let resp = self
            .client
            .post(&url)
            .json(&serde_json::json!({ "name": name }))
            .send()
            .await
            .map_err(|_| DaemonClientError::NotReachable(self.port))?;
        resp.json::<ApiCommandResponse>()
            .await
            .map_err(|e| DaemonClientError::InvalidResponse(e.to_string()))
    }

    /// Remove a node via DELETE /api/v1/nodes/{name}.
    pub async fn remove_node(&self, name: &str) -> Result<ApiCommandResponse> {
        let url = format!("{}/nodes/{}", self.base_url(), name);
        let resp = self
            .client
            .delete(&url)
            .send()
            .await
            .map_err(|_| DaemonClientError::NotReachable(self.port))?;
        resp.json::<ApiCommandResponse>()
            .await
            .map_err(|e| DaemonClientError::InvalidResponse(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_client_default_port() {
        // Should use default port 8088 when env var is not set
        let client = DaemonClient::new();
        assert_eq!(client.base_url(), "http://127.0.0.1:8088/api/v1");
    }

    #[test]
    fn test_health_response_deserialization() {
        let json = r#"{"status": "ok", "version": "0.0.6", "nodes_total": 5, "nodes_running": 3}"#;
        let resp: HealthResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.status, "ok");
        assert_eq!(resp.nodes_total, 5);
        assert_eq!(resp.nodes_running, 3);
    }

    #[test]
    fn test_health_response_minimal() {
        let json = r#"{"status": "ok"}"#;
        let resp: HealthResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.status, "ok");
        assert_eq!(resp.nodes_total, 0);
    }
}
