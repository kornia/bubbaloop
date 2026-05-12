//! MCP bearer token authentication.
//!
//! Token is auto-generated on first daemon start and stored in
//! `~/.bubbaloop/mcp-token` with 0600 permissions.

use std::path::PathBuf;

/// Path to the MCP authentication token.
pub fn token_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".bubbaloop")
        .join("mcp-token")
}

/// Load or generate the MCP authentication token from the default path.
///
/// Thread-safe: concurrent callers within the same process always receive the
/// same token without racing to the filesystem.
pub fn load_or_generate_token() -> Result<String, std::io::Error> {
    static CACHED: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    if let Some(token) = CACHED.get() {
        return Ok(token.clone());
    }
    let token = load_or_generate_token_at(&token_path())?;
    // First setter wins; if another thread beat us, CACHED already holds their
    // token (which equals ours — both read from the same file).
    let _ = CACHED.set(token);
    Ok(CACHED.get().unwrap().clone())
}

/// Load or generate the MCP authentication token at the given path.
pub fn load_or_generate_token_at(path: &std::path::Path) -> Result<String, std::io::Error> {
    match std::fs::read_to_string(path) {
        Ok(content) => {
            let token = content.trim().to_string();
            if !token.is_empty() {
                return Ok(token);
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }

    // Generate a new token
    let token = format!("bb_{}", uuid::Uuid::new_v4().as_simple());

    // Ensure directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
        {
            Ok(mut file) => write!(file, "{}", token)?,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                // Another caller created the file first. Spin-read until their
                // write completes — the window between O_EXCL create and write()
                // is tiny but real under thread contention.
                return wait_for_token(path);
            }
            Err(e) => return Err(e),
        }
    }
    #[cfg(not(unix))]
    {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
        {
            Ok(mut file) => {
                use std::io::Write;
                write!(file, "{}", token)?;
                // Restrict permissions on Windows: remove inherited ACEs and grant
                // only the current user full control. Uses icacls which is available
                // on all supported Windows versions (Vista+).
                #[cfg(windows)]
                {
                    let path_str = path.to_string_lossy();
                    if let Ok(username) = std::env::var("USERNAME") {
                        let icacls_result = std::process::Command::new("icacls")
                            .args([
                                path_str.as_ref(),
                                "/inheritance:r",
                                "/grant:r",
                                &format!("{}:F", username),
                            ])
                            .output();
                        match icacls_result {
                            Ok(output) if output.status.success() => {
                                log::debug!("Restricted token file permissions via icacls");
                            }
                            Ok(output) => {
                                log::warn!(
                                    "icacls failed to restrict token file permissions: {}",
                                    String::from_utf8_lossy(&output.stderr)
                                );
                            }
                            Err(e) => {
                                log::warn!(
                                    "Failed to run icacls to restrict token file permissions: {}",
                                    e
                                );
                            }
                        }
                    } else {
                        log::warn!(
                            "USERNAME env var not set — cannot restrict token file permissions on Windows"
                        );
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                return wait_for_token(path);
            }
            Err(e) => return Err(e),
        }
    }

    // SECURITY: Never log the token value itself
    log::info!("Generated MCP token at {:?}", path);
    Ok(token)
}

/// Spin-read `path` until it contains a non-empty token.
///
/// Called after a concurrent `create_new` returns `AlreadyExists`. The winning
/// writer holds the file open between `O_EXCL` and `write()`, so the file can
/// briefly be empty. 20 × 5 ms = 100 ms max — ample for a 35-byte local write.
fn wait_for_token(path: &std::path::Path) -> Result<String, std::io::Error> {
    for _ in 0..20 {
        let content = std::fs::read_to_string(path)?;
        let existing = content.trim().to_string();
        if !existing.is_empty() {
            return Ok(existing);
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::TimedOut,
        "timed out waiting for concurrent token write to complete",
    ))
}

/// Validate a bearer token from an Authorization header.
pub fn validate_token(header_value: &str, expected: &str) -> bool {
    let token = header_value.strip_prefix("Bearer ").unwrap_or(header_value);
    constant_time_eq(token.as_bytes(), expected.as_bytes())
}

/// Constant-time byte comparison.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    // NOTE: Length comparison is not constant-time. This leaks the token length,
    // which is acceptable because the token format (bb_ + UUID = 35 chars) is
    // publicly known. For truly secret-length tokens, use the `subtle` crate.
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_path() {
        let path = token_path();
        assert!(path.to_string_lossy().contains(".bubbaloop"));
        assert!(path.to_string_lossy().ends_with("mcp-token"));
    }

    #[test]
    fn test_load_or_generate_creates_token() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp-token");
        let token = load_or_generate_token_at(&path).unwrap();
        assert!(token.starts_with("bb_"));
        assert_eq!(token.len(), 35); // "bb_" + 32 hex chars
                                     // Verify file was created and is readable
        let loaded = load_or_generate_token_at(&path).unwrap();
        assert_eq!(loaded, token); // Same token on second load
    }

    #[test]
    fn test_concurrent_token_generation() {
        let dir = tempfile::tempdir().unwrap();
        let path = std::sync::Arc::new(dir.path().join("mcp-token"));
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let p = path.clone();
                std::thread::spawn(move || load_or_generate_token_at(&p))
            })
            .collect();
        let tokens: Vec<String> = handles
            .into_iter()
            .map(|h| h.join().unwrap().unwrap())
            .collect();
        // Every thread must get the same token regardless of which one won the race
        assert!(tokens.windows(2).all(|w| w[0] == w[1]));
        assert!(tokens[0].starts_with("bb_"));
    }

    #[cfg(unix)]
    #[test]
    fn test_token_file_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp-token");
        let _token = load_or_generate_token_at(&path).unwrap();
        let perms = std::fs::metadata(&path).unwrap().permissions();
        assert_eq!(perms.mode() & 0o777, 0o600);
    }

    #[test]
    fn test_validate_token_with_bearer_prefix() {
        assert!(validate_token("Bearer bb_abc123", "bb_abc123"));
    }

    #[test]
    fn test_validate_token_without_prefix() {
        assert!(validate_token("bb_abc123", "bb_abc123"));
    }

    #[test]
    fn test_validate_token_incorrect() {
        assert!(!validate_token("Bearer bb_wrong", "bb_abc123"));
        assert!(!validate_token("", "bb_abc123"));
    }

    #[test]
    fn test_constant_time_eq() {
        assert!(constant_time_eq(b"hello", b"hello"));
        assert!(!constant_time_eq(b"hello", b"world"));
        assert!(!constant_time_eq(b"short", b"longer"));
    }
}
