//! `bubbaloop dataflow` — reconstruct the runtime DAG from manifest queryables.
//!
//! Hits Zenoh directly (no MCP auth required). Each node served by the
//! Bubbaloop SDKs exposes a CBOR-encoded manifest at
//! `bubbaloop/global/{machine_id}/{instance_name}/manifest`; this
//! command queries `bubbaloop/**/manifest`, decodes the replies,
//! and prints either a tree (default) or JSON.

use std::collections::BTreeMap;
use std::time::Duration;

use argh::FromArgs;
use zenoh::query::{ConsolidationMode, QueryTarget};

use crate::cli::zenoh_session::create_zenoh_session;

/// Reconstruct and print the runtime dataflow graph.
#[derive(FromArgs, Debug)]
#[argh(subcommand, name = "dataflow")]
pub struct DataflowCommand {
    /// emit JSON instead of the default tree view
    #[argh(switch)]
    pub json: bool,

    /// zenoh endpoint to connect to (default: env BUBBALOOP_ZENOH_ENDPOINT or tcp/127.0.0.1:7447)
    #[argh(option, short = 'z')]
    pub zenoh_endpoint: Option<String>,

    /// query timeout in seconds (default 2)
    #[argh(option, default = "2")]
    pub timeout_secs: u64,

    /// include topics that were declared but never fired (default: false)
    #[argh(switch)]
    pub include_declared_but_unused: bool,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
struct IoEntry {
    topic: String,
    #[serde(default)]
    ever_fired: bool,
    #[serde(default = "default_true")]
    still_live: bool,
    #[serde(default)]
    declared_at_ns: u64,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
struct WireManifest {
    instance_name: String,
    machine_id: String,
    #[serde(default = "default_role")]
    role: String,
    #[serde(default)]
    inputs: Vec<IoEntry>,
    #[serde(default)]
    outputs: Vec<IoEntry>,
    #[serde(default)]
    schema_version: u32,
    #[serde(default)]
    started_at_ns: u64,
    #[serde(default = "default_kind")]
    node_kind: String,
}

fn default_role() -> String {
    "unknown".into()
}
fn default_kind() -> String {
    "unknown".into()
}

#[derive(Debug, serde::Serialize)]
struct DataflowGraph {
    nodes: Vec<NodeOut>,
    edges: Vec<EdgeOut>,
    orphan_inputs: Vec<DanglingOut>,
    unconsumed_outputs: Vec<DanglingOut>,
}

#[derive(Debug, serde::Serialize)]
struct NodeOut {
    instance: String,
    role: String,
    machine_id: String,
    node_kind: String,
    started_at_ns: u64,
    schema_version: u32,
}

#[derive(Debug, serde::Serialize)]
struct EdgeOut {
    from_instance: String,
    to_instance: String,
    topic: String,
}

#[derive(Debug, serde::Serialize)]
struct DanglingOut {
    instance: String,
    topic: String,
}

/// Returns true if the entry is considered "active" given the user's filter.
/// Default: both `still_live && ever_fired`. With `include_declared_but_unused`:
/// `still_live` only (no ever_fired requirement).
fn entry_active(e: &IoEntry, include_declared_but_unused: bool) -> bool {
    if include_declared_but_unused {
        e.still_live
    } else {
        e.still_live && e.ever_fired
    }
}

impl DataflowCommand {
    pub async fn run(self) -> anyhow::Result<()> {
        let session = create_zenoh_session(self.zenoh_endpoint.as_deref()).await?;
        let replies = session
            .get("bubbaloop/**/manifest")
            .target(QueryTarget::All)
            .consolidation(ConsolidationMode::None)
            .timeout(Duration::from_secs(self.timeout_secs))
            .await
            .map_err(|e| anyhow::anyhow!("zenoh get failed: {e}"))?;

        let mut manifests: Vec<WireManifest> = Vec::new();
        while let Ok(reply) = replies.recv_async().await {
            if let Ok(sample) = reply.result() {
                let bytes = sample.payload().to_bytes();
                match ciborium::from_reader::<WireManifest, _>(&bytes[..]) {
                    Ok(m) => manifests.push(m),
                    Err(e) => log::debug!("undecodable manifest: {e}"),
                }
            }
        }

        manifests.sort_by(|a, b| {
            (&a.machine_id, &a.instance_name).cmp(&(&b.machine_id, &b.instance_name))
        });
        manifests
            .dedup_by(|a, b| a.machine_id == b.machine_id && a.instance_name == b.instance_name);

        let graph = build_graph(&manifests, self.include_declared_but_unused);

        if self.json {
            println!("{}", serde_json::to_string_pretty(&graph)?);
        } else {
            print_tree(&manifests, &graph, self.include_declared_but_unused);
        }
        Ok(())
    }
}

fn build_graph(manifests: &[WireManifest], include_declared_but_unused: bool) -> DataflowGraph {
    let nodes: Vec<NodeOut> = manifests
        .iter()
        .map(|m| NodeOut {
            instance: m.instance_name.clone(),
            role: m.role.clone(),
            machine_id: m.machine_id.clone(),
            node_kind: m.node_kind.clone(),
            started_at_ns: m.started_at_ns,
            schema_version: m.schema_version,
        })
        .collect();

    let mut edges = Vec::new();
    let mut produced = std::collections::HashSet::new();
    for producer in manifests {
        for out in producer
            .outputs
            .iter()
            .filter(|e| entry_active(e, include_declared_but_unused))
        {
            produced.insert(out.topic.clone());
            for consumer in manifests {
                if consumer
                    .inputs
                    .iter()
                    .any(|i| entry_active(i, include_declared_but_unused) && i.topic == out.topic)
                {
                    edges.push(EdgeOut {
                        from_instance: producer.instance_name.clone(),
                        to_instance: consumer.instance_name.clone(),
                        topic: out.topic.clone(),
                    });
                }
            }
        }
    }

    let mut orphan_inputs = Vec::new();
    for m in manifests {
        for inp in m
            .inputs
            .iter()
            .filter(|e| entry_active(e, include_declared_but_unused))
        {
            if !produced.contains(&inp.topic) {
                orphan_inputs.push(DanglingOut {
                    instance: m.instance_name.clone(),
                    topic: inp.topic.clone(),
                });
            }
        }
    }

    let mut unconsumed_outputs = Vec::new();
    for m in manifests {
        for out in m
            .outputs
            .iter()
            .filter(|e| entry_active(e, include_declared_but_unused))
        {
            let any_consumer = manifests.iter().any(|c| {
                c.inputs
                    .iter()
                    .any(|i| entry_active(i, include_declared_but_unused) && i.topic == out.topic)
            });
            if !any_consumer {
                unconsumed_outputs.push(DanglingOut {
                    instance: m.instance_name.clone(),
                    topic: out.topic.clone(),
                });
            }
        }
    }

    DataflowGraph {
        nodes,
        edges,
        orphan_inputs,
        unconsumed_outputs,
    }
}

fn print_tree(
    manifests: &[WireManifest],
    graph: &DataflowGraph,
    include_declared_but_unused: bool,
) {
    if manifests.is_empty() {
        println!("(no nodes responded — is the daemon running and are nodes started?)");
        return;
    }
    let mut by_pub: BTreeMap<&str, BTreeMap<&str, Vec<&str>>> = BTreeMap::new();
    for e in &graph.edges {
        by_pub
            .entry(e.from_instance.as_str())
            .or_default()
            .entry(e.topic.as_str())
            .or_default()
            .push(e.to_instance.as_str());
    }
    let mut sorted: Vec<&WireManifest> = manifests.iter().collect();
    sorted.sort_by(|a, b| a.instance_name.cmp(&b.instance_name));
    for m in sorted {
        println!(
            "{} [{}]  ({}, {})",
            m.instance_name, m.role, m.node_kind, m.machine_id
        );
        let active_outputs: Vec<&IoEntry> = m
            .outputs
            .iter()
            .filter(|e| entry_active(e, include_declared_but_unused))
            .collect();
        if active_outputs.is_empty() {
            continue;
        }
        let topics = by_pub.get(m.instance_name.as_str());
        for out in active_outputs {
            let tag = if !out.ever_fired { " (idle)" } else { "" };
            let consumers = topics
                .and_then(|t| t.get(out.topic.as_str()))
                .cloned()
                .unwrap_or_default();
            if consumers.is_empty() {
                println!("  └─ {}{}  (no subscribers)", out.topic, tag);
            } else {
                let mut iter = consumers.into_iter();
                if let Some(first) = iter.next() {
                    println!("  └─ {}{} → {}", out.topic, tag, first);
                    for rest in iter {
                        let pad = " ".repeat(out.topic.len() + 5);
                        println!("  {}→ {}", pad, rest);
                    }
                }
            }
        }
    }
    if !graph.orphan_inputs.is_empty() {
        println!("\norphan inputs (no producer):");
        for d in &graph.orphan_inputs {
            println!("  {} ← {}", d.instance, d.topic);
        }
    }
}
