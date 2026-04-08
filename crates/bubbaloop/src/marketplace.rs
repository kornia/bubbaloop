//! Shared marketplace download logic for precompiled node binaries.
//!
//! Extracts reusable helpers from `cli/node.rs` so both CLI and MCP
//! can install nodes from the official marketplace registry.

use crate::registry::{self, find_curl, RegistryNode};
use std::path::Path;
use std::process::Command;

/// Errors from marketplace download operations.
#[derive(Debug, thiserror::Error)]
pub enum MarketplaceError {
    #[error("Unsupported architecture: {0}")]
    UnsupportedArch(String),
    #[error("curl not found in standard paths (/usr/bin, /usr/local/bin, /bin)")]
    CurlNotFound,
    #[error("Download failed: {0}")]
    DownloadFailed(String),
    #[error("Checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },
    #[error("Node '{0}' not found in marketplace registry")]
    NodeNotFound(String),
    #[error("No precompiled binary available for '{0}'")]
    NoBinary(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, MarketplaceError>;

/// Detect the current CPU architecture and return the GitHub release arch suffix.
pub fn detect_arch() -> Result<&'static str> {
    match std::env::consts::ARCH {
        "x86_64" => Ok("amd64"),
        "aarch64" => Ok("arm64"),
        other => Err(MarketplaceError::UnsupportedArch(other.to_string())),
    }
}

/// Download a file from a URL to a local path using curl.
pub fn download_file(url: &str, dest: &Path) -> Result<()> {
    let curl = find_curl().ok_or(MarketplaceError::CurlNotFound)?;

    let output = Command::new(curl)
        .args([
            "-sSfL",
            "--connect-timeout",
            "10",
            "--max-time",
            "300",
            "-o",
            &dest.to_string_lossy(),
            "--",
            url,
        ])
        .output()?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(MarketplaceError::DownloadFailed(stderr.to_string()))
    }
}

/// Download a small text file (e.g., checksum) and return its contents.
pub fn download_text(url: &str) -> Result<String> {
    let curl = find_curl().ok_or(MarketplaceError::CurlNotFound)?;

    let output = Command::new(curl)
        .args([
            "-sSfL",
            "--connect-timeout",
            "10",
            "--max-time",
            "30",
            "--",
            url,
        ])
        .output()?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(MarketplaceError::DownloadFailed(stderr.to_string()))
    }
}

/// Find sha256sum or shasum in standard system paths.
///
/// Searches `/usr/bin/sha256sum`, `/usr/local/bin/sha256sum`, `/usr/bin/shasum`
/// in order, returning the first existing path.
pub fn find_sha256sum() -> Option<(std::path::PathBuf, bool)> {
    // (path, is_shasum) — shasum needs `-a 256` flag
    for (dir, binary, is_shasum) in &[
        ("/usr/bin", "sha256sum", false),
        ("/usr/local/bin", "sha256sum", false),
        ("/usr/bin", "shasum", true),
        ("/usr/local/bin", "shasum", true),
    ] {
        let path = Path::new(dir).join(binary);
        if path.exists() {
            return Some((path, *is_shasum));
        }
    }
    None
}

/// Verify that a file matches an expected SHA256 checksum.
///
/// The `expected` string should be in the format output by `sha256sum`:
/// `<hex_hash>  <filename>\n` or just `<hex_hash>`.
pub fn verify_sha256(path: &Path, expected: &str) -> Result<()> {
    let (sha_bin, is_shasum) = find_sha256sum().ok_or_else(|| {
        MarketplaceError::DownloadFailed(
            "sha256sum/shasum not found in standard paths (/usr/bin, /usr/local/bin)".to_string(),
        )
    })?;

    let mut cmd = Command::new(sha_bin);
    if is_shasum {
        cmd.args(["-a", "256"]);
    }
    cmd.arg(path);

    let output = cmd.output().map_err(|e| {
        MarketplaceError::DownloadFailed(format!("sha256sum failed to execute: {}", e))
    })?;

    if !output.status.success() {
        return Err(MarketplaceError::DownloadFailed(
            "sha256sum failed".to_string(),
        ));
    }

    let actual_line = String::from_utf8_lossy(&output.stdout);
    let actual_hash = actual_line.split_whitespace().next().unwrap_or("");
    let expected_hash = expected.split_whitespace().next().unwrap_or("");

    if actual_hash == expected_hash {
        Ok(())
    } else {
        Err(MarketplaceError::ChecksumMismatch {
            expected: expected_hash.to_string(),
            actual: actual_hash.to_string(),
        })
    }
}

/// Set a file as executable (chmod 755).
#[cfg(unix)]
pub fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::Permissions::from_mode(0o755);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

/// Set a file as executable (no-op on non-Unix).
#[cfg(not(unix))]
pub fn set_executable(_path: &Path) -> Result<()> {
    Ok(())
}

/// Download a precompiled binary for a registry node.
///
/// Returns the node directory path on success. The caller can then
/// register this path with the daemon via `AddNode`.
pub fn download_precompiled(entry: &RegistryNode) -> Result<String> {
    let binary_name = entry
        .binary
        .as_ref()
        .ok_or_else(|| MarketplaceError::NoBinary(entry.name.clone()))?;

    if entry.node_type != "rust" {
        return Err(MarketplaceError::NoBinary(format!(
            "{} (type: {}, only rust nodes have precompiled binaries)",
            entry.name, entry.node_type
        )));
    }

    let arch = detect_arch()?;

    let url = registry::precompiled_url(entry, arch)
        .ok_or_else(|| MarketplaceError::NoBinary(entry.name.clone()))?;
    let checksum_url = format!("{}.sha256", url);

    // Create node directory: ~/.bubbaloop/nodes/<repo-name>/<subdir>/
    let repo_name = entry
        .repo
        .rsplit('/')
        .next()
        .unwrap_or("bubbaloop-nodes-official");
    let node_dir = crate::daemon::registry::get_bubbaloop_home()
        .join("nodes")
        .join(repo_name)
        .join(&entry.subdir);
    let binary_dir = node_dir.join("target").join("release");
    std::fs::create_dir_all(&binary_dir)?;

    // Write a minimal node.yaml so the daemon can read it
    let node_yaml = format!(
        "name: {}\nversion: {}\ntype: {}\ndescription: \"{}\"\ncommand: \"./target/release/{}\"\n",
        entry.name, entry.version, entry.node_type, entry.description, binary_name
    );
    std::fs::write(node_dir.join("node.yaml"), node_yaml)?;

    // Download checksum first (fast fail if release doesn't exist)
    log::info!("Downloading checksum from {}", checksum_url);
    let expected_checksum = download_text(&checksum_url)?;

    // Download binary to a temp file, verify, then atomically rename
    let binary_path = binary_dir.join(binary_name);
    let tmp_path = binary_dir.join(format!("{}.tmp", binary_name));
    log::info!("Downloading binary from {}", url);
    download_file(&url, &tmp_path)?;

    // Verify checksum on the temp file before moving into place
    if let Err(e) = verify_sha256(&tmp_path, &expected_checksum) {
        // Clean up the temp file on checksum failure
        let _ = std::fs::remove_file(&tmp_path);
        return Err(e);
    }

    // Atomic rename to final path
    std::fs::rename(&tmp_path, &binary_path)?;

    // Make executable
    set_executable(&binary_path)?;

    Ok(node_dir.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_arch_returns_known_value() {
        let result = detect_arch();
        // On any supported platform this should succeed
        match std::env::consts::ARCH {
            "x86_64" => assert_eq!(result.unwrap(), "amd64"),
            "aarch64" => assert_eq!(result.unwrap(), "arm64"),
            _ => assert!(result.is_err()),
        }
    }

    #[test]
    fn find_curl_returns_existing_path() {
        // curl should be available on most Linux systems
        if let Some(path) = find_curl() {
            assert!(path.exists());
            assert!(path.to_string_lossy().contains("curl"));
        }
    }

    #[test]
    fn marketplace_error_display() {
        let err = MarketplaceError::CurlNotFound;
        assert!(err.to_string().contains("curl not found"));

        let err = MarketplaceError::NodeNotFound("foo".to_string());
        assert!(err.to_string().contains("foo"));

        let err = MarketplaceError::ChecksumMismatch {
            expected: "abc".to_string(),
            actual: "def".to_string(),
        };
        assert!(err.to_string().contains("abc"));
        assert!(err.to_string().contains("def"));
    }

    #[test]
    fn marketplace_error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<MarketplaceError>();
    }
}
