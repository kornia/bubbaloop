//! The async sync **queue driver** (spec §3.4.2) — the bounded queue, the
//! concurrency cap, and the shared-backoff orchestration that sit on top of the
//! [`sync`](super::sync) core.
//!
//! The driver is the third trigger source in §3.4.1 made real: it consumes
//! [`ChunkFinalized`] events over an in-process MPSC channel (emitted by the
//! recorder when it renames `.mcap.active` → `.mcap`), plus an optional
//! startup-scan seed of anything left un-uploaded ([`scan_pending`]). For each
//! chunk it runs the §3.4.4 upload sequence via [`upload_one`](super::sync::upload_one),
//! then re-uploads the recording's `manifest.json` (coalesced per batch, §3.4.4
//! step 5) so remote state stays crash-consistent.
//!
//! Policy lives in the synchronous, clock-injected [`SyncQueue`] state machine so
//! it unit-tests without async/timing flakiness; [`run_driver`] is the thin async
//! shell that does the IO and the waiting. Shared backoff (§3.4.3) pauses the
//! whole queue as a unit on failure and resets on the first success;
//! `NoSuchBucket` pauses sync entirely until the daemon is reconfigured.

use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::backend::StorageBackend;
use super::sync::{
    classify, mark_chunk_uploaded, upload_one, BackoffSchedule, DeadLetterEntry, DeadLetterList,
    ErrorClass, SharedBackoff, UploadJob, UploadOutcome,
};
use super::{manifest, object_key_chunk, object_key_manifest, recordings_dir};

/// A finalized chunk ready to upload (spec §3.4.1 trigger 1). Carries everything
/// the driver needs to build the upload job, mark the manifest, and re-upload it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkFinalized {
    /// Recording the chunk belongs to.
    pub recording_name: String,
    /// Recording directory (root of `manifest.json` + `chunks/`).
    pub recording_dir: PathBuf,
    /// Machine that produced it (object-key prefix).
    pub machine_id: String,
    /// Zero-based chunk index.
    pub chunk_index: u32,
    /// Manifest SHA-256 (lowercase hex).
    pub sha256: String,
    /// Local chunk file to upload.
    pub local_path: PathBuf,
}

impl ChunkFinalized {
    /// The dedup key — a chunk is never queued twice.
    fn key(&self) -> (String, u32) {
        (self.recording_name.clone(), self.chunk_index)
    }

    /// Build the per-chunk [`UploadJob`] (deterministic object key, §8).
    fn to_job(&self) -> UploadJob {
        UploadJob {
            recording_name: self.recording_name.clone(),
            chunk_index: self.chunk_index,
            local_path: self.local_path.clone(),
            sha256: self.sha256.clone(),
            object_key: object_key_chunk(
                &self.machine_id,
                &self.recording_name,
                self.chunk_index,
                &self.sha256,
            ),
        }
    }
}

/// Tuning knobs for the driver (spec §3.4.2 defaults).
#[derive(Debug, Clone)]
pub struct SyncConfig {
    /// Max concurrent uploads (default 3).
    pub max_concurrent: usize,
    /// Bounded queue capacity (default 1000); enqueues past this are rejected.
    pub queue_cap: usize,
    /// Shared backoff schedule.
    pub backoff: BackoffSchedule,
    /// Where the persisted dead-letter list lives.
    pub dead_letter_path: PathBuf,
}

impl SyncConfig {
    /// Defaults with the dead-letter list at `dead_letter_path`.
    pub fn with_dead_letter_path(dead_letter_path: PathBuf) -> Self {
        Self {
            max_concurrent: 3,
            queue_cap: 1000,
            backoff: BackoffSchedule::default(),
            dead_letter_path,
        }
    }
}

/// Result of trying to enqueue a chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnqueueResult {
    /// Newly queued.
    Accepted,
    /// Already queued (same recording + chunk index) — ignored.
    Duplicate,
    /// The queue is at `queue_cap` — back-pressure (§3.4.2).
    Full,
}

/// The synchronous policy core: a bounded FIFO of pending chunks plus the shared
/// backoff gate, pause flag, and dead-letter list. No IO except dead-letter
/// persistence; no clock except the `now_ns` callers pass in, so every transition
/// is unit-testable.
pub struct SyncQueue {
    pending: VecDeque<ChunkFinalized>,
    queued: HashSet<(String, u32)>,
    cap: usize,
    backoff: SharedBackoff,
    /// Earliest time (ns) a retry may run — the shared backoff gate.
    gate_ns: u64,
    /// Set on `NoSuchBucket`: sync stops uploading until reconfigured (§3.4.3).
    paused: bool,
    dead_letters: DeadLetterList,
    dead_letter_path: PathBuf,
}

impl SyncQueue {
    /// Build a queue, loading any persisted dead-letter list (so terminal
    /// failures survive a daemon restart, §3.4.6).
    pub fn new(cap: usize, backoff: BackoffSchedule, dead_letter_path: PathBuf) -> Self {
        let dead_letters = DeadLetterList::load(&dead_letter_path).unwrap_or_default();
        Self {
            pending: VecDeque::new(),
            queued: HashSet::new(),
            cap,
            backoff: SharedBackoff::new(backoff),
            gate_ns: 0,
            paused: false,
            dead_letters,
            dead_letter_path,
        }
    }

    /// Enqueue a chunk unless it is a duplicate or the queue is full.
    pub fn enqueue(&mut self, event: ChunkFinalized) -> EnqueueResult {
        let key = event.key();
        if self.queued.contains(&key) {
            return EnqueueResult::Duplicate;
        }
        if self.pending.len() >= self.cap {
            return EnqueueResult::Full;
        }
        self.queued.insert(key);
        self.pending.push_back(event);
        EnqueueResult::Accepted
    }

    /// Whether sync is paused (misconfiguration, §3.4.3).
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Number of chunks waiting (queued, not in flight).
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// The shared backoff gate (earliest retry time, ns).
    pub fn gate_ns(&self) -> u64 {
        self.gate_ns
    }

    /// Whether at least one job can run right now (`now_ns` past the gate, not
    /// paused, something pending).
    pub fn ready(&self, now_ns: u64) -> bool {
        !self.paused && now_ns >= self.gate_ns && !self.pending.is_empty()
    }

    /// Pop up to `n` jobs that are ready to run now (FIFO).
    pub fn take_ready(&mut self, now_ns: u64, n: usize) -> Vec<ChunkFinalized> {
        if !self.ready(now_ns) {
            return Vec::new();
        }
        let mut out = Vec::new();
        while out.len() < n {
            match self.pending.pop_front() {
                Some(ev) => {
                    self.queued.remove(&ev.key());
                    out.push(ev);
                }
                None => break,
            }
        }
        out
    }

    /// A chunk uploaded successfully — reset the shared backoff for everyone and
    /// drop the gate (§3.4.3: any success resets to attempt 1).
    pub fn on_success(&mut self) {
        self.backoff.on_success();
        self.gate_ns = 0;
    }

    /// A retryable failure: re-queue the chunk at the front and advance the shared
    /// backoff. If the failure streak ran past `max_elapsed`, dead-letter it
    /// instead of retrying forever (§3.4.3). Returns `true` if it was retried.
    pub fn on_retryable_failure(
        &mut self,
        event: ChunkFinalized,
        reason: String,
        now_ns: u64,
    ) -> Result<bool, super::sync::SyncError> {
        let decision = self.backoff.on_failure(now_ns);
        if decision.dead_letter {
            self.dead_letter(event, reason, decision.failures, now_ns)?;
            return Ok(false);
        }
        self.gate_ns =
            now_ns.saturating_add(decision.delay.as_nanos().min(u64::MAX as u128) as u64);
        // Re-queue at the front so the failed chunk is retried before newer ones.
        self.queued.insert(event.key());
        self.pending.push_front(event);
        Ok(true)
    }

    /// A terminal failure (bad auth, local corruption, conflict): dead-letter
    /// immediately, never retry (§3.4.3 / §3.4.4 step 3).
    pub fn on_terminal_failure(
        &mut self,
        event: ChunkFinalized,
        reason: String,
        now_ns: u64,
    ) -> Result<(), super::sync::SyncError> {
        self.dead_letter(event, reason, self.backoff.failures(), now_ns)
    }

    /// A misconfiguration affecting every job (`NoSuchBucket`): pause sync
    /// entirely and re-queue the chunk for when sync resumes (§3.4.3).
    pub fn on_pause(&mut self, event: ChunkFinalized) {
        self.paused = true;
        self.queued.insert(event.key());
        self.pending.push_front(event);
    }

    fn dead_letter(
        &mut self,
        event: ChunkFinalized,
        reason: String,
        attempts: u32,
        now_ns: u64,
    ) -> Result<(), super::sync::SyncError> {
        self.dead_letters.add(DeadLetterEntry {
            recording_name: event.recording_name.clone(),
            chunk_index: event.chunk_index,
            object_key: event.to_job().object_key,
            sha256: event.sha256.clone(),
            reason,
            failed_at_ns: now_ns,
            attempts,
        });
        self.dead_letters.save_atomic(&self.dead_letter_path)
    }

    /// The persisted dead-letter list.
    pub fn dead_letters(&self) -> &DeadLetterList {
        &self.dead_letters
    }
}

/// Walk `~/.bubbaloop/recordings/*/manifest.json` and build a [`ChunkFinalized`]
/// for every chunk with no `uploaded_at_ns` — the startup scan (§3.4.1 trigger 3)
/// that recovers anything the daemon missed while down.
pub fn scan_pending() -> Vec<ChunkFinalized> {
    match recordings_dir() {
        Ok(root) => scan_pending_in(&root),
        Err(_) => Vec::new(),
    }
}

/// [`scan_pending`] against an explicit recordings root (testable).
pub fn scan_pending_in(recordings_root: &Path) -> Vec<ChunkFinalized> {
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(recordings_root) {
        Ok(e) => e,
        Err(_) => return out,
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let Ok(recording) = manifest::load_dir(&dir) else {
            continue;
        };
        let chunks_dir = dir.join("chunks");
        for chunk in recording.chunks.iter().filter(|c| !c.is_uploaded()) {
            out.push(ChunkFinalized {
                recording_name: recording.name.clone(),
                recording_dir: dir.clone(),
                machine_id: recording.machine_id.clone(),
                chunk_index: chunk.index,
                sha256: chunk.sha256.clone(),
                local_path: chunks_dir.join(&chunk.name),
            });
        }
    }
    out
}

/// Current wall clock in nanoseconds since the Unix epoch.
fn now_ns() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos().min(u64::MAX as u128) as u64)
        .unwrap_or(0)
}

/// Re-upload one recording's `manifest.json` to the backend (§3.4.4 step 5). The
/// manifest is small and mutable, so it goes without a checksum header.
async fn upload_manifest(
    backend: &dyn StorageBackend,
    recording_dir: &Path,
    machine_id: &str,
    recording_name: &str,
) -> Result<(), super::backend::BackendError> {
    let path = manifest::manifest_path(recording_dir);
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|e| super::backend::BackendError::Io {
            key: path.display().to_string(),
            detail: e.to_string(),
        })?;
    let key = object_key_manifest(machine_id, recording_name);
    backend.put(&key, &bytes, None).await.map(|_| ())
}

/// A handle the recorder uses to notify the driver of finalized chunks.
#[derive(Debug, Clone)]
pub struct SyncHandle {
    tx: mpsc::Sender<ChunkFinalized>,
}

impl SyncHandle {
    /// Notify the driver that a chunk was finalized. Returns `false` if the driver
    /// has shut down (receiver dropped).
    pub async fn notify_chunk(&self, event: ChunkFinalized) -> bool {
        self.tx.send(event).await.is_ok()
    }
}

/// Spawn the driver on the Tokio runtime, seeding it with a startup scan, and
/// return a [`SyncHandle`] plus the task's `JoinHandle`. The task runs until
/// `cancel` fires or the handle (and all clones) are dropped and the queue drains.
pub fn spawn(
    backend: Arc<dyn StorageBackend>,
    cfg: SyncConfig,
    initial: Vec<ChunkFinalized>,
    cancel: CancellationToken,
) -> (SyncHandle, tokio::task::JoinHandle<()>) {
    let (tx, rx) = mpsc::channel(cfg.queue_cap.max(1));
    let handle = SyncHandle { tx };
    let task = tokio::spawn(run_driver(backend, cfg, initial, rx, cancel));
    (handle, task)
}

/// The async driver loop. Seeds the queue with `initial`, then services
/// [`ChunkFinalized`] events from `rx` until the channel closes and the queue
/// drains, or `cancel` fires. Uploads run in batches of up to
/// `cfg.max_concurrent`; the shared backoff gates the whole queue between
/// failures; successful uploads mark the manifest and coalesce one manifest
/// re-upload per recording per batch.
pub async fn run_driver(
    backend: Arc<dyn StorageBackend>,
    cfg: SyncConfig,
    initial: Vec<ChunkFinalized>,
    mut rx: mpsc::Receiver<ChunkFinalized>,
    cancel: CancellationToken,
) {
    let mut queue = SyncQueue::new(cfg.queue_cap, cfg.backoff, cfg.dead_letter_path.clone());
    for ev in initial {
        queue.enqueue(ev);
    }

    let mut channel_open = true;
    loop {
        if cancel.is_cancelled() {
            return;
        }
        // Absorb everything immediately available without blocking.
        loop {
            match rx.try_recv() {
                Ok(ev) => {
                    queue.enqueue(ev);
                }
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    channel_open = false;
                    break;
                }
            }
        }

        // Paused (misconfig): stop uploading, just keep draining the channel so we
        // notice shutdown. Stays paused until the process is reconfigured/restarted.
        if queue.is_paused() {
            if !channel_open {
                return;
            }
            tokio::select! {
                _ = cancel.cancelled() => return,
                ev = rx.recv() => match ev {
                    Some(e) => { queue.enqueue(e); }
                    None => channel_open = false,
                },
            }
            continue;
        }

        let now = now_ns();
        if queue.ready(now) {
            run_batch(backend.as_ref(), &mut queue, cfg.max_concurrent, now).await;
            continue;
        }

        if queue.pending_len() > 0 {
            // Something is queued but gated by backoff — wait out the gate (or a
            // new event / cancel).
            let wait = Duration::from_nanos(queue.gate_ns().saturating_sub(now));
            if channel_open {
                tokio::select! {
                    _ = cancel.cancelled() => return,
                    _ = tokio::time::sleep(wait) => {}
                    ev = rx.recv() => match ev {
                        Some(e) => { queue.enqueue(e); }
                        None => channel_open = false,
                    },
                }
            } else {
                tokio::select! {
                    _ = cancel.cancelled() => return,
                    _ = tokio::time::sleep(wait) => {}
                }
            }
            continue;
        }

        // Queue empty: block for the next event, or exit when the channel is closed.
        if !channel_open {
            return;
        }
        tokio::select! {
            _ = cancel.cancelled() => return,
            ev = rx.recv() => match ev {
                Some(e) => { queue.enqueue(e); }
                None => channel_open = false,
            },
        }
    }
}

/// Run up to `max_concurrent` ready uploads, then apply their outcomes and
/// coalesce one manifest re-upload per touched recording.
async fn run_batch(
    backend: &dyn StorageBackend,
    queue: &mut SyncQueue,
    max_concurrent: usize,
    now: u64,
) {
    let batch = queue.take_ready(now, max_concurrent);
    if batch.is_empty() {
        return;
    }

    // Upload the batch concurrently (each future owns its event).
    let results = futures::future::join_all(batch.into_iter().map(|ev| async move {
        let result = upload_one(backend, &ev.to_job()).await;
        (ev, result)
    }))
    .await;

    // Recordings that had a successful chunk → one coalesced manifest re-upload.
    let mut to_refresh_manifest: Vec<(PathBuf, String, String)> = Vec::new();

    for (event, result) in results {
        match result {
            Ok(outcome) => {
                let etag = match &outcome {
                    UploadOutcome::Uploaded { etag } | UploadOutcome::AlreadyPresent { etag } => {
                        etag.clone()
                    }
                    UploadOutcome::Conflict => None,
                };
                if matches!(outcome, UploadOutcome::Conflict) {
                    // Same key, different bytes — possible bucket collision (§3.4.4).
                    log::warn!(
                        "sync: chunk {}#{} conflicts with existing remote object; dead-lettering",
                        event.recording_name,
                        event.chunk_index
                    );
                    let _ = queue.on_terminal_failure(
                        event,
                        "remote object exists with different content".to_string(),
                        now,
                    );
                    continue;
                }
                // Mark the chunk uploaded in the manifest (§3.4.5).
                match mark_chunk_uploaded(&event.recording_dir, event.chunk_index, now, etag) {
                    Ok(_) => {
                        queue.on_success();
                        if !to_refresh_manifest
                            .iter()
                            .any(|(_, _, n)| n == &event.recording_name)
                        {
                            to_refresh_manifest.push((
                                event.recording_dir.clone(),
                                event.machine_id.clone(),
                                event.recording_name.clone(),
                            ));
                        }
                    }
                    Err(e) => log::error!(
                        "sync: uploaded {}#{} but failed to mark manifest: {e}",
                        event.recording_name,
                        event.chunk_index
                    ),
                }
            }
            Err(err) => match classify(&err) {
                ErrorClass::PauseSync => {
                    log::error!(
                        "sync: backend misconfigured ({err}); pausing sync until reconfigured"
                    );
                    queue.on_pause(event);
                }
                ErrorClass::Terminal => {
                    log::error!(
                        "sync: terminal error on {}#{}: {err}; dead-lettering",
                        event.recording_name,
                        event.chunk_index
                    );
                    let _ = queue.on_terminal_failure(event, err.to_string(), now);
                }
                ErrorClass::Retryable => {
                    log::warn!(
                        "sync: retryable error on {}#{}: {err}",
                        event.recording_name,
                        event.chunk_index
                    );
                    let _ = queue.on_retryable_failure(event, err.to_string(), now);
                }
            },
        }
    }

    // Coalesced manifest re-uploads (best-effort; a failure just means the next
    // chunk's success will refresh it).
    for (dir, machine_id, name) in to_refresh_manifest {
        if let Err(e) = upload_manifest(backend, &dir, &machine_id, &name).await {
            log::warn!("sync: manifest re-upload for {name} failed: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::backend::local::LocalFs;
    use crate::storage::recording::{RecordingMode, Selection};
    use crate::storage::ring_buffer::{seal_samples, McapWriteConfig, Sample};
    use crate::storage::{manifest, object_key_chunk};

    const SEC: u64 = 1_000_000_000;

    fn sample(topic: &str, ns: u64) -> Sample {
        Sample {
            topic: topic.to_string(),
            message_encoding: "raw".into(),
            zenoh_encoding: "zenoh/bytes".into(),
            schema_name: None,
            publish_time_ns: ns,
            log_time_ns: ns,
            data: vec![1, 2, 3, 4],
        }
    }

    /// Seal a real 2-chunk recording under `root/{name}` and return its dir.
    fn make_recording(root: &Path, name: &str, machine: &str) -> (PathBuf, Vec<ChunkFinalized>) {
        let dir = root.join(name);
        let cfg = McapWriteConfig {
            chunk_size_bytes: 1, // force one chunk per sample
            ..McapWriteConfig::default()
        };
        let samples = vec![sample("t/a", SEC), sample("t/b", 2 * SEC)];
        let recording = seal_samples(
            &dir,
            name,
            machine,
            RecordingMode::Streaming,
            None,
            Selection::default(),
            &samples,
            &cfg,
            0,
        )
        .unwrap();
        let events = recording
            .chunks
            .iter()
            .map(|c| ChunkFinalized {
                recording_name: name.to_string(),
                recording_dir: dir.clone(),
                machine_id: machine.to_string(),
                chunk_index: c.index,
                sha256: c.sha256.clone(),
                local_path: dir.join("chunks").join(&c.name),
            })
            .collect();
        (dir, events)
    }

    #[test]
    fn queue_dedups_and_caps() {
        let dl = std::env::temp_dir().join("dl_test_unused.json");
        let mut q = SyncQueue::new(2, BackoffSchedule::default(), dl);
        let ev = ChunkFinalized {
            recording_name: "r".into(),
            recording_dir: PathBuf::from("/x"),
            machine_id: "m".into(),
            chunk_index: 0,
            sha256: "ab".repeat(32),
            local_path: PathBuf::from("/x/c0"),
        };
        assert_eq!(q.enqueue(ev.clone()), EnqueueResult::Accepted);
        assert_eq!(q.enqueue(ev.clone()), EnqueueResult::Duplicate);
        let ev1 = ChunkFinalized {
            chunk_index: 1,
            ..ev.clone()
        };
        let ev2 = ChunkFinalized {
            chunk_index: 2,
            ..ev.clone()
        };
        assert_eq!(q.enqueue(ev1), EnqueueResult::Accepted);
        assert_eq!(q.enqueue(ev2), EnqueueResult::Full); // cap is 2
    }

    #[test]
    fn retryable_failure_gates_then_dead_letters_past_max_elapsed() {
        let tmp = tempfile::tempdir().unwrap();
        let dl = tmp.path().join("dead_letter.json");
        // max_elapsed = 0 → the very first failure dead-letters.
        let schedule = BackoffSchedule {
            initial: Duration::from_millis(5),
            max: Duration::from_millis(5),
            max_elapsed: Duration::ZERO,
        };
        let mut q = SyncQueue::new(10, schedule, dl);
        let ev = ChunkFinalized {
            recording_name: "r".into(),
            recording_dir: PathBuf::from("/x"),
            machine_id: "m".into(),
            chunk_index: 0,
            sha256: "ab".repeat(32),
            local_path: PathBuf::from("/x/c0"),
        };
        let retried = q.on_retryable_failure(ev, "boom".into(), 1_000).unwrap();
        assert!(!retried, "max_elapsed=0 → dead-letter on first failure");
        assert_eq!(q.dead_letters().len(), 1);
    }

    #[tokio::test]
    async fn driver_uploads_all_chunks_and_marks_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let recs = tmp.path().join("recordings");
        std::fs::create_dir_all(&recs).unwrap();
        let (dir, events) = make_recording(&recs, "rec", "m");
        assert_eq!(events.len(), 2);

        let backend = Arc::new(LocalFs::new(tmp.path().join("remote")));
        let cfg = SyncConfig::with_dead_letter_path(tmp.path().join("dead_letter.json"));
        let cancel = CancellationToken::new();
        let (handle, task) = spawn(backend.clone(), cfg, Vec::new(), cancel);

        for ev in events {
            assert!(handle.notify_chunk(ev).await);
        }
        drop(handle); // close the channel → driver drains then exits
        task.await.unwrap();

        // Every chunk is marked uploaded in the manifest...
        let recording = manifest::load_dir(&dir).unwrap();
        assert!(recording.is_fully_uploaded());
        // ...and present in the backend, plus the manifest object.
        for chunk in &recording.chunks {
            let key = object_key_chunk("m", "rec", chunk.index, &chunk.sha256);
            assert!(
                backend.head(&key).await.unwrap().is_some(),
                "chunk in backend"
            );
        }
        let mkey = crate::storage::object_key_manifest("m", "rec");
        assert!(
            backend.head(&mkey).await.unwrap().is_some(),
            "manifest in backend"
        );
    }

    #[tokio::test]
    async fn driver_dead_letters_corrupt_chunk() {
        let tmp = tempfile::tempdir().unwrap();
        let recs = tmp.path().join("recordings");
        std::fs::create_dir_all(&recs).unwrap();
        let (dir, events) = make_recording(&recs, "rec", "m");

        // Corrupt the first chunk's bytes on disk → SHA-256 mismatch (terminal).
        let bad = &events[0].local_path;
        std::fs::write(bad, b"corrupted not-an-mcap").unwrap();

        let backend = Arc::new(LocalFs::new(tmp.path().join("remote")));
        let dl_path = tmp.path().join("dead_letter.json");
        let cfg = SyncConfig::with_dead_letter_path(dl_path.clone());
        let (handle, task) = spawn(backend.clone(), cfg, events, CancellationToken::new());
        drop(handle);
        task.await.unwrap();

        // The corrupt chunk is dead-lettered and NOT marked uploaded; the healthy
        // chunk still uploads.
        let dead = DeadLetterList::load(&dl_path).unwrap();
        assert_eq!(dead.len(), 1);
        assert_eq!(dead.entries()[0].chunk_index, 0);
        let recording = manifest::load_dir(&dir).unwrap();
        assert!(!recording.chunks[0].is_uploaded());
        assert!(recording.chunks[1].is_uploaded());
    }

    #[test]
    fn scan_pending_finds_unuploaded_chunks() {
        let tmp = tempfile::tempdir().unwrap();
        let recs = tmp.path().join("recordings");
        std::fs::create_dir_all(&recs).unwrap();
        make_recording(&recs, "rec_a", "m");
        make_recording(&recs, "rec_b", "m");
        let pending = scan_pending_in(&recs);
        // 2 recordings × 2 chunks each, all un-uploaded.
        assert_eq!(pending.len(), 4);
    }
}
