//! Local filesystem backend (spec §3.2 `backend/local.rs`).
//!
//! `LocalFs` roots all objects under a directory (e.g.
//! `~/.bubbaloop/recordings/`) and maps S3-style forward-slash keys onto paths
//! beneath it. It is useful both as the `backend = "local"` target and as a
//! checksum-aware stand-in for R2 in tests, since it honours the same
//! put-with-checksum contract.

use super::{validate_key, BackendError, ObjectMeta, PutResult, StorageBackend};
use crate::storage::integrity;
use async_trait::async_trait;
use std::path::{Path, PathBuf};

/// A filesystem-backed [`StorageBackend`] rooted at `root`.
#[derive(Debug, Clone)]
pub struct LocalFs {
    root: PathBuf,
}

impl LocalFs {
    /// Create a backend rooted at `root` (created on first write if absent).
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Resolve a validated key to an absolute path beneath the root.
    fn path_for(&self, key: &str) -> Result<PathBuf, BackendError> {
        validate_key(key)?;
        Ok(self.root.join(key))
    }
}

#[async_trait]
impl StorageBackend for LocalFs {
    async fn put(
        &self,
        key: &str,
        bytes: &[u8],
        checksum_sha256: Option<&integrity::Sha256Digest>,
    ) -> Result<PutResult, BackendError> {
        let path = self.path_for(key)?;

        if let Some(expected) = checksum_sha256 {
            let actual = integrity::sha256(bytes);
            if &actual != expected {
                return Err(BackendError::BadDigest {
                    key: key.to_string(),
                    expected: integrity::to_hex(expected),
                    actual: integrity::to_hex(&actual),
                });
            }
        }

        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| io_err(key, e))?;
        }
        tokio::fs::write(&path, bytes)
            .await
            .map_err(|e| io_err(key, e))?;

        let etag = format!("\"{}\"", integrity::to_hex(&integrity::sha256(bytes)));
        Ok(PutResult { etag: Some(etag) })
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>, BackendError> {
        let path = self.path_for(key)?;
        match tokio::fs::read(&path).await {
            Ok(bytes) => Ok(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(BackendError::NotFound {
                key: key.to_string(),
            }),
            Err(e) => Err(io_err(key, e)),
        }
    }

    async fn head(&self, key: &str) -> Result<Option<ObjectMeta>, BackendError> {
        let path = self.path_for(key)?;
        match tokio::fs::read(&path).await {
            Ok(bytes) => {
                let digest = integrity::sha256(&bytes);
                Ok(Some(ObjectMeta {
                    key: key.to_string(),
                    size_bytes: bytes.len() as u64,
                    etag: Some(format!("\"{}\"", integrity::to_hex(&digest))),
                    sha256: Some(digest),
                }))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(io_err(key, e)),
        }
    }

    async fn list(&self, prefix: &str) -> Result<Vec<ObjectMeta>, BackendError> {
        let root = self.root.clone();
        let prefix = prefix.to_string();
        let prefix_for_err = prefix.clone();
        // walkdir is synchronous; run it off the async runtime's worker threads.
        tokio::task::spawn_blocking(move || list_blocking(&root, &prefix))
            .await
            .map_err(|e| BackendError::Io {
                key: prefix_for_err,
                detail: format!("list task panicked: {e}"),
            })?
    }

    async fn delete(&self, key: &str) -> Result<(), BackendError> {
        let path = self.path_for(key)?;
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            // Deleting a missing key is a no-op (idempotent), like S3.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(io_err(key, e)),
        }
    }
}

fn io_err(key: &str, e: std::io::Error) -> BackendError {
    BackendError::Io {
        key: key.to_string(),
        detail: e.to_string(),
    }
}

fn list_blocking(root: &Path, prefix: &str) -> Result<Vec<ObjectMeta>, BackendError> {
    let mut out = Vec::new();
    if !root.exists() {
        return Ok(out);
    }
    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = match entry.path().strip_prefix(root) {
            Ok(r) => r,
            Err(_) => continue,
        };
        // Normalize to forward-slash, S3-style keys.
        let key = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        if !key.starts_with(prefix) {
            continue;
        }
        let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
        out.push(ObjectMeta {
            key,
            size_bytes,
            etag: None,
            sha256: None,
        });
    }
    out.sort_by(|a, b| a.key.cmp(&b.key));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backend() -> (tempfile::TempDir, LocalFs) {
        let dir = tempfile::tempdir().unwrap();
        let fs = LocalFs::new(dir.path());
        (dir, fs)
    }

    #[tokio::test]
    async fn put_get_head_delete_roundtrip() {
        let (_dir, fs) = backend();
        let key = "jetson/rec_a/chunks/chunk-000000-aabbccdd.mcap";
        let data = b"hello chunk";

        assert!(fs.head(key).await.unwrap().is_none());
        fs.put(key, data, None).await.unwrap();

        let got = fs.get(key).await.unwrap();
        assert_eq!(got, data);

        let meta = fs.head(key).await.unwrap().unwrap();
        assert_eq!(meta.size_bytes, data.len() as u64);
        assert_eq!(meta.sha256, Some(integrity::sha256(data)));
        assert!(meta.etag.is_some());

        fs.delete(key).await.unwrap();
        assert!(fs.head(key).await.unwrap().is_none());
        // Idempotent delete.
        fs.delete(key).await.unwrap();
    }

    #[tokio::test]
    async fn put_verifies_checksum() {
        let (_dir, fs) = backend();
        let data = b"verified payload";
        let good = integrity::sha256(data);
        fs.put("k/ok", data, Some(&good)).await.unwrap();

        let mut bad = good;
        bad[0] ^= 0xff;
        let err = fs.put("k/bad", data, Some(&bad)).await.unwrap_err();
        assert!(matches!(err, BackendError::BadDigest { .. }));
        // The bad object must not have been written.
        assert!(fs.head("k/bad").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn get_missing_is_not_found() {
        let (_dir, fs) = backend();
        let err = fs.get("nope").await.unwrap_err();
        assert!(matches!(err, BackendError::NotFound { .. }));
    }

    #[tokio::test]
    async fn list_filters_by_prefix_and_sorts() {
        let (_dir, fs) = backend();
        fs.put("m/recA/chunks/c0", b"0", None).await.unwrap();
        fs.put("m/recA/chunks/c1", b"11", None).await.unwrap();
        fs.put("m/recB/chunks/c0", b"222", None).await.unwrap();

        let a = fs.list("m/recA/").await.unwrap();
        let keys: Vec<&str> = a.iter().map(|o| o.key.as_str()).collect();
        assert_eq!(keys, vec!["m/recA/chunks/c0", "m/recA/chunks/c1"]);

        let all = fs.list("m/").await.unwrap();
        assert_eq!(all.len(), 3);
    }

    #[tokio::test]
    async fn rejects_traversal_keys() {
        let (_dir, fs) = backend();
        assert!(matches!(
            fs.put("a/../../escape", b"x", None).await,
            Err(BackendError::InvalidKey(_))
        ));
        assert!(matches!(
            fs.get("/etc/passwd").await,
            Err(BackendError::InvalidKey(_))
        ));
    }

    #[tokio::test]
    async fn list_empty_root_is_ok() {
        let dir = tempfile::tempdir().unwrap();
        let fs = LocalFs::new(dir.path().join("not-created-yet"));
        assert!(fs.list("").await.unwrap().is_empty());
    }
}
