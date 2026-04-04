use std::{collections::HashSet, sync::Arc, time::Duration};

use zenoh::handlers::FifoChannel;

use crate::error::{NodeError, Result};

/// Identity of a live bubbaloop node discovered via health heartbeats.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeInfo {
    pub scope: String,
    pub machine_id: String,
    pub node_name: String,
}

impl NodeInfo {
    /// `bubbaloop/{scope}/{machine_id}/{node_name}`
    pub fn base_topic(&self) -> String {
        format!(
            "bubbaloop/{}/{}/{}",
            self.scope, self.machine_id, self.node_name
        )
    }

    /// `bubbaloop/{scope}/{machine_id}/{node_name}/schema`
    pub fn schema_topic(&self) -> String {
        format!("{}/schema", self.base_topic())
    }

    /// `bubbaloop/{scope}/{machine_id}/{node_name}/{resource}`
    pub fn topic(&self, resource: &str) -> String {
        format!("{}/{}", self.base_topic(), resource)
    }
}

impl std::fmt::Display for NodeInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.base_topic())
    }
}

/// Discover all live nodes by collecting health heartbeats for `timeout`.
///
/// Nodes publish `"ok"` to `bubbaloop/{scope}/{machine_id}/{node_name}/health`
/// every 5 seconds. Waiting slightly longer than one interval (default: 6.5 s)
/// is enough to hear from every live node.
///
/// Returns a sorted, deduplicated [`Vec<NodeInfo>`].
///
/// # Example
///
/// ```ignore
/// let nodes = discover_nodes(&session, Duration::from_secs_f32(6.5)).await?;
/// for node in &nodes {
///     println!("{}", node);          // bubbaloop/local/nvidia_orin00/tapo_terrace
///     println!("{}", node.schema_topic());
/// }
/// ```
pub async fn discover_nodes(
    session: &Arc<zenoh::Session>,
    timeout: Duration,
) -> Result<Vec<NodeInfo>> {
    let subscriber = session
        .declare_subscriber("bubbaloop/**/health")
        .with(FifoChannel::new(32))
        .await
        .map_err(|e| NodeError::SubscriberDeclare {
            topic: "bubbaloop/**/health".to_string(),
            source: e,
        })?;

    let mut seen: HashSet<NodeInfo> = HashSet::new();
    let rx = subscriber.handler();

    // Drain for `timeout`, collecting every unique health sender.
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, rx.recv_async()).await {
            Ok(Ok(sample)) => {
                if let Some(info) = parse_health_key(sample.key_expr().as_str()) {
                    seen.insert(info);
                }
            }
            // channel closed or timed out — done
            Ok(Err(_)) | Err(_) => break,
        }
    }

    let mut nodes: Vec<NodeInfo> = seen.into_iter().collect();
    nodes.sort();
    Ok(nodes)
}

/// Parse `bubbaloop/{scope}/{machine_id}/{node_name}/health` → `NodeInfo`.
fn parse_health_key(key: &str) -> Option<NodeInfo> {
    let parts: Vec<&str> = key.split('/').collect();
    // ["bubbaloop", scope, machine_id, node_name, "health"]
    if parts.len() != 5 || parts[0] != "bubbaloop" || parts[4] != "health" {
        return None;
    }
    Some(NodeInfo {
        scope: parts[1].to_string(),
        machine_id: parts[2].to_string(),
        node_name: parts[3].to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_health_key() {
        let info = parse_health_key("bubbaloop/local/nvidia_orin00/tapo_terrace/health").unwrap();
        assert_eq!(info.scope, "local");
        assert_eq!(info.machine_id, "nvidia_orin00");
        assert_eq!(info.node_name, "tapo_terrace");
        assert_eq!(
            info.base_topic(),
            "bubbaloop/local/nvidia_orin00/tapo_terrace"
        );
        assert_eq!(
            info.schema_topic(),
            "bubbaloop/local/nvidia_orin00/tapo_terrace/schema"
        );
        assert_eq!(
            info.topic("compressed"),
            "bubbaloop/local/nvidia_orin00/tapo_terrace/compressed"
        );
    }

    #[test]
    fn parse_rejects_bad_keys() {
        assert!(parse_health_key("bubbaloop/local/host/node").is_none()); // no /health
        assert!(parse_health_key("other/local/host/node/health").is_none()); // wrong prefix
        assert!(parse_health_key("bubbaloop/local/host/node/data").is_none()); // not health
        assert!(parse_health_key("bubbaloop/a/b/c/d/health").is_none()); // too many parts
    }

    #[test]
    fn display_and_ordering() {
        let mut nodes = vec![
            NodeInfo {
                scope: "local".into(),
                machine_id: "host".into(),
                node_name: "z_node".into(),
            },
            NodeInfo {
                scope: "local".into(),
                machine_id: "host".into(),
                node_name: "a_node".into(),
            },
        ];
        nodes.sort();
        assert_eq!(nodes[0].node_name, "a_node");
        assert_eq!(format!("{}", nodes[1]), "bubbaloop/local/host/z_node");
    }
}
