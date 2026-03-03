//! Node install, uninstall, and related download/clone helpers.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::{send_command, InstallArgs, NodeError, Result};
use crate::registry;

/// Copy canonical header.proto to a node's protos/ directory if it exists.
/// This ensures nodes use the correct version of header.proto.
pub(crate) fn copy_canonical_header_proto(node_path: &Path) {
    let protos_dir = node_path.join("protos");
    if !protos_dir.exists() {
        return;
    }

    let dest = protos_dir.join("header.proto");
    if let Err(e) = std::fs::write(&dest, crate::HEADER_PROTO) {
        log::warn!(
            "Could not copy canonical header.proto to {}: {}",
            dest.display(),
            e
        );
    } else {
        log::info!("Copied canonical header.proto to {}", dest.display());
    }
}

pub(crate) fn normalize_git_url(source: &str) -> String {
    // If it's an existing local path, return it unchanged
    if std::path::Path::new(source).exists() {
        return source.to_string();
    }
    if source.starts_with("https://") || source.starts_with("git@") {
        source.to_string()
    } else if source.starts_with("github.com/") {
        format!("https://{}", source)
    } else if source.contains('/')
        && !source.contains(':')
        && !source.starts_with('/')
        && !source.starts_with('.')
    {
        // Shorthand: user/repo -> https://github.com/user/repo
        format!("https://github.com/{}", source)
    } else {
        source.to_string()
    }
}

pub(crate) fn is_git_url(source: &str) -> bool {
    source.starts_with("https://github.com/")
        || source.starts_with("git@github.com:")
        || (source.contains("github.com/") && !source.starts_with('/') && !source.starts_with('.'))
}

pub(crate) fn extract_node_name(path: &str) -> Result<String> {
    let node_yaml = Path::new(path).join("node.yaml");
    if node_yaml.exists() {
        let content = std::fs::read_to_string(&node_yaml)?;
        let manifest: serde_yaml::Value =
            serde_yaml::from_str(&content).map_err(|e| NodeError::CommandFailed(e.to_string()))?;
        if let Some(name) = manifest.get("name").and_then(|v| v.as_str()) {
            return Ok(name.to_string());
        }
    }
    // Fallback to directory name
    Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .ok_or_else(|| NodeError::CommandFailed("Cannot extract node name".into()))
}

pub(crate) fn clone_from_github(url: &str, output: Option<&str>, branch: &str) -> Result<String> {
    // Prevent argument injection via branch or URL starting with '-'
    if branch.starts_with('-') {
        return Err(NodeError::InvalidUrl(format!(
            "Invalid branch name: {}",
            branch
        )));
    }
    if url.starts_with('-') {
        return Err(NodeError::InvalidUrl(format!("Invalid URL: {}", url)));
    }

    // Extract repo name from URL
    let repo_name = url
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .rsplit('/')
        .next()
        .ok_or_else(|| NodeError::InvalidUrl(url.to_string()))?;

    // Determine target directory
    let target_dir = if let Some(out) = output {
        PathBuf::from(out)
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        PathBuf::from(home)
            .join(".bubbaloop")
            .join("nodes")
            .join(repo_name)
    };

    if target_dir.exists() {
        // Reuse existing clone (e.g., installing a second node from the same multi-node repo)
        println!("Using existing clone at {}", target_dir.display());
    } else {
        // Create parent directory
        if let Some(parent) = target_dir.parent() {
            std::fs::create_dir_all(parent)?;
        }

        println!(
            "Cloning {} (branch: {}) to {}...",
            url,
            branch,
            target_dir.display()
        );

        // Clone the repository with branch
        let clone_output = Command::new("git")
            .args([
                "clone",
                "--depth",
                "1",
                "--branch",
                branch,
                "--", // Prevent URL from being treated as an option
                url,
                &target_dir.to_string_lossy(),
            ])
            .output()?;

        if !clone_output.status.success() {
            let stderr = String::from_utf8_lossy(&clone_output.stderr);
            return Err(NodeError::GitClone(stderr.to_string()));
        }

        println!("Cloned successfully!");
    }

    // Check for node.yaml
    let manifest = target_dir.join("node.yaml");
    if !manifest.exists() {
        eprintln!("Warning: No node.yaml found in repository. You may need to create one.");
    }

    Ok(target_dir.to_string_lossy().to_string())
}

/// Attempt to download a precompiled binary for a registry node.
///
/// Returns the node directory path on success. On failure, the caller should
/// fall back to cloning and building from source.
///
/// Delegates to `crate::marketplace::download_precompiled` for the actual
/// download logic (shared with MCP).
pub(crate) fn try_download_precompiled(entry: &registry::RegistryNode) -> Result<String> {
    println!("Downloading precompiled binary...");
    crate::marketplace::download_precompiled(entry)
        .map_err(|e| NodeError::CommandFailed(e.to_string()))
}

/// Handle `node install`: if the node is already registered with the daemon,
/// install it as a systemd service (existing behavior). Otherwise, look up the
/// name in the marketplace registry, clone, register, build, and install.
pub(crate) async fn handle_install(args: InstallArgs) -> Result<()> {
    // First, check if node is already registered with the daemon via REST API
    let client = crate::cli::daemon_client::DaemonClient::new();

    let is_registered = match client.list_nodes().await {
        Ok(data) => data.nodes.iter().any(|n| n.name == args.name),
        Err(_) => false,
    };

    if is_registered {
        log::info!(
            "node install: '{}' is registered, installing systemd service",
            args.name
        );
        return send_command(&args.name, "install").await;
    }

    // Not registered -> try marketplace lookup
    log::info!(
        "node install: '{}' not registered, checking marketplace",
        args.name
    );
    println!(
        "Node '{}' not registered. Checking marketplace...",
        args.name
    );

    if let Err(e) = registry::refresh_cache() {
        log::warn!("registry refresh failed: {}", e);
        eprintln!("Warning: could not refresh registry (using cache): {}", e);
    }
    let nodes = registry::load_cached_registry();

    let entry = match registry::find_by_name(&nodes, &args.name) {
        Some(entry) => entry,
        None => {
            // Search for suggestions
            let suggestions = registry::search_registry(&nodes, &args.name, None, None);
            let mut msg = format!("Node '{}' not found in registry.", args.name);
            if !suggestions.is_empty() {
                msg.push_str("\n\nDid you mean:");
                for s in suggestions.iter().take(5) {
                    msg.push_str(&format!("\n  {}", s.name));
                }
            }
            msg.push_str("\n\nTry: bubbaloop node search");
            return Err(NodeError::NotFound(msg));
        }
    };

    log::info!(
        "node install: found '{}' in marketplace (repo={}, subdir={})",
        entry.name,
        entry.repo,
        entry.subdir
    );
    println!("Found '{}' in marketplace ({})", entry.name, entry.repo);

    // Validate repo before constructing URL
    registry::validate_repo(&entry.repo)
        .map_err(|e| NodeError::InvalidUrl(format!("Invalid registry repo: {}", e)))?;

    // Try precompiled binary first (fast path)
    match try_download_precompiled(&entry) {
        Ok(node_path) => {
            println!("Downloaded precompiled binary for '{}'", args.name);

            // Register with daemon via REST API
            let resp = client.add_node(&node_path, None, None).await?;
            if !resp.success {
                return Err(NodeError::CommandFailed(
                    "Failed to register node with daemon".into(),
                ));
            }

            println!("Registered node: {}", args.name);

            // Install as systemd service
            println!("Installing {} as systemd service...", args.name);
            send_command(&args.name, "install").await?;

            log::info!(
                "node install: completed precompiled install of '{}' from {}",
                args.name,
                entry.repo
            );
            println!(
                "\nInstalled '{}' from {} (precompiled)",
                args.name, entry.repo
            );
            return Ok(());
        }
        Err(e) => {
            log::info!(
                "Precompiled binary not available ({}), building from source",
                e
            );
            println!("Precompiled binary not available, building from source...");
        }
    }

    // Clone from GitHub
    let url = format!("https://github.com/{}", entry.repo);
    log::info!("node install: cloning {} branch={}", url, args.branch);
    let base_path = clone_from_github(&url, None, &args.branch)?;

    // Resolve subdir
    let node_path = super::resolve_node_path(&base_path, Some(&entry.subdir))?;

    // Copy canonical header.proto if protos/ directory exists
    copy_canonical_header_proto(Path::new(&node_path));

    // Register with daemon via REST API
    let resp = client.add_node(&node_path, None, None).await?;
    if !resp.success {
        return Err(NodeError::CommandFailed(
            "Failed to register node with daemon".into(),
        ));
    }

    log::info!("node install: registered '{}' with daemon", args.name);
    println!("Registered node: {}", args.name);

    // Build only if --build is passed
    if args.build {
        println!("Building {}...", args.name);
        send_command(&args.name, "build").await?;
    }

    // Install as systemd service
    println!("Installing {} as systemd service...", args.name);
    send_command(&args.name, "install").await?;

    log::info!(
        "node install: completed marketplace install of '{}' from {}",
        args.name,
        entry.repo
    );
    println!("\nInstalled '{}' from {}", args.name, entry.repo);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn test_normalize_git_url_full_https() {
        assert_eq!(
            normalize_git_url("https://github.com/user/repo"),
            "https://github.com/user/repo"
        );
    }

    #[test]
    fn test_normalize_git_url_ssh() {
        assert_eq!(
            normalize_git_url("git@github.com:user/repo.git"),
            "git@github.com:user/repo.git"
        );
    }

    #[test]
    fn test_normalize_git_url_with_github_prefix() {
        assert_eq!(
            normalize_git_url("github.com/user/repo"),
            "https://github.com/user/repo"
        );
    }

    #[test]
    fn test_normalize_git_url_shorthand() {
        assert_eq!(
            normalize_git_url("user/repo"),
            "https://github.com/user/repo"
        );
    }

    #[test]
    fn test_normalize_git_url_local_path() {
        assert_eq!(normalize_git_url("/path/to/node"), "/path/to/node");
    }

    #[test]
    fn test_normalize_git_url_relative_path() {
        // Relative paths starting with . should be preserved as local paths
        assert_eq!(normalize_git_url("./node"), "./node");
        assert_eq!(normalize_git_url("../my-node"), "../my-node");
        assert_eq!(normalize_git_url("./path/to/node"), "./path/to/node");
    }

    #[test]
    fn test_is_git_url_https() {
        assert!(is_git_url("https://github.com/user/repo"));
    }

    #[test]
    fn test_is_git_url_ssh() {
        assert!(is_git_url("git@github.com:user/repo.git"));
    }

    #[test]
    fn test_is_git_url_with_prefix() {
        assert!(is_git_url("github.com/user/repo"));
    }

    #[test]
    fn test_is_git_url_local_path() {
        assert!(!is_git_url("/path/to/node"));
    }

    #[test]
    fn test_is_git_url_relative_path() {
        assert!(!is_git_url("./node"));
    }

    #[test]
    fn test_clone_rejects_branch_argument_injection() {
        // Branch starting with '-' could be interpreted as a git flag
        let result = clone_from_github("https://github.com/user/repo", None, "--upload-pack=evil");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Invalid branch name"));
    }

    #[test]
    fn test_clone_rejects_url_argument_injection() {
        let result = clone_from_github("--upload-pack=evil", None, "main");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Invalid URL"));
    }

    #[test]
    fn test_clone_accepts_valid_branch() {
        // This will fail at the git clone step (no network), but should not
        // fail at the argument validation step. We check by verifying the error
        // is NOT about an invalid branch/URL.
        let result = clone_from_github(
            "https://github.com/user/repo",
            Some("/tmp/bubbaloop-test-nonexistent"),
            "main",
        );
        // Either succeeds or fails for a reason other than argument injection
        if let Err(e) = result {
            let msg = e.to_string();
            assert!(!msg.contains("Invalid branch name"));
            assert!(!msg.contains("Invalid URL"));
        }
    }

    #[test]
    fn test_detect_arch() {
        let arch = crate::marketplace::detect_arch();
        assert!(arch.is_ok());
        let arch = arch.unwrap();
        #[cfg(target_arch = "x86_64")]
        assert_eq!(arch, "amd64");
        #[cfg(target_arch = "aarch64")]
        assert_eq!(arch, "arm64");
    }

    #[test]
    fn test_precompiled_url_construction() {
        let entry = crate::registry::RegistryNode {
            name: "system-telemetry".into(),
            version: "0.1.0".into(),
            node_type: "rust".into(),
            description: "System metrics".into(),
            category: "monitoring".into(),
            tags: vec![],
            repo: "kornia/bubbaloop-nodes-official".into(),
            subdir: "system-telemetry".into(),
            binary: Some("system_telemetry_node".into()),
        };

        let url = crate::registry::precompiled_url(&entry, "arm64");
        assert_eq!(
            url.as_deref(),
            Some("https://github.com/kornia/bubbaloop-nodes-official/releases/latest/download/system-telemetry-linux-arm64")
        );
    }

    #[test]
    fn test_precompiled_url_python_returns_none() {
        let entry = crate::registry::RegistryNode {
            name: "network-monitor".into(),
            version: "0.1.0".into(),
            node_type: "python".into(),
            description: "Network monitor".into(),
            category: "monitoring".into(),
            tags: vec![],
            repo: "kornia/bubbaloop-nodes-official".into(),
            subdir: "network-monitor".into(),
            binary: None,
        };

        assert!(crate::registry::precompiled_url(&entry, "arm64").is_none());
    }

    #[test]
    fn test_verify_sha256_valid() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test_binary");
        std::fs::write(&file_path, b"hello world\n").unwrap();

        // Compute expected checksum
        let output = Command::new("sha256sum").arg(&file_path).output().unwrap();
        let expected = String::from_utf8_lossy(&output.stdout).to_string();

        let result = crate::marketplace::verify_sha256(&file_path, &expected);
        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_sha256_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test_binary");
        std::fs::write(&file_path, b"hello world\n").unwrap();

        let wrong_checksum =
            "0000000000000000000000000000000000000000000000000000000000000000  test_binary";
        let result = crate::marketplace::verify_sha256(&file_path, wrong_checksum);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Checksum mismatch"));
    }

    #[test]
    fn test_set_executable() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test_bin");
        std::fs::write(&file_path, b"#!/bin/sh\necho hi").unwrap();

        crate::marketplace::set_executable(&file_path).unwrap();
        let perms = std::fs::metadata(&file_path).unwrap().permissions();
        assert_eq!(perms.mode() & 0o777, 0o755);
    }

    #[test]
    fn test_try_download_precompiled_no_binary_field() {
        let entry = crate::registry::RegistryNode {
            name: "test".into(),
            version: "0.1.0".into(),
            node_type: "rust".into(),
            description: "test".into(),
            category: "test".into(),
            tags: vec![],
            repo: "user/repo".into(),
            subdir: "test".into(),
            binary: None,
        };
        let result = try_download_precompiled(&entry);
        assert!(result.is_err());
        // MarketplaceError::NoBinary: "No precompiled binary available for 'test'"
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No precompiled binary"));
    }

    #[test]
    fn test_try_download_precompiled_python_node() {
        let entry = crate::registry::RegistryNode {
            name: "test".into(),
            version: "0.1.0".into(),
            node_type: "python".into(),
            description: "test".into(),
            category: "test".into(),
            tags: vec![],
            repo: "user/repo".into(),
            subdir: "test".into(),
            binary: Some("test_bin".into()),
        };
        let result = try_download_precompiled(&entry);
        assert!(result.is_err());
        // MarketplaceError::NoBinary includes type info: "only rust nodes have precompiled binaries"
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("precompiled binary"));
    }

    #[test]
    fn test_copy_canonical_header_proto_creates_file() {
        use tempfile::tempdir;

        let tmp = tempdir().unwrap();
        let node_path = tmp.path();
        let protos_dir = node_path.join("protos");
        std::fs::create_dir_all(&protos_dir).unwrap();

        // Call the function
        copy_canonical_header_proto(node_path);

        // Verify header.proto was created
        let header_path = protos_dir.join("header.proto");
        assert!(header_path.exists(), "header.proto should be created");

        // Verify content matches the embedded canonical version
        let written = std::fs::read(&header_path).unwrap();
        assert_eq!(
            written,
            crate::HEADER_PROTO,
            "Written header.proto should match canonical version"
        );
    }

    #[test]
    fn test_copy_canonical_header_proto_no_protos_dir() {
        use tempfile::tempdir;

        let tmp = tempdir().unwrap();
        let node_path = tmp.path();

        // Don't create protos/ dir
        copy_canonical_header_proto(node_path);

        // Should not panic, just silently skip
        let header_path = node_path.join("protos").join("header.proto");
        assert!(
            !header_path.exists(),
            "Should not create header.proto without protos/ dir"
        );
    }

    #[test]
    fn test_copy_canonical_header_proto_overwrites_existing() {
        use tempfile::tempdir;

        let tmp = tempdir().unwrap();
        let node_path = tmp.path();
        let protos_dir = node_path.join("protos");
        std::fs::create_dir_all(&protos_dir).unwrap();

        // Create an old/different header.proto
        let header_path = protos_dir.join("header.proto");
        std::fs::write(&header_path, b"old header content").unwrap();

        // Call the function
        copy_canonical_header_proto(node_path);

        // Verify it was overwritten with canonical version
        let written = std::fs::read(&header_path).unwrap();
        assert_eq!(
            written,
            crate::HEADER_PROTO,
            "Should overwrite old header.proto with canonical version"
        );
        assert_ne!(written, b"old header content");
    }
}
