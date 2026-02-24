//! Agent Logic Layer — lightweight rule engine for autonomous sensor-to-action.
//!
//! Discovers node topics via manifests, subscribes to sensor data,
//! evaluates rules, and executes actions (log, command, publish).
//!
//! Rules are defined in `~/.bubbaloop/rules.yaml`. The agent is a "logic gate",
//! not an LLM — the LLM lives externally via MCP.

mod rules;

pub use rules::{Action, AgentStatus, Condition, Operator, Rule, RuleConfig, RuleTriggerLog};

use crate::daemon::node_manager::NodeManager;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{watch, RwLock};

/// Path to the rules configuration file.
fn rules_path() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".bubbaloop")
        .join("rules.yaml")
}

/// The agent service — runs inside the daemon, evaluates rules on sensor events.
#[allow(dead_code)]
pub struct Agent {
    session: Arc<zenoh::Session>,
    node_manager: Arc<NodeManager>,
    rules: Arc<RwLock<Vec<Rule>>>,
    /// Log of recent rule triggers (rule_name -> last trigger info).
    trigger_log: Arc<RwLock<HashMap<String, RuleTriggerLog>>>,
    /// Active human overrides (node_name -> override details).
    overrides: Arc<RwLock<HashMap<String, serde_json::Value>>>,
    machine_id: String,
    scope: String,
}

impl Agent {
    pub fn new(session: Arc<zenoh::Session>, node_manager: Arc<NodeManager>) -> Self {
        let machine_id = crate::daemon::util::get_machine_id();
        let scope = std::env::var("BUBBALOOP_SCOPE").unwrap_or_else(|_| "local".to_string());

        // Load rules from file
        let rules = match Self::load_rules() {
            Ok(r) => {
                log::info!("Agent loaded {} rules from {:?}", r.len(), rules_path());
                r
            }
            Err(e) => {
                log::warn!(
                    "Failed to load rules from {:?}: {}. Starting with empty rules.",
                    rules_path(),
                    e
                );
                Vec::new()
            }
        };

        Self {
            session,
            node_manager,
            rules: Arc::new(RwLock::new(rules)),
            trigger_log: Arc::new(RwLock::new(HashMap::new())),
            overrides: Arc::new(RwLock::new(HashMap::new())),
            machine_id,
            scope,
        }
    }

    fn load_rules() -> Result<Vec<Rule>, Box<dyn std::error::Error>> {
        let path = rules_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let content = std::fs::read_to_string(&path)?;
        let config: RuleConfig = serde_yaml::from_str(&content)?;
        Ok(config.rules)
    }

    /// Maximum number of rules to prevent resource exhaustion.
    const MAX_RULES: usize = 100;

    /// Persist rules to disk atomically (write-to-temp + rename).
    fn save_rules(rules: &[Rule]) -> Result<(), Box<dyn std::error::Error>> {
        let config = RuleConfig {
            rules: rules.to_vec(),
        };
        let yaml = serde_yaml::to_string(&config)?;
        let path = rules_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
            let mut temp = tempfile::NamedTempFile::new_in(parent)?;
            std::io::Write::write_all(&mut temp, yaml.as_bytes())?;
            temp.persist(&path)?;
        }
        Ok(())
    }

    /// Get current agent status (for MCP tools and status queryable).
    pub async fn get_status(&self) -> AgentStatus {
        let rules = self.rules.read().await;
        let trigger_log = self.trigger_log.read().await;
        let overrides = self.overrides.read().await;
        AgentStatus {
            rule_count: rules.len(),
            rules: rules.iter().map(|r| r.name.clone()).collect(),
            recent_triggers: trigger_log.clone(),
            active_overrides: overrides.len(),
        }
    }

    /// Get a snapshot of the current rules.
    pub async fn get_rules(&self) -> Vec<Rule> {
        self.rules.read().await.clone()
    }

    /// Add a new rule and persist to disk.
    pub async fn add_rule(&self, rule: Rule) -> Result<String, String> {
        let mut rules = self.rules.write().await;
        if rules.len() >= Self::MAX_RULES {
            return Err(format!("Maximum rule count ({}) reached", Self::MAX_RULES));
        }
        if rules.iter().any(|r| r.name == rule.name) {
            return Err(format!("Rule '{}' already exists", rule.name));
        }
        let name = rule.name.clone();
        rules.push(rule);
        let snapshot = rules.clone();
        drop(rules);
        Self::save_rules(&snapshot).map_err(|e| {
            log::error!("Failed to save rules: {}", e);
            "Failed to persist rules".to_string()
        })?;
        Ok(format!("Rule '{}' added", name))
    }

    /// Remove a rule by name and persist to disk.
    pub async fn remove_rule(&self, name: &str) -> Result<String, String> {
        let mut rules = self.rules.write().await;
        let before = rules.len();
        rules.retain(|r| r.name != name);
        if rules.len() == before {
            return Err(format!("Rule '{}' not found", name));
        }
        let snapshot = rules.clone();
        drop(rules);
        Self::save_rules(&snapshot).map_err(|e| {
            log::error!("Failed to save rules: {}", e);
            "Failed to persist rules".to_string()
        })?;
        Ok(format!("Rule '{}' removed", name))
    }

    /// Get a snapshot of the trigger log (recent rule triggers).
    pub async fn get_trigger_log(&self) -> HashMap<String, RuleTriggerLog> {
        self.trigger_log.read().await.clone()
    }

    /// Test a rule's condition against sample data without executing the action.
    /// Returns a JSON result indicating whether the condition matches.
    pub async fn test_rule(
        &self,
        rule_name: &str,
        sample_data: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let rules = self.rules.read().await;
        let rule = rules
            .iter()
            .find(|r| r.name == rule_name)
            .ok_or_else(|| format!("Rule '{}' not found", rule_name))?;

        let condition_met = match &rule.condition {
            Some(cond) => cond.evaluate(sample_data),
            None => true, // No condition = always triggers
        };

        Ok(serde_json::json!({
            "rule_name": rule_name,
            "condition_met": condition_met,
            "has_condition": rule.condition.is_some(),
            "trigger": rule.trigger,
            "enabled": rule.enabled,
        }))
    }

    /// Update an existing rule and persist to disk.
    pub async fn update_rule(&self, rule: Rule) -> Result<String, String> {
        let mut rules = self.rules.write().await;
        if let Some(existing) = rules.iter_mut().find(|r| r.name == rule.name) {
            *existing = rule.clone();
            let snapshot = rules.clone();
            drop(rules);
            Self::save_rules(&snapshot).map_err(|e| {
                log::error!("Failed to save rules: {}", e);
                "Failed to persist rules".to_string()
            })?;
            Ok(format!("Rule '{}' updated", rule.name))
        } else {
            Err(format!("Rule '{}' not found", rule.name))
        }
    }

    /// Run the agent event loop. Blocks until shutdown.
    pub async fn run(self: Arc<Self>, mut shutdown_rx: watch::Receiver<()>) {
        log::info!(
            "Agent starting (machine_id={}, scope={})",
            self.machine_id,
            self.scope
        );

        // Subscribe to human override namespace
        let override_key = format!(
            "bubbaloop/{}/{}/human/override/**",
            self.scope, self.machine_id
        );
        let override_agent = self.clone();
        let override_sub = match self.session.declare_subscriber(&override_key).await {
            Ok(sub) => {
                log::info!("Agent subscribed to overrides: {}", override_key);
                Some(sub)
            }
            Err(e) => {
                log::warn!("Failed to subscribe to overrides: {}", e);
                None
            }
        };

        // Publish agent status queryable
        let status_key = format!("bubbaloop/{}/{}/agent/status", self.scope, self.machine_id);
        let status_agent = self.clone();
        let _status_queryable = match self
            .session
            .declare_queryable(&status_key)
            .callback(move |query| {
                use zenoh::Wait;
                let agent = status_agent.clone();
                // We need to block here since callback is sync; use a small runtime
                let status = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(agent.get_status())
                });
                let json = serde_json::to_string(&status).unwrap_or_else(|_| "{}".to_string());
                if let Err(e) = query
                    .reply(
                        query.key_expr(),
                        zenoh::bytes::ZBytes::from(json.into_bytes()),
                    )
                    .wait()
                {
                    log::warn!("Failed to reply to agent status query: {}", e);
                }
            })
            .await
        {
            Ok(q) => {
                log::info!("Agent status queryable: {}", status_key);
                Some(q)
            }
            Err(e) => {
                log::warn!("Failed to create agent status queryable: {}", e);
                None
            }
        };

        // Main loop: discover topics, subscribe, evaluate rules
        let mut discovery_interval = tokio::time::interval(Duration::from_secs(30));
        let mut subscriptions: HashMap<String, zenoh::pubsub::Subscriber<()>> = HashMap::new();

        loop {
            tokio::select! {
                _ = shutdown_rx.changed() => {
                    log::info!("Agent shutting down.");
                    break;
                }
                _ = discovery_interval.tick() => {
                    // Discover topics from manifests and subscribe to rule triggers
                    self.discover_and_subscribe(&mut subscriptions).await;
                }
                // Process override messages
                _ = async {
                    if let Some(ref sub) = override_sub {
                        if let Ok(sample) = sub.recv_async().await {
                            override_agent.handle_override(&sample).await;
                        }
                    } else {
                        // No override sub — just sleep to avoid busy loop
                        tokio::time::sleep(Duration::from_secs(60)).await;
                    }
                } => {}
            }
        }
    }

    /// Discover node topics via manifests and subscribe to rule trigger patterns.
    async fn discover_and_subscribe(
        &self,
        subscriptions: &mut HashMap<String, zenoh::pubsub::Subscriber<()>>,
    ) {
        let rules = self.rules.read().await;
        if rules.is_empty() {
            return;
        }

        // Collect unique trigger patterns from rules
        let patterns: Vec<String> = rules
            .iter()
            .map(|r| r.trigger.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        for pattern in &patterns {
            if subscriptions.contains_key(pattern) {
                continue; // Already subscribed
            }

            let rules_ref = self.rules.clone();
            let trigger_log = self.trigger_log.clone();
            let overrides = self.overrides.clone();
            let session = self.session.clone();
            let pattern_for_cb = pattern.clone();
            let scope_for_cb = self.scope.clone();
            let machine_id_for_cb = self.machine_id.clone();

            match self
                .session
                .declare_subscriber(pattern)
                .callback(move |sample| {
                    let rules = rules_ref.clone();
                    let trigger_log = trigger_log.clone();
                    let overrides = overrides.clone();
                    let session = session.clone();
                    let pattern = pattern_for_cb.clone();
                    let scope = scope_for_cb.clone();
                    let machine_id = machine_id_for_cb.clone();

                    tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current().block_on(async {
                            Self::evaluate_rules_for_sample(
                                &rules,
                                &trigger_log,
                                &overrides,
                                &session,
                                &sample,
                                &pattern,
                                &scope,
                                &machine_id,
                            )
                            .await;
                        });
                    });
                })
                .await
            {
                Ok(sub) => {
                    log::info!("Agent subscribed to trigger: {}", pattern);
                    subscriptions.insert(pattern.clone(), sub);
                }
                Err(e) => {
                    log::warn!("Agent failed to subscribe to {}: {}", pattern, e);
                }
            }
        }
    }

    /// Evaluate all matching rules for an incoming sample.
    #[allow(clippy::too_many_arguments)]
    async fn evaluate_rules_for_sample(
        rules: &RwLock<Vec<Rule>>,
        trigger_log: &RwLock<HashMap<String, RuleTriggerLog>>,
        overrides: &RwLock<HashMap<String, serde_json::Value>>,
        session: &zenoh::Session,
        sample: &zenoh::sample::Sample,
        trigger_pattern: &str,
        scope: &str,
        machine_id: &str,
    ) {
        let payload = sample.payload().to_bytes();
        let key = sample.key_expr().to_string();

        // Try to parse payload as JSON
        let json_value: Option<serde_json::Value> = serde_json::from_slice(&payload).ok();

        let rules = rules.read().await;
        for rule in rules.iter() {
            if rule.trigger != trigger_pattern {
                continue;
            }
            if !rule.enabled {
                continue;
            }

            // Evaluate condition
            let condition_met = match (&rule.condition, &json_value) {
                (Some(cond), Some(json)) => cond.evaluate(json),
                (None, _) => true,        // No condition = always trigger
                (Some(_), None) => false, // Condition requires JSON but payload isn't JSON
            };

            if !condition_met {
                continue;
            }

            // Check human override
            if let Some(ref target_node) = rule.action.target_node() {
                let overrides = overrides.read().await;
                if overrides.contains_key(target_node.as_str()) {
                    log::info!(
                        "Rule '{}' skipped: human override active for '{}'",
                        rule.name,
                        target_node
                    );
                    continue;
                }
            }

            // Execute action
            log::info!("Rule '{}' triggered by {}", rule.name, key);
            rule.action.execute(session, scope, machine_id).await;

            // Log trigger
            let mut log = trigger_log.write().await;
            let prev_count = log.get(&rule.name).map(|l| l.trigger_count).unwrap_or(0);
            log.insert(
                rule.name.clone(),
                RuleTriggerLog {
                    last_triggered_ms: now_ms(),
                    trigger_key: key.clone(),
                    trigger_count: prev_count + 1,
                },
            );
        }
    }

    /// Handle a human override message.
    async fn handle_override(&self, sample: &zenoh::sample::Sample) {
        let payload = sample.payload().to_bytes();
        match serde_json::from_slice::<serde_json::Value>(&payload) {
            Ok(override_val) => {
                if let Some(node) = override_val.get("node").and_then(|v| v.as_str()) {
                    let expires_s = override_val
                        .get("expires_s")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(300);
                    log::info!(
                        "Human override received for '{}' (expires in {}s)",
                        node,
                        expires_s
                    );
                    let mut overrides = self.overrides.write().await;
                    overrides.insert(node.to_string(), override_val.clone());

                    // Schedule removal after expiry
                    let overrides_ref = self.overrides.clone();
                    let node_name = node.to_string();
                    tokio::spawn(async move {
                        tokio::time::sleep(Duration::from_secs(expires_s)).await;
                        let mut overrides = overrides_ref.write().await;
                        if overrides.remove(&node_name).is_some() {
                            log::info!("Human override expired for '{}'", node_name);
                        }
                    });
                }
            }
            Err(e) => {
                log::warn!("Invalid override payload: {}", e);
            }
        }
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
