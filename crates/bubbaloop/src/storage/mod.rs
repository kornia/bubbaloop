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
//!
//! Still to come (later PRs from the spec): `discover`, the S3Compat backend, the
//! sync queue driver, `replay`, `ring_buffer`, the `mcap-recorder` node, the
//! CLI/MCP surfaces, and the dashboard tab.

pub mod backend;
pub mod integrity;
pub mod manifest;
pub mod profile;
pub mod reconcile;
pub mod recording;
pub mod secrets;
pub mod sync;

use std::path::{Path, PathBuf};

// Re-export the most commonly used types at the subsystem root.
pub use backend::{local::LocalFs, BackendError, ObjectMeta, PutResult, StorageBackend};
pub use profile::Profile;
pub use reconcile::{reconcile, ReconcileError, ReconcileOptions, ReconcileReport};
pub use recording::{Channel, Chunk, Lifecycle, Recording, RecordingMode, Selection, Trigger};
pub use sync::{
    classify, BackoffSchedule, DeadLetterEntry, DeadLetterList, ErrorClass, SharedBackoff,
    SyncError, UploadJob, UploadOutcome,
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

/// Directory for one recording: `~/.bubbaloop/recordings/{name}`.
pub fn recording_dir(name: &str) -> Result<PathBuf, StoragePathError> {
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
    fn chunk_key_is_a_valid_backend_key() {
        let hex = "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2";
        let key = object_key_chunk("m", "r", 0, hex);
        // The deterministic key must be accepted by the backend key validator.
        assert!(backend::validate_key(&key).is_ok());
    }
}
