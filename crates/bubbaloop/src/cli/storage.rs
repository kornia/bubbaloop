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
    self, integrity, manifest, object_key_chunk, object_key_manifest, reconcile::ReconcileReport,
    recording::Recording, ReconcileOptions, StorageBackend, StorageConfig, UploadJob,
    UploadOutcome,
};

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

    /// `complete` once the manifest is closed, else `open`.
    fn status(&self) -> &'static str {
        if self.ended_at_ns.is_some() {
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
    let backend = StorageConfig::load()?.build_backend()?;
    let objects = backend.list("").await.context("listing remote objects")?;
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
    let recording =
        manifest::load_dir(&dir).with_context(|| format!("no recording named '{}'", args.name))?;
    let backend = StorageConfig::load()?.build_backend()?;

    let pending: Vec<_> = recording
        .chunks
        .iter()
        .filter(|c| !c.is_uploaded())
        .cloned()
        .collect();
    if pending.is_empty() {
        log::info!(
            "'{}' is already fully uploaded ({} chunks).",
            args.name,
            recording.chunks.len()
        );
        return Ok(());
    }

    let mut uploaded = 0usize;
    let mut conflicts = 0usize;
    let mut failed = 0usize;
    for chunk in &pending {
        let job = UploadJob::for_chunk(&recording.machine_id, &dir, &recording.name, chunk);
        match storage::sync::upload_one(&*backend, &job).await {
            Ok(UploadOutcome::Uploaded { etag }) | Ok(UploadOutcome::AlreadyPresent { etag }) => {
                storage::sync::mark_chunk_uploaded(&dir, chunk.index, now_ns(), etag)
                    .with_context(|| format!("recording chunk {} as uploaded", chunk.index))?;
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

    // Push the manifest so remote state reflects the upload (§3.4.4 step 5).
    upload_manifest(&*backend, &dir, &recording.machine_id, &recording.name).await?;

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
        std::fs::write(chunks_dir.join(&chunk.name), &bytes)
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
    let dir = storage::recording_dir(&args.name)?;

    if args.remote {
        // Determine the machine id (object keys are machine-scoped).
        let machine = match manifest::load_dir(&dir) {
            Ok(r) => r.machine_id,
            Err(_) => args
                .machine
                .clone()
                .context("no local manifest; pass --machine <id> to delete remote objects")?,
        };
        let backend = StorageConfig::load()?.build_backend()?;
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
        log::info!("Deleted {count} remote object(s) for '{}'.", args.name);
    }

    if dir.exists() {
        std::fs::remove_dir_all(&dir).with_context(|| format!("removing {}", dir.display()))?;
        log::info!("Deleted local recording '{}'.", args.name);
    } else if !args.remote {
        bail!("no local recording named '{}'", args.name);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

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
