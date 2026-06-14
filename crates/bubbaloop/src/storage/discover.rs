//! Topic discovery for recording (spec §3.2, §5 `storage topics`).
//!
//! Every Bubbaloop node publishes a CBOR manifest queryable at
//! `bubbaloop/global/{machine_id}/{instance}/manifest` listing its live
//! `inputs`/`outputs` with per-topic liveness (`ever_fired`, `still_live`). This
//! module queries `bubbaloop/**/manifest`, decodes the replies, and groups the
//! *output* topics (the ones a recorder would capture) by
//! `machine_id → node_instance → topic` so the user can see what's available to
//! record before starting a session.
//!
//! The Zenoh I/O ([`query_nodes`]) is kept thin; the grouping/filtering
//! ([`group`]) is pure and unit-tested.

use std::time::Duration;

use serde::Deserialize;

/// Per-topic liveness, mirrored from the node manifest's IO entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicInfo {
    /// Full Zenoh topic key.
    pub topic: String,
    /// Whether the topic has ever published a sample.
    pub ever_fired: bool,
    /// Whether the producing node still considers it live.
    pub still_live: bool,
}

impl TopicInfo {
    /// A topic is recordable-live when it's still declared and has actually
    /// fired at least once (matches `bubbaloop dataflow`'s default edge rule).
    pub fn is_live(&self) -> bool {
        self.still_live && self.ever_fired
    }
}

/// One node's published topics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredNode {
    /// Machine the node runs on.
    pub machine_id: String,
    /// Node instance name.
    pub instance_name: String,
    /// Node role (`source`/`processor`/`sink`/…).
    pub role: String,
    /// Output topics with liveness.
    pub topics: Vec<TopicInfo>,
}

/// Topics grouped under one node, for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeTopics {
    pub instance_name: String,
    pub role: String,
    pub topics: Vec<TopicInfo>,
}

/// All recordable topics on one machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineTopics {
    pub machine_id: String,
    pub nodes: Vec<NodeTopics>,
}

/// Errors from topic discovery.
#[derive(Debug, thiserror::Error)]
pub enum DiscoverError {
    /// The Zenoh query failed.
    #[error("zenoh query failed: {0}")]
    Zenoh(String),
}

// ---------------------------------------------------------------------------
// Pure grouping/filtering (unit-tested without Zenoh)
// ---------------------------------------------------------------------------

/// Group discovered nodes by `machine_id → instance → topic`, applying optional
/// `machine` (exact match) and `grep` (case-insensitive topic substring)
/// filters. Output is sorted by machine, then instance, then topic; nodes whose
/// topics are all filtered out are dropped, as are machines left empty.
pub fn group(
    nodes: Vec<DiscoveredNode>,
    grep: Option<&str>,
    machine: Option<&str>,
) -> Vec<MachineTopics> {
    let grep_lc = grep.map(|g| g.to_lowercase());
    let mut by_machine: std::collections::BTreeMap<String, Vec<NodeTopics>> =
        std::collections::BTreeMap::new();

    for node in nodes {
        if let Some(m) = machine {
            if node.machine_id != m {
                continue;
            }
        }
        let mut topics: Vec<TopicInfo> = node
            .topics
            .into_iter()
            .filter(|t| {
                grep_lc
                    .as_ref()
                    .map(|g| t.topic.to_lowercase().contains(g))
                    .unwrap_or(true)
            })
            .collect();
        if topics.is_empty() {
            continue;
        }
        topics.sort_by(|a, b| a.topic.cmp(&b.topic));
        by_machine
            .entry(node.machine_id)
            .or_default()
            .push(NodeTopics {
                instance_name: node.instance_name,
                role: node.role,
                topics,
            });
    }

    by_machine
        .into_iter()
        .map(|(machine_id, mut nodes)| {
            nodes.sort_by(|a, b| a.instance_name.cmp(&b.instance_name));
            MachineTopics { machine_id, nodes }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Zenoh query (thin I/O)
// ---------------------------------------------------------------------------

/// Wire shape of a node manifest reply (subset we need: identity + outputs).
#[derive(Debug, Deserialize)]
struct WireManifest {
    instance_name: String,
    machine_id: String,
    #[serde(default = "unknown_role")]
    role: String,
    #[serde(default)]
    outputs: Vec<WireIo>,
}

#[derive(Debug, Deserialize)]
struct WireIo {
    topic: String,
    #[serde(default)]
    ever_fired: bool,
    #[serde(default = "wire_default_true")]
    still_live: bool,
}

fn unknown_role() -> String {
    "unknown".to_string()
}
fn wire_default_true() -> bool {
    true
}

/// Query `bubbaloop/**/manifest` and return one [`DiscoveredNode`] per live node
/// (deduplicated by `machine_id`+`instance`). Undecodable replies are skipped.
pub async fn query_nodes(
    session: &zenoh::Session,
    timeout: Duration,
) -> Result<Vec<DiscoveredNode>, DiscoverError> {
    use zenoh::query::{ConsolidationMode, QueryTarget};

    let replies = session
        .get("bubbaloop/**/manifest")
        .target(QueryTarget::All)
        .consolidation(ConsolidationMode::None)
        .timeout(timeout)
        .await
        .map_err(|e| DiscoverError::Zenoh(e.to_string()))?;

    let mut seen = std::collections::BTreeSet::new();
    let mut nodes = Vec::new();
    while let Ok(reply) = replies.recv_async().await {
        if let Ok(sample) = reply.result() {
            let bytes = sample.payload().to_bytes();
            match ciborium::from_reader::<WireManifest, _>(&bytes[..]) {
                Ok(m) => {
                    if seen.insert((m.machine_id.clone(), m.instance_name.clone())) {
                        nodes.push(DiscoveredNode {
                            machine_id: m.machine_id,
                            instance_name: m.instance_name,
                            role: m.role,
                            topics: m
                                .outputs
                                .into_iter()
                                .map(|io| TopicInfo {
                                    topic: io.topic,
                                    ever_fired: io.ever_fired,
                                    still_live: io.still_live,
                                })
                                .collect(),
                        });
                    }
                }
                Err(e) => log::debug!("undecodable manifest: {e}"),
            }
        }
    }
    Ok(nodes)
}

/// Discover and group recordable topics across the fleet.
pub async fn discover(
    session: &zenoh::Session,
    timeout: Duration,
    grep: Option<&str>,
    machine: Option<&str>,
) -> Result<Vec<MachineTopics>, DiscoverError> {
    let nodes = query_nodes(session, timeout).await?;
    Ok(group(nodes, grep, machine))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn topic(t: &str, fired: bool, live: bool) -> TopicInfo {
        TopicInfo {
            topic: t.to_string(),
            ever_fired: fired,
            still_live: live,
        }
    }

    fn node(machine: &str, instance: &str, topics: Vec<TopicInfo>) -> DiscoveredNode {
        DiscoveredNode {
            machine_id: machine.to_string(),
            instance_name: instance.to_string(),
            role: "source".to_string(),
            topics,
        }
    }

    #[test]
    fn groups_by_machine_then_instance_then_topic() {
        let nodes = vec![
            node(
                "beta",
                "lidar",
                vec![topic("bubbaloop/global/beta/lidar/points", true, true)],
            ),
            node(
                "alpha",
                "cam_front",
                vec![
                    topic("bubbaloop/global/alpha/cam_front/compressed", true, true),
                    topic("bubbaloop/global/alpha/cam_front/info", false, true),
                ],
            ),
        ];
        let grouped = group(nodes, None, None);
        assert_eq!(grouped.len(), 2);
        // machines sorted: alpha before beta
        assert_eq!(grouped[0].machine_id, "alpha");
        assert_eq!(grouped[1].machine_id, "beta");
        // topics sorted within node: compressed before info
        assert!(grouped[0].nodes[0].topics[0].topic.contains("compressed"));
        assert!(grouped[0].nodes[0].topics[0].is_live());
        assert!(!grouped[0].nodes[0].topics[1].is_live()); // never fired
    }

    #[test]
    fn machine_filter_excludes_others() {
        let nodes = vec![
            node("alpha", "a", vec![topic("t1", true, true)]),
            node("beta", "b", vec![topic("t2", true, true)]),
        ];
        let grouped = group(nodes, None, Some("alpha"));
        assert_eq!(grouped.len(), 1);
        assert_eq!(grouped[0].machine_id, "alpha");
    }

    #[test]
    fn grep_filters_topics_case_insensitively_and_drops_empty_nodes() {
        let nodes = vec![
            node(
                "m",
                "cam",
                vec![
                    topic("bubbaloop/global/m/cam/Compressed", true, true),
                    topic("bubbaloop/global/m/cam/health", true, true),
                ],
            ),
            node(
                "m",
                "imu",
                vec![topic("bubbaloop/global/m/imu/data", true, true)],
            ),
        ];
        let grouped = group(nodes, Some("compressed"), None);
        // Only the cam node survives, with its one matching topic.
        assert_eq!(grouped.len(), 1);
        assert_eq!(grouped[0].nodes.len(), 1);
        assert_eq!(grouped[0].nodes[0].instance_name, "cam");
        assert_eq!(grouped[0].nodes[0].topics.len(), 1);
    }
}
