//! `bubbaloop storage` CLI verbs (spec §5).
//!
//! This slice wires the data-plane verbs that work against any
//! [`StorageBackend`](crate::storage::StorageBackend) — including the `local`
//! backend, so the whole flow is exercisable end-to-end without R2:
//!
//! - `list` — scan local recordings (or `--remote` objects) and summarise them,
//! - `info` — pretty-print one recording's `manifest.json`,
//! - `upload` — flush un-uploaded chunks to the backend (idempotent, §3.4.4),
//! - `download` — pull + SHA-256-verify each chunk,
//! - `reconcile` — heal local/remote divergence (§3.5),
//! - `rm` — delete a recording locally (and `--remote`).
//! - `profile create/list/show/rm` — manage recording profiles.
//! - `configure` — write the `[storage]` config (+ `secrets.toml`).
//! - `topics` — list live fleet topics via the node manifest queryables (Zenoh).
//!
//! The cloud backend is reached via `configure --backend r2 …` (built with the
//! `s3` feature). The recording-control verbs (`record`, `replay`) land with the
//! recorder in a later PR. Per CLAUDE.md this uses `argh` (not clap) and emits
//! all user-facing output via `log` (never `println!`).

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use argh::FromArgs;
use serde::Serialize;

use crate::storage::{
    self, integrity, manifest, object_key_chunk, object_key_manifest,
    reconcile::ReconcileReport,
    recording::{Chunk, Recording},
    ReconcileOptions, StorageBackend, StorageConfig,
};

/// Build the configured backend for a `--remote` operation, refusing to run
/// against a non-remote (local) backend so `--remote` can't silently target the
/// local object store.
fn require_remote_backend() -> Result<Box<dyn StorageBackend>> {
    let cfg = StorageConfig::load()?;
    if !cfg.is_remote() {
        bail!(
            "no remote backend configured ([storage].backend = {:?}); set it to r2/s3/gcs/minio to use --remote",
            cfg.backend
        );
    }
    Ok(cfg.build_backend()?)
}

/// Manage fleet recordings (list/info/upload/download/reconcile/rm).
#[derive(FromArgs)]
#[argh(subcommand, name = "storage")]
pub struct StorageCommand {
    #[argh(subcommand)]
    action: StorageAction,
}

#[derive(FromArgs)]
#[argh(subcommand)]
enum StorageAction {
    List(ListArgs),
    Info(InfoArgs),
    Upload(UploadArgs),
    Download(DownloadArgs),
    Reconcile(ReconcileArgs),
    Rm(RmArgs),
    Replay(ReplayArgs),
    Profile(ProfileCommand),
    Configure(ConfigureArgs),
    Topics(TopicsArgs),
}

/// List recordings.
#[derive(FromArgs)]
#[argh(subcommand, name = "list")]
struct ListArgs {
    /// list recordings in the configured remote backend instead of locally
    #[argh(switch)]
    remote: bool,
    /// only show recordings from this machine id
    #[argh(option)]
    machine: Option<String>,
    /// only show recordings carrying this tag
    #[argh(option)]
    tag: Option<String>,
    /// only show recordings started on/after this date (YYYY-MM-DD)
    #[argh(option)]
    since: Option<String>,
}

/// Print a recording's manifest.json.
#[derive(FromArgs)]
#[argh(subcommand, name = "info")]
struct InfoArgs {
    /// recording name
    #[argh(positional)]
    name: String,
}

/// Upload a recording's un-uploaded chunks to the backend (idempotent).
#[derive(FromArgs)]
#[argh(subcommand, name = "upload")]
struct UploadArgs {
    /// recording name
    #[argh(positional)]
    name: String,
}

/// Download + verify a recording from the backend.
#[derive(FromArgs)]
#[argh(subcommand, name = "download")]
struct DownloadArgs {
    /// recording name
    #[argh(positional)]
    name: String,
    /// destination directory (default: the local recording directory)
    #[argh(option)]
    to: Option<String>,
    /// machine id to use when there is no local manifest to read it from
    #[argh(option)]
    machine: Option<String>,
}

/// Reconcile a recording's local manifest, local files, and remote objects.
#[derive(FromArgs)]
#[argh(subcommand, name = "reconcile")]
struct ReconcileArgs {
    /// recording name
    #[argh(positional)]
    name: String,
    /// also re-download chunks present remotely but missing locally
    #[argh(switch)]
    restore: bool,
}

/// Delete a recording locally (and optionally remotely).
#[derive(FromArgs)]
#[argh(subcommand, name = "rm")]
struct RmArgs {
    /// recording name
    #[argh(positional)]
    name: String,
    /// also delete the recording's objects from the remote backend
    #[argh(switch)]
    remote: bool,
    /// machine id for remote deletion when there is no local manifest
    #[argh(option)]
    machine: Option<String>,
}

/// Replay a recording back into Zenoh (ros2 bag-style).
#[derive(FromArgs)]
#[argh(subcommand, name = "replay")]
struct ReplayArgs {
    /// recording name
    #[argh(positional)]
    name: String,
    /// playback speed multiplier (default 1.0; 2.0 = twice as fast)
    #[argh(option, default = "1.0")]
    rate: f64,
    /// loop the recording until interrupted
    #[argh(switch, long = "loop")]
    loop_playback: bool,
    /// start offset into the recording, in seconds (skip earlier messages)
    #[argh(option)]
    start_time: Option<f64>,
    /// end offset into the recording, in seconds (skip later messages)
    #[argh(option)]
    end_time: Option<f64>,
    /// only replay topics intersecting these key patterns (comma-separated)
    #[argh(option)]
    topics: Option<String>,
    /// drop topics intersecting these key patterns (comma-separated)
    #[argh(option)]
    exclude: Option<String>,
    /// remap a topic on publish; repeatable, form `from:=to`
    #[argh(option)]
    remap: Vec<String>,
    /// schedule by recorder log_time instead of publish_time
    #[argh(switch)]
    use_log_time: bool,
    /// zenoh router endpoint (defaults to BUBBALOOP_ZENOH_ENDPOINT or localhost)
    #[argh(option)]
    zenoh_endpoint: Option<String>,
}

impl StorageCommand {
    /// Dispatch the chosen verb.
    pub async fn run(self) -> Result<()> {
        match self.action {
            StorageAction::List(a) => run_list(a).await,
            StorageAction::Info(a) => run_info(a),
            StorageAction::Upload(a) => run_upload(a).await,
            StorageAction::Download(a) => run_download(a).await,
            StorageAction::Reconcile(a) => run_reconcile(a).await,
            StorageAction::Rm(a) => run_rm(a).await,
            StorageAction::Replay(a) => run_replay_cmd(a).await,
            StorageAction::Profile(c) => run_profile(c),
            StorageAction::Configure(a) => run_configure(a),
            StorageAction::Topics(a) => run_topics(a).await,
        }
    }
}

// ---------------------------------------------------------------------------
// list
// ---------------------------------------------------------------------------

/// A one-line summary of a recording, for `list`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingSummary {
    pub name: String,
    pub machine_id: String,
    pub mode: String,
    pub chunk_count: usize,
    pub uploaded_count: usize,
    pub size_bytes: u64,
    pub started_at_ns: u64,
    pub ended_at_ns: Option<u64>,
    pub tags: Vec<String>,
    pub corrupt: bool,
}

impl RecordingSummary {
    fn from_recording(r: &Recording) -> Self {
        Self {
            name: r.name.clone(),
            machine_id: r.machine_id.clone(),
            mode: format!("{:?}", r.mode).to_lowercase(),
            chunk_count: r.chunks.len(),
            uploaded_count: r.uploaded_chunk_count(),
            size_bytes: r.size_bytes,
            started_at_ns: r.started_at_ns,
            ended_at_ns: r.ended_at_ns,
            tags: r.tags.clone(),
            corrupt: r.corrupt,
        }
    }

    /// `synced` once every chunk is uploaded, else `local`.
    fn sync_state(&self) -> &'static str {
        if self.chunk_count > 0 && self.uploaded_count == self.chunk_count {
            "synced"
        } else {
            "local"
        }
    }

    /// `corrupt` if reconcile flagged it, else `complete` once the manifest is
    /// closed, else `open`.
    fn status(&self) -> &'static str {
        if self.corrupt {
            "corrupt"
        } else if self.ended_at_ns.is_some() {
            "complete"
        } else {
            "open"
        }
    }
}

/// Filters applied by `list`.
#[derive(Debug, Default, Clone)]
pub struct ListFilter {
    pub machine: Option<String>,
    pub tag: Option<String>,
    pub since_ns: Option<u64>,
}

impl ListFilter {
    fn accepts(&self, s: &RecordingSummary) -> bool {
        if let Some(m) = &self.machine {
            if &s.machine_id != m {
                return false;
            }
        }
        if let Some(t) = &self.tag {
            if !s.tags.iter().any(|x| x == t) {
                return false;
            }
        }
        if let Some(since) = self.since_ns {
            if s.started_at_ns < since {
                return false;
            }
        }
        true
    }
}

/// Scan a recordings root directory and return summaries passing `filter`,
/// sorted by start time. Unreadable manifests are skipped with a warning. Pure
/// and testable — no global state.
pub fn collect_local(recordings_dir: &Path, filter: &ListFilter) -> Vec<RecordingSummary> {
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(recordings_dir) {
        Ok(e) => e,
        Err(_) => return out, // missing dir → nothing recorded yet
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        match manifest::load_dir(&path) {
            Ok(r) => {
                let summary = RecordingSummary::from_recording(&r);
                if filter.accepts(&summary) {
                    out.push(summary);
                }
            }
            Err(e) => log::warn!("skipping {}: {e}", path.display()),
        }
    }
    out.sort_by(|a, b| {
        a.started_at_ns
            .cmp(&b.started_at_ns)
            .then_with(|| a.name.cmp(&b.name))
    });
    out
}

async fn run_list(args: ListArgs) -> Result<()> {
    let filter = ListFilter {
        machine: args.machine,
        tag: args.tag,
        since_ns: args
            .since
            .as_deref()
            .map(parse_since)
            .transpose()
            .context("parsing --since")?,
    };

    let summaries = if args.remote {
        collect_remote(&filter).await?
    } else {
        let dir = storage::recordings_dir()?;
        collect_local(&dir, &filter)
    };

    if summaries.is_empty() {
        log::info!("No recordings found.");
        return Ok(());
    }
    for line in format_table(&summaries) {
        log::info!("{line}");
    }
    Ok(())
}

/// Gather summaries from the configured backend by listing + fetching manifests.
async fn collect_remote(filter: &ListFilter) -> Result<Vec<RecordingSummary>> {
    let backend = require_remote_backend()?;
    // Scope the listing to one machine's prefix when --machine is given, instead
    // of enumerating the whole (cross-machine) bucket and filtering after.
    let prefix = filter
        .machine
        .as_deref()
        .map(|m| format!("{m}/"))
        .unwrap_or_default();
    let objects = backend
        .list(&prefix)
        .await
        .context("listing remote objects")?;
    let mut out = Vec::new();
    for obj in objects {
        if !obj.key.ends_with("/manifest.json") {
            continue;
        }
        let bytes = backend.get(&obj.key).await.context("fetching manifest")?;
        match manifest::parse(&String::from_utf8_lossy(&bytes)) {
            Ok(r) => {
                let s = RecordingSummary::from_recording(&r);
                if filter.accepts(&s) {
                    out.push(s);
                }
            }
            Err(e) => log::warn!("skipping remote {}: {e}", obj.key),
        }
    }
    out.sort_by_key(|s| s.started_at_ns);
    Ok(out)
}

/// Render summaries as aligned text rows (header + one line per recording).
pub fn format_table(summaries: &[RecordingSummary]) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(format!(
        "{:<28} {:<14} {:<11} {:>7} {:<8} {:<9} {:<19}",
        "NAME", "MACHINE", "MODE", "CHUNKS", "SYNC", "STATUS", "STARTED"
    ));
    for s in summaries {
        lines.push(format!(
            "{:<28} {:<14} {:<11} {:>7} {:<8} {:<9} {:<19}",
            truncate(&s.name, 28),
            truncate(&s.machine_id, 14),
            truncate(&s.mode, 11),
            format!("{}/{}", s.uploaded_count, s.chunk_count),
            s.sync_state(),
            s.status(),
            format_time(s.started_at_ns),
        ));
    }
    lines
}

// ---------------------------------------------------------------------------
// info
// ---------------------------------------------------------------------------

fn run_info(args: InfoArgs) -> Result<()> {
    let dir = storage::recording_dir(&args.name)?;
    let recording = manifest::load_dir(&dir)
        .with_context(|| format!("no recording named '{}' at {}", args.name, dir.display()))?;
    let pretty = serde_json::to_string_pretty(&recording).context("serializing manifest")?;
    log::info!("{pretty}");
    Ok(())
}

// ---------------------------------------------------------------------------
// upload
// ---------------------------------------------------------------------------

async fn run_upload(args: UploadArgs) -> Result<()> {
    let dir = storage::recording_dir(&args.name)?;
    let backend = StorageConfig::load()?.build_backend()?;
    let summary = storage::upload_pending(&*backend, &dir, now_ns())
        .await
        .with_context(|| format!("uploading '{}'", args.name))?;

    if summary.uploaded == 0 && summary.is_complete() {
        log::info!(
            "'{}' is already fully uploaded ({} chunk(s)).",
            args.name,
            summary.total_chunks
        );
        return Ok(());
    }
    log::info!(
        "Uploaded {} chunk(s); {} conflict(s); {} failure(s) for '{}'.",
        summary.uploaded,
        summary.conflicts,
        summary.failed,
        args.name
    );
    if !summary.is_complete() {
        bail!(
            "upload incomplete: {} conflict(s), {} failure(s)",
            summary.conflicts,
            summary.failed
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// download
// ---------------------------------------------------------------------------

/// Structured result of [`download_recording`].
#[derive(Debug, Clone, Serialize)]
pub(crate) struct DownloadSummary {
    pub name: String,
    pub dest: String,
    pub verified_chunks: usize,
}

/// Download + SHA-256-verify every chunk of `name` from the configured backend
/// into `to` (default: the local recording dir), then persist the manifest
/// alongside. `machine` is only needed when there is no local manifest from which
/// to read the machine-scoped object keys. Shared by the `download` CLI verb and
/// the `storage_download` MCP tool (§6.1).
pub(crate) async fn download_recording(
    name: &str,
    to: Option<String>,
    machine: Option<String>,
) -> Result<DownloadSummary> {
    let local_dir = storage::recording_dir(name)?;
    let dest: PathBuf = match &to {
        Some(p) => PathBuf::from(p),
        None => local_dir.clone(),
    };
    let backend = StorageConfig::load()?.build_backend()?;

    // Prefer a local manifest; otherwise fetch it from the backend (needs the
    // machine id, since the object key is machine-scoped).
    let recording = match manifest::load_dir(&local_dir) {
        Ok(r) => r,
        Err(_) => {
            let machine = machine.clone().context(
                "no local manifest; pass machine id so the remote manifest key can be built",
            )?;
            let key = object_key_manifest(&machine, name);
            let bytes = backend
                .get(&key)
                .await
                .with_context(|| format!("fetching remote manifest {key}"))?;
            manifest::parse(&String::from_utf8_lossy(&bytes))?
        }
    };

    let chunks_dir = dest.join("chunks");
    std::fs::create_dir_all(&chunks_dir)
        .with_context(|| format!("creating {}", chunks_dir.display()))?;

    let mut verified = 0usize;
    for chunk in &recording.chunks {
        let key = object_key_chunk(
            &recording.machine_id,
            &recording.name,
            chunk.index,
            &chunk.sha256,
        );
        let bytes = backend
            .get(&key)
            .await
            .with_context(|| format!("downloading {key}"))?;
        // Verify on receipt; refuse to write a corrupted file (§8).
        let expected = integrity::from_hex(&chunk.sha256)
            .with_context(|| format!("chunk {} has an invalid manifest sha256", chunk.index))?;
        let actual = integrity::sha256(&bytes);
        if actual != expected {
            bail!(
                "chunk {} failed verification (expected {}, got {}) — not written",
                chunk.index,
                chunk.sha256,
                integrity::to_hex(&actual)
            );
        }
        // Derive the on-disk name from (index, sha256) rather than trusting the
        // manifest's `name` field as a path segment. (Manifest validation already
        // requires it to be canonical, but deriving here removes all doubt.)
        let safe_name = Chunk::canonical_name(chunk.index, &chunk.sha256);
        std::fs::write(chunks_dir.join(&safe_name), &bytes)
            .with_context(|| format!("writing chunk {}", chunk.index))?;
        verified += 1;
    }

    // Persist the manifest alongside the chunks.
    manifest::save_dir(&dest, &recording).context("writing manifest")?;

    Ok(DownloadSummary {
        name: name.to_string(),
        dest: dest.display().to_string(),
        verified_chunks: verified,
    })
}

async fn run_download(args: DownloadArgs) -> Result<()> {
    let summary = download_recording(&args.name, args.to, args.machine).await?;
    log::info!(
        "Downloaded and verified {} chunk(s) for '{}' to {}.",
        summary.verified_chunks,
        summary.name,
        summary.dest
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// reconcile
// ---------------------------------------------------------------------------

async fn run_reconcile(args: ReconcileArgs) -> Result<()> {
    let dir = storage::recording_dir(&args.name)?;
    let backend = StorageConfig::load()?.build_backend()?;
    let report = storage::reconcile(
        &dir,
        &*backend,
        &ReconcileOptions {
            restore: args.restore,
        },
        now_ns(),
    )
    .await
    .with_context(|| format!("reconciling '{}'", args.name))?;

    for line in format_reconcile(&report) {
        log::info!("{line}");
    }
    if !report.is_clean() {
        bail!("reconcile finished with {} error(s)", report.errors.len());
    }
    Ok(())
}

/// Human-readable reconcile report lines.
fn format_reconcile(r: &ReconcileReport) -> Vec<String> {
    let mut lines = vec![format!(
        "diffed {} | uploaded {} | re-uploaded {} | re-downloaded {} | verified {} | marked {}",
        r.chunks_diffed,
        r.chunks_uploaded,
        r.chunks_reuploaded,
        r.chunks_redownloaded,
        r.chunks_verified_ok,
        r.chunks_marked_uploaded,
    )];
    if !r.orphans_local.is_empty() {
        lines.push(format!("orphan local files: {}", r.orphans_local.len()));
    }
    if !r.orphans_remote.is_empty() {
        lines.push(format!("orphan remote objects: {}", r.orphans_remote.len()));
    }
    for e in &r.errors {
        lines.push(format!(
            "ERROR chunk {}: {:?} — {}",
            e.index, e.kind, e.detail
        ));
    }
    lines
}

// ---------------------------------------------------------------------------
// rm
// ---------------------------------------------------------------------------

/// Structured result of [`remove_recording`].
#[derive(Debug, Clone, Serialize)]
pub(crate) struct RemoveSummary {
    pub name: String,
    pub removed_local: bool,
    pub removed_remote_objects: usize,
}

/// Delete a recording locally and (when `remote`) its remote objects. `machine`
/// is only consulted for remote deletion when there is no local manifest. A name
/// that matches nothing (local or remote) is an error. Shared by the `rm` CLI
/// verb and the `storage_rm` MCP tool (§6.1).
pub(crate) async fn remove_recording(
    name: &str,
    remote: bool,
    machine: Option<String>,
) -> Result<RemoveSummary> {
    // recording_dir validates the name, so a traversal name can't reach
    // remove_dir_all or a remote prefix.
    let dir = storage::recording_dir(name)?;
    let mut removed_remote_objects = 0usize;

    if remote {
        let backend = require_remote_backend()?;
        // Determine the machine id (object keys are machine-scoped).
        let machine = match manifest::load_dir(&dir) {
            Ok(r) => r.machine_id,
            Err(_) => machine
                .clone()
                .context("no local manifest; pass machine id to delete remote objects")?,
        };
        let prefix = format!("{machine}/{name}/");
        let objects = backend
            .list(&prefix)
            .await
            .context("listing remote objects")?;
        for obj in objects {
            backend
                .delete(&obj.key)
                .await
                .with_context(|| format!("deleting {}", obj.key))?;
            removed_remote_objects += 1;
        }
    }

    let removed_local = dir.exists();
    if removed_local {
        std::fs::remove_dir_all(&dir).with_context(|| format!("removing {}", dir.display()))?;
    }

    // A name that matched nothing (local or remote) is an error, not a silent
    // success — otherwise a typo with --remote exits 0 having done nothing.
    if !removed_local && removed_remote_objects == 0 {
        let scope = if remote {
            "no local directory and no matching remote objects"
        } else {
            "no local directory"
        };
        bail!("no recording named '{name}' found ({scope})");
    }
    Ok(RemoveSummary {
        name: name.to_string(),
        removed_local,
        removed_remote_objects,
    })
}

async fn run_rm(args: RmArgs) -> Result<()> {
    let summary = remove_recording(&args.name, args.remote, args.machine).await?;
    if summary.removed_remote_objects > 0 {
        log::info!(
            "Deleted {} remote object(s) for '{}'.",
            summary.removed_remote_objects,
            summary.name
        );
    }
    if summary.removed_local {
        log::info!("Deleted local recording '{}'.", summary.name);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// replay
// ---------------------------------------------------------------------------

/// A [`ReplaySink`] that re-publishes onto a live Zenoh session, preserving the
/// recorded encoding (§4.5). Shared with the `storage_replay_start` MCP tool so
/// CLI and agent replay are byte-identical (§6.1).
pub(crate) struct ZenohReplaySink {
    pub(crate) session: std::sync::Arc<zenoh::Session>,
}

#[async_trait::async_trait]
impl storage::ReplaySink for ZenohReplaySink {
    async fn publish(
        &self,
        topic: &str,
        encoding: &str,
        payload: &[u8],
    ) -> std::result::Result<(), storage::ReplayError> {
        // `CongestionControl::Block` (not the put-default `Drop`) so a slow link
        // or subscriber back-pressures the replay instead of silently dropping
        // samples — replay must be faithful (§4.5), and `run_replay` already
        // paces publishes, so blocking only stretches timing, never loses data.
        self.session
            .put(topic, payload.to_vec())
            .encoding(zenoh::bytes::Encoding::from(encoding))
            .congestion_control(zenoh::qos::CongestionControl::Block)
            .await
            .map_err(|e| storage::ReplayError::Publish {
                topic: topic.to_string(),
                detail: e.to_string(),
            })
    }
}

async fn run_replay_cmd(args: ReplayArgs) -> Result<()> {
    // Load the local recording and recover its messages from the MCAP chunks.
    let dir = storage::recording_dir(&args.name)?;
    let recording = manifest::load_dir(&dir)
        .with_context(|| format!("no recording named '{}' at {}", args.name, dir.display()))?;
    // v1 buffers the whole recording in memory (no streaming reader yet); warn
    // before pulling a large one in so an OOM is at least diagnosable.
    const LARGE_RECORDING_BYTES: u64 = 1 << 30; // 1 GiB
    if recording.size_bytes >= LARGE_RECORDING_BYTES {
        log::warn!(
            "recording '{}' is {:.1} GiB; replay loads it fully into memory",
            args.name,
            recording.size_bytes as f64 / (1u64 << 30) as f64,
        );
    }
    let messages = storage::read_recording_messages(&dir, &recording)
        .with_context(|| format!("reading MCAP chunks for '{}'", args.name))?;
    if messages.is_empty() {
        log::warn!("recording '{}' has no messages to replay", args.name);
        return Ok(());
    }

    // `--start-time`/`--end-time` are offsets in seconds from the recording's
    // first message (on the selected timeline); convert to absolute ns for the
    // planner, which trims against the recorded timestamps.
    let base = messages
        .iter()
        .map(|m| {
            if args.use_log_time {
                m.log_time_ns
            } else {
                m.publish_time_ns
            }
        })
        .min()
        .unwrap_or(0);
    let to_abs = |secs: f64| -> u64 { base.saturating_add((secs.max(0.0) * 1e9) as u64) };

    let remap = args
        .remap
        .iter()
        .map(|r| storage::replay::parse_remap(r))
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let opts = storage::ReplayOptions {
        rate: args.rate,
        loop_playback: args.loop_playback,
        start_time_ns: args.start_time.map(to_abs),
        end_time_ns: args.end_time.map(to_abs),
        topics: split_csv(args.topics.as_deref()),
        exclude: split_csv(args.exclude.as_deref()),
        remap,
        use_log_time: args.use_log_time,
    };

    let session = crate::cli::zenoh_session::create_zenoh_session(args.zenoh_endpoint.as_deref())
        .await
        .context("connecting to zenoh")?;
    let sink = ZenohReplaySink {
        session: session.clone(),
    };

    // Ctrl-C cancels the replay (and is the only way to stop `--loop`).
    let cancel = tokio_util::sync::CancellationToken::new();
    let cancel_handle = cancel.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            cancel_handle.cancel();
        }
    });

    log::info!(
        "replaying '{}' ({} messages, rate {}x{}){}",
        args.name,
        messages.len(),
        args.rate,
        if args.use_log_time { ", log-time" } else { "" },
        if args.loop_playback { ", looping" } else { "" },
    );
    let stats = storage::run_replay(&messages, &opts, &sink, cancel)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    log::info!(
        "replay finished: {} messages published across {} pass(es)",
        stats.messages_published,
        stats.loops_completed,
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// topics
// ---------------------------------------------------------------------------

/// List live topics across the fleet (via the node manifest queryables).
#[derive(FromArgs)]
#[argh(subcommand, name = "topics")]
pub struct TopicsArgs {
    /// only show topics from this machine id
    #[argh(option)]
    machine: Option<String>,
    /// only show topics whose key contains this substring (case-insensitive)
    #[argh(option)]
    grep: Option<String>,
    /// zenoh endpoint (default: env BUBBALOOP_ZENOH_ENDPOINT or tcp/127.0.0.1:7447)
    #[argh(option, short = 'z')]
    zenoh_endpoint: Option<String>,
    /// query timeout in seconds (default 2)
    #[argh(option, default = "2")]
    timeout_secs: u64,
}

async fn run_topics(args: TopicsArgs) -> Result<()> {
    let session = crate::cli::zenoh_session::create_zenoh_session(args.zenoh_endpoint.as_deref())
        .await
        .context("connecting to zenoh")?;
    let machines = storage::discover::discover(
        &session,
        std::time::Duration::from_secs(args.timeout_secs),
        args.grep.as_deref(),
        args.machine.as_deref(),
    )
    .await?;

    if machines.is_empty() {
        log::info!("No live topics found.");
        return Ok(());
    }
    for m in &machines {
        log::info!("{}", m.machine_id);
        for node in &m.nodes {
            log::info!("  {} ({})", node.instance_name, node.role);
            for t in &node.topics {
                let mark = if t.is_live() { "live" } else { "idle" };
                log::info!("    [{mark}] {}", t.topic);
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// configure
// ---------------------------------------------------------------------------

/// Configure the storage backend (writes the `[storage]` table of config.toml,
/// and `secrets.toml` when credentials are supplied).
///
/// Flag-driven and non-interactive so it is scriptable and testable; the
/// interactive wizard (a later slice) collects the same fields and calls this
/// same handler (spec §6.2).
#[derive(FromArgs)]
#[argh(subcommand, name = "configure")]
pub struct ConfigureArgs {
    /// backend: r2 | s3 | gcs | minio | local
    #[argh(option)]
    backend: String,
    /// S3-compatible endpoint URL (cloud backends)
    #[argh(option)]
    endpoint: Option<String>,
    /// region (R2 uses "auto")
    #[argh(option)]
    region: Option<String>,
    /// bucket name (the sharing unit; required for cloud backends)
    #[argh(option)]
    bucket: Option<String>,
    /// disable background sync of finalized chunks
    #[argh(switch)]
    no_auto_upload: bool,
    /// auto-delete local recordings N days after successful upload
    #[argh(option)]
    retention_days_local: Option<u32>,
    /// soft cap on the local recordings directory (GiB)
    #[argh(option)]
    disk_quota_gb: Option<u32>,
    /// object-store root for the local backend
    #[argh(option)]
    local_path: Option<String>,
    /// access key id (written to secrets.toml, chmod 0600)
    #[argh(option)]
    access_key_id: Option<String>,
    /// secret access key (written to secrets.toml, chmod 0600)
    #[argh(option)]
    secret_access_key: Option<String>,
}

/// Flag/field-driven inputs to [`apply_configure`] — the shared core behind the
/// `configure` CLI verb and the `storage_configure` MCP tool (§6.1).
#[derive(Debug, Clone, Default)]
pub(crate) struct ConfigureParams {
    pub backend: String,
    pub endpoint: Option<String>,
    pub region: Option<String>,
    pub bucket: Option<String>,
    pub no_auto_upload: bool,
    pub retention_days_local: Option<u32>,
    pub disk_quota_gb: Option<u32>,
    pub local_path: Option<String>,
    pub access_key_id: Option<String>,
    pub secret_access_key: Option<String>,
}

/// Structured result of [`apply_configure`] (no secrets — those only ever touch
/// `secrets.toml`).
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ConfigureSummary {
    pub backend: String,
    pub bucket: Option<String>,
    pub auto_upload: bool,
    pub secrets_written: bool,
    /// Set when a remote backend was configured without credentials.
    pub credentials_warning: Option<String>,
}

/// Validate inputs, write the `[storage]` config (and `secrets.toml` when both
/// credential halves are supplied), and return a structured summary. Performs no
/// user-facing logging so it composes cleanly into both the CLI and MCP.
pub(crate) fn apply_configure(p: ConfigureParams) -> Result<ConfigureSummary> {
    const BACKENDS: [&str; 5] = ["r2", "s3", "gcs", "minio", "local"];
    if !BACKENDS.contains(&p.backend.as_str()) {
        bail!(
            "unknown backend {:?} (expected one of: {})",
            p.backend,
            BACKENDS.join(", ")
        );
    }
    let is_remote = p.backend != "local";
    if is_remote && p.bucket.is_none() {
        bail!("bucket is required for the {:?} backend", p.backend);
    }
    // Reject a lone credential half before writing anything.
    if p.access_key_id.is_some() != p.secret_access_key.is_some() {
        bail!("access_key_id and secret_access_key must be provided together");
    }

    let cfg = StorageConfig {
        backend: p.backend.clone(),
        endpoint: p.endpoint,
        region: p.region,
        bucket: p.bucket.clone(),
        auto_upload: !p.no_auto_upload,
        retention_days_local: p.retention_days_local,
        disk_quota_gb: p.disk_quota_gb,
        recorder_instance: None,
        local_path: p.local_path.map(PathBuf::from),
    };
    cfg.save().context("writing config.toml")?;

    let mut secrets_written = false;
    if let (Some(id), Some(secret)) = (p.access_key_id, p.secret_access_key) {
        use storage::secrets::{R2Credentials, Secret};
        let creds = R2Credentials {
            access_key_id: Secret::new(id),
            secret_access_key: Secret::new(secret),
        };
        let path = storage::secrets::default_path()?;
        storage::secrets::save_r2(&path, &creds).context("writing secrets.toml")?;
        secrets_written = true;
    }

    let credentials_warning = (is_remote && !secrets_written).then(|| {
        "no credentials configured; set them (secrets.toml, chmod 0600) before uploading"
            .to_string()
    });

    Ok(ConfigureSummary {
        backend: cfg.backend,
        bucket: cfg.bucket,
        auto_upload: cfg.auto_upload,
        secrets_written,
        credentials_warning,
    })
}

fn run_configure(args: ConfigureArgs) -> Result<()> {
    let summary = apply_configure(ConfigureParams {
        backend: args.backend,
        endpoint: args.endpoint,
        region: args.region,
        bucket: args.bucket,
        no_auto_upload: args.no_auto_upload,
        retention_days_local: args.retention_days_local,
        disk_quota_gb: args.disk_quota_gb,
        local_path: args.local_path,
        access_key_id: args.access_key_id,
        secret_access_key: args.secret_access_key,
    })?;
    log::info!("Wrote [storage] config (backend = {}).", summary.backend);
    if summary.secrets_written {
        log::info!("Wrote credentials to secrets.toml (chmod 0600).");
    }
    if let Some(warn) = &summary.credentials_warning {
        log::warn!("{warn}");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// profile create/list/show/rm
// ---------------------------------------------------------------------------

/// Manage recording profiles (`~/.bubbaloop/profiles/*.yaml`).
#[derive(FromArgs)]
#[argh(subcommand, name = "profile")]
pub struct ProfileCommand {
    #[argh(subcommand)]
    action: ProfileAction,
}

#[derive(FromArgs)]
#[argh(subcommand)]
enum ProfileAction {
    Create(ProfileCreateArgs),
    List(ProfileListArgs),
    Show(ProfileShowArgs),
    Rm(ProfileRmArgs),
}

/// Create a profile from flags, validating it against the v1 schema.
#[derive(FromArgs)]
#[argh(subcommand, name = "create")]
struct ProfileCreateArgs {
    /// profile name
    #[argh(positional)]
    name: String,
    /// human description
    #[argh(option)]
    description: Option<String>,
    /// additive include patterns (comma-separated)
    #[argh(option)]
    topics: Option<String>,
    /// additive regex include
    #[argh(option)]
    regex: Option<String>,
    /// subtractive exclude patterns (comma-separated)
    #[argh(option)]
    exclude: Option<String>,
    /// also include SHM-only bubbaloop/local/** topics
    #[argh(switch)]
    include_local: bool,
    /// target chunk size in bytes (default 786432)
    #[argh(option)]
    chunk_size_bytes: Option<u32>,
    /// compression codec: zstd | lz4 | none (default zstd)
    #[argh(option)]
    compression: Option<String>,
    /// capture mode: streaming | ring_buffer (default streaming)
    #[argh(option)]
    mode: Option<String>,
    /// ring-buffer window length in seconds (required with --mode ring_buffer)
    #[argh(option)]
    window_secs: Option<u64>,
    /// ring-buffer trigger: manual | on-event (required with --mode ring_buffer)
    #[argh(option)]
    trigger: Option<String>,
    /// overwrite an existing profile of the same name
    #[argh(switch)]
    force: bool,
}

/// List profiles.
#[derive(FromArgs)]
#[argh(subcommand, name = "list")]
struct ProfileListArgs {}

/// Print a profile's YAML.
#[derive(FromArgs)]
#[argh(subcommand, name = "show")]
struct ProfileShowArgs {
    /// profile name
    #[argh(positional)]
    name: String,
}

/// Delete a profile.
#[derive(FromArgs)]
#[argh(subcommand, name = "rm")]
struct ProfileRmArgs {
    /// profile name
    #[argh(positional)]
    name: String,
}

fn run_profile(cmd: ProfileCommand) -> Result<()> {
    match cmd.action {
        ProfileAction::Create(a) => run_profile_create(a),
        ProfileAction::List(_) => run_profile_list(),
        ProfileAction::Show(a) => run_profile_show(a),
        ProfileAction::Rm(a) => run_profile_rm(a),
    }
}

/// Resolve a validated profile name to its `.yaml` path under the profiles dir.
fn profile_path(name: &str) -> Result<PathBuf> {
    // Reuse the recording-name guard so a profile name can't traverse the path.
    storage::validate_recording_name(name)
        .with_context(|| format!("invalid profile name {name:?}"))?;
    Ok(storage::profile::profiles_dir()?.join(format!("{name}.yaml")))
}

/// Inputs to [`apply_profile_create`] — shared by the `profile create` CLI verb
/// and the `storage_profile_create` MCP tool (§6.1).
#[derive(Debug, Clone, Default)]
pub(crate) struct ProfileCreateParams {
    pub name: String,
    pub description: Option<String>,
    pub topics: Option<String>,
    pub regex: Option<String>,
    pub exclude: Option<String>,
    pub include_local: bool,
    pub chunk_size_bytes: Option<u32>,
    pub compression: Option<String>,
    pub mode: Option<String>,
    pub window_secs: Option<u64>,
    pub trigger: Option<String>,
    pub force: bool,
}

/// Structured result of [`apply_profile_create`].
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ProfileSummary {
    pub name: String,
    pub path: String,
    pub sha256: String,
}

/// Validate and write a profile (the v1 schema check happens in `profile::save`),
/// returning a structured summary. No user-facing logging.
pub(crate) fn apply_profile_create(p: ProfileCreateParams) -> Result<ProfileSummary> {
    use storage::profile::{CompressionKind, Profile, DEFAULT_CHUNK_SIZE_BYTES};
    use storage::recording::{RecordingMode, Trigger};

    let path = profile_path(&p.name)?;
    if path.exists() && !p.force {
        bail!(
            "profile '{}' already exists (use force/--force to overwrite)",
            p.name
        );
    }

    let compression = match p.compression.as_deref() {
        None | Some("zstd") => CompressionKind::Zstd,
        Some("lz4") => CompressionKind::Lz4,
        Some("none") => CompressionKind::None,
        Some(other) => bail!("unknown compression {other:?} (expected zstd|lz4|none)"),
    };
    let mode = match p.mode.as_deref() {
        None | Some("streaming") => RecordingMode::Streaming,
        Some("ring_buffer") => RecordingMode::RingBuffer,
        Some(other) => bail!("unknown mode {other:?} (expected streaming|ring_buffer)"),
    };
    let trigger = match p.trigger.as_deref() {
        None => None,
        Some("manual") => Some(Trigger::Manual),
        Some("on-event") => Some(Trigger::OnEvent),
        Some(other) => bail!("unknown trigger {other:?} (expected manual|on-event)"),
    };

    let profile = Profile {
        name: p.name.clone(),
        description: p.description,
        topics: split_csv(p.topics.as_deref()),
        regex: p.regex,
        exclude: split_csv(p.exclude.as_deref()),
        include_local: p.include_local,
        chunk_size_bytes: p.chunk_size_bytes.unwrap_or(DEFAULT_CHUNK_SIZE_BYTES),
        compression,
        compression_level: Default::default(),
        chunk_crc: true,
        mode,
        window_secs: p.window_secs,
        trigger,
        sending_queue: Default::default(),
        retry_on_failure: Default::default(),
    };

    storage::profile::save(&path, &profile)
        .with_context(|| format!("saving profile '{}'", p.name))?;
    Ok(ProfileSummary {
        name: p.name,
        path: path.display().to_string(),
        sha256: storage::profile::canonical_sha256(&profile),
    })
}

fn run_profile_create(args: ProfileCreateArgs) -> Result<()> {
    let summary = apply_profile_create(ProfileCreateParams {
        name: args.name,
        description: args.description,
        topics: args.topics,
        regex: args.regex,
        exclude: args.exclude,
        include_local: args.include_local,
        chunk_size_bytes: args.chunk_size_bytes,
        compression: args.compression,
        mode: args.mode,
        window_secs: args.window_secs,
        trigger: args.trigger,
        force: args.force,
    })?;
    log::info!(
        "Created profile '{}' at {} (sha256 {}).",
        summary.name,
        summary.path,
        summary.sha256
    );
    Ok(())
}

fn run_profile_list() -> Result<()> {
    let dir = storage::profile::profiles_dir()?;
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => {
            log::info!("No profiles found.");
            return Ok(());
        }
    };
    let mut names: Vec<String> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("yaml") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                names.push(stem.to_string());
            }
        }
    }
    names.sort();
    if names.is_empty() {
        log::info!("No profiles found.");
        return Ok(());
    }
    log::info!("{:<24} {:<12} {}", "NAME", "SHA256", "DESCRIPTION");
    for name in names {
        match storage::profile::load_named(&name) {
            Ok(p) => log::info!(
                "{:<24} {:<12} {}",
                truncate(&name, 24),
                &storage::profile::canonical_sha256(&p)[..12],
                p.description.as_deref().unwrap_or("")
            ),
            Err(e) => log::warn!("{name}: unreadable ({e})"),
        }
    }
    Ok(())
}

fn run_profile_show(args: ProfileShowArgs) -> Result<()> {
    let path = profile_path(&args.name)?;
    // Validate it parses, then print the on-disk YAML verbatim.
    storage::profile::load(&path).with_context(|| format!("no profile named '{}'", args.name))?;
    let yaml = std::fs::read_to_string(&path).context("reading profile")?;
    log::info!("{yaml}");
    Ok(())
}

fn run_profile_rm(args: ProfileRmArgs) -> Result<()> {
    let path = profile_path(&args.name)?;
    if !path.exists() {
        bail!("no profile named '{}'", args.name);
    }
    std::fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
    log::info!("Deleted profile '{}'.", args.name);
    Ok(())
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Split a comma-separated flag value into trimmed, non-empty items.
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

pub(crate) fn now_ns() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

/// Parse a `YYYY-MM-DD` date into nanoseconds since the Unix epoch (UTC midnight).
fn parse_since(s: &str) -> Result<u64> {
    use chrono::{NaiveDate, NaiveTime};
    let date = NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
        .with_context(|| format!("expected YYYY-MM-DD, got '{s}'"))?;
    let dt = date.and_time(NaiveTime::MIN).and_utc();
    Ok(dt.timestamp_nanos_opt().unwrap_or(0).max(0) as u64)
}

/// Format nanoseconds since the epoch as `YYYY-MM-DD HH:MM:SS` (UTC).
fn format_time(ns: u64) -> String {
    use chrono::DateTime;
    let secs = (ns / 1_000_000_000) as i64;
    let nsec = (ns % 1_000_000_000) as u32;
    match DateTime::from_timestamp(secs, nsec) {
        Some(dt) => dt.format("%Y-%m-%d %H:%M:%S").to_string(),
        None => "-".to_string(),
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let keep = max.saturating_sub(1);
        format!("{}…", s.chars().take(keep).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::recording::{Chunk, Recording};

    fn make_recording(name: &str, machine: &str, started: u64) -> Recording {
        let mut r = Recording::new(name, machine, started);
        r.size_bytes = 1024;
        r
    }

    fn push_chunk(r: &mut Recording, i: u32, uploaded: bool) {
        let sha = format!("{i:02x}").repeat(32);
        r.chunks.push(Chunk {
            name: Chunk::canonical_name(i, &sha),
            index: i,
            size_bytes: 100,
            sha256: sha,
            log_time_first_ns: None,
            log_time_last_ns: None,
            uploaded_at_ns: if uploaded { Some(1) } else { None },
            remote_etag: None,
            extra: Default::default(),
        });
    }

    fn write_recording(root: &Path, r: &Recording) {
        let dir = root.join(&r.name);
        std::fs::create_dir_all(dir.join("chunks")).unwrap();
        manifest::save_dir(&dir, r).unwrap();
    }

    #[test]
    fn collect_local_filters_and_sorts() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let mut a = make_recording("rec_a", "jetson_alpha", 2_000);
        push_chunk(&mut a, 0, true);
        a.tags = vec!["outdoor".into()];
        let mut b = make_recording("rec_b", "jetson_beta", 1_000);
        push_chunk(&mut b, 0, false);
        write_recording(root, &a);
        write_recording(root, &b);

        // No filter: both, sorted by start time (b before a).
        let all = collect_local(root, &ListFilter::default());
        assert_eq!(
            all.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
            vec!["rec_b", "rec_a"]
        );

        // Machine filter.
        let only_alpha = collect_local(
            root,
            &ListFilter {
                machine: Some("jetson_alpha".into()),
                ..Default::default()
            },
        );
        assert_eq!(only_alpha.len(), 1);
        assert_eq!(only_alpha[0].name, "rec_a");
        assert_eq!(only_alpha[0].sync_state(), "synced");

        // Tag filter.
        let tagged = collect_local(
            root,
            &ListFilter {
                tag: Some("outdoor".into()),
                ..Default::default()
            },
        );
        assert_eq!(tagged.len(), 1);

        // Since filter (started >= 1_500 excludes rec_b at 1_000).
        let recent = collect_local(
            root,
            &ListFilter {
                since_ns: Some(1_500),
                ..Default::default()
            },
        );
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].name, "rec_a");
    }

    #[test]
    fn collect_local_missing_dir_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert!(collect_local(&dir.path().join("nope"), &ListFilter::default()).is_empty());
    }

    #[test]
    fn sync_state_and_status_derivation() {
        let mut r = make_recording("r", "m", 1);
        push_chunk(&mut r, 0, true);
        push_chunk(&mut r, 1, false);
        let s = RecordingSummary::from_recording(&r);
        assert_eq!(s.sync_state(), "local"); // not all uploaded
        assert_eq!(s.status(), "open"); // no ended_at_ns
        assert_eq!(s.chunk_count, 2);
        assert_eq!(s.uploaded_count, 1);
    }

    #[test]
    fn format_table_has_header_and_rows() {
        let mut r = make_recording("rec_a", "m", 1_700_000_000_000_000_000);
        push_chunk(&mut r, 0, true);
        let rows = format_table(&[RecordingSummary::from_recording(&r)]);
        assert_eq!(rows.len(), 2);
        assert!(rows[0].contains("NAME"));
        assert!(rows[1].contains("rec_a"));
        assert!(rows[1].contains("synced"));
    }

    #[test]
    fn parse_since_accepts_iso_date() {
        let ns = parse_since("2026-01-01").unwrap();
        // 2026-01-01T00:00:00Z is well after the epoch.
        assert!(ns > 1_700_000_000_000_000_000);
        assert!(parse_since("not-a-date").is_err());
    }

    #[test]
    fn split_csv_trims_and_drops_empties() {
        assert_eq!(split_csv(None), Vec::<String>::new());
        assert_eq!(split_csv(Some("")), Vec::<String>::new());
        assert_eq!(
            split_csv(Some("cam_*/compressed, lidar/* ,, ")),
            vec!["cam_*/compressed".to_string(), "lidar/*".to_string()]
        );
    }

    #[test]
    fn truncate_adds_ellipsis() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("a_very_long_recording_name", 6), "a_ver…");
    }

    #[test]
    fn format_time_is_stable() {
        // 2023-11-14T22:13:20Z
        assert_eq!(
            format_time(1_700_000_000_000_000_000),
            "2023-11-14 22:13:20"
        );
        assert_eq!(format_time(0), "1970-01-01 00:00:00");
    }
}
