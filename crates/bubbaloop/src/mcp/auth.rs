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
pub fn load_or_generate_token() -> Result<String, std::io::Error> {
    load_or_generate_token_at(&token_path())
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
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)?;
        write!(file, "{}", token)?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, &token)?;
    }

    // SECURITY: Never log the token value itself
    log::info!("Generated MCP token at {:?}", path);
    Ok(token)
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
