use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use zenoh::Session;

use crate::app::NodeInfo;

#[allow(dead_code)]
const API_PREFIX: &str = "bubbaloop/daemon/api";
const API_HEALTH: &str = "bubbaloop/daemon/api/health";
const API_NODES: &str = "bubbaloop/daemon/api/nodes";
const API_NODES_ADD: &str = "bubbaloop/daemon/api/nodes/add";

#[derive(Debug, Serialize, Deserialize)]
struct HealthResponse {
    status: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct NodeState {
    name: String,
    path: String,
    status: String,
    #[allow(dead_code)]
    installed: bool,
    #[allow(dead_code)]
    autostart_enabled: bool,
    version: String,
    description: String,
    node_type: String,
    is_built: bool,
    #[allow(dead_code)]
    build_output: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct NodeListResponse {
    nodes: Vec<NodeState>,
    #[allow(dead_code)]
    timestamp_ms: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct CommandRequest {
    command: String,
    node_path: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct CommandResponse {
    success: bool,
    message: String,
    #[allow(dead_code)]
    output: String,
}

pub struct DaemonClient {
    session: Arc<Session>,
}

impl DaemonClient {
    pub async fn new() -> Result<Self> {
        let config = zenoh::Config::default();
        let session = zenoh::open(config)
            .await
            .map_err(|e| anyhow!("Failed to connect to zenoh: {}", e))?;
        Ok(Self {
            session: Arc::new(session),
        })
    }

    async fn query<T: for<'de> Deserialize<'de>>(
        &self,
        key_expr: &str,
        payload: Option<&str>,
    ) -> Result<T> {
        let mut getter = self.session.get(key_expr);

        if let Some(p) = payload {
            getter = getter.payload(p.as_bytes());
        }

        let replies = getter
            .timeout(Duration::from_millis(500))
            .await
            .map_err(|e| anyhow!("Zenoh query failed: {}", e))?;

        while let Ok(reply) = replies.recv_async().await {
            if let Ok(sample) = reply.result() {
                let bytes = sample.payload().to_bytes();
                let text = String::from_utf8_lossy(&bytes);
                let result: T = serde_json::from_str(&text)?;
                return Ok(result);
            }
        }

        Err(anyhow!("No reply received for {}", key_expr))
    }

    pub async fn is_available(&self) -> bool {
        match self.query::<HealthResponse>(API_HEALTH, None).await {
            Ok(response) => response.status == "ok",
            Err(_) => false,
        }
    }

    pub async fn list_nodes(&self) -> Result<Vec<NodeInfo>> {
        let response: NodeListResponse = self.query(API_NODES, None).await?;
        Ok(response
            .nodes
            .into_iter()
            .map(|n| NodeInfo {
                name: n.name,
                path: n.path,
                version: n.version,
                node_type: n.node_type,
                description: n.description,
                status: n.status,
                is_built: n.is_built,
            })
            .collect())
    }

    pub async fn execute_command(&self, node_name: &str, command: &str) -> Result<()> {
        let key_expr = format!("{}/{}/command", API_NODES, node_name);
        let payload = serde_json::to_string(&CommandRequest {
            command: command.to_string(),
            node_path: String::new(),
        })?;

        let response: CommandResponse = self.query(&key_expr, Some(&payload)).await?;

        if response.success {
            Ok(())
        } else {
            Err(anyhow!("{}", response.message))
        }
    }

    pub async fn add_node(&self, path: &str) -> Result<()> {
        let payload = serde_json::to_string(&CommandRequest {
            command: "add".to_string(),
            node_path: path.to_string(),
        })?;

        let response: CommandResponse = self.query(API_NODES_ADD, Some(&payload)).await?;

        if response.success {
            Ok(())
        } else {
            Err(anyhow!("{}", response.message))
        }
    }
}
