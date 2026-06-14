//! Storage configuration — the `[storage]` table of `~/.bubbaloop/config.toml`
//! (spec §4.1) and the backend factory that turns it into a live
//! [`StorageBackend`].
//!
//! Secrets are **never** read from here — credentials live only in
//! `~/.bubbaloop/secrets.toml` (chmod 0600, see [`super::secrets`]). This struct
//! holds the non-secret connection shape (backend kind, endpoint, region,
//! bucket) plus local-store knobs.
//!
//! In this slice only the `local` backend is constructible; the cloud backends
//! (`r2`/`s3`/`gcs`/`minio`) parse fine but [`StorageConfig::build_backend`]
//! returns [`ConfigError::BackendNotImplemented`] until the S3-compatible backend
//! lands.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::backend::local::LocalFs;
use super::{bubbaloop_dir, StorageBackend, StoragePathError};

/// The `[storage]` configuration table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Backend kind: `r2` | `s3` | `gcs` | `minio` | `local`.
    #[serde(default = "default_backend")]
    pub backend: String,
    /// S3-compatible endpoint URL (cloud backends).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    /// Region (R2 uses `auto`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    /// Bucket name — the sharing unit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bucket: Option<String>,
    /// Whether the daemon background-syncs finalized chunks.
    #[serde(default = "default_true")]
    pub auto_upload: bool,
    /// Auto-delete local recordings this many days after successful upload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retention_days_local: Option<u32>,
    /// Soft cap on the local recordings directory (GiB).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disk_quota_gb: Option<u32>,
    /// Which recorder instance to command, when several are installed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recorder_instance: Option<String>,
    /// Root directory for the `local` backend's object store. Defaults to
    /// `~/.bubbaloop/storage/objects`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_path: Option<PathBuf>,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            backend: default_backend(),
            endpoint: None,
            region: None,
            bucket: None,
            auto_upload: true,
            retention_days_local: None,
            disk_quota_gb: None,
            recorder_instance: None,
            local_path: None,
        }
    }
}

/// Whole-file shape, so we can pluck `[storage]` out of a `config.toml` that also
/// holds unrelated tables. Unknown tables/keys are ignored (forward-compat).
#[derive(Debug, Default, Deserialize)]
struct ConfigFile {
    #[serde(default)]
    storage: Option<StorageConfig>,
}

impl StorageConfig {
    /// Load the `[storage]` table from `~/.bubbaloop/config.toml`. A missing file
    /// or missing `[storage]` table yields [`StorageConfig::default`] (the
    /// `local` backend), so the data-plane verbs work out of the box.
    pub fn load() -> Result<Self, ConfigError> {
        let path = bubbaloop_dir()?.join("config.toml");
        Self::load_from(&path)
    }

    /// Load from an explicit path (testable). Missing file → default.
    pub fn load_from(path: &std::path::Path) -> Result<Self, ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(s) => Self::parse(&s),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(ConfigError::Io {
                path: path.display().to_string(),
                detail: e.to_string(),
            }),
        }
    }

    /// Parse a `config.toml` body and return its `[storage]` table (or default).
    pub fn parse(toml_str: &str) -> Result<Self, ConfigError> {
        let file: ConfigFile =
            toml::from_str(toml_str).map_err(|e| ConfigError::Parse(e.to_string()))?;
        Ok(file.storage.unwrap_or_default())
    }

    /// Whether the configured backend is an actual remote (i.e. not the local
    /// object store). CLI `--remote` flags require this so they can't silently
    /// operate on the local backend.
    pub fn is_remote(&self) -> bool {
        self.backend != "local"
    }

    /// Resolve the `local` backend's object-store root.
    pub fn local_root(&self) -> Result<PathBuf, ConfigError> {
        match &self.local_path {
            Some(p) => Ok(p.clone()),
            None => Ok(bubbaloop_dir()?.join("storage").join("objects")),
        }
    }

    /// Build a live [`StorageBackend`] from this config.
    pub fn build_backend(&self) -> Result<Box<dyn StorageBackend>, ConfigError> {
        match self.backend.as_str() {
            "local" => Ok(Box::new(LocalFs::new(self.local_root()?))),
            "r2" | "s3" | "gcs" | "minio" => {
                Err(ConfigError::BackendNotImplemented(self.backend.clone()))
            }
            other => Err(ConfigError::UnknownBackend(other.to_string())),
        }
    }
}

fn default_backend() -> String {
    "local".to_string()
}
fn default_true() -> bool {
    true
}

/// Errors loading storage config or building a backend.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Filesystem error reading the config file.
    #[error("config io error at {path}: {detail}")]
    Io { path: String, detail: String },
    /// TOML failed to parse.
    #[error("config parse error: {0}")]
    Parse(String),
    /// Storage path could not be resolved.
    #[error("storage path error: {0}")]
    Path(#[from] StoragePathError),
    /// The backend kind is recognised but not yet implemented in this build.
    #[error("storage backend '{0}' is not yet implemented (only 'local' is available so far)")]
    BackendNotImplemented(String),
    /// The backend kind is not one of r2/s3/gcs/minio/local.
    #[error("unknown storage backend '{0}' (expected one of: r2, s3, gcs, minio, local)")]
    UnknownBackend(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_yields_default_local_backend() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = StorageConfig::load_from(&dir.path().join("nope.toml")).unwrap();
        assert_eq!(cfg.backend, "local");
        assert!(cfg.auto_upload);
    }

    #[test]
    fn parses_spec_example_storage_table() {
        let toml = r#"
            [storage]
            backend = "r2"
            endpoint = "https://acct.r2.cloudflarestorage.com"
            region = "auto"
            bucket = "kornia-recordings"
            auto_upload = true
            retention_days_local = 7
            disk_quota_gb = 50
            recorder_instance = "mcap_recorder"

            [agent]
            model = "claude"
        "#;
        let cfg = StorageConfig::parse(toml).unwrap();
        assert_eq!(cfg.backend, "r2");
        assert_eq!(cfg.bucket.as_deref(), Some("kornia-recordings"));
        assert_eq!(cfg.region.as_deref(), Some("auto"));
        assert_eq!(cfg.retention_days_local, Some(7));
        assert_eq!(cfg.recorder_instance.as_deref(), Some("mcap_recorder"));
    }

    #[test]
    fn unrelated_config_without_storage_is_default() {
        let cfg = StorageConfig::parse("[agent]\nmodel = \"x\"\n").unwrap();
        assert_eq!(cfg.backend, "local");
    }

    #[test]
    fn build_local_backend_uses_configured_path() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = StorageConfig {
            backend: "local".into(),
            local_path: Some(dir.path().join("objs")),
            ..Default::default()
        };
        // Just exercising the factory: it must succeed and not panic.
        assert!(cfg.build_backend().is_ok());
        assert_eq!(cfg.local_root().unwrap(), dir.path().join("objs"));
    }

    #[test]
    fn cloud_backends_are_not_yet_implemented() {
        for backend in ["r2", "s3", "gcs", "minio"] {
            let cfg = StorageConfig {
                backend: backend.into(),
                ..Default::default()
            };
            assert!(matches!(
                cfg.build_backend(),
                Err(ConfigError::BackendNotImplemented(_))
            ));
        }
    }

    #[test]
    fn is_remote_only_for_cloud_backends() {
        assert!(!StorageConfig::default().is_remote()); // default = local
        for backend in ["r2", "s3", "gcs", "minio"] {
            let cfg = StorageConfig {
                backend: backend.into(),
                ..Default::default()
            };
            assert!(cfg.is_remote());
        }
    }

    #[test]
    fn unknown_backend_is_rejected() {
        let cfg = StorageConfig {
            backend: "ftp".into(),
            ..Default::default()
        };
        assert!(matches!(
            cfg.build_backend(),
            Err(ConfigError::UnknownBackend(_))
        ));
    }
}
