//! Real platform implementation backed by NodeManager + Zenoh session.

use super::platform::{NodeCommand, NodeInfo, PlatformError, PlatformOperations, PlatformResult};
use crate::daemon::node_manager::NodeManager;
use crate::schemas::daemon::v1::{
    CommandType, HealthStatus, NodeCommand as ProtoNodeCommand, NodeStatus,
};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use zenoh::Session;

/// Real platform backed by NodeManager + Zenoh session.
pub struct DaemonPlatform {
    pub node_manager: Arc<NodeManager>,
    pub session: Arc<Session>,
    pub scope: String,
    pub machine_id: String,
    /// Cached path to the default agent's memory.db.
    agent_db_path: PathBuf,
}

impl DaemonPlatform {
    /// Create a new DaemonPlatform, caching the agent DB path at construction.
    pub fn new(
        node_manager: Arc<NodeManager>,
        session: Arc<Session>,
        scope: String,
        machine_id: String,
    ) -> Self {
        let agent_db_path = Self::compute_agent_db_path();
        Self {
            node_manager,
            session,
            scope,
            machine_id,
            agent_db_path,
        }
    }

    /// Compute the memory.db path for the default agent.
    ///
    /// Per-agent memory lives at `~/.bubbaloop/agents/{id}/memory.db`.
    /// MCP tools operate on the default agent's memory (from agents.toml).
    fn compute_agent_db_path() -> PathBuf {
        let config = crate::agent::runtime::AgentsConfig::load_or_default();
        let agent_id = config.default_agent().unwrap_or("jean-clawd").to_string();
        crate::daemon::registry::get_bubbaloop_home()
            .join("agents")
            .join(agent_id)
            .join("memory.db")
    }
}

/// Build a ProtoNodeCommand with standard defaults.
///
/// Eliminates repetition of request_id, timestamp, source_machine, and
/// empty-string fields across the many command constructions.
fn build_node_command(cmd_type: CommandType, node_name: &str) -> ProtoNodeCommand {
    ProtoNodeCommand {
        command: cmd_type as i32,
        node_name: node_name.to_string(),
        request_id: uuid::Uuid::new_v4().to_string(),
        timestamp_ms: now_ms(),
        source_machine: "mcp-platform".to_string(),
        target_machine: String::new(),
        node_path: String::new(),
        name_override: String::new(),
        config_override: String::new(),
    }
}

impl PlatformOperations for DaemonPlatform {
    async fn list_nodes(&self) -> PlatformResult<Vec<NodeInfo>> {
        let node_list = self.node_manager.get_node_list().await;
        let nodes = node_list
            .nodes
            .iter()
            .map(|n| {
                let status = NodeStatus::try_from(n.status).unwrap_or(NodeStatus::Unknown);
                let health =
                    HealthStatus::try_from(n.health_status).unwrap_or(HealthStatus::Unknown);
                NodeInfo {
                    name: n.name.clone(),
                    status: format!("{:?}", status),
                    health: format!("{:?}", health),
                    node_type: n.node_type.clone(),
                    installed: n.installed,
                    is_built: n.is_built,
                }
            })
            .collect();
        Ok(nodes)
    }

    async fn get_node_detail(&self, name: &str) -> PlatformResult<Value> {
        match self.node_manager.get_node(name).await {
            Some(node) => {
                let status = NodeStatus::try_from(node.status).unwrap_or(NodeStatus::Unknown);
                let health =
                    HealthStatus::try_from(node.health_status).unwrap_or(HealthStatus::Unknown);
                let detail = serde_json::json!({
                    "name": node.name,
                    "status": format!("{:?}", status),
                    "health_status": format!("{:?}", health),
                    "node_type": node.node_type,
                    "installed": node.installed,
                    "is_built": node.is_built,
                    "last_health_check_ms": node.last_health_check_ms,
                    "last_updated_ms": node.last_updated_ms,
                    "path": node.path,
                    "version": node.version,
                    "description": node.description,
                    "machine_id": node.machine_id,
                });
                Ok(detail)
            }
            None => Err(PlatformError::NodeNotFound(name.to_string())),
        }
    }

    async fn execute_command(&self, name: &str, cmd: NodeCommand) -> PlatformResult<String> {
        let cmd_type = match cmd {
            NodeCommand::Start => CommandType::Start,
            NodeCommand::Stop => CommandType::Stop,
            NodeCommand::Restart => CommandType::Restart,
            NodeCommand::Build => CommandType::Build,
            NodeCommand::GetLogs => CommandType::GetLogs,
            NodeCommand::Install => CommandType::Install,
            NodeCommand::Uninstall => CommandType::Uninstall,
            NodeCommand::Clean => CommandType::Clean,
            NodeCommand::EnableAutostart => CommandType::EnableAutostart,
            NodeCommand::DisableAutostart => CommandType::DisableAutostart,
        };

        let proto_cmd = build_node_command(cmd_type, name);
        let result = self.node_manager.execute_command(proto_cmd).await;
        if result.success {
            if result.output.is_empty() {
                Ok(result.message)
            } else {
                Ok(format!("{}\n{}", result.message, result.output))
            }
        } else {
            Err(PlatformError::CommandFailed(result.message))
        }
    }

    async fn get_node_config(&self, name: &str) -> PlatformResult<Value> {
        let key_expr = format!(
            "bubbaloop/{}/{}/{}/config",
            self.scope, self.machine_id, name
        );
        let text = zenoh_get_text(&self.session, &key_expr).await;
        serde_json::from_str(&text).or_else(|_| Ok(serde_json::json!({ "raw": text })))
    }

    async fn query_zenoh(&self, key_expr: &str) -> PlatformResult<String> {
        Ok(zenoh_get_text(&self.session, key_expr).await)
    }

    async fn send_zenoh_query(
        &self,
        key_expr: &str,
        payload: Vec<u8>,
    ) -> PlatformResult<Vec<String>> {
        match self
            .session
            .get(key_expr)
            .payload(zenoh::bytes::ZBytes::from(payload))
            .timeout(std::time::Duration::from_secs(5))
            .await
        {
            Ok(replies) => {
                let mut results = Vec::new();
                while let Ok(reply) = replies.recv_async().await {
                    match reply.result() {
                        Ok(sample) => {
                            let bytes = sample.payload().to_bytes();
                            match String::from_utf8(bytes.to_vec()) {
                                Ok(text) => results.push(text),
                                Err(_) => results.push(format!("<{} bytes binary>", bytes.len())),
                            }
                        }
                        Err(err) => {
                            results.push(format!("Error: {:?}", err.payload().to_bytes()));
                        }
                    }
                }
                Ok(results)
            }
            Err(e) => Err(PlatformError::Internal(format!(
                "Zenoh query failed: {}",
                e
            ))),
        }
    }

    async fn get_manifests(
        &self,
        capability_filter: Option<&str>,
    ) -> PlatformResult<Vec<(String, Value)>> {
        let cached = self.node_manager.get_cached_manifests().await;
        let results: Vec<(String, Value)> = cached
            .into_iter()
            .filter(|(_name, manifest)| {
                if let Some(filter) = capability_filter {
                    // Match capability by its serde-serialized snake_case name
                    let filter_lower = filter.to_lowercase();
                    manifest.capabilities.iter().any(|cap| {
                        let cap_str = serde_json::to_value(cap)
                            .ok()
                            .and_then(|v| v.as_str().map(String::from));
                        cap_str.map(|s| s == filter_lower).unwrap_or(false)
                    })
                } else {
                    true
                }
            })
            .filter_map(|(name, manifest)| serde_json::to_value(&manifest).ok().map(|v| (name, v)))
            .collect();
        Ok(results)
    }

    async fn install_node(&self, source: &str) -> PlatformResult<String> {
        // Step 1: Register the node with AddNode
        let mut add_cmd = build_node_command(CommandType::AddNode, "");
        add_cmd.node_path = source.to_string();
        let add_result = self.node_manager.execute_command(add_cmd).await;
        if !add_result.success {
            return Err(PlatformError::CommandFailed(add_result.message));
        }

        // Step 2: Parse node name from "Added node: <name>" message
        let node_name = add_result
            .message
            .strip_prefix("Added node: ")
            .unwrap_or(&add_result.message)
            .trim()
            .to_string();

        // Step 3: Create systemd service via Install command
        let install_cmd = build_node_command(CommandType::Install, &node_name);
        let install_result = self.node_manager.execute_command(install_cmd).await;
        if install_result.success {
            Ok(format!(
                "Registered and installed node '{}' from {}",
                node_name, source
            ))
        } else {
            // Node was added but install failed -- report both
            Err(PlatformError::CommandFailed(format!(
                "Node '{}' registered but install failed: {}",
                node_name, install_result.message
            )))
        }
    }

    async fn add_node(
        &self,
        source: &str,
        name_override: Option<&str>,
        config_override: Option<&str>,
    ) -> PlatformResult<String> {
        let mut cmd = build_node_command(CommandType::AddNode, "");
        cmd.node_path = source.to_string();
        cmd.name_override = name_override.unwrap_or_default().to_string();
        cmd.config_override = config_override.unwrap_or_default().to_string();
        let result = self.node_manager.execute_command(cmd).await;
        if result.success {
            Ok(result.message)
        } else {
            Err(PlatformError::CommandFailed(result.message))
        }
    }

    async fn install_from_marketplace(&self, name: &str) -> PlatformResult<String> {
        let marketplace_name = name.to_string();

        // Steps 1-3: Refresh registry, find node, download binary (all blocking I/O)
        let node_dir = tokio::task::spawn_blocking(move || {
            // Refresh the marketplace cache
            crate::registry::refresh_cache().map_err(|e| {
                PlatformError::CommandFailed(format!("Registry refresh failed: {}", e))
            })?;

            // Load and find the node
            let nodes = crate::registry::load_cached_registry();
            let entry =
                crate::registry::find_by_name(&nodes, &marketplace_name).ok_or_else(|| {
                    PlatformError::CommandFailed(format!(
                        "Node '{}' not found in marketplace registry",
                        marketplace_name
                    ))
                })?;

            // Download the precompiled binary
            crate::marketplace::download_precompiled(&entry).map_err(|e| {
                PlatformError::CommandFailed(format!(
                    "Download failed for '{}': {}",
                    marketplace_name, e
                ))
            })
        })
        .await
        .map_err(|e| PlatformError::Internal(format!("Task join error: {}", e)))??;

        // Step 4: Register with daemon via AddNode
        let mut add_cmd = build_node_command(CommandType::AddNode, "");
        add_cmd.node_path = node_dir;
        let add_result = self.node_manager.execute_command(add_cmd).await;
        if !add_result.success {
            return Err(PlatformError::CommandFailed(add_result.message));
        }

        // Step 5: Parse node name from result
        let node_name = add_result
            .message
            .strip_prefix("Added node: ")
            .unwrap_or(&add_result.message)
            .trim()
            .to_string();

        // Step 6: Create systemd service via Install
        let install_cmd = build_node_command(CommandType::Install, &node_name);
        let install_result = self.node_manager.execute_command(install_cmd).await;
        if install_result.success {
            Ok(format!(
                "Installed '{}' from marketplace (registered + systemd service created)",
                node_name
            ))
        } else {
            Err(PlatformError::CommandFailed(format!(
                "Node '{}' registered but service install failed: {}",
                node_name, install_result.message
            )))
        }
    }

    async fn remove_node(&self, name: &str) -> PlatformResult<String> {
        let proto_cmd = build_node_command(CommandType::RemoveNode, name);
        let result = self.node_manager.execute_command(proto_cmd).await;
        if result.success {
            Ok(result.message)
        } else {
            Err(PlatformError::CommandFailed(result.message))
        }
    }

    async fn list_proposals(&self, status_filter: Option<&str>) -> PlatformResult<String> {
        let store = crate::agent::memory::semantic::SemanticStore::open(&self.agent_db_path)
            .map_err(|e| PlatformError::Internal(e.to_string()))?;
        let proposals = store
            .list_proposals(status_filter)
            .map_err(|e| PlatformError::Internal(e.to_string()))?;
        serde_json::to_string_pretty(&proposals).map_err(|e| PlatformError::Internal(e.to_string()))
    }

    async fn approve_proposal(&self, id: &str, decided_by: &str) -> PlatformResult<String> {
        let store = crate::agent::memory::semantic::SemanticStore::open(&self.agent_db_path)
            .map_err(|e| PlatformError::Internal(e.to_string()))?;
        store
            .approve_proposal(id, decided_by)
            .map_err(|e| PlatformError::Internal(e.to_string()))?;
        Ok(format!("Proposal '{}' approved by {}", id, decided_by))
    }

    async fn reject_proposal(&self, id: &str, decided_by: &str) -> PlatformResult<String> {
        let store = crate::agent::memory::semantic::SemanticStore::open(&self.agent_db_path)
            .map_err(|e| PlatformError::Internal(e.to_string()))?;
        store
            .reject_proposal(id, decided_by)
            .map_err(|e| PlatformError::Internal(e.to_string()))?;
        Ok(format!("Proposal '{}' rejected by {}", id, decided_by))
    }

    async fn schedule_job(
        &self,
        prompt: &str,
        cron_schedule: Option<&str>,
        recurrence: bool,
    ) -> PlatformResult<String> {
        use crate::agent::memory::semantic::{Job, SemanticStore};
        let store = SemanticStore::open(&self.agent_db_path)
            .map_err(|e| PlatformError::Internal(e.to_string()))?;
        let next_run: i64 = match cron_schedule {
            Some(cron) => crate::agent::scheduler::next_run_after(
                cron,
                crate::agent::scheduler::now_epoch_secs(),
            )
            .map_err(|e| PlatformError::InvalidInput(e.to_string()))?
                as i64,
            None => crate::agent::scheduler::now_epoch_secs() as i64,
        };
        let job_id = uuid::Uuid::new_v4().to_string();
        let job = Job {
            id: job_id.clone(),
            cron_schedule: cron_schedule.map(|s| s.to_string()),
            next_run_at: next_run,
            prompt_payload: prompt.to_string(),
            status: "pending".to_string(),
            recurrence,
            retry_count: 0,
            last_error: None,
        };
        store
            .create_job(&job)
            .map_err(|e| PlatformError::Internal(e.to_string()))?;
        Ok(format!("Job '{}' scheduled", job_id))
    }

    async fn list_jobs(&self, status_filter: Option<&str>) -> PlatformResult<String> {
        let store = crate::agent::memory::semantic::SemanticStore::open(&self.agent_db_path)
            .map_err(|e| PlatformError::Internal(e.to_string()))?;
        let jobs = store
            .list_jobs(status_filter)
            .map_err(|e| PlatformError::Internal(e.to_string()))?;
        serde_json::to_string_pretty(&jobs).map_err(|e| PlatformError::Internal(e.to_string()))
    }

    async fn delete_job(&self, id: &str) -> PlatformResult<String> {
        let store = crate::agent::memory::semantic::SemanticStore::open(&self.agent_db_path)
            .map_err(|e| PlatformError::Internal(e.to_string()))?;
        store
            .delete_job(id)
            .map_err(|e| PlatformError::Internal(e.to_string()))?;
        Ok(format!("Job '{}' deleted", id))
    }

    async fn configure_context(
        &self,
        params: super::platform::ConfigureContextParams,
    ) -> PlatformResult<String> {
        if params.topic_pattern.is_empty() {
            return Err(PlatformError::InvalidInput(
                "topic_pattern must not be empty".to_string(),
            ));
        }
        if params.world_state_key_template.is_empty() {
            return Err(PlatformError::InvalidInput(
                "world_state_key_template must not be empty".to_string(),
            ));
        }

        let provider_id = format!("cp-{}", uuid::Uuid::new_v4());

        let cfg = crate::daemon::context_provider::ProviderConfig {
            id: provider_id.clone(),
            mission_id: params.mission_id.clone(),
            topic_pattern: params.topic_pattern,
            world_state_key_template: params.world_state_key_template,
            value_field: params.value_field,
            filter: params.filter,
            min_interval_secs: params.min_interval_secs.unwrap_or(30),
            max_age_secs: params.max_age_secs.unwrap_or(300),
            confidence_field: params.confidence_field,
            token_budget: params.token_budget.unwrap_or(50),
        };

        // Store the provider config in the providers database next to the agent memory.db
        let providers_db_path = self
            .agent_db_path
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .join("providers.db");
        let store = crate::daemon::context_provider::ProviderStore::open(&providers_db_path)
            .map_err(|e| PlatformError::Internal(e.to_string()))?;
        store
            .save_provider(&cfg)
            .map_err(|e| PlatformError::Internal(e.to_string()))?;

        log::info!(
            "[MCP] tool=configure_context mission_id={} provider_id={}",
            params.mission_id,
            provider_id
        );

        Ok(format!(
            "Context provider '{}' configured (mission={})",
            provider_id, params.mission_id
        ))
    }

    async fn clear_episodic_memory(&self, older_than_days: u32) -> PlatformResult<String> {
        let base = self
            .agent_db_path
            .parent()
            .unwrap_or(std::path::Path::new("."));
        let log_dir = base.join("memory");

        let episodic =
            crate::agent::memory::episodic::EpisodicLog::new(&log_dir, &self.agent_db_path)
                .map_err(|e| PlatformError::Internal(e.to_string()))?;
        let pruned = episodic
            .prune_old_logs(older_than_days)
            .map_err(|e| PlatformError::Internal(e.to_string()))?;
        Ok(format!(
            "Pruned {} episodic log file(s) older than {} days",
            pruned, older_than_days
        ))
    }
}

/// Query a Zenoh key expression and return text results.
async fn zenoh_get_text(session: &Session, key_expr: &str) -> String {
    match session
        .get(key_expr)
        .timeout(std::time::Duration::from_secs(3))
        .await
    {
        Ok(replies) => {
            let mut results = Vec::new();
            while let Ok(reply) = replies.recv_async().await {
                match reply.result() {
                    Ok(sample) => {
                        let key = sample.key_expr().to_string();
                        let bytes = sample.payload().to_bytes();
                        match String::from_utf8(bytes.to_vec()) {
                            Ok(text) => results.push(format!("[{}] {}", key, text)),
                            Err(_) => {
                                results.push(format!("[{}] <{} bytes binary>", key, bytes.len()))
                            }
                        }
                    }
                    Err(err) => {
                        results.push(format!("Error: {:?}", err.payload().to_bytes()));
                    }
                }
            }
            if results.is_empty() {
                "No responses received".to_string()
            } else {
                results.join("\n")
            }
        }
        Err(e) => format!("Zenoh query failed: {}", e),
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
