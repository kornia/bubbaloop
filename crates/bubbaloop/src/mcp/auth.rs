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

/// Load or generate the MCP authentication token.
pub fn load_or_generate_token() -> Result<String, std::io::Error> {
    let path = token_path();
    if path.exists() {
        let token = std::fs::read_to_string(&path)?.trim().to_string();
        if !token.is_empty() {
            return Ok(token);
        }
    }

    // Generate a new token
    let token = format!("bb_{}", uuid::Uuid::new_v4().as_simple());

    // Ensure directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(&path, &token)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }

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
        let token = format!("bb_{}", uuid::Uuid::new_v4().as_simple());
        std::fs::write(&path, &token).unwrap();
        let loaded = std::fs::read_to_string(&path).unwrap().trim().to_string();
        assert_eq!(loaded, token);
        assert!(loaded.starts_with("bb_"));
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
