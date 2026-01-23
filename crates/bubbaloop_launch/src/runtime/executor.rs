//! Launch executor - orchestrates process spawning and shutdown

use crate::config::{ArgValue, EnabledValue, LaunchFile, SubstitutionContext};
use crate::runtime::dependency::{DependencyError, DependencyGraph, ResolvedNode};
use crate::runtime::process::{ManagedProcess, ProcessConfig, ProcessEvent, ProcessStatus};
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::{mpsc, watch};

/// Launch executor configuration
#[derive(Debug, Clone)]
pub struct ExecutorConfig {
    /// Project root directory
    pub project_root: PathBuf,
    /// Default shutdown timeout per process
    pub shutdown_timeout: Duration,
    /// Groups to include (None = all groups)
    pub include_groups: Option<HashSet<String>>,
    /// Nodes to explicitly enable
    pub enable_nodes: HashSet<String>,
    /// Nodes to explicitly disable
    pub disable_nodes: HashSet<String>,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            project_root: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            shutdown_timeout: Duration::from_secs(5),
            include_groups: None,
            enable_nodes: HashSet::new(),
            disable_nodes: HashSet::new(),
        }
    }
}

/// Launch executor state
pub struct Executor {
    /// Configuration
    config: ExecutorConfig,
    /// Parsed launch file
    launch_file: LaunchFile,
    /// Substitution context
    subst_ctx: SubstitutionContext,
    /// Managed processes
    processes: IndexMap<String, ManagedProcess>,
    /// Event channel
    event_tx: mpsc::UnboundedSender<(String, ProcessEvent)>,
    event_rx: mpsc::UnboundedReceiver<(String, ProcessEvent)>,
}

/// Launch plan for dry-run mode
#[derive(Debug)]
pub struct LaunchPlan {
    /// Nodes in launch order
    pub nodes: Vec<LaunchPlanNode>,
    /// Resolved arguments
    pub args: HashMap<String, String>,
    /// Global environment
    pub env: HashMap<String, String>,
}

/// A node in the launch plan
#[derive(Debug)]
pub struct LaunchPlanNode {
    pub name: String,
    pub executable: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub group: Option<String>,
    pub dependencies: Vec<String>,
    pub startup_delay_ms: Option<u64>,
}

impl Executor {
    /// Create a new executor
    pub fn new(
        launch_file: LaunchFile,
        config: ExecutorConfig,
        arg_overrides: HashMap<String, String>,
    ) -> Result<Self, ExecutorError> {
        // Build substitution context from launch file args + overrides
        let mut args = HashMap::new();

        for (name, def) in &launch_file.args {
            args.insert(name.clone(), def.default.as_str());
        }

        // Apply overrides
        for (name, value) in &arg_overrides {
            if !launch_file.args.contains_key(name) {
                return Err(ExecutorError::UnknownArgument(name.clone()));
            }
            args.insert(name.clone(), value.clone());
        }

        let subst_ctx = SubstitutionContext::new()
            .with_args(args)
            .with_envs(launch_file.env.clone());

        let (event_tx, event_rx) = mpsc::unbounded_channel();

        Ok(Self {
            config,
            launch_file,
            subst_ctx,
            processes: IndexMap::new(),
            event_tx,
            event_rx,
        })
    }

    /// Determine which nodes are enabled
    fn resolve_enabled_nodes(&self) -> Result<HashSet<String>, ExecutorError> {
        let mut enabled = HashSet::new();

        for (name, node) in &self.launch_file.nodes {
            // Check if node is in selected groups
            if let Some(ref groups) = self.config.include_groups {
                if let Some(ref group) = node.group {
                    if !groups.contains(group) {
                        continue;
                    }
                } else {
                    // Node has no group, skip if groups filter is active
                    continue;
                }
            }

            // Check explicit disable
            if self.config.disable_nodes.contains(name) {
                continue;
            }

            // Check explicit enable (overrides enabled field)
            if self.config.enable_nodes.contains(name) {
                enabled.insert(name.clone());
                continue;
            }

            // Check node's enabled field
            let is_enabled = match &node.enabled {
                EnabledValue::Bool(b) => *b,
                EnabledValue::String(s) => {
                    let resolved = self.subst_ctx.substitute(s).map_err(|e| {
                        ExecutorError::SubstitutionFailed {
                            context: format!("node '{}' enabled field", name),
                            source: e,
                        }
                    })?;
                    ArgValue::from_str(&resolved).is_truthy()
                }
            };

            if is_enabled {
                enabled.insert(name.clone());
            }
        }

        Ok(enabled)
    }

    /// Build the dependency graph for enabled nodes
    fn build_dependency_graph(&self) -> Result<DependencyGraph, ExecutorError> {
        let enabled = self.resolve_enabled_nodes()?;
        DependencyGraph::build(&self.launch_file, &enabled).map_err(ExecutorError::Dependency)
    }

    /// Generate a launch plan (for dry-run mode)
    pub fn plan(&self) -> Result<LaunchPlan, ExecutorError> {
        let graph = self.build_dependency_graph()?;
        let mut nodes = Vec::new();

        for resolved_node in graph.launch_order() {
            let plan_node = self.build_plan_node(resolved_node)?;
            nodes.push(plan_node);
        }

        // Substitute global env vars for display
        let mut resolved_env = HashMap::new();
        for (k, v) in &self.launch_file.env {
            let resolved = self.subst_ctx.substitute(v).map_err(|e| {
                ExecutorError::SubstitutionFailed {
                    context: format!("global env '{}'", k),
                    source: e,
                }
            })?;
            resolved_env.insert(k.clone(), resolved);
        }

        Ok(LaunchPlan {
            nodes,
            args: self.subst_ctx.args.clone(),
            env: resolved_env,
        })
    }

    /// Build a plan node from a resolved node
    fn build_plan_node(&self, node: &ResolvedNode) -> Result<LaunchPlanNode, ExecutorError> {
        let (executable, args) = self.resolve_command(node)?;

        // Substitute global env vars first
        let mut env = HashMap::new();
        for (k, v) in &self.launch_file.env {
            let resolved = self.subst_ctx.substitute(v).map_err(|e| {
                ExecutorError::SubstitutionFailed {
                    context: format!("global env '{}'", k),
                    source: e,
                }
            })?;
            env.insert(k.clone(), resolved);
        }

        // Then add/override with node-specific env vars
        for (k, v) in &node.config.env {
            let resolved = self.subst_ctx.substitute(v).map_err(|e| {
                ExecutorError::SubstitutionFailed {
                    context: format!("node '{}' env '{}'", node.name, k),
                    source: e,
                }
            })?;
            env.insert(k.clone(), resolved);
        }

        Ok(LaunchPlanNode {
            name: node.name.clone(),
            executable,
            args,
            env,
            group: node.config.group.clone(),
            dependencies: node.dependencies.iter().map(|(n, _)| n.clone()).collect(),
            startup_delay_ms: node.config.startup_delay_ms,
        })
    }

    /// Resolve the command for a node
    fn resolve_command(&self, node: &ResolvedNode) -> Result<(String, Vec<String>), ExecutorError> {
        let executable = if let Some(ref exec) = node.config.executable {
            // Direct executable path
            self.subst_ctx.substitute(exec).map_err(|e| {
                ExecutorError::SubstitutionFailed {
                    context: format!("node '{}' executable", node.name),
                    source: e,
                }
            })?
        } else if let (Some(_package), Some(binary)) =
            (&node.config.package, &node.config.binary)
        {
            // Cargo binary - resolve path
            let target_dir = self.config.project_root.join("target/release");
            target_dir
                .join(binary)
                .to_string_lossy()
                .into_owned()
        } else {
            return Err(ExecutorError::InvalidNodeConfig(format!(
                "Node '{}' has no executable or package+binary",
                node.name
            )));
        };

        // Build arguments
        let mut args = Vec::new();

        // Add raw args first
        for arg in &node.config.raw_args {
            let resolved = self.subst_ctx.substitute(arg).map_err(|e| {
                ExecutorError::SubstitutionFailed {
                    context: format!("node '{}' raw_args", node.name),
                    source: e,
                }
            })?;
            args.push(resolved);
        }

        // Add named args
        for (key, value) in &node.config.args {
            let resolved = self.subst_ctx.substitute(value).map_err(|e| {
                ExecutorError::SubstitutionFailed {
                    context: format!("node '{}' args.{}", node.name, key),
                    source: e,
                }
            })?;
            args.push(format!("--{}", key));
            args.push(resolved);
        }

        Ok((executable, args))
    }

    /// Launch all nodes according to the dependency graph
    pub async fn launch(&mut self, shutdown_rx: watch::Receiver<()>) -> Result<(), ExecutorError> {
        let graph = self.build_dependency_graph()?;

        log::info!("Launching {} nodes...", graph.nodes.len());

        // Create processes for all nodes
        for node in graph.launch_order() {
            let (executable, args) = self.resolve_command(node)?;

            // Build environment
            let mut env = self.launch_file.env.clone();
            for (k, v) in &node.config.env {
                let resolved = self.subst_ctx.substitute(v).map_err(|e| {
                    ExecutorError::SubstitutionFailed {
                        context: format!("node '{}' env '{}'", node.name, k),
                        source: e,
                    }
                })?;
                env.insert(k.clone(), resolved);
            }

            let config = ProcessConfig {
                name: node.name.clone(),
                executable,
                args,
                env,
                working_dir: node.config.working_dir.as_ref().map(PathBuf::from),
            };

            let process = ManagedProcess::new(config)
                .with_event_sender(self.event_tx.clone());

            self.processes.insert(node.name.clone(), process);
        }

        // Launch processes in order
        for node in graph.launch_order() {
            // Check for shutdown signal
            if shutdown_rx.has_changed().unwrap_or(false) {
                log::info!("Shutdown requested, aborting launch");
                break;
            }

            // Wait for dependencies to be running
            for (dep_name, _condition) in &node.dependencies {
                loop {
                    if let Some(dep_process) = self.processes.get_mut(dep_name) {
                        let status = dep_process.check_status().await;
                        if status == ProcessStatus::Running {
                            break;
                        }
                        if status.is_stopped() {
                            return Err(ExecutorError::DependencyFailed {
                                node: node.name.clone(),
                                dependency: dep_name.clone(),
                            });
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }

            // Apply startup delay
            if let Some(delay) = node.config.startup_delay_ms {
                log::debug!("[{}] Waiting {}ms before start", node.name, delay);
                tokio::time::sleep(Duration::from_millis(delay)).await;
            }

            // Start the process
            if let Some(process) = self.processes.get_mut(&node.name) {
                process.start().await.map_err(|e| ExecutorError::ProcessFailed {
                    node: node.name.clone(),
                    source: Box::new(e),
                })?;
            }
        }

        log::info!("All nodes launched successfully");
        Ok(())
    }

    /// Wait for all processes or shutdown signal
    pub async fn wait(&mut self, mut shutdown_rx: watch::Receiver<()>) {
        loop {
            tokio::select! {
                // Check for shutdown signal
                _ = shutdown_rx.changed() => {
                    log::info!("Shutdown signal received");
                    break;
                }

                // Process events
                event = self.event_rx.recv() => {
                    if let Some((name, event)) = event {
                        match event {
                            ProcessEvent::Output { line, is_stderr } => {
                                if is_stderr {
                                    log::warn!("[{}] {}", name, line);
                                } else {
                                    log::info!("[{}] {}", name, line);
                                }
                            }
                            ProcessEvent::Exited { code } => {
                                log::info!("[{}] Process exited with code: {:?}", name, code);
                            }
                            ProcessEvent::Failed { error } => {
                                log::error!("[{}] Process failed: {}", name, error);
                            }
                            ProcessEvent::Started { pid } => {
                                log::info!("[{}] Process started with PID: {}", name, pid);
                            }
                        }
                    }
                }

                // Check process status periodically
                _ = tokio::time::sleep(Duration::from_secs(1)) => {
                    let mut all_stopped = true;
                    for (_, process) in self.processes.iter_mut() {
                        let status = process.check_status().await;
                        if status.is_running() {
                            all_stopped = false;
                        }
                    }
                    if all_stopped {
                        log::info!("All processes have stopped");
                        break;
                    }
                }
            }
        }
    }

    /// Shutdown all processes in reverse order
    pub async fn shutdown(&mut self) {
        log::info!("Shutting down all processes...");

        // Collect node names in reverse order
        let names: Vec<String> = self.processes.keys().cloned().collect();

        for name in names.into_iter().rev() {
            if let Some(process) = self.processes.get_mut(&name) {
                if process.status.is_running() {
                    if let Err(e) = process.stop(self.config.shutdown_timeout).await {
                        log::error!("[{}] Error stopping process: {}", name, e);
                    }
                }
            }
        }

        log::info!("All processes shut down");
    }

    /// Get process status summary
    pub fn status(&self) -> Vec<(&str, ProcessStatus)> {
        self.processes
            .iter()
            .map(|(name, proc)| (name.as_str(), proc.status))
            .collect()
    }
}

/// Errors that can occur in the executor
#[derive(Debug, thiserror::Error)]
pub enum ExecutorError {
    #[error("Unknown argument: {0}")]
    UnknownArgument(String),

    #[error("Dependency error: {0}")]
    Dependency(#[from] DependencyError),

    #[error("Substitution failed in {context}: {source}")]
    SubstitutionFailed {
        context: String,
        #[source]
        source: crate::config::SubstitutionError,
    },

    #[error("Invalid node configuration: {0}")]
    InvalidNodeConfig(String),

    #[error("Process failed for node '{node}': {source}")]
    ProcessFailed {
        node: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("Dependency '{dependency}' failed before node '{node}' could start")]
    DependencyFailed { node: String, dependency: String },
}

/// Display the launch plan in a human-readable format
impl std::fmt::Display for LaunchPlan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Launch Plan")?;
        writeln!(f, "===========")?;
        writeln!(f)?;

        if !self.args.is_empty() {
            writeln!(f, "Arguments:")?;
            for (key, value) in &self.args {
                writeln!(f, "  {}: {}", key, value)?;
            }
            writeln!(f)?;
        }

        if !self.env.is_empty() {
            writeln!(f, "Global Environment:")?;
            for (key, value) in &self.env {
                writeln!(f, "  {}={}", key, value)?;
            }
            writeln!(f)?;
        }

        writeln!(f, "Nodes (in launch order):")?;
        for (i, node) in self.nodes.iter().enumerate() {
            writeln!(f)?;
            writeln!(
                f,
                "  {}. {} {}",
                i + 1,
                node.name,
                node.group
                    .as_ref()
                    .map(|g| format!("[{}]", g))
                    .unwrap_or_default()
            )?;
            writeln!(f, "     Command: {} {}", node.executable, node.args.join(" "))?;

            if !node.dependencies.is_empty() {
                writeln!(f, "     Depends on: {}", node.dependencies.join(", "))?;
            }

            if let Some(delay) = node.startup_delay_ms {
                writeln!(f, "     Startup delay: {}ms", delay)?;
            }

            if !node.env.is_empty() {
                writeln!(f, "     Environment:")?;
                for (key, value) in &node.env {
                    writeln!(f, "       {}={}", key, value)?;
                }
            }
        }

        Ok(())
    }
}
