//! Storage subsystem — fleet-aware recordings (spec
//! `docs/superpowers/specs/2026-05-26-bubbaloop-storage-design.md`).
//!
//! This is the foundation layer (spec §17, "PR1"): the pure, Zenoh-free core that
//! everything else builds on.
//!
//! - [`integrity`] — mandatory SHA-256 per chunk (streaming digest + verify).
//! - [`recording`] — the `manifest.json` data model (the source of truth).
//! - [`manifest`] — atomic load/save/validate of manifests.
//! - [`profile`] — `~/.bubbaloop/profiles/*.yaml` with the v1 validator + canonical hash.
//! - [`secrets`] — opaque, zeroize-on-drop credentials in `secrets.toml` (chmod 0600).
//! - [`backend`] — the `StorageBackend` trait + `LocalFs` (S3Compat lands in PR2).
//! - [`reconcile`] — the diff-matrix healing primitive (§3.5), backend-agnostic.
//! - [`sync`] — the background-upload core (§3.4): backoff, dead-letter,
//!   per-chunk upload sequence (the async queue driver lands with the recorder).
//! - [`ring_buffer`] — the sliding in-memory window (§3.3.5) + the circular MCAP
//!   writer (`seal_samples`) that materializes a snapshot into a recording.
//! - [`replay`] — read a recording's MCAP chunks and re-publish into Zenoh,
//!   preserving both timestamps (§4.5).
//!
//! Still to come (later PRs from the spec): the sync queue driver, the daemon-side
//! `record` path + `mcap-recorder` node, the MCP surface, and the dashboard tab.

pub mod backend;
pub mod config;
pub mod discover;
pub mod integrity;
pub mod manifest;
pub mod profile;
pub mod reconcile;
pub mod recording;
pub mod replay;
pub mod ring_buffer;
pub mod secrets;
pub mod sync;
pub mod sync_driver;

use std::path::{Path, PathBuf};

// Re-export the most commonly used types at the subsystem root.
pub use backend::{local::LocalFs, BackendError, ObjectMeta, PutResult, StorageBackend};
pub use config::{ConfigError, StorageConfig};
pub use profile::Profile;
pub use reconcile::{reconcile, ReconcileError, ReconcileOptions, ReconcileReport};
pub use recording::{Channel, Chunk, Lifecycle, Recording, RecordingMode, Selection, Trigger};
pub use replay::{
    plan as plan_replay, read_recording_messages, run_replay, ReplayError, ReplayMessage,
    ReplayOptions, ReplaySink, ReplayStats,
};
pub use ring_buffer::{seal_samples, McapWriteConfig, RingBuffer, RingBufferError, Sample};
pub use sync::{
    classify, BackoffSchedule, DeadLetterEntry, DeadLetterList, ErrorClass, SharedBackoff,
    SyncError, UploadJob, UploadOutcome,
};
pub use sync_driver::{
    run_driver, scan_pending, scan_pending_in, spawn as spawn_sync, ChunkFinalized, EnqueueResult,
    SyncConfig, SyncHandle, SyncQueue,
};

/// Base bubbaloop directory: `~/.bubbaloop`.
pub fn bubbaloop_dir() -> Result<PathBuf, StoragePathError> {
    let home = dirs::home_dir().ok_or(StoragePathError::NoHomeDir)?;
    Ok(home.join(".bubbaloop"))
}

/// Local recordings root: `~/.bubbaloop/recordings`.
pub fn recordings_dir() -> Result<PathBuf, StoragePathError> {
    Ok(bubbaloop_dir()?.join("recordings"))
}

/// Maximum length of a recording name.
pub const MAX_RECORDING_NAME_LEN: usize = 128;

/// Validate a caller-supplied recording name before it is used as a filesystem
/// path segment or remote object-key component.
///
/// Allowed: 1..=[`MAX_RECORDING_NAME_LEN`] characters of `[A-Za-z0-9._-]`, and
/// not the traversal segments `.` / `..`. This is the single guard that every
/// path builder funnels through, so no caller can smuggle a `..`, `/`, or NUL
/// into [`recording_dir`]/[`chunks_dir`] (and thence into `remove_dir_all`,
/// reads, or a remote prefix). Mirrors the node-name rules in CLAUDE.md.
pub fn validate_recording_name(name: &str) -> Result<(), StoragePathError> {
    let valid_chars = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if name.is_empty()
        || name.len() > MAX_RECORDING_NAME_LEN
        || !valid_chars
        || name == "."
        || name == ".."
    {
        return Err(StoragePathError::InvalidName(name.to_string()));
    }
    Ok(())
}

/// Directory for one recording: `~/.bubbaloop/recordings/{name}`.
///
/// Rejects names that fail [`validate_recording_name`] so the returned path can
/// never escape the recordings root.
pub fn recording_dir(name: &str) -> Result<PathBuf, StoragePathError> {
    validate_recording_name(name)?;
    Ok(recordings_dir()?.join(name))
}

/// Chunks directory for one recording: `.../{name}/chunks`.
pub fn chunks_dir(name: &str) -> Result<PathBuf, StoragePathError> {
    Ok(recording_dir(name)?.join("chunks"))
}

/// Storage state directory (dead-letter list, future file-backed queue):
/// `~/.bubbaloop/storage`.
pub fn storage_state_dir() -> Result<PathBuf, StoragePathError> {
    Ok(bubbaloop_dir()?.join("storage"))
}

/// The `chunks/` subpath under a recording directory.
pub fn chunks_path(recording_dir: impl AsRef<Path>) -> PathBuf {
    recording_dir.as_ref().join("chunks")
}

/// Atomically write `bytes` to `path` via a sibling temp file + fsync + rename.
///
/// The temp file is `{path}.tmp` (same directory → same filesystem → the rename
/// is atomic), fsynced before the rename, so a crash mid-write leaves either the
/// old file or the complete new one — never a partial. Shared by every durable
/// write in the subsystem (`manifest.json`, sealed chunk files) so the
/// crash-atomicity guarantee lives in exactly one place.
pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}

/// Deterministic remote object key for a recording's manifest (spec §8):
/// `{machine_id}/{recording_name}/manifest.json`.
pub fn object_key_manifest(machine_id: &str, recording_name: &str) -> String {
    format!("{machine_id}/{recording_name}/manifest.json")
}

/// Deterministic remote object key for a chunk (spec §8):
/// `{machine_id}/{recording_name}/chunks/chunk-{idx:06}-{sha256_prefix8}.mcap`.
///
/// The SHA-256 prefix in the key makes re-uploads idempotent by HEAD check:
/// identical content → identical key → no-op; changed content → different key →
/// no silent overwrite.
pub fn object_key_chunk(
    machine_id: &str,
    recording_name: &str,
    chunk_index: u32,
    sha256_hex: &str,
) -> String {
    let name = Chunk::canonical_name(chunk_index, sha256_hex);
    format!("{machine_id}/{recording_name}/chunks/{name}")
}

/// Errors resolving storage paths.
#[derive(Debug, thiserror::Error)]
pub enum StoragePathError {
    /// No home directory could be determined.
    #[error("could not determine home directory")]
    NoHomeDir,
    /// The recording name is empty, too long, or contains illegal characters
    /// (allowed: `[A-Za-z0-9._-]`, and not `.`/`..`).
    #[error("invalid recording name: {0:?}")]
    InvalidName(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_object_key() {
        assert_eq!(
            object_key_manifest("jetson_alpha", "outdoor_test_3"),
            "jetson_alpha/outdoor_test_3/manifest.json"
        );
    }

    #[test]
    fn chunk_object_key_embeds_index_and_prefix() {
        let hex = "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2";
        assert_eq!(
            object_key_chunk("jetson_alpha", "rec", 7, hex),
            "jetson_alpha/rec/chunks/chunk-000007-a1b2c3d4.mcap"
        );
    }

    #[test]
    fn recording_name_validation_blocks_traversal() {
        assert!(validate_recording_name("rec_2026-05-26T12.34.56_jetson").is_ok());
        assert!(validate_recording_name("a").is_ok());
        // Traversal and separators must be rejected.
        assert!(validate_recording_name("..").is_err());
        assert!(validate_recording_name(".").is_err());
        assert!(validate_recording_name("../../etc").is_err());
        assert!(validate_recording_name("a/b").is_err());
        assert!(validate_recording_name("/abs").is_err());
        assert!(validate_recording_name("a\0b").is_err());
        assert!(validate_recording_name("").is_err());
        assert!(validate_recording_name(&"x".repeat(MAX_RECORDING_NAME_LEN + 1)).is_err());
        // A `..` substring inside a single segment is fine (not a traversal).
        assert!(validate_recording_name("a..b").is_ok());
    }

    #[test]
    fn recording_dir_rejects_traversal_names() {
        assert!(recording_dir("../../escape").is_err());
        assert!(chunks_dir("..").is_err());
    }

    #[test]
    fn chunk_key_is_a_valid_backend_key() {
        let hex = "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2";
        let key = object_key_chunk("m", "r", 0, hex);
        // The deterministic key must be accepted by the backend key validator.
        assert!(backend::validate_key(&key).is_ok());
    }
}
