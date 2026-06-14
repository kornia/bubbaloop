//! Storage backend abstraction (spec §3.2, §3.5).
//!
//! A [`StorageBackend`] is the put/get/list/delete/head surface that `sync` and
//! `reconcile` drive. Two implementations are planned: [`local::LocalFs`] (this
//! slice) and an `S3Compat` backend over `aws-sdk-s3` for R2/AWS/GCS/MinIO
//! (deferred to PR2 — it pulls a heavy SDK and needs network/credential plumbing
//! that belongs with the sync work).
//!
//! The trait is async and object-safe (via `async-trait`) so the daemon can hold
//! a `Box<dyn StorageBackend>` chosen at runtime from `[storage].backend`.

pub mod local;

use async_trait::async_trait;

use super::integrity::Sha256Digest;

/// Metadata about a stored object, as returned by `head`/`list`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectMeta {
    /// Object key (path relative to the backend root / bucket prefix).
    pub key: String,
    /// Size in bytes.
    pub size_bytes: u64,
    /// Backend ETag, when available.
    pub etag: Option<String>,
    /// SHA-256 of the object content, when the backend can supply it cheaply
    /// (HEAD on a checksum-aware backend). `list` may leave this `None`.
    pub sha256: Option<Sha256Digest>,
}

/// Result of a successful `put`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PutResult {
    /// ETag assigned by the backend, when available.
    pub etag: Option<String>,
}

/// The backend interface shared by local and cloud storage.
#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// Store `bytes` at `key`. If `checksum_sha256` is supplied, the backend
    /// verifies the content against it and returns [`BackendError::BadDigest`] on
    /// mismatch — mirroring R2's server-side `x-amz-checksum-sha256` validation.
    async fn put(
        &self,
        key: &str,
        bytes: &[u8],
        checksum_sha256: Option<&Sha256Digest>,
    ) -> Result<PutResult, BackendError>;

    /// Fetch the full object at `key`.
    async fn get(&self, key: &str) -> Result<Vec<u8>, BackendError>;

    /// Return metadata for `key`, or `None` if it does not exist.
    async fn head(&self, key: &str) -> Result<Option<ObjectMeta>, BackendError>;

    /// List objects whose key starts with `prefix`.
    async fn list(&self, prefix: &str) -> Result<Vec<ObjectMeta>, BackendError>;

    /// Delete the object at `key`. Deleting a missing key is **not** an error
    /// (idempotent), matching S3 `DeleteObject` semantics.
    async fn delete(&self, key: &str) -> Result<(), BackendError>;
}

/// Errors returned by storage backends. The retryable/terminal split mirrors the
/// sync retry policy (spec §3.4.3).
#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    /// The requested key does not exist.
    #[error("object not found: {key}")]
    NotFound { key: String },
    /// Content did not match the supplied checksum (terminal — local corruption).
    #[error("checksum mismatch for {key}: expected {expected}, got {actual}")]
    BadDigest {
        key: String,
        expected: String,
        actual: String,
    },
    /// The key is invalid (e.g. path traversal, empty).
    #[error("invalid object key: {0}")]
    InvalidKey(String),
    /// Authentication/authorization was rejected (terminal — credentials won't
    /// fix themselves; sync dead-letters immediately, §3.4.3). Produced by cloud
    /// backends; `LocalFs` never returns it.
    #[error("forbidden for {key}")]
    Forbidden { key: String },
    /// The configured bucket does not exist (terminal misconfiguration — sync
    /// pauses entirely until reconfigured, §3.4.3). Produced by cloud backends.
    #[error("no such bucket: {bucket}")]
    NoSuchBucket { bucket: String },
    /// Filesystem / transport error (generally retryable).
    #[error("backend io error for {key}: {detail}")]
    Io { key: String, detail: String },
}

/// Reject keys that are empty, absolute, or contain `..` traversal components.
/// Keys are forward-slash separated, S3-style, even on Windows.
pub(crate) fn validate_key(key: &str) -> Result<(), BackendError> {
    if key.is_empty() {
        return Err(BackendError::InvalidKey("empty key".into()));
    }
    if key.starts_with('/') {
        return Err(BackendError::InvalidKey(format!("absolute key: {key}")));
    }
    for component in key.split('/') {
        if component == ".." {
            return Err(BackendError::InvalidKey(format!(
                "key contains traversal component: {key}"
            )));
        }
        if component.contains('\0') {
            return Err(BackendError::InvalidKey("key contains NUL".into()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_validation() {
        assert!(validate_key("machine/rec/chunks/chunk-000000-aabbccdd.mcap").is_ok());
        assert!(validate_key("").is_err());
        assert!(validate_key("/abs/path").is_err());
        assert!(validate_key("a/../../etc/passwd").is_err());
        assert!(validate_key("a/b\0c").is_err());
    }
}
