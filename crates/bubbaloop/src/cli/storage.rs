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
//!
//! The recording-control verbs (`configure`, `topics`, `profile`, `record`,
//! `replay`) and the cloud backends land in later slices. Per CLAUDE.md this uses
//! `argh` (not clap) and emits all user-facing output via `log` (never
//! `println!`).

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use argh::FromArgs;

use crate::storage::{
    self, integrity, manifest, object_key_chunk, object_key_manifest,
    reconcile::ReconcileReport,
    recording::{Chunk, Recording},
    ReconcileOptions, StorageBackend, StorageConfig, UploadJob, UploadOutcome,
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
    let mut recording =
        manifest::load_dir(&dir).with_context(|| format!("no recording named '{}'", args.name))?;
    let backend = StorageConfig::load()?.build_backend()?;

    let pending: Vec<usize> = recording
        .chunks
        .iter()
        .enumerate()
        .filter(|(_, c)| !c.is_uploaded())
        .map(|(i, _)| i)
        .collect();
    if pending.is_empty() {
        log::info!(
            "'{}' is already fully uploaded ({} chunks).",
            args.name,
            recording.chunks.len()
        );
        return Ok(());
    }

    let ts = now_ns();
    let mut uploaded = 0usize;
    let mut conflicts = 0usize;
    let mut failed = 0usize;
    for i in pending {
        let chunk = recording.chunks[i].clone();
        let job = UploadJob::for_chunk(&recording.machine_id, &dir, &recording.name, &chunk);
        match storage::sync::upload_one(&*backend, &job).await {
            Ok(UploadOutcome::Uploaded { etag }) | Ok(UploadOutcome::AlreadyPresent { etag }) => {
                // Mutate in memory; persist the manifest once after the loop
                // rather than reloading + rewriting it per chunk.
                recording.chunks[i].uploaded_at_ns = Some(ts);
                if etag.is_some() {
                    recording.chunks[i].remote_etag = etag;
                }
                uploaded += 1;
            }
            Ok(UploadOutcome::Conflict) => {
                log::warn!(
                    "chunk {} conflicts with different remote content at {} — skipping (run reconcile)",
                    chunk.index,
                    job.object_key
                );
                conflicts += 1;
            }
            Err(e) => {
                log::error!(
                    "chunk {} upload failed: {e} ({:?})",
                    chunk.index,
                    storage::classify(&e)
                );
                failed += 1;
            }
        }
    }

    // Only persist + re-upload the manifest when something actually changed, so
    // an all-failed/all-conflict run never clobbers newer remote manifest state.
    if uploaded > 0 {
        manifest::save_dir(&dir, &recording).context("saving updated manifest")?;
        upload_manifest(&*backend, &dir, &recording.machine_id, &recording.name).await?;
    }

    log::info!(
        "Uploaded {uploaded} chunk(s); {conflicts} conflict(s); {failed} failure(s) for '{}'.",
        args.name
    );
    if failed > 0 || conflicts > 0 {
        bail!("upload incomplete: {conflicts} conflict(s), {failed} failure(s)");
    }
    Ok(())
}

async fn upload_manifest(
    backend: &dyn StorageBackend,
    recording_dir: &Path,
    machine_id: &str,
    name: &str,
) -> Result<()> {
    let path = manifest::manifest_path(recording_dir);
    let bytes = std::fs::read(&path).context("reading manifest for upload")?;
    let digest = integrity::sha256(&bytes);
    let key = object_key_manifest(machine_id, name);
    backend
        .put(&key, &bytes, Some(&digest))
        .await
        .context("uploading manifest")?;
    Ok(())
}

// ---------------------------------------------------------------------------
// download
// ---------------------------------------------------------------------------

async fn run_download(args: DownloadArgs) -> Result<()> {
    let local_dir = storage::recording_dir(&args.name)?;
    let dest: PathBuf = match &args.to {
        Some(p) => PathBuf::from(p),
        None => local_dir.clone(),
    };
    let backend = StorageConfig::load()?.build_backend()?;

    // Prefer a local manifest; otherwise fetch it from the backend (needs the
    // machine id, since the object key is machine-scoped).
    let recording = match manifest::load_dir(&local_dir) {
        Ok(r) => r,
        Err(_) => {
            let machine = args.machine.clone().context(
                "no local manifest; pass --machine <id> so the remote manifest key can be built",
            )?;
            let key = object_key_manifest(&machine, &args.name);
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

    log::info!(
        "Downloaded and verified {verified} chunk(s) for '{}' to {}.",
        args.name,
        dest.display()
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

async fn run_rm(args: RmArgs) -> Result<()> {
    // recording_dir validates the name, so a traversal name can't reach
    // remove_dir_all or a remote prefix.
    let dir = storage::recording_dir(&args.name)?;
    let mut removed_anything = false;

    if args.remote {
        let backend = require_remote_backend()?;
        // Determine the machine id (object keys are machine-scoped).
        let machine = match manifest::load_dir(&dir) {
            Ok(r) => r.machine_id,
            Err(_) => args
                .machine
                .clone()
                .context("no local manifest; pass --machine <id> to delete remote objects")?,
        };
        let prefix = format!("{machine}/{}/", args.name);
        let objects = backend
            .list(&prefix)
            .await
            .context("listing remote objects")?;
        let count = objects.len();
        for obj in objects {
            backend
                .delete(&obj.key)
                .await
                .with_context(|| format!("deleting {}", obj.key))?;
        }
        if count > 0 {
            removed_anything = true;
        }
        log::info!("Deleted {count} remote object(s) for '{}'.", args.name);
    }

    if dir.exists() {
        std::fs::remove_dir_all(&dir).with_context(|| format!("removing {}", dir.display()))?;
        removed_anything = true;
        log::info!("Deleted local recording '{}'.", args.name);
    }

    // A name that matched nothing (local or remote) is an error, not a silent
    // success — otherwise a typo with --remote exits 0 having done nothing.
    if !removed_anything {
        let scope = if args.remote {
            "no local directory and no matching remote objects"
        } else {
            "no local directory"
        };
        bail!("no recording named '{}' found ({scope})", args.name);
    }
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

fn run_configure(args: ConfigureArgs) -> Result<()> {
    const BACKENDS: [&str; 5] = ["r2", "s3", "gcs", "minio", "local"];
    if !BACKENDS.contains(&args.backend.as_str()) {
        bail!(
            "unknown backend {:?} (expected one of: {})",
            args.backend,
            BACKENDS.join(", ")
        );
    }
    let is_remote = args.backend != "local";
    if is_remote && args.bucket.is_none() {
        bail!("--bucket is required for the {:?} backend", args.backend);
    }

    let cfg = StorageConfig {
        backend: args.backend.clone(),
        endpoint: args.endpoint,
        region: args.region,
        bucket: args.bucket,
        auto_upload: !args.no_auto_upload,
        retention_days_local: args.retention_days_local,
        disk_quota_gb: args.disk_quota_gb,
        recorder_instance: None,
        local_path: args.local_path.map(PathBuf::from),
    };
    cfg.save().context("writing config.toml")?;
    log::info!("Wrote [storage] config (backend = {}).", cfg.backend);

    // Credentials: write secrets.toml only when both halves are supplied.
    match (args.access_key_id, args.secret_access_key) {
        (Some(id), Some(secret)) => {
            use storage::secrets::{R2Credentials, Secret};
            let creds = R2Credentials {
                access_key_id: Secret::new(id),
                secret_access_key: Secret::new(secret),
            };
            let path = storage::secrets::default_path()?;
            storage::secrets::save_r2(&path, &creds).context("writing secrets.toml")?;
            log::info!("Wrote credentials to {} (chmod 0600).", path.display());
        }
        (None, None) => {
            if is_remote {
                log::warn!(
                    "no credentials supplied; set --access-key-id/--secret-access-key (or edit ~/.bubbaloop/secrets.toml) before uploading"
                );
            }
        }
        _ => bail!("--access-key-id and --secret-access-key must be provided together"),
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

fn run_profile_create(args: ProfileCreateArgs) -> Result<()> {
    use storage::profile::{CompressionKind, Profile, DEFAULT_CHUNK_SIZE_BYTES};
    use storage::recording::{RecordingMode, Trigger};

    let path = profile_path(&args.name)?;
    if path.exists() && !args.force {
        bail!(
            "profile '{}' already exists (use --force to overwrite)",
            args.name
        );
    }

    let compression = match args.compression.as_deref() {
        None | Some("zstd") => CompressionKind::Zstd,
        Some("lz4") => CompressionKind::Lz4,
        Some("none") => CompressionKind::None,
        Some(other) => bail!("unknown --compression {other:?} (expected zstd|lz4|none)"),
    };
    let mode = match args.mode.as_deref() {
        None | Some("streaming") => RecordingMode::Streaming,
        Some("ring_buffer") => RecordingMode::RingBuffer,
        Some(other) => bail!("unknown --mode {other:?} (expected streaming|ring_buffer)"),
    };
    let trigger = match args.trigger.as_deref() {
        None => None,
        Some("manual") => Some(Trigger::Manual),
        Some("on-event") => Some(Trigger::OnEvent),
        Some(other) => bail!("unknown --trigger {other:?} (expected manual|on-event)"),
    };

    let profile = Profile {
        name: args.name.clone(),
        description: args.description,
        topics: split_csv(args.topics.as_deref()),
        regex: args.regex,
        exclude: split_csv(args.exclude.as_deref()),
        include_local: args.include_local,
        chunk_size_bytes: args.chunk_size_bytes.unwrap_or(DEFAULT_CHUNK_SIZE_BYTES),
        compression,
        compression_level: Default::default(),
        chunk_crc: true,
        mode,
        window_secs: args.window_secs,
        trigger,
        sending_queue: Default::default(),
        retry_on_failure: Default::default(),
    };

    // save() validates against the v1 schema before writing.
    storage::profile::save(&path, &profile)
        .with_context(|| format!("saving profile '{}'", args.name))?;
    log::info!(
        "Created profile '{}' at {} (sha256 {}).",
        args.name,
        path.display(),
        storage::profile::canonical_sha256(&profile)
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

fn now_ns() -> u64 {
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
