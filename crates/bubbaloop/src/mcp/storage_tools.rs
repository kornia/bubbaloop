//! Storage MCP tools (spec §6) — the read-only **observability** slice.
//!
//! The spec's §6.1 completeness rule pairs every `storage` CLI verb with an MCP
//! tool because "agents are first-class operators … anything a human can do at
//! the terminal, an agent can do through MCP." The most load-bearing of those for
//! an agent are the **read** tools: an agent must be able to see what is
//! configured, what is recorded, and what profiles exist *before* it acts
//! (§6.2 stresses this for `storage_config_get` / `storage_list`).
//!
//! This module registers those read tools as a second `ToolRouter` merged into
//! the server in [`BubbaLoopMcpServer::new`](super::BubbaLoopMcpServer). They are
//! process-local (filesystem under `~/.bubbaloop`), so they dispatch straight to
//! the [`storage`](crate::storage) core rather than through `PlatformOperations`.
//! The mutating tools (`storage_upload/download/reconcile/rm/configure/profile_*`)
//! and the Zenoh/long-running ones (`storage_topics_list`, `storage_replay_*`,
//! and the recorder-backed `storage_record_*`) land in a follow-up.

use super::platform::PlatformOperations;
use super::BubbaLoopMcpServer;
use crate::storage::{self, manifest, profile};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

/// The names of the storage tools registered by this module — used by the RBAC
/// map and the CLI↔MCP parity check so the two never drift.
#[allow(dead_code)] // referenced from rbac tests + the future §6.1 parity test
pub(crate) const STORAGE_READ_TOOLS: &[&str] = &[
    "storage_config_get",
    "storage_list",
    "storage_info",
    "storage_profile_list",
    "storage_profile_show",
];

#[derive(Deserialize, JsonSchema)]
pub(crate) struct StorageNameRequest {
    /// Name of the recording (for `storage_info`) or profile (for
    /// `storage_profile_show`).
    pub(crate) name: String,
}

#[derive(Deserialize, JsonSchema, Default)]
pub(crate) struct StorageListRequest {
    /// Only list recordings from this machine id.
    #[serde(default)]
    machine: Option<String>,
    /// Only list recordings carrying this tag.
    #[serde(default)]
    tag: Option<String>,
    /// Cap the number of summaries returned (default 100) to bound token use.
    #[serde(default)]
    limit: Option<u32>,
}

/// Serialize `v` as pretty JSON into a successful tool result. Shared with the
/// mutating storage tools in [`storage_tools_write`](super::storage_tools_write).
pub(crate) fn ok_json<T: serde::Serialize>(v: &T) -> CallToolResult {
    CallToolResult::success(vec![Content::text(
        serde_json::to_string_pretty(v).unwrap_or_else(|_| "null".to_string()),
    )])
}

/// A user-facing error returned as tool text (matches the existing tool
/// convention of surfacing errors as content rather than protocol errors).
pub(crate) fn err_text(msg: impl Into<String>) -> CallToolResult {
    CallToolResult::success(vec![Content::text(format!("Error: {}", msg.into()))])
}

/// The §6.2 list summary for one recording.
fn recording_summary(r: &storage::Recording) -> serde_json::Value {
    serde_json::json!({
        "name": r.name,
        "machine_id": r.machine_id,
        "duration_ns": r.duration_ns,
        "size_bytes": r.size_bytes,
        "chunk_count": r.chunks.len(),
        "uploaded_chunk_count": r.uploaded_chunk_count(),
        "started_at_ns": r.started_at_ns,
        "tags": r.tags,
    })
}

#[tool_router(router = storage_tool_router, vis = "pub(crate)")]
impl<P: PlatformOperations> BubbaLoopMcpServer<P> {
    #[tool(
        name = "storage_config_get",
        description = "Return the current [storage] config block (backend, endpoint, bucket, auto_upload, retention). Secrets are never included — they live in secrets.toml. Lets an agent see what storage is configured before suggesting changes.",
        annotations(
            title = "Get storage config",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn storage_config_get(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=storage_config_get");
        match storage::StorageConfig::load() {
            Ok(cfg) => Ok(ok_json(&cfg)),
            Err(e) => Ok(err_text(format!("no storage config available: {e}"))),
        }
    }

    #[tool(
        name = "storage_list",
        description = "List local recordings with a one-line summary each (name, machine_id, duration_ns, size_bytes, chunk_count, uploaded_chunk_count, started_at_ns, tags). Optional machine/tag filters; limit defaults to 100.",
        annotations(
            title = "List recordings",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn storage_list(
        &self,
        Parameters(req): Parameters<StorageListRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=storage_list");
        let root = match storage::recordings_dir() {
            Ok(r) => r,
            Err(e) => return Ok(err_text(format!("cannot resolve recordings dir: {e}"))),
        };
        let mut summaries: Vec<(u64, serde_json::Value)> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&root) {
            for entry in entries.flatten() {
                let dir = entry.path();
                if !dir.is_dir() {
                    continue;
                }
                let Ok(rec) = manifest::load_dir(&dir) else {
                    continue;
                };
                if let Some(m) = &req.machine {
                    if &rec.machine_id != m {
                        continue;
                    }
                }
                if let Some(t) = &req.tag {
                    if !rec.tags.contains(t) {
                        continue;
                    }
                }
                summaries.push((rec.started_at_ns, recording_summary(&rec)));
            }
        }
        // Most recent first, then bound the count.
        summaries.sort_by_key(|(ts, _)| std::cmp::Reverse(*ts));
        let limit = req.limit.unwrap_or(100) as usize;
        let out: Vec<serde_json::Value> =
            summaries.into_iter().take(limit).map(|(_, v)| v).collect();
        Ok(ok_json(&out))
    }

    #[tool(
        name = "storage_info",
        description = "Return the full parsed manifest.json for one recording, including per-chunk upload status. Use storage_list to discover names.",
        annotations(
            title = "Recording info",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn storage_info(
        &self,
        Parameters(req): Parameters<StorageNameRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=storage_info name={}", req.name);
        let dir = match storage::recording_dir(&req.name) {
            Ok(d) => d,
            Err(e) => return Ok(err_text(format!("invalid recording name: {e}"))),
        };
        match manifest::load_dir(&dir) {
            Ok(rec) => Ok(ok_json(&rec)),
            Err(e) => Ok(err_text(format!("no recording '{}': {e}", req.name))),
        }
    }

    #[tool(
        name = "storage_profile_list",
        description = "List recording profiles in ~/.bubbaloop/profiles as {name, description, profile_sha256}.",
        annotations(
            title = "List profiles",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn storage_profile_list(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=storage_profile_list");
        let dir = match profile::profiles_dir() {
            Ok(d) => d,
            Err(e) => return Ok(err_text(format!("cannot resolve profiles dir: {e}"))),
        };
        let mut out = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("yaml") {
                    continue;
                }
                let Ok(p) = profile::load(&path) else {
                    continue;
                };
                out.push(serde_json::json!({
                    "name": p.name,
                    "description": p.description,
                    "profile_sha256": profile::canonical_sha256(&p),
                }));
            }
        }
        out.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
        Ok(ok_json(&out))
    }

    #[tool(
        name = "storage_profile_show",
        description = "Return one recording profile (parsed to JSON). Use storage_profile_list to discover names.",
        annotations(
            title = "Show profile",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn storage_profile_show(
        &self,
        Parameters(req): Parameters<StorageNameRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=storage_profile_show name={}", req.name);
        match profile::load_named(&req.name) {
            Ok(p) => Ok(ok_json(&p)),
            Err(e) => Ok(err_text(format!("no profile '{}': {e}", req.name))),
        }
    }
}
