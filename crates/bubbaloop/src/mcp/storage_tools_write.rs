//! Storage MCP tools (spec §6) — the **mutating**, **Zenoh**, and **replay**
//! slice that completes the §6.1 CLI↔MCP parity surface.
//!
//! The read-only observability tools live in
//! [`storage_tools`](super::storage_tools); this module adds everything that
//! changes state or talks to the live fleet:
//!
//! - mutating: `storage_configure`, `storage_profile_create`,
//!   `storage_profile_rm`, `storage_upload`, `storage_download`,
//!   `storage_reconcile`, `storage_rm`,
//! - Zenoh: `storage_topics_list` (live fleet topics via the node manifests),
//! - replay: `storage_replay_start` / `storage_replay_stop` /
//!   `storage_replay_status` (a long-running in-process replay session).
//!
//! Each tool dispatches to the same core the CLI verb uses — the
//! [`apply_configure`](crate::cli::storage::apply_configure) /
//! [`apply_profile_create`](crate::cli::storage::apply_profile_create) /
//! [`download_recording`](crate::cli::storage::download_recording) /
//! [`remove_recording`](crate::cli::storage::remove_recording) cores and the
//! shared [`upload_pending`](crate::storage::upload_pending) helper — so an agent
//! and a human get byte-identical behavior (§6.1). The tools are registered as a
//! third [`ToolRouter`] merged into the server in
//! [`BubbaLoopMcpServer::new`](super::BubbaLoopMcpServer).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use super::platform::PlatformOperations;
use super::storage_tools::{err_text, ok_json, StorageNameRequest};
use super::BubbaLoopMcpServer;
use crate::storage::{self, manifest, ReplayOptions};

/// Tool names registered by this module — used by the RBAC map and the §6.1
/// parity check so the surfaces never drift.
#[allow(dead_code)] // referenced from rbac tests + the §6.1 parity test
pub(crate) const STORAGE_WRITE_TOOLS: &[&str] = &[
    "storage_configure",
    "storage_profile_create",
    "storage_profile_rm",
    "storage_upload",
    "storage_download",
    "storage_reconcile",
    "storage_rm",
    "storage_topics_list",
    "storage_replay_start",
    "storage_replay_stop",
    "storage_replay_status",
];

// ---------------------------------------------------------------------------
// In-process replay-session registry (backs storage_replay_*)
// ---------------------------------------------------------------------------

/// A registry of live/finished replay sessions, shared (via `Arc`) across every
/// clone of the MCP server so `start` on one connection is visible to `status`
/// on another. Cheap to clone; a poisoned lock degrades to an empty view rather
/// than panicking the tool call.
#[derive(Clone, Default)]
pub(crate) struct ReplayRegistry {
    sessions: Arc<Mutex<HashMap<String, ReplaySession>>>,
    /// Monotonic sequence for collision-free session ids.
    next_seq: Arc<std::sync::atomic::AtomicU64>,
}

struct ReplaySession {
    recording_name: String,
    started_at_ns: u64,
    rate: f64,
    cancel: CancellationToken,
    state: Arc<Mutex<ReplayRunState>>,
}

impl ReplayRunState {
    /// Whether the session has run to completion (success or failure) — used to
    /// evict it from the registry so finished sessions can't accumulate forever.
    fn is_terminal(&self) -> bool {
        !matches!(self, ReplayRunState::Running)
    }
}

/// Lifecycle of one replay session. `run_replay` only yields its totals at the
/// end, so a running session reports no progress count (v1 limitation).
enum ReplayRunState {
    Running,
    Finished {
        messages_published: u64,
        loops_completed: u64,
    },
    Failed {
        error: String,
    },
}

impl ReplayRegistry {
    /// A collision-free session id (`{recording}-{seq}`). Unlike a wall-clock
    /// suffix, the monotonic counter can't repeat across rapid starts.
    fn next_id(&self, recording_name: &str) -> String {
        let seq = self
            .next_seq
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        format!("{recording_name}-{seq}")
    }

    fn insert(&self, id: String, session: ReplaySession) {
        if let Ok(mut g) = self.sessions.lock() {
            // Evict completed sessions first so the registry can't grow without
            // bound over a long-lived daemon (the leak a looping replay would cause).
            g.retain(|_, s| {
                !s.state
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .is_terminal()
            });
            g.insert(id, session);
        }
    }

    /// Cancel a session by id; returns `false` if no such session exists.
    fn stop(&self, id: &str) -> bool {
        if let Ok(g) = self.sessions.lock() {
            if let Some(s) = g.get(id) {
                s.cancel.cancel();
                return true;
            }
        }
        false
    }

    fn status_json(&self, id: &str) -> Option<serde_json::Value> {
        let g = self.sessions.lock().ok()?;
        g.get(id).map(|s| session_json(id, s))
    }

    fn all_json(&self) -> Vec<serde_json::Value> {
        match self.sessions.lock() {
            Ok(g) => g.iter().map(|(id, s)| session_json(id, s)).collect(),
            Err(_) => Vec::new(),
        }
    }
}

fn session_json(id: &str, s: &ReplaySession) -> serde_json::Value {
    let guard = s.state.lock().unwrap_or_else(|p| p.into_inner());
    let status = match &*guard {
        ReplayRunState::Running => serde_json::json!({ "state": "running" }),
        ReplayRunState::Finished {
            messages_published,
            loops_completed,
        } => serde_json::json!({
            "state": "finished",
            "messages_published": messages_published,
            "loops_completed": loops_completed,
        }),
        ReplayRunState::Failed { error } => serde_json::json!({
            "state": "failed",
            "error": error,
        }),
    };
    serde_json::json!({
        "id": id,
        "recording_name": s.recording_name,
        "rate": s.rate,
        "started_at_ns": s.started_at_ns,
        "status": status,
    })
}

// ---------------------------------------------------------------------------
// Request payloads
// ---------------------------------------------------------------------------

#[derive(Deserialize, JsonSchema)]
pub(crate) struct StorageConfigureRequest {
    /// Backend: r2 | s3 | gcs | minio | local.
    backend: String,
    /// S3-compatible endpoint URL (cloud backends).
    #[serde(default)]
    endpoint: Option<String>,
    /// Region (R2 uses "auto").
    #[serde(default)]
    region: Option<String>,
    /// Bucket name (required for cloud backends).
    #[serde(default)]
    bucket: Option<String>,
    /// Disable background sync of finalized chunks.
    #[serde(default)]
    no_auto_upload: bool,
    /// Auto-delete local recordings N days after successful upload.
    #[serde(default)]
    retention_days_local: Option<u32>,
    /// Soft cap on the local recordings directory (GiB).
    #[serde(default)]
    disk_quota_gb: Option<u32>,
    /// Object-store root for the local backend.
    #[serde(default)]
    local_path: Option<String>,
    /// Access key id (written to secrets.toml, chmod 0600; never echoed back).
    #[serde(default)]
    access_key_id: Option<String>,
    /// Secret access key (written to secrets.toml, chmod 0600; never echoed back).
    #[serde(default)]
    secret_access_key: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct StorageProfileCreateRequest {
    /// Profile name.
    name: String,
    /// Human description.
    #[serde(default)]
    description: Option<String>,
    /// Additive include patterns (comma-separated).
    #[serde(default)]
    topics: Option<String>,
    /// Additive regex include.
    #[serde(default)]
    regex: Option<String>,
    /// Subtractive exclude patterns (comma-separated).
    #[serde(default)]
    exclude: Option<String>,
    /// Also include SHM-only bubbaloop/local/** topics.
    #[serde(default)]
    include_local: bool,
    /// Target chunk size in bytes (default 786432).
    #[serde(default)]
    chunk_size_bytes: Option<u32>,
    /// Compression codec: zstd | lz4 | none (default zstd).
    #[serde(default)]
    compression: Option<String>,
    /// Capture mode: streaming | ring_buffer (default streaming).
    #[serde(default)]
    mode: Option<String>,
    /// Ring-buffer window length in seconds (required with mode=ring_buffer).
    #[serde(default)]
    window_secs: Option<u64>,
    /// Ring-buffer trigger: manual | on-event (required with mode=ring_buffer).
    #[serde(default)]
    trigger: Option<String>,
    /// Overwrite an existing profile of the same name.
    #[serde(default)]
    force: bool,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct StorageDownloadRequest {
    /// Recording name.
    name: String,
    /// Machine id, only needed when there is no local manifest to read it from.
    #[serde(default)]
    machine: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct StorageReconcileRequest {
    /// Recording name.
    name: String,
    /// Also re-download chunks present remotely but missing locally.
    #[serde(default)]
    restore: bool,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct StorageRmRequest {
    /// Recording name.
    name: String,
    /// Also delete the recording's objects from the remote backend.
    #[serde(default)]
    remote: bool,
    /// Machine id for remote deletion when there is no local manifest.
    #[serde(default)]
    machine: Option<String>,
}

#[derive(Deserialize, JsonSchema, Default)]
pub(crate) struct StorageTopicsRequest {
    /// Only show topics from this machine id.
    #[serde(default)]
    machine: Option<String>,
    /// Only show topics whose key contains this substring (case-insensitive).
    #[serde(default)]
    grep: Option<String>,
    /// Query timeout in seconds (default 2).
    #[serde(default)]
    timeout_secs: Option<u64>,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct StorageReplayStartRequest {
    /// Recording name.
    name: String,
    /// Playback speed multiplier (default 1.0; 2.0 = twice as fast).
    #[serde(default)]
    rate: Option<f64>,
    /// Loop the recording until stopped.
    #[serde(default, rename = "loop")]
    loop_playback: bool,
    /// Start offset into the recording, in seconds.
    #[serde(default)]
    start_time: Option<f64>,
    /// End offset into the recording, in seconds.
    #[serde(default)]
    end_time: Option<f64>,
    /// Only replay topics intersecting these key patterns (comma-separated).
    #[serde(default)]
    topics: Option<String>,
    /// Drop topics intersecting these key patterns (comma-separated).
    #[serde(default)]
    exclude: Option<String>,
    /// Remap topics on publish; each entry `from:=to`.
    #[serde(default)]
    remap: Vec<String>,
    /// Schedule by recorder log_time instead of publish_time.
    #[serde(default)]
    use_log_time: bool,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct StorageReplayIdRequest {
    /// Replay session id returned by storage_replay_start.
    id: String,
}

#[derive(Deserialize, JsonSchema, Default)]
pub(crate) struct StorageReplayStatusRequest {
    /// Replay session id; omit to list all sessions.
    #[serde(default)]
    id: Option<String>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Split a comma-separated value into trimmed, non-empty items.
fn split_csv(value: Option<&str>) -> Vec<String> {
    value
        .map(|s| {
            s.split(',')
                .map(|x| x.trim())
                .filter(|x| !x.is_empty())
                .map(|x| x.to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// Build the configured backend, surfacing config/credential errors as tool text.
fn load_backend() -> Result<Box<dyn storage::StorageBackend>, String> {
    storage::StorageConfig::load()
        .map_err(|e| format!("no storage config: {e}"))?
        .build_backend()
        .map_err(|e| format!("cannot build backend: {e}"))
}

fn reconcile_json(r: &storage::ReconcileReport) -> serde_json::Value {
    serde_json::json!({
        "is_clean": r.is_clean(),
        "chunks_diffed": r.chunks_diffed,
        "chunks_uploaded": r.chunks_uploaded,
        "chunks_reuploaded": r.chunks_reuploaded,
        "chunks_redownloaded": r.chunks_redownloaded,
        "chunks_verified_ok": r.chunks_verified_ok,
        "chunks_marked_uploaded": r.chunks_marked_uploaded,
        "orphans_local": r.orphans_local.len(),
        "orphans_remote": r.orphans_remote.len(),
        "errors": r.errors.iter().map(|e| serde_json::json!({
            "chunk_index": e.index,
            "kind": format!("{:?}", e.kind),
            "detail": e.detail,
        })).collect::<Vec<_>>(),
    })
}

fn topics_json(machines: &[storage::discover::MachineTopics]) -> serde_json::Value {
    serde_json::Value::Array(
        machines
            .iter()
            .map(|m| {
                serde_json::json!({
                    "machine_id": m.machine_id,
                    "nodes": m.nodes.iter().map(|n| serde_json::json!({
                        "instance_name": n.instance_name,
                        "role": n.role,
                        "topics": n.topics.iter().map(|t| serde_json::json!({
                            "topic": t.topic,
                            "live": t.is_live(),
                        })).collect::<Vec<_>>(),
                    })).collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

#[tool_router(router = storage_write_tool_router, vis = "pub(crate)")]
impl<P: PlatformOperations> BubbaLoopMcpServer<P> {
    #[tool(
        name = "storage_configure",
        description = "Write the [storage] config block (backend, endpoint, region, bucket, auto_upload, retention, local_path) and, when both access_key_id and secret_access_key are given, the credentials to secrets.toml (chmod 0600). Secrets are never echoed back. Admin tier.",
        annotations(
            title = "Configure storage",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn storage_configure(
        &self,
        Parameters(req): Parameters<StorageConfigureRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=storage_configure backend={}", req.backend);
        let params = crate::cli::storage::ConfigureParams {
            backend: req.backend,
            endpoint: req.endpoint,
            region: req.region,
            bucket: req.bucket,
            no_auto_upload: req.no_auto_upload,
            retention_days_local: req.retention_days_local,
            disk_quota_gb: req.disk_quota_gb,
            local_path: req.local_path,
            access_key_id: req.access_key_id,
            secret_access_key: req.secret_access_key,
        };
        match crate::cli::storage::apply_configure(params) {
            Ok(summary) => Ok(ok_json(&summary)),
            Err(e) => Ok(err_text(e.to_string())),
        }
    }

    #[tool(
        name = "storage_profile_create",
        description = "Create a recording profile (validated against the v1 schema) under ~/.bubbaloop/profiles. Returns {name, path, sha256}. Operator tier.",
        annotations(
            title = "Create profile",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn storage_profile_create(
        &self,
        Parameters(req): Parameters<StorageProfileCreateRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=storage_profile_create name={}", req.name);
        let params = crate::cli::storage::ProfileCreateParams {
            name: req.name,
            description: req.description,
            topics: req.topics,
            regex: req.regex,
            exclude: req.exclude,
            include_local: req.include_local,
            chunk_size_bytes: req.chunk_size_bytes,
            compression: req.compression,
            mode: req.mode,
            window_secs: req.window_secs,
            trigger: req.trigger,
            force: req.force,
        };
        match crate::cli::storage::apply_profile_create(params) {
            Ok(summary) => Ok(ok_json(&summary)),
            Err(e) => Ok(err_text(e.to_string())),
        }
    }

    #[tool(
        name = "storage_profile_rm",
        description = "Delete a recording profile by name from ~/.bubbaloop/profiles. Operator tier.",
        annotations(
            title = "Delete profile",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn storage_profile_rm(
        &self,
        Parameters(req): Parameters<StorageNameRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=storage_profile_rm name={}", req.name);
        if let Err(e) = storage::validate_recording_name(&req.name) {
            return Ok(err_text(format!("invalid profile name: {e}")));
        }
        let dir = match storage::profile::profiles_dir() {
            Ok(d) => d,
            Err(e) => return Ok(err_text(format!("cannot resolve profiles dir: {e}"))),
        };
        let path = dir.join(format!("{}.yaml", req.name));
        if !path.exists() {
            return Ok(err_text(format!("no profile '{}'", req.name)));
        }
        match std::fs::remove_file(&path) {
            Ok(_) => Ok(ok_json(&serde_json::json!({ "removed_profile": req.name }))),
            Err(e) => Ok(err_text(format!("removing profile: {e}"))),
        }
    }

    #[tool(
        name = "storage_upload",
        description = "Upload a recording's un-uploaded chunks to the configured backend, following the idempotent HEAD-before-PUT sequence, then re-upload manifest.json. Returns per-recording counts {total_chunks, already_uploaded, uploaded, conflicts, failed}. Operator tier.",
        annotations(
            title = "Upload recording",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    async fn storage_upload(
        &self,
        Parameters(req): Parameters<StorageNameRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=storage_upload name={}", req.name);
        let dir = match storage::recording_dir(&req.name) {
            Ok(d) => d,
            Err(e) => return Ok(err_text(format!("invalid recording name: {e}"))),
        };
        let backend = match load_backend() {
            Ok(b) => b,
            Err(e) => return Ok(err_text(e)),
        };
        match storage::upload_pending(&*backend, &dir, crate::cli::storage::now_ns()).await {
            Ok(summary) => Ok(ok_json(&summary)),
            Err(e) => Ok(err_text(format!("upload failed: {e}"))),
        }
    }

    #[tool(
        name = "storage_download",
        description = "Download + SHA-256-verify every chunk of a recording from the backend into its local recording directory (~/.bubbaloop/recordings/<name>). Pass `machine` only when there is no local manifest. Operator tier.",
        annotations(
            title = "Download recording",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    async fn storage_download(
        &self,
        Parameters(req): Parameters<StorageDownloadRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=storage_download name={}", req.name);
        // Force the canonical local recording dir: unlike the CLI (run by a trusted
        // human), the MCP tool runs inside the daemon, so we never let a caller
        // choose an arbitrary `to` path (would be an arbitrary-write primitive).
        match crate::cli::storage::download_recording(&req.name, None, req.machine).await {
            Ok(summary) => Ok(ok_json(&summary)),
            Err(e) => Ok(err_text(e.to_string())),
        }
    }

    #[tool(
        name = "storage_reconcile",
        description = "Heal a recording's local-manifest / local-files / remote-objects divergence (§3.5): re-upload missing chunks, verify hashes, flag orphans; with restore=true also re-download chunks present remotely but missing locally. Returns the reconcile report. Operator tier.",
        annotations(
            title = "Reconcile recording",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    async fn storage_reconcile(
        &self,
        Parameters(req): Parameters<StorageReconcileRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=storage_reconcile name={}", req.name);
        let dir = match storage::recording_dir(&req.name) {
            Ok(d) => d,
            Err(e) => return Ok(err_text(format!("invalid recording name: {e}"))),
        };
        let backend = match load_backend() {
            Ok(b) => b,
            Err(e) => return Ok(err_text(e)),
        };
        let opts = storage::ReconcileOptions {
            restore: req.restore,
        };
        match storage::reconcile(&dir, &*backend, &opts, crate::cli::storage::now_ns()).await {
            Ok(report) => Ok(ok_json(&reconcile_json(&report))),
            Err(e) => Ok(err_text(format!("reconcile failed: {e}"))),
        }
    }

    #[tool(
        name = "storage_rm",
        description = "Delete a recording locally and, with remote=true, its objects from the backend. Pass `machine` for remote deletion when there is no local manifest. A name matching nothing is an error. Admin tier.",
        annotations(
            title = "Delete recording",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn storage_rm(
        &self,
        Parameters(req): Parameters<StorageRmRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!(
            "[MCP] tool=storage_rm name={} remote={}",
            req.name,
            req.remote
        );
        match crate::cli::storage::remove_recording(&req.name, req.remote, req.machine).await {
            Ok(summary) => Ok(ok_json(&summary)),
            Err(e) => Ok(err_text(e.to_string())),
        }
    }

    #[tool(
        name = "storage_topics_list",
        description = "List live fleet topics by querying the node manifest queryables over Zenoh, grouped by machine and node (each topic flagged live/idle). Optional machine + grep filters. Viewer tier. Requires the daemon's Zenoh session.",
        annotations(
            title = "List live topics",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    async fn storage_topics_list(
        &self,
        Parameters(req): Parameters<StorageTopicsRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=storage_topics_list");
        let session = match self.platform.zenoh_session() {
            Some(s) => s,
            None => {
                return Ok(err_text(
                    "no Zenoh session available (live topics require the daemon)",
                ))
            }
        };
        let timeout = std::time::Duration::from_secs(req.timeout_secs.unwrap_or(2).max(1));
        match storage::discover::discover(
            &session,
            timeout,
            req.grep.as_deref(),
            req.machine.as_deref(),
        )
        .await
        {
            Ok(machines) => Ok(ok_json(&topics_json(&machines))),
            Err(e) => Ok(err_text(format!("topic discovery failed: {e}"))),
        }
    }

    #[tool(
        name = "storage_replay_start",
        description = "Start replaying a recording back into Zenoh (ros2-bag style: rate, loop, start/end offsets in seconds, topic include/exclude, remap from:=to, use_log_time). Returns a replay session id; runs in the background until it finishes or storage_replay_stop is called. Operator tier. Requires the daemon's Zenoh session.",
        annotations(
            title = "Start replay",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn storage_replay_start(
        &self,
        Parameters(req): Parameters<StorageReplayStartRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=storage_replay_start name={}", req.name);
        let session = match self.platform.zenoh_session() {
            Some(s) => s,
            None => {
                return Ok(err_text(
                    "no Zenoh session available (replay requires the daemon)",
                ))
            }
        };
        let dir = match storage::recording_dir(&req.name) {
            Ok(d) => d,
            Err(e) => return Ok(err_text(format!("invalid recording name: {e}"))),
        };
        let rate = req.rate.unwrap_or(1.0);
        if !rate.is_finite() || rate <= 0.0 {
            return Ok(err_text("rate must be a positive, finite number"));
        }
        // Pre-parse remaps so a bad `from:=to` fails fast (before spawning).
        let remap = match req
            .remap
            .iter()
            .map(|r| storage::replay::parse_remap(r))
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(r) => r,
            Err(e) => return Ok(err_text(format!("invalid remap: {e}"))),
        };

        let started_at_ns = crate::cli::storage::now_ns();
        let id = self.replay_sessions.next_id(&req.name);
        let cancel = CancellationToken::new();
        let state = Arc::new(Mutex::new(ReplayRunState::Running));

        // Register the session BEFORE spawning so status/stop are authoritative the
        // moment start returns (the task shares `state`/`cancel` via clones).
        self.replay_sessions.insert(
            id.clone(),
            ReplaySession {
                recording_name: req.name.clone(),
                started_at_ns,
                rate,
                cancel: cancel.clone(),
                state: state.clone(),
            },
        );

        // Capture everything the background task needs.
        let task_state = state;
        let task_cancel = cancel;
        let name = req.name.clone();
        let topics = split_csv(req.topics.as_deref());
        let exclude = split_csv(req.exclude.as_deref());
        let use_log_time = req.use_log_time;
        let loop_playback = req.loop_playback;
        let start_time = req.start_time;
        let end_time = req.end_time;

        tokio::spawn(async move {
            let set_failed = |st: &Arc<Mutex<ReplayRunState>>, msg: String| {
                *st.lock().unwrap_or_else(|p| p.into_inner()) =
                    ReplayRunState::Failed { error: msg };
            };
            // Loading + decoding the MCAP chunks is synchronous, blocking IO that can
            // buffer a large recording fully into memory — run it on the blocking
            // pool so it never stalls an async worker thread.
            let loaded = tokio::task::spawn_blocking(move || {
                let recording =
                    manifest::load_dir(&dir).map_err(|e| format!("no recording '{name}': {e}"))?;
                if recording.size_bytes >= (1 << 30) {
                    log::warn!(
                        "replay '{name}' is {:.1} GiB; it loads fully into memory",
                        recording.size_bytes as f64 / (1u64 << 30) as f64,
                    );
                }
                storage::read_recording_messages(&dir, &recording)
                    .map_err(|e| format!("reading MCAP chunks: {e}"))
            })
            .await;
            let messages = match loaded {
                Ok(Ok(m)) => m,
                Ok(Err(e)) => return set_failed(&task_state, e),
                Err(e) => return set_failed(&task_state, format!("replay read task failed: {e}")),
            };
            if messages.is_empty() {
                *task_state.lock().unwrap_or_else(|p| p.into_inner()) = ReplayRunState::Finished {
                    messages_published: 0,
                    loops_completed: 0,
                };
                return;
            }
            // start/end offsets (seconds) are relative to the first message on the
            // selected timeline; convert to absolute ns for the planner.
            let base = messages
                .iter()
                .map(|m| {
                    if use_log_time {
                        m.log_time_ns
                    } else {
                        m.publish_time_ns
                    }
                })
                .min()
                .unwrap_or(0);
            let to_abs = |secs: f64| -> u64 { base.saturating_add((secs.max(0.0) * 1e9) as u64) };
            let opts = ReplayOptions {
                rate,
                loop_playback,
                start_time_ns: start_time.map(to_abs),
                end_time_ns: end_time.map(to_abs),
                topics,
                exclude,
                remap,
                use_log_time,
            };
            let sink = crate::cli::storage::ZenohReplaySink { session };
            match storage::run_replay(&messages, &opts, &sink, task_cancel).await {
                Ok(stats) => {
                    *task_state.lock().unwrap_or_else(|p| p.into_inner()) =
                        ReplayRunState::Finished {
                            messages_published: stats.messages_published,
                            loops_completed: stats.loops_completed,
                        };
                }
                Err(e) => set_failed(&task_state, format!("replay error: {e}")),
            }
        });

        Ok(ok_json(&serde_json::json!({
            "replay_id": id,
            "recording_name": req.name,
            "state": "running",
        })))
    }

    #[tool(
        name = "storage_replay_stop",
        description = "Stop a running replay session by id (from storage_replay_start). Operator tier.",
        annotations(
            title = "Stop replay",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn storage_replay_stop(
        &self,
        Parameters(req): Parameters<StorageReplayIdRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=storage_replay_stop id={}", req.id);
        if self.replay_sessions.stop(&req.id) {
            Ok(ok_json(&serde_json::json!({ "stopped": req.id })))
        } else {
            Ok(err_text(format!("no replay session '{}'", req.id)))
        }
    }

    #[tool(
        name = "storage_replay_status",
        description = "Report replay session status. Pass an id for one session, or omit to list all (each with state running|finished|failed and, when finished, messages_published + loops_completed). Viewer tier.",
        annotations(
            title = "Replay status",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn storage_replay_status(
        &self,
        Parameters(req): Parameters<StorageReplayStatusRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        log::info!("[MCP] tool=storage_replay_status");
        match req.id {
            Some(id) => match self.replay_sessions.status_json(&id) {
                Some(j) => Ok(ok_json(&j)),
                None => Ok(err_text(format!("no replay session '{id}'"))),
            },
            None => Ok(ok_json(&self.replay_sessions.all_json())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::platform::DaemonPlatform;

    fn session(state: ReplayRunState) -> ReplaySession {
        ReplaySession {
            recording_name: "rec".into(),
            started_at_ns: 0,
            rate: 1.0,
            cancel: CancellationToken::new(),
            state: Arc::new(Mutex::new(state)),
        }
    }

    #[test]
    fn registry_ids_are_unique_and_terminal_sessions_are_evicted() {
        let reg = ReplayRegistry::default();
        let a = reg.next_id("rec");
        let b = reg.next_id("rec");
        assert_ne!(a, b, "ids must not collide");

        // A finished session, then a running one: the finished entry is pruned on
        // the next insert so the registry can't grow without bound.
        reg.insert(
            a.clone(),
            session(ReplayRunState::Finished {
                messages_published: 1,
                loops_completed: 1,
            }),
        );
        reg.insert(b.clone(), session(ReplayRunState::Running));
        assert!(reg.status_json(&a).is_none(), "finished session evicted");
        assert!(reg.status_json(&b).is_some(), "running session retained");
        assert_eq!(reg.all_json().len(), 1);
    }

    #[test]
    fn stop_returns_false_for_unknown_session() {
        let reg = ReplayRegistry::default();
        assert!(!reg.stop("nope"));
        let id = reg.next_id("rec");
        reg.insert(id.clone(), session(ReplayRunState::Running));
        assert!(reg.stop(&id));
    }

    /// Collect every storage tool name registered across the read + write routers.
    fn registered_storage_tools() -> std::collections::HashSet<String> {
        let mut names = std::collections::HashSet::new();
        for tool in BubbaLoopMcpServer::<DaemonPlatform>::storage_tool_router().list_all() {
            names.insert(tool.name.to_string());
        }
        for tool in BubbaLoopMcpServer::<DaemonPlatform>::storage_write_tool_router().list_all() {
            names.insert(tool.name.to_string());
        }
        names
    }

    /// §6.1 parity: every `storage` CLI verb must have a corresponding MCP tool,
    /// because "anything a human can do at the terminal, an agent can do through
    /// MCP". This test is the guardrail that keeps the two surfaces in lockstep.
    #[test]
    fn every_cli_verb_has_an_mcp_tool() {
        let names = registered_storage_tools();
        // (CLI verb, the MCP tool that fulfils it).
        let cli_to_mcp = [
            ("list", "storage_list"),
            ("info", "storage_info"),
            ("upload", "storage_upload"),
            ("download", "storage_download"),
            ("reconcile", "storage_reconcile"),
            ("rm", "storage_rm"),
            ("replay", "storage_replay_start"),
            ("profile create", "storage_profile_create"),
            ("profile list", "storage_profile_list"),
            ("profile show", "storage_profile_show"),
            ("profile rm", "storage_profile_rm"),
            ("configure", "storage_configure"),
            ("topics", "storage_topics_list"),
        ];
        for (verb, tool) in cli_to_mcp {
            assert!(
                names.contains(tool),
                "CLI verb `storage {verb}` has no MCP tool `{tool}` (§6.1 parity)"
            );
        }
    }

    /// Every name in [`STORAGE_WRITE_TOOLS`] must actually be registered by the
    /// write router (catches a const that drifts from the `#[tool]` methods).
    #[test]
    fn write_tools_const_matches_router() {
        let registered: std::collections::HashSet<String> =
            BubbaLoopMcpServer::<DaemonPlatform>::storage_write_tool_router()
                .list_all()
                .into_iter()
                .map(|t| t.name.to_string())
                .collect();
        for name in STORAGE_WRITE_TOOLS {
            assert!(
                registered.contains(*name),
                "STORAGE_WRITE_TOOLS lists `{name}` but no such tool is registered"
            );
        }
        assert_eq!(
            registered.len(),
            STORAGE_WRITE_TOOLS.len(),
            "write router has tools not listed in STORAGE_WRITE_TOOLS"
        );
    }
}
