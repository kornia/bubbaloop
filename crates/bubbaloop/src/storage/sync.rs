//! Sync — background upload of finalized chunks (spec §3.4).
//!
//! Sync replaces the standalone `uploader/` node. In production it runs as one
//! background task in the daemon, driven by three triggers (§3.4.1): a
//! chunk-finalize event from the recorder, a manifest-finalize event on `record
//! stop`, and a startup scan that re-enqueues anything left un-uploaded.
//!
//! This module is the **deterministic core** the driver task is built from:
//!
//! - [`BackoffSchedule`] / [`SharedBackoff`] — the shared exponential backoff
//!   (§3.4.3): immediate, then 5s→10s→20s→40s→60s clamp, dead-letter after
//!   `max_elapsed_time`. Backoff is shared across jobs (a down R2 fails them all)
//!   and resets on the first success.
//! - [`classify`] — retryable vs terminal vs pause-sync error mapping (§3.4.3).
//! - [`DeadLetterList`] — the persisted `~/.bubbaloop/storage/dead_letter.json`
//!   that survives daemon restarts and that `reconcile` picks back up (§3.4.6).
//! - [`upload_one`] — the per-chunk upload sequence (§3.4.4): re-verify the local
//!   SHA-256, HEAD-before-PUT for idempotency, PUT with the checksum header.
//! - [`pending_jobs`] / [`mark_chunk_uploaded`] — the startup-scan enqueue and
//!   the manifest mutation that records `uploaded_at_ns`.
//!
//! The async **queue driver** (the bounded queue, the concurrency cap, and the
//! in-process MPSC of `ChunkFinalized` events) lives in
//! [`sync_driver`](super::sync_driver), built on this core. The recorder will
//! emit those events; until then the driver is exercised end-to-end against
//! [`LocalFs`](super::backend::local) as a stand-in remote. Everything the driver
//! needs to make decisions lives in this module with no clock or network
//! dependency (callers pass `now_ns`).

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::backend::{BackendError, StorageBackend};
use super::integrity;
use super::profile::RetryOnFailure;
use super::recording::{Chunk, Recording};
use super::{manifest, object_key_chunk, StoragePathError};

// ---------------------------------------------------------------------------
// Error classification (§3.4.3)
// ---------------------------------------------------------------------------

/// How sync should treat a backend failure (spec §3.4.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    /// Transient — retry with the shared backoff (network, 5xx, 429, DNS).
    Retryable,
    /// Permanent for this job — dead-letter immediately, do not keep retrying
    /// (bad auth, local corruption, invalid key).
    Terminal,
    /// Misconfiguration affecting *every* job — pause sync entirely until the
    /// operator fixes it (`NoSuchBucket`).
    PauseSync,
}

/// Classify a [`BackendError`] for the retry policy.
pub fn classify(err: &BackendError) -> ErrorClass {
    match err {
        BackendError::NoSuchBucket { .. } => ErrorClass::PauseSync,
        BackendError::BadDigest { .. }
        | BackendError::InvalidKey(_)
        | BackendError::Forbidden { .. } => ErrorClass::Terminal,
        // NotFound isn't expected on the upload path; treat transport/IO and any
        // other transient condition as retryable.
        BackendError::NotFound { .. } | BackendError::Io { .. } => ErrorClass::Retryable,
    }
}

// ---------------------------------------------------------------------------
// Backoff (§3.4.3)
// ---------------------------------------------------------------------------

/// The exponential backoff parameters, derived from a profile's
/// `retry_on_failure` block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackoffSchedule {
    /// First non-zero delay (attempt 2). Default 5s.
    pub initial: Duration,
    /// Upper clamp on the delay. Default 60s.
    pub max: Duration,
    /// Total failure streak duration after which a job is dead-lettered.
    /// Default 5m.
    pub max_elapsed: Duration,
}

impl Default for BackoffSchedule {
    fn default() -> Self {
        Self {
            initial: Duration::from_secs(5),
            max: Duration::from_secs(60),
            max_elapsed: Duration::from_secs(300),
        }
    }
}

impl BackoffSchedule {
    /// Build from a profile's `retry_on_failure` block.
    pub fn from_profile(r: &RetryOnFailure) -> Self {
        Self {
            initial: r.initial_interval.as_duration(),
            max: r.max_interval.as_duration(),
            max_elapsed: r.max_elapsed_time.as_duration(),
        }
    }

    /// Delay before `attempt` (1-based). Attempt 1 is immediate; attempt 2 is
    /// `initial`; each subsequent attempt doubles, clamped to `max` (§3.4.3):
    /// `0, 5s, 10s, 20s, 40s, 60s, 60s, …`.
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        if attempt <= 1 {
            return Duration::ZERO;
        }
        // attempt 2 → 2^0 × initial, attempt 3 → 2^1 × initial, …
        let shifts = (attempt - 2).min(32);
        let initial_ms = self.initial.as_millis();
        let scaled = initial_ms.saturating_mul(1u128 << shifts);
        let clamped = scaled.min(self.max.as_millis());
        Duration::from_millis(clamped.min(u64::MAX as u128) as u64)
    }
}

/// The shared backoff state. One instance is shared by the whole queue: a single
/// R2 outage fails every job, so per-job backoff would just burn the queue
/// retrying hopeless work (§3.4.3). Any job's success resets it for everyone.
#[derive(Debug, Clone)]
pub struct SharedBackoff {
    schedule: BackoffSchedule,
    /// Number of consecutive failures in the current streak.
    failures: u32,
    /// When the current failure streak started (nanoseconds since epoch).
    streak_started_ns: Option<u64>,
}

/// The decision returned after recording a failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackoffDecision {
    /// How long to wait before the next attempt.
    pub delay: Duration,
    /// Whether the failure streak has exceeded `max_elapsed` and the job should
    /// be dead-lettered instead of retried.
    pub dead_letter: bool,
    /// Failure count in the current streak (1-based).
    pub failures: u32,
}

impl SharedBackoff {
    /// New backoff in the rested state (no failures).
    pub fn new(schedule: BackoffSchedule) -> Self {
        Self {
            schedule,
            failures: 0,
            streak_started_ns: None,
        }
    }

    /// Current consecutive-failure count.
    pub fn failures(&self) -> u32 {
        self.failures
    }

    /// Reset after a successful upload (§3.4.3: any success resets the shared
    /// backoff to attempt 1 for everyone).
    pub fn on_success(&mut self) {
        self.failures = 0;
        self.streak_started_ns = None;
    }

    /// Record a failure observed at `now_ns`. Returns the delay before the next
    /// attempt and whether the streak has run past `max_elapsed` (dead-letter).
    pub fn on_failure(&mut self, now_ns: u64) -> BackoffDecision {
        // Anchor the streak on the first failure, and *re-anchor* if the wall
        // clock has jumped backward (now < start). Without this, a backward NTP
        // step / suspend-resume would make `now - start` saturate to 0 forever,
        // so `elapsed >= max_elapsed` could never fire and a hopeless job would
        // retry indefinitely instead of dead-lettering. Re-anchoring restarts the
        // dead-letter timer from the corrected clock, which always moves forward.
        let started = match self.streak_started_ns {
            Some(s) if s <= now_ns => s,
            _ => {
                self.streak_started_ns = Some(now_ns);
                now_ns
            }
        };
        self.failures += 1;
        let elapsed_ns = (now_ns - started) as u128;
        let dead_letter = elapsed_ns >= self.schedule.max_elapsed.as_nanos();
        // The next attempt number is failures + 1 (after one failure, attempt 2).
        let delay = self.schedule.delay_for_attempt(self.failures + 1);
        BackoffDecision {
            delay,
            dead_letter,
            failures: self.failures,
        }
    }
}

// ---------------------------------------------------------------------------
// Upload job + per-chunk sequence (§3.4.4)
// ---------------------------------------------------------------------------

/// A single chunk waiting to be uploaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadJob {
    /// Recording this chunk belongs to.
    pub recording_name: String,
    /// Zero-based chunk index.
    pub chunk_index: u32,
    /// Local file to upload.
    pub local_path: PathBuf,
    /// Manifest SHA-256 (lowercase hex).
    pub sha256: String,
    /// Deterministic remote object key.
    pub object_key: String,
}

impl UploadJob {
    /// Build the job for `chunk` of recording `recording_name` on `machine_id`,
    /// whose files live under `recording_dir`.
    pub fn for_chunk(
        machine_id: &str,
        recording_dir: &Path,
        recording_name: &str,
        chunk: &Chunk,
    ) -> Self {
        Self {
            recording_name: recording_name.to_string(),
            chunk_index: chunk.index,
            local_path: super::chunks_path(recording_dir).join(&chunk.name),
            sha256: chunk.sha256.clone(),
            object_key: object_key_chunk(machine_id, recording_name, chunk.index, &chunk.sha256),
        }
    }
}

/// Result of [`upload_one`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadOutcome {
    /// The object was freshly PUT.
    Uploaded { etag: Option<String> },
    /// The object already existed with matching content (HEAD short-circuit).
    AlreadyPresent { etag: Option<String> },
    /// An object exists at our key but its content differs — a possible bucket
    /// collision. Sync dead-letters this for human review (§3.4.4 step 3).
    Conflict,
}

/// Upload one chunk following the §3.4.4 sequence:
///
/// 1. re-verify the local file's SHA-256 against the manifest digest (never
///    trusted — a mismatch is local corruption → terminal [`BackendError::BadDigest`]),
/// 2. HEAD the deterministic key: matching content ⇒ [`UploadOutcome::AlreadyPresent`];
///    differing content ⇒ [`UploadOutcome::Conflict`]; absent ⇒ proceed,
/// 3. PUT with the SHA-256 checksum so the backend validates server-side.
///
/// Returns a [`BackendError`] for the caller to [`classify`]; does **not** mutate
/// the manifest (see [`mark_chunk_uploaded`]).
pub async fn upload_one(
    backend: &dyn StorageBackend,
    job: &UploadJob,
) -> Result<UploadOutcome, BackendError> {
    let digest = integrity::from_hex(&job.sha256).map_err(|_| BackendError::BadDigest {
        key: job.object_key.clone(),
        expected: job.sha256.clone(),
        actual: "<unparseable manifest sha256>".to_string(),
    })?;

    // Step 2: re-hash the local file; mismatch = local corruption (terminal).
    let actual = integrity::sha256_file(&job.local_path).map_err(|e| BackendError::Io {
        key: job.object_key.clone(),
        detail: format!("hashing {}: {e}", job.local_path.display()),
    })?;
    if actual != digest {
        return Err(BackendError::BadDigest {
            key: job.object_key.clone(),
            expected: job.sha256.clone(),
            actual: integrity::to_hex(&actual),
        });
    }

    // Step 3: HEAD-before-PUT idempotency.
    if let Some(meta) = backend.head(&job.object_key).await? {
        match meta.sha256 {
            Some(remote) if remote == digest => {
                return Ok(UploadOutcome::AlreadyPresent { etag: meta.etag })
            }
            Some(_) => return Ok(UploadOutcome::Conflict),
            // Backend can't supply a hash; the key already encodes the content
            // address, so a hit means the same content.
            None => return Ok(UploadOutcome::AlreadyPresent { etag: meta.etag }),
        }
    }

    // Step 4: PUT with the checksum header.
    let bytes = tokio::fs::read(&job.local_path)
        .await
        .map_err(|e| BackendError::Io {
            key: job.object_key.clone(),
            detail: format!("reading {}: {e}", job.local_path.display()),
        })?;
    let result = backend.put(&job.object_key, &bytes, Some(&digest)).await?;
    Ok(UploadOutcome::Uploaded { etag: result.etag })
}

// ---------------------------------------------------------------------------
// Startup scan + manifest mutation (§3.4.1 trigger 3, §3.4.5)
// ---------------------------------------------------------------------------

/// Build upload jobs for every chunk in `recording` that has no `uploaded_at_ns`
/// — the startup-scan enqueue (§3.4.1 trigger 3). `recording_dir` is where the
/// recording's files live; the machine id and name come from the manifest.
pub fn pending_jobs(recording_dir: &Path, recording: &Recording) -> Vec<UploadJob> {
    recording
        .chunks
        .iter()
        .filter(|c| !c.is_uploaded())
        .map(|c| UploadJob::for_chunk(&recording.machine_id, recording_dir, &recording.name, c))
        .collect()
}

/// Record that chunk `chunk_index` of the recording at `recording_dir` was
/// uploaded at `now_ns` (optionally stamping the remote ETag), persisting the
/// manifest atomically (§3.4.5). Returns `false` if the chunk index is unknown.
///
/// The daemon serializes this behind the per-recording `ManifestHandle` mutex so
/// the recorder's append path and sync's mark path don't race; this function
/// performs the atomic file mutation that sits under that lock.
pub fn mark_chunk_uploaded(
    recording_dir: &Path,
    chunk_index: u32,
    now_ns: u64,
    etag: Option<String>,
) -> Result<bool, SyncError> {
    let mut recording = manifest::load_dir(recording_dir)?;
    let Some(chunk) = recording.chunks.iter_mut().find(|c| c.index == chunk_index) else {
        return Ok(false);
    };
    chunk.uploaded_at_ns = Some(now_ns);
    if etag.is_some() {
        chunk.remote_etag = etag;
    }
    manifest::save_dir(recording_dir, &recording)?;
    Ok(true)
}

// ---------------------------------------------------------------------------
// Dead-letter list (§3.4.6)
// ---------------------------------------------------------------------------

/// One dead-lettered chunk: an upload that exhausted retries or hit a terminal
/// error. Persisted so it survives daemon restarts; `reconcile` flushes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeadLetterEntry {
    /// Recording the chunk belongs to.
    pub recording_name: String,
    /// Zero-based chunk index.
    pub chunk_index: u32,
    /// Deterministic remote object key the upload targeted.
    pub object_key: String,
    /// Manifest SHA-256 (lowercase hex).
    pub sha256: String,
    /// Human-readable failure reason (classification + backend detail).
    pub reason: String,
    /// When the chunk was dead-lettered (nanoseconds since epoch).
    pub failed_at_ns: u64,
    /// How many attempts had been made.
    pub attempts: u32,
}

/// The persisted dead-letter list (`~/.bubbaloop/storage/dead_letter.json`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DeadLetterList {
    entries: Vec<DeadLetterEntry>,
}

impl DeadLetterList {
    /// Default path: `~/.bubbaloop/storage/dead_letter.json`.
    pub fn default_path() -> Result<PathBuf, SyncError> {
        Ok(super::storage_state_dir()?.join("dead_letter.json"))
    }

    /// Load from `path`. A missing file is an empty list (not an error).
    pub fn load(path: impl AsRef<Path>) -> Result<Self, SyncError> {
        let path = path.as_ref();
        match std::fs::read_to_string(path) {
            Ok(s) => serde_json::from_str(&s).map_err(|e| SyncError::Parse(e.to_string())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(SyncError::Io {
                path: path.display().to_string(),
                detail: e.to_string(),
            }),
        }
    }

    /// Atomically persist to `path` (write-tmp-then-rename, §3.4.6).
    pub fn save_atomic(&self, path: impl AsRef<Path>) -> Result<(), SyncError> {
        let path = path.as_ref();
        let bytes =
            serde_json::to_vec_pretty(self).map_err(|e| SyncError::Serialize(e.to_string()))?;
        let parent = path.parent().ok_or_else(|| SyncError::Io {
            path: path.display().to_string(),
            detail: "dead-letter path has no parent".to_string(),
        })?;
        std::fs::create_dir_all(parent).map_err(|e| SyncError::Io {
            path: parent.display().to_string(),
            detail: e.to_string(),
        })?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &bytes).map_err(|e| SyncError::Io {
            path: tmp.display().to_string(),
            detail: e.to_string(),
        })?;
        std::fs::rename(&tmp, path).map_err(|e| SyncError::Io {
            path: path.display().to_string(),
            detail: e.to_string(),
        })?;
        Ok(())
    }

    /// Current entries.
    pub fn entries(&self) -> &[DeadLetterEntry] {
        &self.entries
    }

    /// Number of dead-lettered chunks.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the list is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Add (or replace) an entry, keyed by `(recording_name, chunk_index)` so a
    /// chunk is never dead-lettered twice.
    pub fn add(&mut self, entry: DeadLetterEntry) {
        if let Some(existing) = self.entries.iter_mut().find(|e| {
            e.recording_name == entry.recording_name && e.chunk_index == entry.chunk_index
        }) {
            *existing = entry;
        } else {
            self.entries.push(entry);
        }
    }

    /// Remove the entry for `(recording_name, chunk_index)`, returning whether one
    /// was present (e.g. after `reconcile` heals it).
    pub fn remove(&mut self, recording_name: &str, chunk_index: u32) -> bool {
        let before = self.entries.len();
        self.entries
            .retain(|e| !(e.recording_name == recording_name && e.chunk_index == chunk_index));
        self.entries.len() != before
    }

    /// All dead-letter entries for one recording.
    pub fn for_recording<'a>(&'a self, recording_name: &str) -> Vec<&'a DeadLetterEntry> {
        self.entries
            .iter()
            .filter(|e| e.recording_name == recording_name)
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors from the sync core (manifest mutation, dead-letter persistence).
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    /// Filesystem error.
    #[error("sync io error at {path}: {detail}")]
    Io { path: String, detail: String },
    /// JSON failed to parse.
    #[error("dead-letter parse error: {0}")]
    Parse(String),
    /// JSON failed to serialize.
    #[error("dead-letter serialize error: {0}")]
    Serialize(String),
    /// Manifest load/save failed.
    #[error("manifest error: {0}")]
    Manifest(#[from] manifest::ManifestError),
    /// Storage path could not be resolved.
    #[error("storage path error: {0}")]
    Path(#[from] StoragePathError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::backend::local::LocalFs;
    use crate::storage::object_key_manifest;
    use crate::storage::recording::Recording;

    const NOW: u64 = 1_700_000_000_000_000_000;
    const SEC: u64 = 1_000_000_000;

    // ---- error classification ----------------------------------------------

    #[test]
    fn error_classification_matches_spec() {
        assert_eq!(
            classify(&BackendError::NoSuchBucket { bucket: "b".into() }),
            ErrorClass::PauseSync
        );
        assert_eq!(
            classify(&BackendError::Forbidden { key: "k".into() }),
            ErrorClass::Terminal
        );
        assert_eq!(
            classify(&BackendError::BadDigest {
                key: "k".into(),
                expected: "a".into(),
                actual: "b".into()
            }),
            ErrorClass::Terminal
        );
        assert_eq!(
            classify(&BackendError::Io {
                key: "k".into(),
                detail: "timeout".into()
            }),
            ErrorClass::Retryable
        );
    }

    // ---- backoff schedule ---------------------------------------------------

    #[test]
    fn backoff_schedule_follows_spec_curve() {
        let s = BackoffSchedule::default();
        assert_eq!(s.delay_for_attempt(1), Duration::ZERO);
        assert_eq!(s.delay_for_attempt(2), Duration::from_secs(5));
        assert_eq!(s.delay_for_attempt(3), Duration::from_secs(10));
        assert_eq!(s.delay_for_attempt(4), Duration::from_secs(20));
        assert_eq!(s.delay_for_attempt(5), Duration::from_secs(40));
        assert_eq!(s.delay_for_attempt(6), Duration::from_secs(60)); // clamp (80→60)
        assert_eq!(s.delay_for_attempt(7), Duration::from_secs(60));
        assert_eq!(s.delay_for_attempt(100), Duration::from_secs(60));
    }

    #[test]
    fn shared_backoff_streak_and_reset() {
        let mut b = SharedBackoff::new(BackoffSchedule::default());
        assert_eq!(b.failures(), 0);

        let d1 = b.on_failure(NOW);
        assert_eq!(d1.failures, 1);
        assert_eq!(d1.delay, Duration::from_secs(5)); // next attempt = 2
        assert!(!d1.dead_letter);

        let d2 = b.on_failure(NOW + SEC);
        assert_eq!(d2.failures, 2);
        assert_eq!(d2.delay, Duration::from_secs(10));
        assert!(!d2.dead_letter);

        // A success resets the shared streak for everyone.
        b.on_success();
        assert_eq!(b.failures(), 0);
        let d3 = b.on_failure(NOW + 2 * SEC);
        assert_eq!(d3.failures, 1);
        assert_eq!(d3.delay, Duration::from_secs(5));
    }

    #[test]
    fn dead_letter_after_max_elapsed() {
        let mut b = SharedBackoff::new(BackoffSchedule::default()); // max_elapsed 5m
        let first = b.on_failure(NOW);
        assert!(!first.dead_letter);
        // 5 minutes later, still failing → dead-letter.
        let later = b.on_failure(NOW + 300 * SEC);
        assert!(later.dead_letter);
    }

    #[test]
    fn backward_clock_jump_still_dead_letters() {
        let mut b = SharedBackoff::new(BackoffSchedule::default());
        b.on_failure(NOW);
        // Clock steps backward: must not underflow, must not be "dead" yet, and
        // must re-anchor so the timer makes forward progress again.
        let back = b.on_failure(NOW - 100 * SEC);
        assert!(!back.dead_letter);
        // From the re-anchored start, 5 more minutes of forward time dead-letters.
        let forward = b.on_failure(NOW - 100 * SEC + 300 * SEC);
        assert!(forward.dead_letter);
    }

    // ---- upload sequence ----------------------------------------------------

    fn write_chunk(rec_dir: &Path, rec: &mut Recording, i: u32, bytes: &[u8]) {
        let digest = integrity::sha256(bytes);
        let sha = integrity::to_hex(&digest);
        let name = Chunk::canonical_name(i, &sha);
        let chunks = super::super::chunks_path(rec_dir);
        std::fs::create_dir_all(&chunks).unwrap();
        std::fs::write(chunks.join(&name), bytes).unwrap();
        rec.chunks.push(Chunk {
            name,
            index: i,
            size_bytes: bytes.len() as u64,
            sha256: sha,
            log_time_first_ns: None,
            log_time_last_ns: None,
            uploaded_at_ns: None,
            remote_etag: None,
            extra: Default::default(),
        });
    }

    fn remote() -> (tempfile::TempDir, LocalFs) {
        let dir = tempfile::tempdir().unwrap();
        let fs = LocalFs::new(dir.path());
        (dir, fs)
    }

    #[tokio::test]
    async fn upload_one_puts_then_short_circuits() {
        let dir = tempfile::tempdir().unwrap();
        let rec_dir = dir.path().join("rec_a");
        let mut rec = Recording::new("rec_a", "jetson_alpha", 1_000);
        write_chunk(&rec_dir, &mut rec, 0, b"chunk zero");
        let job = UploadJob::for_chunk("jetson_alpha", &rec_dir, "rec_a", &rec.chunks[0]);
        let (_rd, fs) = remote();

        match upload_one(&fs, &job).await.unwrap() {
            UploadOutcome::Uploaded { etag } => assert!(etag.is_some()),
            other => panic!("expected Uploaded, got {other:?}"),
        }
        // Second time: HEAD short-circuits to AlreadyPresent.
        assert!(matches!(
            upload_one(&fs, &job).await.unwrap(),
            UploadOutcome::AlreadyPresent { .. }
        ));
    }

    #[tokio::test]
    async fn upload_one_detects_local_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let rec_dir = dir.path().join("rec_a");
        let mut rec = Recording::new("rec_a", "m", 1);
        write_chunk(&rec_dir, &mut rec, 0, b"good bytes");
        let job = UploadJob::for_chunk("m", &rec_dir, "rec_a", &rec.chunks[0]);
        // Corrupt the local file after the manifest digest was computed.
        std::fs::write(&job.local_path, b"tampered").unwrap();
        let (_rd, fs) = remote();

        let err = upload_one(&fs, &job).await.unwrap_err();
        assert!(matches!(err, BackendError::BadDigest { .. }));
        assert_eq!(classify(&err), ErrorClass::Terminal);
    }

    #[tokio::test]
    async fn upload_one_reports_conflict_on_remote_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let rec_dir = dir.path().join("rec_a");
        let mut rec = Recording::new("rec_a", "m", 1);
        write_chunk(&rec_dir, &mut rec, 0, b"local content");
        let job = UploadJob::for_chunk("m", &rec_dir, "rec_a", &rec.chunks[0]);
        let (_rd, fs) = remote();
        // Different content already sitting at our deterministic key.
        fs.put(&job.object_key, b"someone elses content", None)
            .await
            .unwrap();

        assert_eq!(
            upload_one(&fs, &job).await.unwrap(),
            UploadOutcome::Conflict
        );
    }

    // ---- startup scan + manifest mutation -----------------------------------

    #[test]
    fn pending_jobs_skips_uploaded_chunks() {
        let dir = tempfile::tempdir().unwrap();
        let rec_dir = dir.path().join("rec_a");
        let mut rec = Recording::new("rec_a", "m", 1);
        write_chunk(&rec_dir, &mut rec, 0, b"a");
        write_chunk(&rec_dir, &mut rec, 1, b"bb");
        write_chunk(&rec_dir, &mut rec, 2, b"ccc");
        rec.chunks[1].uploaded_at_ns = Some(NOW); // already done

        let jobs = pending_jobs(&rec_dir, &rec);
        let idxs: Vec<u32> = jobs.iter().map(|j| j.chunk_index).collect();
        assert_eq!(idxs, vec![0, 2]);
        assert_eq!(jobs[0].recording_name, "rec_a");
        assert!(jobs[0].object_key.starts_with("m/rec_a/chunks/"));
    }

    #[test]
    fn mark_chunk_uploaded_persists_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let rec_dir = dir.path().join("rec_a");
        let mut rec = Recording::new("rec_a", "m", 1);
        write_chunk(&rec_dir, &mut rec, 0, b"a");
        manifest::save_dir(&rec_dir, &rec).unwrap();

        let marked = mark_chunk_uploaded(&rec_dir, 0, NOW, Some("\"etag123\"".into())).unwrap();
        assert!(marked);
        let reloaded = manifest::load_dir(&rec_dir).unwrap();
        assert_eq!(reloaded.chunks[0].uploaded_at_ns, Some(NOW));
        assert_eq!(
            reloaded.chunks[0].remote_etag.as_deref(),
            Some("\"etag123\"")
        );

        // Unknown index → false, no change.
        assert!(!mark_chunk_uploaded(&rec_dir, 99, NOW, None).unwrap());
    }

    // ---- dead-letter list ---------------------------------------------------

    fn entry(rec: &str, idx: u32) -> DeadLetterEntry {
        DeadLetterEntry {
            recording_name: rec.into(),
            chunk_index: idx,
            object_key: format!("m/{rec}/chunks/chunk-{idx:06}-deadbeef.mcap"),
            sha256: "de".repeat(32),
            reason: "Forbidden".into(),
            failed_at_ns: NOW,
            attempts: 7,
        }
    }

    #[test]
    fn dead_letter_dedup_add_and_remove() {
        let mut dl = DeadLetterList::default();
        assert!(dl.is_empty());
        dl.add(entry("rec_a", 0));
        dl.add(entry("rec_a", 1));
        dl.add(entry("rec_a", 0)); // replace, not duplicate
        assert_eq!(dl.len(), 2);

        assert_eq!(dl.for_recording("rec_a").len(), 2);
        assert!(dl.remove("rec_a", 0));
        assert!(!dl.remove("rec_a", 0)); // already gone
        assert_eq!(dl.len(), 1);
    }

    #[test]
    fn dead_letter_roundtrips_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("storage").join("dead_letter.json");

        // Missing file loads as empty.
        assert!(DeadLetterList::load(&path).unwrap().is_empty());

        let mut dl = DeadLetterList::default();
        dl.add(entry("rec_a", 0));
        dl.add(entry("rec_b", 3));
        dl.save_atomic(&path).unwrap();

        let reloaded = DeadLetterList::load(&path).unwrap();
        assert_eq!(reloaded, dl);
        assert_eq!(reloaded.for_recording("rec_b")[0].chunk_index, 3);
    }

    // Touch object_key_manifest import in an assertion so the helper stays wired
    // for the eventual manifest-finalize trigger.
    #[test]
    fn manifest_key_shape() {
        assert_eq!(object_key_manifest("m", "rec_a"), "m/rec_a/manifest.json");
    }
}
