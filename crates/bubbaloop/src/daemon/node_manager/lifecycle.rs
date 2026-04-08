//! Node lifecycle operations: start, stop, restart, install, uninstall, autostart.
//!
//! Also includes registry operations: add_node, remove_node.

use super::{NodeManager, NodeManagerError, Result};
use crate::daemon::registry;
use std::sync::Arc;

impl NodeManager {
    /// Start a node
    pub(crate) async fn start_node(self: &Arc<Self>, name: &str) -> Result<String> {
        self.supervisor.start_unit(name).await?;
        self.spawn_refresh_and_emit("started", name);
        Ok(format!("Started {}", name))
    }

    /// Stop a node
    pub(crate) async fn stop_node(self: &Arc<Self>, name: &str) -> Result<String> {
        self.supervisor.stop_unit(name).await?;
        self.spawn_refresh_and_emit("stopped", name);
        Ok(format!("Stopped {}", name))
    }

    /// Restart a node
    pub(crate) async fn restart_node(self: &Arc<Self>, name: &str) -> Result<String> {
        self.supervisor.restart_unit(name).await?;
        self.spawn_refresh_and_emit("restarted", name);
        Ok(format!("Restarted {}", name))
    }

    /// Install a node's service
    pub(crate) async fn install_node(self: &Arc<Self>, name: &str) -> Result<String> {
        // Look up by effective name (the HashMap key)
        let nodes = self.nodes.read().await;
        let node = nodes
            .get(name)
            .ok_or_else(|| NodeManagerError::NodeNotFound(name.to_string()))?;

        let path = node.path.clone();
        let manifest = node
            .manifest
            .as_ref()
            .ok_or_else(|| NodeManagerError::NodeNotFound(name.to_string()))?;

        // If config_override is set, append -c <config> to the command
        let command = if let Some(ref config_path) = node.config_override {
            let base_cmd = manifest
                .command
                .as_deref()
                .unwrap_or("./target/release/unknown");
            Some(format!("{} -c \"{}\"", base_cmd, config_path))
        } else {
            manifest.command.clone()
        };

        self.supervisor
            .install_service(
                &path,
                name,
                &manifest.node_type,
                command.as_deref(),
                &manifest.depends_on,
            )
            .await?;

        drop(nodes);

        self.spawn_refresh_and_emit("installed", name);
        Ok(format!("Installed {}", name))
    }

    /// Uninstall a node's service
    pub(crate) async fn uninstall_node(self: &Arc<Self>, name: &str) -> Result<String> {
        self.supervisor.uninstall_service(name).await?;
        self.spawn_refresh_and_emit("uninstalled", name);
        Ok(format!("Uninstalled {}", name))
    }

    /// Enable autostart for a node
    pub(crate) async fn enable_autostart(&self, name: &str) -> Result<String> {
        self.supervisor.enable_unit(name).await?;

        self.refresh_all().await?;
        self.emit_event("autostart_enabled", name).await;

        Ok(format!("Enabled autostart for {}", name))
    }

    /// Disable autostart for a node
    pub(crate) async fn disable_autostart(&self, name: &str) -> Result<String> {
        self.supervisor.disable_unit(name).await?;

        self.refresh_all().await?;
        self.emit_event("autostart_disabled", name).await;

        Ok(format!("Disabled autostart for {}", name))
    }

    /// Add a node to the registry, optionally with instance overrides
    pub(crate) async fn add_node(
        &self,
        path: &str,
        name_override: Option<&str>,
        config_override: Option<&str>,
    ) -> Result<String> {
        let (_manifest, eff_name) = registry::register_node(path, name_override, config_override)?;

        self.refresh_all().await?;
        self.emit_event("added", &eff_name).await;

        Ok(format!("Added node: {}", eff_name))
    }

    /// Remove a node from the registry
    pub(crate) async fn remove_node(&self, name: &str) -> Result<String> {
        // Verify the node exists
        let _path = self.find_node_path(name).await?;

        // Uninstall service if installed before removing from registry
        if self.supervisor.is_installed(name) {
            log::info!("Uninstalling service {} before removal", name);
            let _ = self.supervisor.uninstall_service(name).await;
        }

        // Unregister by effective name (handles multi-instance correctly)
        registry::unregister_node(name)?;

        self.refresh_all().await?;
        self.emit_event("removed", name).await;

        Ok(format!("Removed node: {}", name))
    }
}
