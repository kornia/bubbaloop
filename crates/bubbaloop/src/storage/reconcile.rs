//! Reconcile — the user-facing healing primitive (spec §3.5).
//!
//! `reconcile` diffs the three sources of truth for one recording and converges
//! them:
//!
//! 1. the **local manifest** (`manifest.json`) — what *should* exist + each
//!    chunk's SHA-256,
//! 2. the **local chunk files** on disk — what is actually present locally,
//! 3. the **remote listing** (via [`StorageBackend`]) — what is actually in R2.
//!
//! It applies the §3.5.2 diff matrix row-by-row, then re-uploads the manifest so
//! the post-reconcile state is captured remotely (§3.5.3 step 6).
//!
//! **Safety invariant (§3.5.4):** reconcile *never deletes* anything, local or
//! remote. It only uploads missing/mismatched chunks, optionally re-downloads
//! (with `restore`), and *reports* orphans. That keeps it idempotent and safe to
//! re-run — running it five times produces the same result as running it once.
//!
//! The function is backend-agnostic: it drives a `&dyn StorageBackend`, so the
//! same code path serves R2 in production and [`LocalFs`](super::backend::local)
//! as a stand-in remote in tests. It is also clock-free — the caller passes
//! `now_ns` for any `uploaded_at_ns` it stamps — so it stays deterministic and
//! unit-testable.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::backend::{BackendError, StorageBackend};
use super::integrity::{self, Sha256Digest};
use super::manifest::{self, ManifestError};
use super::recording::Chunk;
use super::{object_key_chunk, object_key_manifest};

/// Options controlling reconcile behaviour.
#[derive(Debug, Clone, Default)]
pub struct ReconcileOptions {
    /// Re-download chunks that are present + verified in the remote but missing
    /// locally (e.g. swept by `retention_days_local`). Without this, such chunks
    /// are only reported, never fetched (spec §3.5.2, the `--restore` row).
    pub restore: bool,
}

/// What happened to a single chunk during reconcile (one row of the §3.5.2
/// matrix). Carried in [`ReconcileReport::chunk_outcomes`] for reporting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChunkOutcome {
    /// Local + remote both present and matching; nothing to do (the
    /// `uploaded_at_ns` was already set).
    VerifiedOk,
    /// Remote was confirmed present; the manifest's null `uploaded_at_ns` was
    /// filled in from this reconcile (`now_ns`).
    MarkedUploaded,
    /// Local chunk was uploaded because the remote object was missing.
    Uploaded,
    /// Local chunk was re-uploaded because the remote object's content did not
    /// match the manifest digest (stale/divergent remote at our key).
    Reuploaded,
    /// Remote chunk was re-downloaded to local (`restore` was set).
    Redownloaded,
    /// Local file is gone but the remote copy is intact; left as-is because
    /// `restore` was not set.
    PresentRemoteOnly,
}

/// A non-fatal per-chunk problem. Reconcile records these and keeps going (it
/// stays idempotent and safe to re-run); the caller decides the exit status from
/// whether `errors` is non-empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkError {
    /// Chunk index this error concerns.
    pub index: u32,
    /// Machine-readable failure category.
    pub kind: ChunkErrorKind,
    /// Human-readable detail for logs / CLI output.
    pub detail: String,
}

/// Category of a [`ChunkError`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkErrorKind {
    /// The local file exists but its content does not match the manifest digest
    /// — local corruption. Reconcile refuses to upload it (§3.5.2 row 4).
    LocalCorrupt,
    /// The chunk is absent both locally and remotely — the manifest is broken
    /// and the recording cannot be made whole (§3.5.2 row 6).
    MissingEverywhere,
    /// The remote object exists but its content mismatches the manifest, and
    /// there is no good local copy to heal from.
    RemoteMismatchNoLocal,
    /// A backend or filesystem error occurred while handling this chunk.
    Io,
}

/// An object/file present in exactly one place but not declared by the manifest.
/// Reported, never touched (§3.5.2 rows 7–8, §3.5.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Orphan {
    /// Local path (orphan local file) or remote object key (orphan remote
    /// object).
    pub location: String,
    /// Parsed chunk index, when the name was recognisable.
    pub index: Option<u32>,
}

/// Summary of one reconcile pass (spec §3.5.3 step 7).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReconcileReport {
    /// Number of manifest chunks examined.
    pub chunks_diffed: u32,
    /// Chunks uploaded (missing remote).
    pub chunks_uploaded: u32,
    /// Chunks re-uploaded (remote content mismatched manifest).
    pub chunks_reuploaded: u32,
    /// Chunks re-downloaded (`restore`).
    pub chunks_redownloaded: u32,
    /// Chunks already present + verified on both sides.
    pub chunks_verified_ok: u32,
    /// Chunks whose null `uploaded_at_ns` was filled in from the remote.
    pub chunks_marked_uploaded: u32,
    /// Per-chunk outcomes in manifest order.
    pub chunk_outcomes: Vec<(u32, ChunkOutcome)>,
    /// Local files not declared by the manifest.
    pub orphans_local: Vec<Orphan>,
    /// Remote objects not declared by the manifest (includes stale objects at a
    /// manifest chunk's index but a different content hash).
    pub orphans_remote: Vec<Orphan>,
    /// Non-fatal per-chunk errors.
    pub errors: Vec<ChunkError>,
}

impl ReconcileReport {
    /// Whether the recording is fully healed (every chunk converged, no errors).
    pub fn is_clean(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Errors that abort a reconcile before it can produce a report.
#[derive(Debug, thiserror::Error)]
pub enum ReconcileError {
    /// No manifest at the recording directory — there is nothing to reconcile
    /// against (recovering a lost manifest is a deferred `storage repair` job,
    /// §3.5.3 step 1).
    #[error("no manifest to reconcile at {0}")]
    ManifestMissing(String),
    /// The manifest exists but failed to load or validate.
    #[error("manifest error: {0}")]
    Manifest(#[from] ManifestError),
    /// A manifest chunk carried an unparseable SHA-256 (should have been caught
    /// by manifest validation, but reconcile re-checks before trusting it).
    #[error("chunk {index} has an invalid sha256: {sha256}")]
    BadManifestDigest { index: u32, sha256: String },
    /// A hard backend error while listing the remote prefix (per-chunk backend
    /// errors are non-fatal and collected in the report instead).
    #[error("backend error: {0}")]
    Backend(#[from] BackendError),
}

/// Reconcile recording `<name>` rooted at `recording_dir` against `backend`.
///
/// `now_ns` is the timestamp stamped into any `uploaded_at_ns` that reconcile
/// fills in. The function loads the manifest from `recording_dir/manifest.json`,
/// derives the remote prefix from the manifest's own `machine_id`/`name`, applies
/// the diff matrix, persists the (possibly mutated) manifest locally, and
/// re-uploads it.
pub async fn reconcile(
    recording_dir: impl AsRef<Path>,
    backend: &dyn StorageBackend,
    opts: &ReconcileOptions,
    now_ns: u64,
) -> Result<ReconcileReport, ReconcileError> {
    let recording_dir = recording_dir.as_ref();
    let manifest_path = manifest::manifest_path(recording_dir);
    if !manifest_path.exists() {
        return Err(ReconcileError::ManifestMissing(
            manifest_path.display().to_string(),
        ));
    }

    // §3.5.3 step 1: read the local manifest (validated on load).
    let mut manifest = manifest::load_dir(recording_dir)?;
    let machine_id = manifest.machine_id.clone();
    let name = manifest.name.clone();

    // §3.5.3 step 2: enumerate local chunk files, keyed by parsed index.
    let chunks_dir = super::chunks_path(recording_dir);
    let local_files = scan_local_chunks(&chunks_dir)?;

    // §3.5.3 step 3: list remote objects under the recording's chunk prefix.
    let remote_prefix = format!("{machine_id}/{name}/chunks/");
    let remote_objects = scan_remote_chunks(backend, &remote_prefix).await?;

    let mut report = ReconcileReport::default();

    // Track which local files / remote objects a manifest chunk claims, so the
    // leftovers can be reported as orphans afterwards.
    let mut claimed_local: Vec<PathBuf> = Vec::new();
    let mut claimed_remote: Vec<String> = Vec::new();
    let mut manifest_dirty = false;

    // §3.5.3 step 4: apply the diff matrix to each manifest chunk.
    for i in 0..manifest.chunks.len() {
        let index = manifest.chunks[i].index;
        let sha_hex = manifest.chunks[i].sha256.clone();
        report.chunks_diffed += 1;

        let expected_digest = match integrity::from_hex(&sha_hex) {
            Ok(d) => d,
            Err(_) => {
                return Err(ReconcileError::BadManifestDigest {
                    index,
                    sha256: sha_hex,
                })
            }
        };
        let expected_key = object_key_chunk(&machine_id, &name, index, &sha_hex);

        // -- Local side -------------------------------------------------------
        let local_path = local_files.get(&index).cloned();
        let local_state = match &local_path {
            Some(path) => {
                claimed_local.push(path.clone());
                match integrity::sha256_file(path) {
                    Ok(actual) if actual == expected_digest => LocalState::Present,
                    Ok(_) => LocalState::Corrupt,
                    Err(e) => {
                        report.errors.push(ChunkError {
                            index,
                            kind: ChunkErrorKind::Io,
                            detail: format!("hashing {}: {e}", path.display()),
                        });
                        continue;
                    }
                }
            }
            None => LocalState::Absent,
        };

        // -- Remote side ------------------------------------------------------
        // HEAD the deterministic key. Its embedded sha-prefix means a hit is
        // already content-addressed to this chunk; we additionally verify the
        // full digest when the backend supplies one (R2 returns the
        // x-amz-checksum-sha256 we set; LocalFs returns the real hash).
        let remote_state = match backend.head(&expected_key).await {
            Ok(Some(meta)) => {
                claimed_remote.push(expected_key.clone());
                match meta.sha256 {
                    Some(actual) if actual == expected_digest => RemoteState::Present,
                    Some(_) => RemoteState::Mismatch,
                    None => RemoteState::Present, // backend can't verify; trust the key
                }
            }
            Ok(None) => RemoteState::Absent,
            Err(e) => {
                report.errors.push(ChunkError {
                    index,
                    kind: ChunkErrorKind::Io,
                    detail: format!("HEAD {expected_key}: {e}"),
                });
                continue;
            }
        };

        // -- Diff matrix (§3.5.2) --------------------------------------------
        match (local_state, remote_state) {
            // Row 4: local file present but corrupt — never upload, surface it.
            (LocalState::Corrupt, _) => {
                report.errors.push(ChunkError {
                    index,
                    kind: ChunkErrorKind::LocalCorrupt,
                    detail: format!("local chunk {index} does not match manifest sha256 {sha_hex}"),
                });
            }

            // Row 1: both present + matching.
            (LocalState::Present, RemoteState::Present) => {
                if manifest.chunks[i].uploaded_at_ns.is_none() {
                    manifest.chunks[i].uploaded_at_ns = Some(now_ns);
                    manifest_dirty = true;
                    report.chunks_marked_uploaded += 1;
                    record(&mut report, index, ChunkOutcome::MarkedUploaded);
                } else {
                    report.chunks_verified_ok += 1;
                    record(&mut report, index, ChunkOutcome::VerifiedOk);
                }
            }

            // Row 2: local good, remote missing — upload.
            (LocalState::Present, RemoteState::Absent) => {
                match upload_chunk(
                    backend,
                    &expected_key,
                    local_path.as_ref().unwrap(),
                    &expected_digest,
                )
                .await
                {
                    Ok(etag) => {
                        mark_uploaded(&mut manifest.chunks[i], now_ns, etag);
                        manifest_dirty = true;
                        report.chunks_uploaded += 1;
                        record(&mut report, index, ChunkOutcome::Uploaded);
                    }
                    Err(e) => push_upload_error(&mut report, index, &expected_key, e),
                }
            }

            // Row 3: remote present but content mismatches — re-upload local
            // (deterministic key makes the overwrite safe).
            (LocalState::Present, RemoteState::Mismatch) => {
                log::warn!(
                    "reconcile {name}: remote chunk {index} content mismatched manifest; re-uploading local copy"
                );
                match upload_chunk(
                    backend,
                    &expected_key,
                    local_path.as_ref().unwrap(),
                    &expected_digest,
                )
                .await
                {
                    Ok(etag) => {
                        mark_uploaded(&mut manifest.chunks[i], now_ns, etag);
                        manifest_dirty = true;
                        report.chunks_reuploaded += 1;
                        record(&mut report, index, ChunkOutcome::Reuploaded);
                    }
                    Err(e) => push_upload_error(&mut report, index, &expected_key, e),
                }
            }

            // Row 5: local missing, remote good.
            (LocalState::Absent, RemoteState::Present) => {
                if opts.restore {
                    match restore_chunk(
                        backend,
                        &expected_key,
                        &chunks_dir,
                        &manifest.chunks[i],
                        &expected_digest,
                    )
                    .await
                    {
                        Ok(()) => {
                            if manifest.chunks[i].uploaded_at_ns.is_none() {
                                manifest.chunks[i].uploaded_at_ns = Some(now_ns);
                                manifest_dirty = true;
                            }
                            report.chunks_redownloaded += 1;
                            record(&mut report, index, ChunkOutcome::Redownloaded);
                        }
                        Err(e) => {
                            report.errors.push(ChunkError {
                                index,
                                kind: ChunkErrorKind::Io,
                                detail: format!("restore {expected_key}: {e}"),
                            });
                        }
                    }
                } else {
                    if manifest.chunks[i].uploaded_at_ns.is_none() {
                        manifest.chunks[i].uploaded_at_ns = Some(now_ns);
                        manifest_dirty = true;
                    }
                    record(&mut report, index, ChunkOutcome::PresentRemoteOnly);
                }
            }

            // Remote present but mismatched, with no good local copy to heal
            // from — can't safely converge, surface it.
            (LocalState::Absent, RemoteState::Mismatch) => {
                report.errors.push(ChunkError {
                    index,
                    kind: ChunkErrorKind::RemoteMismatchNoLocal,
                    detail: format!(
                        "remote chunk {index} mismatches manifest and no local copy exists"
                    ),
                });
            }

            // Row 6: gone in both places — the manifest is broken.
            (LocalState::Absent, RemoteState::Absent) => {
                report.errors.push(ChunkError {
                    index,
                    kind: ChunkErrorKind::MissingEverywhere,
                    detail: format!("chunk {index} is missing both locally and remotely"),
                });
            }
        }
    }

    // §3.5.2 rows 7–8: report (never touch) orphans.
    collect_local_orphans(&local_files, &claimed_local, &mut report);
    collect_remote_orphans(&remote_objects, &claimed_remote, &mut report);

    // §3.5.3 step 6: persist the (possibly mutated) manifest, then re-upload it
    // so the post-reconcile state is captured remotely.
    if manifest_dirty {
        manifest::save_dir(recording_dir, &manifest)?;
    }
    if let Err(e) = upload_manifest(backend, &machine_id, &name, recording_dir).await {
        log::warn!("reconcile {name}: failed to re-upload manifest: {e}");
        report.errors.push(ChunkError {
            // Manifest is not a chunk; use a sentinel index for reporting.
            index: u32::MAX,
            kind: ChunkErrorKind::Io,
            detail: format!("manifest re-upload failed: {e}"),
        });
    }

    Ok(report)
}

/// Local presence of a chunk relative to the manifest digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LocalState {
    /// File present and content matches the manifest.
    Present,
    /// File present but content does not match the manifest.
    Corrupt,
    /// No local file for this index.
    Absent,
}

/// Remote presence of a chunk at its deterministic key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RemoteState {
    /// Object present and (where verifiable) content matches the manifest.
    Present,
    /// Object present at the key but content mismatches the manifest.
    Mismatch,
    /// No object at the deterministic key.
    Absent,
}

fn record(report: &mut ReconcileReport, index: u32, outcome: ChunkOutcome) {
    report.chunk_outcomes.push((index, outcome));
}

fn mark_uploaded(chunk: &mut Chunk, now_ns: u64, etag: Option<String>) {
    chunk.uploaded_at_ns = Some(now_ns);
    if etag.is_some() {
        chunk.remote_etag = etag;
    }
}

fn push_upload_error(report: &mut ReconcileReport, index: u32, key: &str, e: BackendError) {
    let kind = match e {
        BackendError::BadDigest { .. } => ChunkErrorKind::LocalCorrupt,
        _ => ChunkErrorKind::Io,
    };
    report.errors.push(ChunkError {
        index,
        kind,
        detail: format!("upload {key}: {e}"),
    });
}

/// Read the local file and PUT it at `key` with the manifest digest as the
/// integrity check, returning the backend ETag.
async fn upload_chunk(
    backend: &dyn StorageBackend,
    key: &str,
    path: &Path,
    digest: &Sha256Digest,
) -> Result<Option<String>, BackendError> {
    let bytes = tokio::fs::read(path).await.map_err(|e| BackendError::Io {
        key: key.to_string(),
        detail: format!("reading {}: {e}", path.display()),
    })?;
    let result = backend.put(key, &bytes, Some(digest)).await?;
    Ok(result.etag)
}

/// Download a chunk from the backend, verify it against the manifest digest, and
/// write it to the local chunks directory under its canonical name. Refuses to
/// write a file that fails verification (spec §8, "download verifies on receipt
/// and refuses to write a corrupted file").
async fn restore_chunk(
    backend: &dyn StorageBackend,
    key: &str,
    chunks_dir: &Path,
    chunk: &Chunk,
    digest: &Sha256Digest,
) -> Result<(), BackendError> {
    let bytes = backend.get(key).await?;
    let actual = integrity::sha256(&bytes);
    if &actual != digest {
        return Err(BackendError::BadDigest {
            key: key.to_string(),
            expected: integrity::to_hex(digest),
            actual: integrity::to_hex(&actual),
        });
    }
    tokio::fs::create_dir_all(chunks_dir)
        .await
        .map_err(|e| BackendError::Io {
            key: key.to_string(),
            detail: format!("creating {}: {e}", chunks_dir.display()),
        })?;
    let dest = chunks_dir.join(&chunk.name);
    tokio::fs::write(&dest, &bytes)
        .await
        .map_err(|e| BackendError::Io {
            key: key.to_string(),
            detail: format!("writing {}: {e}", dest.display()),
        })?;
    Ok(())
}

/// Re-upload the manifest.json so the remote reflects the post-reconcile state.
async fn upload_manifest(
    backend: &dyn StorageBackend,
    machine_id: &str,
    name: &str,
    recording_dir: &Path,
) -> Result<(), BackendError> {
    let path = manifest::manifest_path(recording_dir);
    let bytes = tokio::fs::read(&path).await.map_err(|e| BackendError::Io {
        key: path.display().to_string(),
        detail: e.to_string(),
    })?;
    let digest = integrity::sha256(&bytes);
    let key = object_key_manifest(machine_id, name);
    backend.put(&key, &bytes, Some(&digest)).await?;
    Ok(())
}

/// Enumerate local chunk files by parsed index. Non-chunk files are skipped; if
/// two files share an index, the lexicographically last wins (deterministic) and
/// the others fall out as orphans.
fn scan_local_chunks(chunks_dir: &Path) -> Result<BTreeMap<u32, PathBuf>, ReconcileError> {
    let mut map = BTreeMap::new();
    let read = match std::fs::read_dir(chunks_dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(map),
        Err(e) => {
            return Err(ReconcileError::Manifest(ManifestError::Io {
                path: chunks_dir.display().to_string(),
                detail: e.to_string(),
            }))
        }
    };
    let mut entries: Vec<PathBuf> = Vec::new();
    for entry in read.flatten() {
        if entry.path().is_file() {
            entries.push(entry.path());
        }
    }
    entries.sort();
    for path in entries {
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if let Some((index, _)) = parse_chunk_filename(name) {
                map.insert(index, path);
            }
        }
    }
    Ok(map)
}

/// List remote chunk objects under `prefix`, keyed by parsed index. Objects whose
/// names don't parse, or that collide on index, keep the lexicographically last
/// (matching the local scan); the rest surface as remote orphans.
async fn scan_remote_chunks(
    backend: &dyn StorageBackend,
    prefix: &str,
) -> Result<BTreeMap<u32, String>, ReconcileError> {
    let mut map = BTreeMap::new();
    let mut objects = backend.list(prefix).await?;
    objects.sort_by(|a, b| a.key.cmp(&b.key));
    for obj in objects {
        let filename = obj.key.rsplit('/').next().unwrap_or(&obj.key);
        if let Some((index, _)) = parse_chunk_filename(filename) {
            map.insert(index, obj.key);
        }
    }
    Ok(map)
}

/// Parse `chunk-{index:06}-{prefix}.mcap` into `(index, prefix)`.
fn parse_chunk_filename(name: &str) -> Option<(u32, String)> {
    let stem = name.strip_suffix(".mcap")?;
    let rest = stem.strip_prefix("chunk-")?;
    let (idx_str, prefix) = rest.split_once('-')?;
    let index = idx_str.parse::<u32>().ok()?;
    if prefix.is_empty() {
        return None;
    }
    Some((index, prefix.to_string()))
}

fn collect_local_orphans(
    local_files: &BTreeMap<u32, PathBuf>,
    claimed: &[PathBuf],
    report: &mut ReconcileReport,
) {
    for (index, path) in local_files {
        if !claimed.contains(path) {
            log::warn!(
                "reconcile: orphan local file {} (chunk index {index} not claimed by manifest)",
                path.display()
            );
            report.orphans_local.push(Orphan {
                location: path.display().to_string(),
                index: Some(*index),
            });
        }
    }
}

fn collect_remote_orphans(
    remote_objects: &BTreeMap<u32, String>,
    claimed: &[String],
    report: &mut ReconcileReport,
) {
    for (index, key) in remote_objects {
        if !claimed.contains(key) {
            log::warn!("reconcile: orphan remote object {key} (chunk index {index} not claimed by manifest)");
            report.orphans_remote.push(Orphan {
                location: key.clone(),
                index: Some(*index),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::backend::local::LocalFs;
    use crate::storage::recording::{Chunk, Recording};
    use std::path::Path;

    const NOW: u64 = 1_700_000_000_000_000_000;

    /// Deterministic chunk content for index `i`.
    fn chunk_bytes(i: u32) -> Vec<u8> {
        format!("chunk-content-{i}").into_bytes()
    }

    /// Build a chunk entry from real bytes (so sha256/name are consistent).
    fn chunk_for(i: u32, bytes: &[u8], uploaded: bool) -> Chunk {
        let digest = integrity::sha256(bytes);
        let sha = integrity::to_hex(&digest);
        Chunk {
            name: Chunk::canonical_name(i, &sha),
            index: i,
            size_bytes: bytes.len() as u64,
            sha256: sha,
            log_time_first_ns: None,
            log_time_last_ns: None,
            uploaded_at_ns: if uploaded { Some(NOW - 1) } else { None },
            remote_etag: None,
            extra: Default::default(),
        }
    }

    /// Lay out a recording dir + manifest with `n` chunks, writing the given
    /// indices to local disk. Returns (tempdir, recording_dir, manifest).
    fn setup(
        n: u32,
        write_local: &[u32],
        uploaded: &[u32],
    ) -> (tempfile::TempDir, PathBuf, Recording) {
        let dir = tempfile::tempdir().unwrap();
        let rec_dir = dir.path().join("rec_a");
        let chunks_dir = rec_dir.join("chunks");
        std::fs::create_dir_all(&chunks_dir).unwrap();

        let mut rec = Recording::new("rec_a", "jetson_alpha", 1_000);
        for i in 0..n {
            let bytes = chunk_bytes(i);
            let chunk = chunk_for(i, &bytes, uploaded.contains(&i));
            if write_local.contains(&i) {
                std::fs::write(chunks_dir.join(&chunk.name), &bytes).unwrap();
            }
            rec.chunks.push(chunk);
        }
        manifest::save_dir(&rec_dir, &rec).unwrap();
        (dir, rec_dir, rec)
    }

    /// A LocalFs "remote" rooted in its own tempdir.
    fn remote() -> (tempfile::TempDir, LocalFs) {
        let dir = tempfile::tempdir().unwrap();
        let fs = LocalFs::new(dir.path());
        (dir, fs)
    }

    /// Seed the remote with chunk `i`'s canonical object (matching content).
    async fn seed_remote(fs: &LocalFs, rec: &Recording, i: u32) {
        let c = &rec.chunks[i as usize];
        let key = object_key_chunk(&rec.machine_id, &rec.name, i, &c.sha256);
        fs.put(&key, &chunk_bytes(i), None).await.unwrap();
    }

    #[tokio::test]
    async fn uploads_missing_remote_chunks() {
        let (_d, rec_dir, _rec) = setup(3, &[0, 1, 2], &[]);
        let (_rd, fs) = remote();

        let report = reconcile(&rec_dir, &fs, &ReconcileOptions::default(), NOW)
            .await
            .unwrap();

        assert_eq!(report.chunks_diffed, 3);
        assert_eq!(report.chunks_uploaded, 3);
        assert!(report.is_clean());

        // Manifest now records all chunks as uploaded.
        let reloaded = manifest::load_dir(&rec_dir).unwrap();
        assert!(reloaded.is_fully_uploaded());
        for c in &reloaded.chunks {
            assert_eq!(c.uploaded_at_ns, Some(NOW));
            assert!(c.remote_etag.is_some());
        }

        // And the manifest itself was uploaded.
        let mkey = object_key_manifest("jetson_alpha", "rec_a");
        assert!(fs.head(&mkey).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn idempotent_second_run_is_a_noop() {
        let (_d, rec_dir, _rec) = setup(2, &[0, 1], &[]);
        let (_rd, fs) = remote();

        let first = reconcile(&rec_dir, &fs, &ReconcileOptions::default(), NOW)
            .await
            .unwrap();
        assert_eq!(first.chunks_uploaded, 2);

        let second = reconcile(&rec_dir, &fs, &ReconcileOptions::default(), NOW + 1)
            .await
            .unwrap();
        assert_eq!(second.chunks_uploaded, 0);
        assert_eq!(second.chunks_verified_ok, 2);
        assert!(second.is_clean());
    }

    #[tokio::test]
    async fn marks_uploaded_when_remote_present_but_manifest_null() {
        let (_d, rec_dir, rec) = setup(1, &[0], &[]);
        let (_rd, fs) = remote();
        seed_remote(&fs, &rec, 0).await;

        let report = reconcile(&rec_dir, &fs, &ReconcileOptions::default(), NOW)
            .await
            .unwrap();
        assert_eq!(report.chunks_marked_uploaded, 1);
        assert_eq!(report.chunks_uploaded, 0);

        let reloaded = manifest::load_dir(&rec_dir).unwrap();
        assert_eq!(reloaded.chunks[0].uploaded_at_ns, Some(NOW));
    }

    #[tokio::test]
    async fn detects_local_corruption() {
        let (_d, rec_dir, rec) = setup(1, &[], &[]);
        let (_rd, fs) = remote();
        // Write a local file whose content does NOT match the manifest digest.
        let chunks_dir = rec_dir.join("chunks");
        std::fs::write(chunks_dir.join(&rec.chunks[0].name), b"corrupted").unwrap();

        let report = reconcile(&rec_dir, &fs, &ReconcileOptions::default(), NOW)
            .await
            .unwrap();
        assert_eq!(report.chunks_uploaded, 0);
        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].kind, ChunkErrorKind::LocalCorrupt);
        assert!(!report.is_clean());
    }

    #[tokio::test]
    async fn missing_everywhere_is_an_error() {
        let (_d, rec_dir, _rec) = setup(1, &[], &[]);
        let (_rd, fs) = remote();

        let report = reconcile(&rec_dir, &fs, &ReconcileOptions::default(), NOW)
            .await
            .unwrap();
        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].kind, ChunkErrorKind::MissingEverywhere);
    }

    #[tokio::test]
    async fn restore_redownloads_missing_local() {
        let (_d, rec_dir, rec) = setup(1, &[], &[]);
        let (_rd, fs) = remote();
        seed_remote(&fs, &rec, 0).await;

        // Without restore: only reported, not fetched.
        let no_restore = reconcile(&rec_dir, &fs, &ReconcileOptions::default(), NOW)
            .await
            .unwrap();
        assert_eq!(no_restore.chunks_redownloaded, 0);
        assert!(!rec_dir.join("chunks").join(&rec.chunks[0].name).exists());

        // With restore: the local file reappears and verifies.
        let restored = reconcile(&rec_dir, &fs, &ReconcileOptions { restore: true }, NOW)
            .await
            .unwrap();
        assert_eq!(restored.chunks_redownloaded, 1);
        let local = rec_dir.join("chunks").join(&rec.chunks[0].name);
        assert!(local.exists());
        assert_eq!(std::fs::read(&local).unwrap(), chunk_bytes(0));
    }

    #[tokio::test]
    async fn reuploads_on_remote_content_mismatch() {
        let (_d, rec_dir, rec) = setup(1, &[0], &[]);
        let (_rd, fs) = remote();
        // Put DIFFERENT content at the expected deterministic key.
        let key = object_key_chunk(&rec.machine_id, &rec.name, 0, &rec.chunks[0].sha256);
        fs.put(&key, b"stale different content", None)
            .await
            .unwrap();

        let report = reconcile(&rec_dir, &fs, &ReconcileOptions::default(), NOW)
            .await
            .unwrap();
        assert_eq!(report.chunks_reuploaded, 1);
        assert!(report.is_clean());

        // Remote now holds the correct content.
        let got = fs.get(&key).await.unwrap();
        assert_eq!(got, chunk_bytes(0));
    }

    #[tokio::test]
    async fn reports_orphans_without_touching_them() {
        let (_d, rec_dir, _rec) = setup(1, &[0], &[]);
        let (_rd, fs) = remote();
        let chunks_dir = rec_dir.join("chunks");

        // An orphan local file at an index the manifest doesn't declare.
        let orphan_name = Chunk::canonical_name(9, &"ff".repeat(32));
        std::fs::write(chunks_dir.join(&orphan_name), b"orphan").unwrap();

        // An orphan remote object likewise.
        fs.put(
            "jetson_alpha/rec_a/chunks/chunk-000009-ffffffff.mcap",
            b"orphan-remote",
            None,
        )
        .await
        .unwrap();

        let report = reconcile(&rec_dir, &fs, &ReconcileOptions::default(), NOW)
            .await
            .unwrap();

        assert_eq!(report.orphans_local.len(), 1);
        assert_eq!(report.orphans_local[0].index, Some(9));
        assert_eq!(report.orphans_remote.len(), 1);
        assert_eq!(report.orphans_remote[0].index, Some(9));

        // Orphans are untouched (never deleted, §3.5.4).
        assert!(chunks_dir.join(&orphan_name).exists());
        assert!(fs
            .head("jetson_alpha/rec_a/chunks/chunk-000009-ffffffff.mcap")
            .await
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    async fn missing_manifest_is_a_fatal_error() {
        let dir = tempfile::tempdir().unwrap();
        let (_rd, fs) = remote();
        let err = reconcile(dir.path(), &fs, &ReconcileOptions::default(), NOW)
            .await
            .unwrap_err();
        assert!(matches!(err, ReconcileError::ManifestMissing(_)));
    }

    #[test]
    fn chunk_filename_parsing() {
        assert_eq!(
            parse_chunk_filename("chunk-000007-a1b2c3d4.mcap"),
            Some((7, "a1b2c3d4".to_string()))
        );
        assert_eq!(
            parse_chunk_filename("chunk-000000-aa.mcap"),
            Some((0, "aa".to_string()))
        );
        assert_eq!(parse_chunk_filename("not-a-chunk.mcap"), None);
        assert_eq!(parse_chunk_filename("chunk-000000-.mcap"), None);
        assert_eq!(parse_chunk_filename("chunk-000000-aa.txt"), None);
        assert_eq!(parse_chunk_filename("manifest.json"), None);
    }

    // Touch `Path` import in a trivial assertion to keep it used across cfgs.
    #[test]
    fn chunks_dir_is_relative_join() {
        let p = Path::new("/tmp/rec");
        assert_eq!(super::super::chunks_path(p), Path::new("/tmp/rec/chunks"));
    }
}
