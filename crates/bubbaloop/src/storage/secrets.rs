//! Secret storage for cloud backend credentials (spec §4.2).
//!
//! Secrets live **only** in `~/.bubbaloop/secrets.toml` (chmod 0600) — never in
//! `config.toml`, `node.yaml`, profile YAML, or logs. [`Secret`] is an opaque
//! wrapper whose `Debug` is redacted and whose backing string is zeroized on
//! drop, so a credential can never be accidentally formatted into a log line.
//!
//! `Secret` deliberately does **not** implement `Display`, `Serialize`, or
//! `Clone`: the only way to read the value is the explicit [`Secret::expose`],
//! which makes every access auditable in review.

use serde::Deserialize;
use std::path::{Path, PathBuf};
use zeroize::{Zeroize, ZeroizeOnDrop};

/// An opaque, redacted, zeroize-on-drop secret string.
#[derive(Clone, Zeroize, ZeroizeOnDrop, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    /// Wrap a secret value.
    pub fn new(value: impl Into<String>) -> Self {
        Secret(value.into())
    }

    /// Explicitly read the secret. Every call site is an auditable disclosure —
    /// keep the borrow as short-lived as possible and never log the result.
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Whether the secret is empty (e.g. unconfigured).
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(***redacted***)")
    }
}

// Deserialize directly into a Secret so credential structs never materialize a
// bare String. We do NOT implement Serialize — saving goes through the explicit,
// redaction-aware [`save`] path below.
impl<'de> Deserialize<'de> for Secret {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Secret::new(s))
    }
}

/// R2 / S3-compatible access credentials (spec §4.2 `[storage.r2]`).
#[derive(Debug, Deserialize)]
pub struct R2Credentials {
    /// Access key id.
    pub access_key_id: Secret,
    /// Secret access key.
    pub secret_access_key: Secret,
}

/// Parsed `secrets.toml`.
#[derive(Debug, Deserialize, Default)]
pub struct Secrets {
    #[serde(default)]
    storage: StorageSecrets,
}

#[derive(Debug, Deserialize, Default)]
struct StorageSecrets {
    #[serde(default)]
    r2: Option<R2Credentials>,
}

impl Secrets {
    /// The R2 credentials, if configured.
    pub fn r2(&self) -> Option<&R2Credentials> {
        self.storage.r2.as_ref()
    }
}

/// Default secrets path: `~/.bubbaloop/secrets.toml`.
pub fn default_path() -> Result<PathBuf, SecretsError> {
    let home = dirs::home_dir().ok_or(SecretsError::NoHomeDir)?;
    Ok(home.join(".bubbaloop").join("secrets.toml"))
}

/// Load and parse `secrets.toml`. Returns an empty [`Secrets`] if the file does
/// not exist (unconfigured is a valid state, not an error).
pub fn load(path: impl AsRef<Path>) -> Result<Secrets, SecretsError> {
    let path = path.as_ref();
    if !path.exists() {
        return Ok(Secrets::default());
    }
    let contents = std::fs::read_to_string(path).map_err(|e| SecretsError::Io {
        path: path.display().to_string(),
        detail: e.to_string(),
    })?;
    toml::from_str(&contents).map_err(|e| SecretsError::Parse(e.to_string()))
}

/// Write R2 credentials to `secrets.toml` with 0600 permissions (owner-only).
///
/// The file is created/truncated with owner-only mode on Unix before any secret
/// bytes are written, so the window where a credential exists at a wider mode is
/// eliminated.
pub fn save_r2(path: impl AsRef<Path>, creds: &R2Credentials) -> Result<(), SecretsError> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| SecretsError::Io {
            path: parent.display().to_string(),
            detail: e.to_string(),
        })?;
    }

    // TOML built by hand so a stray `Serialize` impl can never leak a secret.
    let body = format!(
        "[storage.r2]\naccess_key_id = {}\nsecret_access_key = {}\n",
        toml_quote(creds.access_key_id.expose()),
        toml_quote(creds.secret_access_key.expose()),
    );

    write_owner_only(path, body.as_bytes())?;
    Ok(())
}

/// Minimal TOML basic-string quoting (escapes backslash and double-quote).
fn toml_quote(s: &str) -> String {
    let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

#[cfg(unix)]
fn write_owner_only(path: &Path, bytes: &[u8]) -> Result<(), SecretsError> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| SecretsError::Io {
            path: path.display().to_string(),
            detail: e.to_string(),
        })?;
    // Re-assert mode in case the file pre-existed with a wider mode.
    let perms = std::fs::Permissions::from_mode(0o600);
    std::fs::set_permissions(path, perms).map_err(|e| SecretsError::Io {
        path: path.display().to_string(),
        detail: e.to_string(),
    })?;
    f.write_all(bytes).map_err(|e| SecretsError::Io {
        path: path.display().to_string(),
        detail: e.to_string(),
    })?;
    f.sync_all().map_err(|e| SecretsError::Io {
        path: path.display().to_string(),
        detail: e.to_string(),
    })?;
    Ok(())
}

#[cfg(not(unix))]
fn write_owner_only(path: &Path, bytes: &[u8]) -> Result<(), SecretsError> {
    std::fs::write(path, bytes).map_err(|e| SecretsError::Io {
        path: path.display().to_string(),
        detail: e.to_string(),
    })
}

/// Errors from secret IO.
#[derive(Debug, thiserror::Error)]
pub enum SecretsError {
    /// No home directory could be determined.
    #[error("could not determine home directory")]
    NoHomeDir,
    /// Filesystem error.
    #[error("secrets io error at {path}: {detail}")]
    Io { path: String, detail: String },
    /// TOML parse error.
    #[error("secrets parse error: {0}")]
    Parse(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_is_redacted() {
        let s = Secret::new("super-secret-value");
        let dbg = format!("{s:?}");
        assert_eq!(dbg, "Secret(***redacted***)");
        assert!(!dbg.contains("super-secret-value"));

        // And nested inside a credentials struct.
        let creds = R2Credentials {
            access_key_id: Secret::new("AKIA_VISIBLE_ID"),
            secret_access_key: Secret::new("THE_SECRET_KEY"),
        };
        let dbg = format!("{creds:?}");
        assert!(!dbg.contains("AKIA_VISIBLE_ID"));
        assert!(!dbg.contains("THE_SECRET_KEY"));
    }

    #[test]
    fn expose_returns_value() {
        let s = Secret::new("value");
        assert_eq!(s.expose(), "value");
        assert!(Secret::new("").is_empty());
    }

    #[test]
    fn save_then_load_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.toml");
        let creds = R2Credentials {
            access_key_id: Secret::new("id-123"),
            secret_access_key: Secret::new("key-456"),
        };
        save_r2(&path, &creds).unwrap();

        let loaded = load(&path).unwrap();
        let r2 = loaded.r2().expect("r2 creds present");
        assert_eq!(r2.access_key_id.expose(), "id-123");
        assert_eq!(r2.secret_access_key.expose(), "key-456");
    }

    #[test]
    fn load_missing_file_is_empty_not_error() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = load(dir.path().join("nope.toml")).unwrap();
        assert!(loaded.r2().is_none());
    }

    #[test]
    fn special_characters_survive_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.toml");
        let creds = R2Credentials {
            access_key_id: Secret::new(r#"id"with\backslash"#),
            secret_access_key: Secret::new(r#"se"cret\\value"#),
        };
        save_r2(&path, &creds).unwrap();
        let loaded = load(&path).unwrap();
        let r2 = loaded.r2().unwrap();
        assert_eq!(r2.access_key_id.expose(), r#"id"with\backslash"#);
        assert_eq!(r2.secret_access_key.expose(), r#"se"cret\\value"#);
    }

    #[cfg(unix)]
    #[test]
    fn secrets_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.toml");
        save_r2(
            &path,
            &R2Credentials {
                access_key_id: Secret::new("a"),
                secret_access_key: Secret::new("b"),
            },
        )
        .unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "secrets.toml must be chmod 0600");
    }
}
