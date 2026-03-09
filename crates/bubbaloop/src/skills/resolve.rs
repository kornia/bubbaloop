//! Driver resolution cascade for `bubbaloop up`.

use std::path::{Path, PathBuf};

use crate::skills::{resolve_driver, DriverKind, SkillConfig};

/// Resolved execution strategy for a skill.
#[derive(Debug, Clone, PartialEq)]
pub enum DriverResolution {
    /// Runs as a tokio task inside the daemon — no binary needed.
    BuiltIn,
    /// A local binary already exists at this path.
    LocalBinary(PathBuf),
    /// Binary needs to be downloaded from the marketplace.
    MarketplaceDownload(String),
}

/// Resolve how a skill should be executed.
///
/// Returns `None` if the driver is unknown.
pub fn resolve(skill: &SkillConfig, nodes_dir: &Path) -> Option<DriverResolution> {
    let entry = resolve_driver(&skill.driver)?;
    if entry.kind == DriverKind::BuiltIn {
        return Some(DriverResolution::BuiltIn);
    }
    let node_name = entry.marketplace_node?;
    if let Some(path) = find_installed_node(node_name, nodes_dir) {
        return Some(DriverResolution::LocalBinary(path));
    }
    Some(DriverResolution::MarketplaceDownload(node_name.to_string()))
}

fn find_installed_node(node_name: &str, nodes_dir: &Path) -> Option<PathBuf> {
    if !nodes_dir.exists() {
        return None;
    }
    for repo_entry in std::fs::read_dir(nodes_dir).ok()?.flatten() {
        let repo_path = repo_entry.path();
        if !repo_path.is_dir() {
            continue;
        }
        for node_entry in std::fs::read_dir(&repo_path).ok()?.flatten() {
            if node_entry.file_name().to_string_lossy() == node_name {
                return Some(node_entry.path());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::SkillConfig;
    use std::collections::HashMap;
    use tempfile::tempdir;

    fn skill(driver: &str) -> SkillConfig {
        SkillConfig {
            name: "test".into(),
            driver: driver.into(),
            enabled: true,
            config: HashMap::new(),
            intent: String::new(),
            on: vec![],
            schedule: None,
            actions: vec![],
        }
    }

    #[test]
    fn builtin_resolves_without_filesystem() {
        assert_eq!(
            resolve(&skill("http-poll"), Path::new("/nonexistent")),
            Some(DriverResolution::BuiltIn)
        );
        assert_eq!(
            resolve(&skill("system"), Path::new("/nonexistent")),
            Some(DriverResolution::BuiltIn)
        );
    }

    #[test]
    fn all_builtin_drivers_resolve_builtin() {
        for d in &["http-poll", "system", "exec", "webhook", "tcp-listen"] {
            assert_eq!(
                resolve(&skill(d), Path::new("/nonexistent")),
                Some(DriverResolution::BuiltIn),
                "driver: {}",
                d
            );
        }
    }

    #[test]
    fn marketplace_no_local_resolves_download() {
        let dir = tempdir().unwrap();
        assert_eq!(
            resolve(&skill("rtsp"), dir.path()),
            Some(DriverResolution::MarketplaceDownload("rtsp-camera".into()))
        );
    }

    #[test]
    fn marketplace_with_local_binary_resolves_local() {
        let dir = tempdir().unwrap();
        let node_path = dir
            .path()
            .join("bubbaloop-nodes-official")
            .join("rtsp-camera");
        std::fs::create_dir_all(&node_path).unwrap();
        assert_eq!(
            resolve(&skill("rtsp"), dir.path()),
            Some(DriverResolution::LocalBinary(node_path))
        );
    }

    #[test]
    fn unknown_driver_resolves_none() {
        assert_eq!(resolve(&skill("not-a-driver"), Path::new("/x")), None);
    }
}
