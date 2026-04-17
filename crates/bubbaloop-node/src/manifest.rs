//! Dataflow manifest queryable.
//!
//! Every node served by this SDK exposes a Zenoh queryable at
//! `bubbaloop/global/{machine_id}/{instance_name}/manifest`.
//! The reply is a CBOR-encoded [`Manifest`] that lists the absolute
//! topic suffixes the node has actually published to and subscribed from,
//! each tagged with liveness bits (`declared_at_ns`, `ever_fired`,
//! `still_live`).
//!
//! This is the source of truth used by the `dataflow` MCP tool to
//! reconstruct the runtime DAG without ever parsing config YAML.
//! Because the lists are populated by the SDK on every publisher /
//! subscriber declaration, the graph can never drift from real wire
//! usage.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::context::NodeContext;
use crate::error::{NodeError, Result};

/// Schema version emitted in every reply. Bump on breaking changes.
///
/// v1: inputs/outputs were plain `Vec<String>`.
/// v2: inputs/outputs are `Vec<IoEntry>` with liveness bits.
pub const MANIFEST_SCHEMA_VERSION: u32 = 2;

/// Per-topic liveness bookkeeping kept by [`NodeContext`].
///
/// Populated when a publisher/subscriber is declared, flipped on the first
/// `put()` / `recv()`, and flipped back on drop/undeclare. Never removed —
/// ghost entries are kept for history so the dataflow tool can distinguish
/// "declared but idle" from "torn down".
#[derive(Debug, Clone)]
pub struct Liveness {
    pub declared_at_ns: u64,
    pub ever_fired: bool,
    pub still_live: bool,
}

impl Liveness {
    pub fn new(declared_at_ns: u64) -> Self {
        Self {
            declared_at_ns,
            ever_fired: false,
            still_live: true,
        }
    }
}

/// Wire entry for an input/output topic in the manifest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IoEntry {
    pub topic: String,
    pub ever_fired: bool,
    pub still_live: bool,
    pub declared_at_ns: u64,
}

/// Wire-level node role. `Unknown` is the default for nodes that do not
/// declare `role:` in their config.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Source,
    Processor,
    Sink,
    Unknown,
}

impl Role {
    pub fn from_str_lossy(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "source" => Role::Source,
            "processor" => Role::Processor,
            "sink" => Role::Sink,
            _ => Role::Unknown,
        }
    }
}

/// Wire payload for `{instance}/manifest` replies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub instance_name: String,
    pub machine_id: String,
    pub role: Role,
    /// Absolute topic suffixes consumed (relative to
    /// `bubbaloop/{global|local}/{machine_id}/`), with per-topic liveness.
    pub inputs: Vec<IoEntry>,
    /// Absolute topic suffixes published, with per-topic liveness.
    pub outputs: Vec<IoEntry>,
    pub schema_version: u32,
    pub started_at_ns: u64,
    pub node_kind: String,
}

/// Queryable key for a node's dataflow manifest.
pub fn manifest_topic(machine_id: &str, instance_name: &str) -> String {
    format!("bubbaloop/global/{}/{}/manifest", machine_id, instance_name)
}

/// Convert internal liveness map into the wire `IoEntry` list.
pub(crate) fn snapshot_entries(map: &BTreeMap<String, Liveness>) -> Vec<IoEntry> {
    map.iter()
        .map(|(topic, l)| IoEntry {
            topic: topic.clone(),
            ever_fired: l.ever_fired,
            still_live: l.still_live,
            declared_at_ns: l.declared_at_ns,
        })
        .collect()
}

/// Build the current manifest snapshot from a context + config.
pub fn build_manifest(
    ctx: &NodeContext,
    role: Role,
    started_at_ns: u64,
    node_kind: &str,
) -> Manifest {
    Manifest {
        instance_name: ctx.instance_name.clone(),
        machine_id: ctx.machine_id.clone(),
        role,
        inputs: ctx.inputs_snapshot(),
        outputs: ctx.outputs_snapshot(),
        schema_version: MANIFEST_SCHEMA_VERSION,
        started_at_ns,
        node_kind: node_kind.to_string(),
    }
}

/// Spawn a background task that serves the dataflow manifest queryable
/// for this node. Replies are CBOR-encoded and rebuilt on every query so
/// publishers/subscribers declared *after* startup are still reflected.
#[allow(clippy::too_many_arguments)]
pub async fn spawn_manifest_queryable(
    session: Arc<zenoh::Session>,
    machine_id: String,
    instance_name: String,
    role: Role,
    started_at_ns: u64,
    node_kind: &'static str,
    inputs: Arc<Mutex<BTreeMap<String, Liveness>>>,
    outputs: Arc<Mutex<BTreeMap<String, Liveness>>>,
    mut shutdown_rx: watch::Receiver<()>,
) -> Result<tokio::task::JoinHandle<()>> {
    let key = manifest_topic(&machine_id, &instance_name);
    log::info!("Dataflow manifest queryable: {}", key);

    let queryable = session
        .declare_queryable(&key)
        .await
        .map_err(|e| NodeError::PublisherDeclare {
            topic: key.clone(),
            source: e,
        })?;

    let handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                biased;
                _ = shutdown_rx.changed() => {
                    log::debug!("Manifest queryable stopping");
                    break;
                }
                query = queryable.recv_async() => {
                    let Ok(query) = query else { break };
                    let snapshot = Manifest {
                        instance_name: instance_name.clone(),
                        machine_id: machine_id.clone(),
                        role,
                        inputs: snapshot_entries(&inputs.lock().expect("inputs mutex poisoned")),
                        outputs: snapshot_entries(&outputs.lock().expect("outputs mutex poisoned")),
                        schema_version: MANIFEST_SCHEMA_VERSION,
                        started_at_ns,
                        node_kind: node_kind.to_string(),
                    };
                    let mut bytes = Vec::new();
                    if let Err(e) = ciborium::into_writer(&snapshot, &mut bytes) {
                        log::warn!("Manifest CBOR encode failed: {}", e);
                        continue;
                    }
                    if let Err(e) = query.reply(query.key_expr(), bytes).await {
                        log::warn!("Manifest reply failed: {}", e);
                    }
                }
            }
        }
    });

    Ok(handle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_parses_known_values() {
        assert_eq!(Role::from_str_lossy("source"), Role::Source);
        assert_eq!(Role::from_str_lossy("Processor"), Role::Processor);
        assert_eq!(Role::from_str_lossy("SINK"), Role::Sink);
        assert_eq!(Role::from_str_lossy("anything-else"), Role::Unknown);
        assert_eq!(Role::from_str_lossy(""), Role::Unknown);
    }

    #[test]
    fn manifest_topic_format() {
        assert_eq!(
            manifest_topic("jetson_01", "tapo_terrace"),
            "bubbaloop/global/jetson_01/tapo_terrace/manifest"
        );
    }

    #[test]
    fn manifest_roundtrips_via_cbor() {
        let m = Manifest {
            instance_name: "n1".into(),
            machine_id: "m1".into(),
            role: Role::Processor,
            inputs: vec![IoEntry {
                topic: "upstream/raw".into(),
                ever_fired: true,
                still_live: true,
                declared_at_ns: 10,
            }],
            outputs: vec![IoEntry {
                topic: "n1/out".into(),
                ever_fired: false,
                still_live: true,
                declared_at_ns: 20,
            }],
            schema_version: MANIFEST_SCHEMA_VERSION,
            started_at_ns: 42,
            node_kind: "rust".into(),
        };
        let mut buf = Vec::new();
        ciborium::into_writer(&m, &mut buf).unwrap();
        let back: Manifest = ciborium::from_reader(&buf[..]).unwrap();
        assert_eq!(back.instance_name, "n1");
        assert_eq!(back.role, Role::Processor);
        assert_eq!(back.inputs.len(), 1);
        assert!(back.inputs[0].ever_fired);
        assert_eq!(back.outputs.len(), 1);
        assert!(!back.outputs[0].ever_fired);
        assert_eq!(back.schema_version, MANIFEST_SCHEMA_VERSION);
    }

    #[test]
    fn snapshot_entries_preserves_liveness_bits() {
        let mut m = BTreeMap::new();
        m.insert(
            "a/out".into(),
            Liveness {
                declared_at_ns: 7,
                ever_fired: true,
                still_live: false,
            },
        );
        let out = snapshot_entries(&m);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].topic, "a/out");
        assert!(out[0].ever_fired);
        assert!(!out[0].still_live);
        assert_eq!(out[0].declared_at_ns, 7);
    }
}
