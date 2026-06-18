//! Load, validate, and atomically persist `manifest.json` (spec §3.4.5, §4.4).
//!
//! Every manifest write is `write_tmp_then_rename`: serialize to a sibling
//! `manifest.json.tmp`, fsync it, then `rename` over the target. A crash
//! mid-write therefore leaves either the old or the new manifest intact, never a
//! half-written one (spec §3.4.5 atomicity).
//!
//! Forward compatibility: loading preserves unknown fields (see
//! [`crate::storage::recording`]). Validation accepts manifests whose
//! `schema_version` is newer than this build — it only rejects manifests that are
//! structurally broken (missing names, non-contiguous chunk indices, malformed
//! digests), so a future writer's extra fields never trip an older reader.

use super::integrity;
use super::recording::{Chunk, Recording, MANIFEST_SCHEMA_VERSION};
use std::path::{Path, PathBuf};

/// Standard manifest filename at a recording's root.
pub const MANIFEST_FILE: &str = "manifest.json";

/// Resolve the manifest path inside a recording directory.
pub fn manifest_path(recording_dir: impl AsRef<Path>) -> PathBuf {
    recording_dir.as_ref().join(MANIFEST_FILE)
}

/// Parse a manifest from a JSON string and validate it.
pub fn parse(json: &str) -> Result<Recording, ManifestError> {
    let recording: Recording =
        serde_json::from_str(json).map_err(|e| ManifestError::Parse(e.to_string()))?;
    validate(&recording)?;
    Ok(recording)
}

/// Load and validate a manifest from a file path.
pub fn load(path: impl AsRef<Path>) -> Result<Recording, ManifestError> {
    let path = path.as_ref();
    let contents = std::fs::read_to_string(path).map_err(|e| ManifestError::Io {
        path: path.display().to_string(),
        detail: e.to_string(),
    })?;
    parse(&contents)
}

/// Load the `manifest.json` from inside a recording directory.
pub fn load_dir(recording_dir: impl AsRef<Path>) -> Result<Recording, ManifestError> {
    load(manifest_path(recording_dir))
}

/// Atomically persist a manifest to `path` via write-tmp-then-rename.
///
/// The temp file is created as a sibling of the target (same directory) so the
/// final `rename` stays within one filesystem and is therefore atomic.
pub fn save_atomic(path: impl AsRef<Path>, recording: &Recording) -> Result<(), ManifestError> {
    let path = path.as_ref();
    validate(recording)?;

    let bytes = serde_json::to_vec_pretty(recording)
        .map_err(|e| ManifestError::Serialize(e.to_string()))?;

    // Durable write-tmp-fsync-rename lives in one shared place (§3.4.5).
    crate::storage::atomic_write(path, &bytes).map_err(|e| ManifestError::Io {
        path: path.display().to_string(),
        detail: e.to_string(),
    })
}

/// Save the manifest into a recording directory (`{dir}/manifest.json`).
pub fn save_dir(
    recording_dir: impl AsRef<Path>,
    recording: &Recording,
) -> Result<(), ManifestError> {
    save_atomic(manifest_path(recording_dir), recording)
}

/// Structural validation. Intentionally permissive about *forward* schema
/// versions (newer manifests are accepted, their extra fields preserved) but
/// strict about internal consistency.
pub fn validate(r: &Recording) -> Result<(), ManifestError> {
    if r.schema_version == 0 {
        return Err(ManifestError::Invalid("schema_version must be >= 1".into()));
    }
    if r.name.trim().is_empty() {
        return Err(ManifestError::Invalid("recording name is empty".into()));
    }
    if r.machine_id.trim().is_empty() {
        return Err(ManifestError::Invalid("machine_id is empty".into()));
    }

    // Chunk indices must be contiguous from 0, in order, with valid digests.
    for (expected_idx, chunk) in r.chunks.iter().enumerate() {
        let expected_idx = expected_idx as u32;
        if chunk.index != expected_idx {
            return Err(ManifestError::Invalid(format!(
                "chunk indices must be contiguous from 0: expected index {expected_idx}, found {}",
                chunk.index
            )));
        }
        integrity::from_hex(&chunk.sha256).map_err(|_| {
            ManifestError::Invalid(format!(
                "chunk {} has an invalid sha256: {}",
                chunk.index, chunk.sha256
            ))
        })?;
        // The filename must be the canonical `chunk-{idx:06}-{prefix8}.mcap`
        // (§3.3.6). Enforcing this here means no consumer can be tricked into
        // treating a tampered/non-canonical `name` (e.g. one fetched from an
        // untrusted remote manifest) as a path segment for a write.
        let expected_name = Chunk::canonical_name(chunk.index, &chunk.sha256);
        if chunk.name != expected_name {
            return Err(ManifestError::Invalid(format!(
                "chunk {} name {:?} is not canonical (expected {:?})",
                chunk.index, chunk.name, expected_name
            )));
        }
    }
    Ok(())
}

/// True if a manifest's schema version is newer than this build can natively
/// understand. Such manifests are still loaded (unknown fields preserved); this
/// just lets callers warn before re-saving and potentially dropping semantics.
pub fn is_future_version(r: &Recording) -> bool {
    r.schema_version > MANIFEST_SCHEMA_VERSION
}

/// Errors from manifest IO and validation.
#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    /// Filesystem error reading or writing the manifest.
    #[error("manifest io error at {path}: {detail}")]
    Io { path: String, detail: String },
    /// JSON failed to parse.
    #[error("manifest parse error: {0}")]
    Parse(String),
    /// JSON failed to serialize.
    #[error("manifest serialize error: {0}")]
    Serialize(String),
    /// The manifest parsed but is structurally invalid.
    #[error("invalid manifest: {0}")]
    Invalid(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::recording::{Chunk, Recording};

    fn chunk(index: u32, byte: u8) -> Chunk {
        let sha = format!("{byte:02x}").repeat(32);
        Chunk {
            name: Chunk::canonical_name(index, &sha),
            index,
            size_bytes: 100,
            sha256: sha,
            log_time_first_ns: None,
            log_time_last_ns: None,
            uploaded_at_ns: None,
            remote_etag: None,
            extra: Default::default(),
        }
    }

    fn valid_recording() -> Recording {
        let mut r = Recording::new("rec_a", "jetson_alpha", 1_000);
        r.chunks.push(chunk(0, 0xab));
        r.chunks.push(chunk(1, 0xcd));
        r
    }

    #[test]
    fn atomic_save_then_load_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let rec_dir = dir.path().join("rec_a");
        let r = valid_recording();

        save_dir(&rec_dir, &r).unwrap();
        assert!(manifest_path(&rec_dir).exists());
        // No temp file left behind.
        assert!(!rec_dir.join("manifest.json.tmp").exists());

        let loaded = load_dir(&rec_dir).unwrap();
        assert_eq!(loaded, r);
    }

    #[test]
    fn save_overwrites_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let rec_dir = dir.path().join("rec_a");
        let mut r = valid_recording();
        save_dir(&rec_dir, &r).unwrap();

        // Mutate (mark a chunk uploaded) and re-save — simulates sync's update path.
        r.chunks[0].uploaded_at_ns = Some(42);
        save_dir(&rec_dir, &r).unwrap();

        let loaded = load_dir(&rec_dir).unwrap();
        assert_eq!(loaded.chunks[0].uploaded_at_ns, Some(42));
        assert!(!rec_dir.join("manifest.json.tmp").exists());
    }

    #[test]
    fn rejects_non_contiguous_chunk_indices() {
        let mut r = valid_recording();
        r.chunks[1].index = 5; // gap
        let err = validate(&r).unwrap_err();
        assert!(matches!(err, ManifestError::Invalid(_)));
    }

    #[test]
    fn rejects_non_canonical_chunk_name() {
        // A manifest whose chunk name has been tampered into a path-traversal
        // string must fail validation (it would otherwise be trusted as a write
        // target on the download path).
        let mut r = valid_recording();
        r.chunks[0].name = "../../evil.mcap".to_string();
        let err = validate(&r).unwrap_err();
        assert!(matches!(err, ManifestError::Invalid(_)));
        // Parsing also rejects it (download fetches remote manifests via parse).
        let json = serde_json::to_string(&r).unwrap();
        assert!(parse(&json).is_err());
    }

    #[test]
    fn rejects_bad_sha256() {
        let mut r = valid_recording();
        r.chunks[0].sha256 = "not-hex".into();
        assert!(matches!(validate(&r), Err(ManifestError::Invalid(_))));
    }

    #[test]
    fn rejects_empty_name() {
        let mut r = valid_recording();
        r.name = "  ".into();
        assert!(matches!(validate(&r), Err(ManifestError::Invalid(_))));
    }

    #[test]
    fn parse_validates() {
        // index gap should be rejected at parse time too.
        let json = r#"{
            "schema_version": 1, "name": "x", "machine_id": "m",
            "started_at_ns": 0, "mode": "streaming",
            "selection": {"topics": [], "exclude": [], "include_local": false},
            "chunks": [
                {"name": "chunk-000000-aaaaaaaa.mcap", "index": 0, "size_bytes": 1, "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},
                {"name": "chunk-000002-bbbbbbbb.mcap", "index": 2, "size_bytes": 1, "sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}
            ]
        }"#;
        assert!(matches!(parse(json), Err(ManifestError::Invalid(_))));
    }

    #[test]
    fn future_version_is_detected_but_loadable() {
        let mut r = valid_recording();
        r.schema_version = 99;
        assert!(is_future_version(&r));
        assert!(validate(&r).is_ok());
    }

    #[test]
    fn load_missing_file_is_io_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = load_dir(dir.path().join("does_not_exist")).unwrap_err();
        assert!(matches!(err, ManifestError::Io { .. }));
    }
}
