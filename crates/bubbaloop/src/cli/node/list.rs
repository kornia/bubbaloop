//! Node listing, search, and discovery CLI commands.

use super::{truncate, Result};
use crate::registry;
use std::path::Path;

pub(crate) async fn list_nodes(format: &str, _base: bool, _instances: bool) -> Result<()> {
    let client = crate::cli::daemon_client::DaemonClient::connect().await?;
    let result = client.list_nodes().await?;

    if format == "json" {
        println!("{}", result);
    } else if result.contains("[]") || result.is_empty() {
        println!("No nodes registered. Use 'bubbaloop node add <path>' to add one.");
    } else {
        // Parse the JSON node list from the gateway response
        let nodes: Vec<crate::mcp::platform::NodeInfo> =
            serde_json::from_str(&result).map_err(|e| {
                super::NodeError::CommandFailed(format!("Invalid daemon response: {}", e))
            })?;
        if nodes.is_empty() {
            println!("No nodes registered. Use 'bubbaloop node add <path>' to add one.");
        } else {
            println!(
                "{:<20} {:<10} {:<12} {:<8} HEALTH",
                "NAME", "STATUS", "TYPE", "BUILT"
            );
            println!("{}", "-".repeat(70));
            for node in &nodes {
                let built = if node.is_built { "yes" } else { "no" };
                println!(
                    "{:<20} {:<10} {:<12} {:<8} {}",
                    node.name, node.status, node.node_type, built, node.health,
                );
            }
        }
    }

    Ok(())
}

pub(crate) fn search_nodes(query: &str, category: Option<&str>, tag: Option<&str>) -> Result<()> {
    log::info!(
        "node search: query={:?} category={:?} tag={:?}",
        query,
        category,
        tag
    );
    println!("Refreshing marketplace registry...");
    if let Err(e) = registry::refresh_cache() {
        log::warn!("registry refresh failed: {}", e);
        eprintln!("Warning: could not refresh registry (using cache): {}", e);
    }
    let all_nodes = registry::load_cached_registry();

    if all_nodes.is_empty() {
        println!("No nodes found in marketplace registry.");
        println!("The registry cache may not have been fetched yet.");
        return Ok(());
    }

    let results = registry::search_registry(&all_nodes, query, category, tag);

    if results.is_empty() {
        println!("No nodes matching your search.");
        if !query.is_empty() || category.is_some() || tag.is_some() {
            println!("Try: bubbaloop node search  (no arguments to list all)");
        }
        return Ok(());
    }

    println!(
        "{:<20} {:<10} {:<8} {:<12} {:<30} REPO",
        "NAME", "VERSION", "TYPE", "CATEGORY", "DESCRIPTION"
    );
    println!("{}", "-".repeat(110));
    for node in &results {
        println!(
            "{:<20} {:<10} {:<8} {:<12} {:<30} {}",
            node.name,
            node.version,
            node.node_type,
            node.category,
            truncate(&node.description, 28),
            node.repo
        );
    }
    println!();
    println!("Install with: bubbaloop node install <name>");

    Ok(())
}

pub(crate) async fn discover_nodes(format: &str) -> Result<()> {
    // Refresh marketplace cache
    if let Err(e) = registry::refresh_cache() {
        log::warn!("registry refresh failed: {}", e);
        eprintln!("Warning: could not refresh registry (using cache): {}", e);
    }
    let all_marketplace = registry::load_cached_registry();

    // Query daemon for registered nodes via Zenoh gateway
    let registered: Vec<crate::mcp::platform::NodeInfo> =
        match crate::cli::daemon_client::DaemonClient::connect().await {
            Ok(client) => match client.list_nodes().await {
                Ok(json) => serde_json::from_str(&json).unwrap_or_else(|e| {
                    log::warn!("Failed to parse daemon node list: {}", e);
                    vec![]
                }),
                Err(_) => vec![],
            },
            Err(_) => vec![],
        };

    if all_marketplace.is_empty() {
        println!(
            "No nodes found in marketplace. The registry cache may not have been fetched yet."
        );
        return Ok(());
    }

    #[derive(serde::Serialize)]
    struct DiscoverEntry {
        name: String,
        version: String,
        node_type: String,
        is_added: bool,
        is_built: bool,
        instance_count: usize,
        repo: String,
        description: String,
    }

    let entries: Vec<DiscoverEntry> = all_marketplace
        .iter()
        .map(|node| {
            let reg = registered.iter().find(|r| r.name == node.name);
            let is_added = reg.is_some();
            let is_built = reg.map(|r| r.is_built).unwrap_or(false);
            let instance_count = 0;
            DiscoverEntry {
                name: node.name.clone(),
                version: node.version.clone(),
                node_type: node.node_type.clone(),
                is_added,
                is_built,
                instance_count,
                repo: node.repo.clone(),
                description: truncate(&node.description, 28),
            }
        })
        .collect();

    if format == "json" {
        println!("{}", serde_json::to_string_pretty(&entries)?);
    } else {
        println!(
            "{:<20} {:<10} {:<8} {:<6} {:<6} {:<5} {:<30} DESCRIPTION",
            "NAME", "VERSION", "TYPE", "ADDED", "BUILT", "INST", "REPO"
        );
        println!("{}", "-".repeat(115));
        for e in &entries {
            let added = if e.is_added { "yes" } else { "-" };
            let built = if e.is_added && e.is_built {
                "yes"
            } else if e.is_added {
                "no"
            } else {
                "-"
            };
            let inst = if e.instance_count > 0 {
                format!("{}", e.instance_count)
            } else {
                "-".to_string()
            };
            println!(
                "{:<20} {:<10} {:<8} {:<6} {:<6} {:<5} {:<30} {}",
                e.name, e.version, e.node_type, added, built, inst, e.repo, e.description
            );
        }
        println!();
        println!("Add with: bubbaloop node add <name>");
        println!("Create instance: bubbaloop node add <path> --name <instance-name>");
    }

    Ok(())
}

/// Discover node.yaml files in immediate subdirectories of a path.
/// Returns Vec<(node_name, subdir_name, node_type)>.
pub(crate) fn discover_nodes_in_subdirs(base_path: &Path) -> Vec<(String, String, String)> {
    let manifest_field = |manifest: &serde_yaml::Value, key: &str| -> String {
        manifest
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string()
    };

    let mut nodes: Vec<_> = std::fs::read_dir(base_path)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|entry| {
            let yaml_path = entry.path().join("node.yaml");
            let content = std::fs::read_to_string(&yaml_path).ok()?;
            match serde_yaml::from_str::<serde_yaml::Value>(&content) {
                Ok(manifest) => {
                    let subdir = entry.file_name().to_string_lossy().to_string();
                    Some((
                        manifest_field(&manifest, "name"),
                        subdir,
                        manifest_field(&manifest, "type"),
                    ))
                }
                Err(e) => {
                    log::warn!("Skipping {}: invalid node.yaml: {}", yaml_path.display(), e);
                    None
                }
            }
        })
        .collect();

    nodes.sort_by(|a, b| a.0.cmp(&b.0));
    nodes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discover_nodes_in_subdirs() {
        let dir = tempfile::tempdir().unwrap();

        for (name, node_type) in &[("sensor", "rust"), ("bridge", "python")] {
            let subdir = dir.path().join(name);
            std::fs::create_dir(&subdir).unwrap();
            std::fs::write(
                subdir.join("node.yaml"),
                format!("name: {}\nversion: \"0.1.0\"\ntype: {}", name, node_type),
            )
            .unwrap();
        }

        let nodes = discover_nodes_in_subdirs(dir.path());
        assert_eq!(nodes.len(), 2);
        // Sorted by name
        assert_eq!(nodes[0].0, "bridge");
        assert_eq!(nodes[0].2, "python");
        assert_eq!(nodes[1].0, "sensor");
        assert_eq!(nodes[1].2, "rust");
    }

    #[test]
    fn test_discover_nodes_ignores_files() {
        let dir = tempfile::tempdir().unwrap();
        // Create a file named "node.yaml" at root (not a subdir)
        std::fs::write(dir.path().join("node.yaml"), "name: root").unwrap();
        // Create a regular file (not a directory)
        std::fs::write(dir.path().join("README.md"), "hello").unwrap();

        let nodes = discover_nodes_in_subdirs(dir.path());
        assert_eq!(nodes.len(), 0); // Only scans directories, not root
    }
}
