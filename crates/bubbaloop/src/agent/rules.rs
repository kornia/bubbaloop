//! Rule definitions, condition evaluation, and action execution.
//!
//! Rules are loaded from `~/.bubbaloop/rules.yaml`:
//!
//! ```yaml
//! rules:
//!   - name: "high-temp-alert"
//!     trigger: "bubbaloop/**/telemetry/status"
//!     condition:
//!       field: "cpu_temp"
//!       operator: ">"
//!       value: 80.0
//!     action:
//!       type: "log"
//!       message: "CPU temperature exceeds 80C"
//!
//!   - name: "restart-unhealthy"
//!     trigger: "bubbaloop/**/health"
//!     condition:
//!       field: "status"
//!       operator: "!="
//!       value: "ok"
//!     action:
//!       type: "command"
//!       node: "openmeteo"
//!       command: "restart"
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Top-level YAML configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleConfig {
    #[serde(default)]
    pub rules: Vec<Rule>,
}

/// A single rule: trigger → condition → action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    /// Human-readable rule name.
    pub name: String,

    /// Zenoh key expression to subscribe to (e.g., "bubbaloop/**/telemetry/status").
    pub trigger: String,

    /// Optional condition on the JSON payload. If absent, rule always triggers.
    #[serde(default)]
    pub condition: Option<Condition>,

    /// Action to execute when the rule triggers.
    pub action: Action,

    /// Whether this rule is active.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// A condition that evaluates a field in a JSON payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Condition {
    /// JSON field path (supports dot notation: "nested.field").
    pub field: String,

    /// Comparison operator.
    pub operator: Operator,

    /// Value to compare against.
    pub value: serde_json::Value,
}

/// Comparison operators for conditions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Operator {
    #[serde(rename = "==", alias = "eq")]
    Eq,
    #[serde(rename = "!=", alias = "ne")]
    Ne,
    #[serde(rename = ">", alias = "gt")]
    Gt,
    #[serde(rename = ">=", alias = "gte")]
    Gte,
    #[serde(rename = "<", alias = "lt")]
    Lt,
    #[serde(rename = "<=", alias = "lte")]
    Lte,
    #[serde(rename = "contains")]
    Contains,
}

impl Condition {
    /// Evaluate this condition against a JSON value.
    pub fn evaluate(&self, json: &serde_json::Value) -> bool {
        let field_val = self.resolve_field(json);
        match field_val {
            Some(val) => self.compare(val, &self.value),
            None => false, // Field not found → condition not met
        }
    }

    /// Resolve a dot-notation field path in a JSON value.
    fn resolve_field<'a>(&self, json: &'a serde_json::Value) -> Option<&'a serde_json::Value> {
        let mut current = json;
        for part in self.field.split('.') {
            current = current.get(part)?;
        }
        Some(current)
    }

    /// Compare two JSON values with the operator.
    fn compare(&self, actual: &serde_json::Value, expected: &serde_json::Value) -> bool {
        match self.operator {
            Operator::Eq => json_eq(actual, expected),
            Operator::Ne => !json_eq(actual, expected),
            Operator::Gt => json_cmp(actual, expected).is_some_and(|o| o.is_gt()),
            Operator::Gte => json_cmp(actual, expected).is_some_and(|o| o.is_ge()),
            Operator::Lt => json_cmp(actual, expected).is_some_and(|o| o.is_lt()),
            Operator::Lte => json_cmp(actual, expected).is_some_and(|o| o.is_le()),
            Operator::Contains => json_contains(actual, expected),
        }
    }
}

/// Compare two JSON values for equality (type-coercing numbers).
fn json_eq(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    match (a, b) {
        (serde_json::Value::Number(a), serde_json::Value::Number(b)) => {
            a.as_f64() == b.as_f64()
        }
        (serde_json::Value::String(a), serde_json::Value::String(b)) => a == b,
        (serde_json::Value::Bool(a), serde_json::Value::Bool(b)) => a == b,
        _ => a == b,
    }
}

/// Compare two JSON values for ordering (numbers only).
fn json_cmp(
    a: &serde_json::Value,
    b: &serde_json::Value,
) -> Option<std::cmp::Ordering> {
    let a_f = a.as_f64()?;
    let b_f = b.as_f64()?;
    a_f.partial_cmp(&b_f)
}

/// Check if a string value contains a substring.
fn json_contains(actual: &serde_json::Value, expected: &serde_json::Value) -> bool {
    match (actual.as_str(), expected.as_str()) {
        (Some(a), Some(b)) => a.contains(b),
        _ => false,
    }
}

/// Action to execute when a rule triggers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Action {
    /// Log a message.
    #[serde(rename = "log")]
    Log {
        message: String,
        #[serde(default = "default_log_level")]
        level: String,
    },

    /// Send a command to a node's command queryable.
    #[serde(rename = "command")]
    Command {
        /// Target node name.
        node: String,
        /// Command name.
        command: String,
        /// Optional command parameters.
        #[serde(default)]
        params: serde_json::Value,
    },

    /// Publish a message to a Zenoh topic.
    #[serde(rename = "publish")]
    Publish {
        /// Zenoh key expression to publish to.
        topic: String,
        /// JSON payload to publish.
        payload: serde_json::Value,
    },
}

fn default_log_level() -> String {
    "warn".to_string()
}

impl Action {
    /// Get the target node name (for override checking).
    pub fn target_node(&self) -> Option<String> {
        match self {
            Action::Command { node, .. } => Some(node.clone()),
            _ => None,
        }
    }

    /// Execute this action.
    /// `scope` and `machine_id` are used to build scoped Zenoh key expressions
    /// that target only the local machine, preventing cross-machine broadcast.
    pub async fn execute(&self, session: &zenoh::Session, scope: &str, machine_id: &str) {
        match self {
            Action::Log { message, level } => {
                match level.as_str() {
                    "error" => log::error!("[RULE] {}", message),
                    "warn" => log::warn!("[RULE] {}", message),
                    "info" => log::info!("[RULE] {}", message),
                    _ => log::debug!("[RULE] {}", message),
                }
            }
            Action::Command {
                node,
                command,
                params,
            } => {
                if let Err(e) = crate::validation::validate_node_name(node) {
                    log::warn!("[RULE] Invalid node name in command action: {}", e);
                    return;
                }
                let key_expr = format!("bubbaloop/{}/{}/{}/command", scope, machine_id, node);
                let payload = serde_json::json!({
                    "command": command,
                    "params": params,
                });
                let payload_bytes = serde_json::to_vec(&payload).unwrap_or_default();

                match session
                    .get(&key_expr)
                    .payload(zenoh::bytes::ZBytes::from(payload_bytes))
                    .timeout(std::time::Duration::from_secs(5))
                    .await
                {
                    Ok(replies) => {
                        while let Ok(reply) = replies.recv_async().await {
                            match reply.result() {
                                Ok(sample) => {
                                    let bytes = sample.payload().to_bytes();
                                    let text = String::from_utf8_lossy(&bytes);
                                    log::info!(
                                        "[RULE] Command '{}' to '{}': {}",
                                        command,
                                        node,
                                        text
                                    );
                                }
                                Err(err) => {
                                    let err_bytes = err.payload().to_bytes();
                                    log::warn!(
                                        "[RULE] Command '{}' to '{}' error: {:?}",
                                        command,
                                        node,
                                        err_bytes
                                    );
                                }
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("[RULE] Failed to send command to '{}': {}", node, e);
                    }
                }
            }
            Action::Publish { topic, payload } => {
                if let Err(e) = crate::validation::validate_publish_topic(topic) {
                    log::warn!("[RULE] Invalid publish topic in action: {}", e);
                    return;
                }
                let payload_bytes = serde_json::to_vec(payload).unwrap_or_default();
                if let Err(e) = session.put(topic, payload_bytes).await {
                    log::warn!("[RULE] Failed to publish to '{}': {}", topic, e);
                } else {
                    log::info!("[RULE] Published to '{}'", topic);
                }
            }
        }
    }
}

/// Record of a rule trigger.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleTriggerLog {
    pub last_triggered_ms: i64,
    pub trigger_key: String,
    pub trigger_count: u64,
}

/// Agent status snapshot (for MCP tools and status queryable).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStatus {
    pub rule_count: usize,
    pub rules: Vec<String>,
    pub recent_triggers: HashMap<String, RuleTriggerLog>,
    pub active_overrides: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_rule_config() {
        let yaml = r#"
rules:
  - name: "high-temp"
    trigger: "bubbaloop/**/telemetry/status"
    condition:
      field: "cpu_temp"
      operator: ">"
      value: 80.0
    action:
      type: "log"
      message: "CPU too hot!"
  - name: "always-log"
    trigger: "bubbaloop/**/health"
    action:
      type: "log"
      message: "Health check received"
"#;
        let config: RuleConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.rules.len(), 2);
        assert_eq!(config.rules[0].name, "high-temp");
        assert!(config.rules[0].condition.is_some());
        assert!(config.rules[0].enabled);
        assert_eq!(config.rules[1].name, "always-log");
        assert!(config.rules[1].condition.is_none());
    }

    #[test]
    fn test_parse_command_action() {
        let yaml = r#"
rules:
  - name: "capture-frame"
    trigger: "bubbaloop/**/motion/detected"
    condition:
      field: "confidence"
      operator: ">="
      value: 0.8
    action:
      type: "command"
      node: "rtsp-camera"
      command: "capture_frame"
      params:
        resolution: "1080p"
"#;
        let config: RuleConfig = serde_yaml::from_str(yaml).unwrap();
        let rule = &config.rules[0];
        match &rule.action {
            Action::Command {
                node,
                command,
                params,
            } => {
                assert_eq!(node, "rtsp-camera");
                assert_eq!(command, "capture_frame");
                assert_eq!(params["resolution"], "1080p");
            }
            _ => panic!("Expected Command action"),
        }
    }

    #[test]
    fn test_parse_publish_action() {
        let yaml = r#"
rules:
  - name: "forward-alert"
    trigger: "bubbaloop/**/alert"
    action:
      type: "publish"
      topic: "bubbaloop/local/alerts/aggregated"
      payload:
        source: "agent"
        severity: "high"
"#;
        let config: RuleConfig = serde_yaml::from_str(yaml).unwrap();
        match &config.rules[0].action {
            Action::Publish { topic, payload } => {
                assert_eq!(topic, "bubbaloop/local/alerts/aggregated");
                assert_eq!(payload["severity"], "high");
            }
            _ => panic!("Expected Publish action"),
        }
    }

    #[test]
    fn test_condition_gt() {
        let cond = Condition {
            field: "cpu_temp".to_string(),
            operator: Operator::Gt,
            value: json!(80.0),
        };
        assert!(cond.evaluate(&json!({"cpu_temp": 85.0})));
        assert!(!cond.evaluate(&json!({"cpu_temp": 75.0})));
        assert!(!cond.evaluate(&json!({"cpu_temp": 80.0})));
    }

    #[test]
    fn test_condition_eq_string() {
        let cond = Condition {
            field: "status".to_string(),
            operator: Operator::Eq,
            value: json!("ok"),
        };
        assert!(cond.evaluate(&json!({"status": "ok"})));
        assert!(!cond.evaluate(&json!({"status": "error"})));
    }

    #[test]
    fn test_condition_ne() {
        let cond = Condition {
            field: "status".to_string(),
            operator: Operator::Ne,
            value: json!("ok"),
        };
        assert!(!cond.evaluate(&json!({"status": "ok"})));
        assert!(cond.evaluate(&json!({"status": "error"})));
    }

    #[test]
    fn test_condition_lte() {
        let cond = Condition {
            field: "memory_pct".to_string(),
            operator: Operator::Lte,
            value: json!(90.0),
        };
        assert!(cond.evaluate(&json!({"memory_pct": 85.0})));
        assert!(cond.evaluate(&json!({"memory_pct": 90.0})));
        assert!(!cond.evaluate(&json!({"memory_pct": 95.0})));
    }

    #[test]
    fn test_condition_contains() {
        let cond = Condition {
            field: "message".to_string(),
            operator: Operator::Contains,
            value: json!("error"),
        };
        assert!(cond.evaluate(&json!({"message": "connection error occurred"})));
        assert!(!cond.evaluate(&json!({"message": "all good"})));
    }

    #[test]
    fn test_condition_nested_field() {
        let cond = Condition {
            field: "sensors.temp.value".to_string(),
            operator: Operator::Gt,
            value: json!(40.0),
        };
        let data = json!({
            "sensors": {
                "temp": {
                    "value": 42.5
                }
            }
        });
        assert!(cond.evaluate(&data));
    }

    #[test]
    fn test_condition_missing_field() {
        let cond = Condition {
            field: "nonexistent".to_string(),
            operator: Operator::Gt,
            value: json!(0),
        };
        assert!(!cond.evaluate(&json!({"other": 10})));
    }

    #[test]
    fn test_disabled_rule() {
        let yaml = r#"
rules:
  - name: "disabled-rule"
    trigger: "bubbaloop/**/status"
    enabled: false
    action:
      type: "log"
      message: "Should not fire"
"#;
        let config: RuleConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(!config.rules[0].enabled);
    }

    #[test]
    fn test_action_target_node() {
        let log_action = Action::Log {
            message: "test".to_string(),
            level: "info".to_string(),
        };
        assert!(log_action.target_node().is_none());

        let cmd_action = Action::Command {
            node: "camera".to_string(),
            command: "capture".to_string(),
            params: json!({}),
        };
        assert_eq!(cmd_action.target_node(), Some("camera".to_string()));
    }

    #[test]
    fn test_empty_rules_file() {
        let yaml = "rules: []";
        let config: RuleConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.rules.is_empty());
    }

    #[test]
    fn test_operator_aliases() {
        // Test that string aliases work
        let yaml = r#"
rules:
  - name: "test-eq"
    trigger: "test"
    condition:
      field: "x"
      operator: "eq"
      value: 1
    action:
      type: "log"
      message: "test"
"#;
        let config: RuleConfig = serde_yaml::from_str(yaml).unwrap();
        let cond = config.rules[0].condition.as_ref().unwrap();
        assert!(cond.evaluate(&json!({"x": 1})));
    }

    #[test]
    fn test_condition_integer_vs_float_comparison() {
        // JSON "1" should equal "1.0" for numeric comparison
        let cond = Condition {
            field: "value".to_string(),
            operator: Operator::Eq,
            value: json!(1.0),
        };
        assert!(cond.evaluate(&json!({"value": 1})));
    }

    #[test]
    fn test_condition_boolean() {
        let cond = Condition {
            field: "active".to_string(),
            operator: Operator::Eq,
            value: json!(true),
        };
        assert!(cond.evaluate(&json!({"active": true})));
        assert!(!cond.evaluate(&json!({"active": false})));
    }

    #[test]
    fn test_condition_deeply_nested() {
        let cond = Condition {
            field: "a.b.c.d".to_string(),
            operator: Operator::Eq,
            value: json!("found"),
        };
        let data = json!({"a": {"b": {"c": {"d": "found"}}}});
        assert!(cond.evaluate(&data));
        let bad_data = json!({"a": {"b": {"x": 1}}});
        assert!(!cond.evaluate(&bad_data));
    }

    #[test]
    fn test_condition_null_field_value() {
        let cond = Condition {
            field: "val".to_string(),
            operator: Operator::Eq,
            value: json!(null),
        };
        assert!(cond.evaluate(&json!({"val": null})));
        assert!(!cond.evaluate(&json!({"val": 0})));
    }

    #[test]
    fn test_condition_lt_boundary() {
        let cond = Condition {
            field: "x".to_string(),
            operator: Operator::Lt,
            value: json!(10),
        };
        assert!(cond.evaluate(&json!({"x": 9})));
        assert!(!cond.evaluate(&json!({"x": 10})));
        assert!(!cond.evaluate(&json!({"x": 11})));
    }

    #[test]
    fn test_condition_gte_boundary() {
        let cond = Condition {
            field: "x".to_string(),
            operator: Operator::Gte,
            value: json!(5.0),
        };
        assert!(cond.evaluate(&json!({"x": 5.0})));
        assert!(cond.evaluate(&json!({"x": 5.1})));
        assert!(!cond.evaluate(&json!({"x": 4.9})));
    }

    #[test]
    fn test_condition_contains_empty_string() {
        let cond = Condition {
            field: "msg".to_string(),
            operator: Operator::Contains,
            value: json!(""),
        };
        // Every string contains the empty string
        assert!(cond.evaluate(&json!({"msg": "anything"})));
    }

    #[test]
    fn test_condition_contains_non_string() {
        // Contains on non-string types should return false
        let cond = Condition {
            field: "num".to_string(),
            operator: Operator::Contains,
            value: json!("5"),
        };
        assert!(!cond.evaluate(&json!({"num": 5})));
    }

    #[test]
    fn test_condition_gt_non_numeric() {
        // Gt on strings should fail gracefully
        let cond = Condition {
            field: "name".to_string(),
            operator: Operator::Gt,
            value: json!("abc"),
        };
        assert!(!cond.evaluate(&json!({"name": "xyz"})));
    }

    #[test]
    fn test_publish_action_target_node() {
        let action = Action::Publish {
            topic: "test/topic".to_string(),
            payload: json!({"key": "value"}),
        };
        assert!(action.target_node().is_none());
    }

    #[test]
    fn test_full_yaml_roundtrip() {
        // Parse a complex config, serialize back, parse again
        let yaml = r#"
rules:
  - name: "complex-rule"
    trigger: "bubbaloop/**/telemetry/status"
    enabled: true
    condition:
      field: "sensors.temp.celsius"
      operator: ">="
      value: 45.5
    action:
      type: "command"
      node: "sprinkler-controller"
      command: "activate"
      params:
        zone: "garden"
        duration_s: 300
"#;
        let config: RuleConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.rules.len(), 1);

        let rule = &config.rules[0];
        assert_eq!(rule.name, "complex-rule");
        assert!(rule.enabled);

        let cond = rule.condition.as_ref().unwrap();
        assert_eq!(cond.field, "sensors.temp.celsius");

        // Verify condition evaluates correctly
        let hot = json!({"sensors": {"temp": {"celsius": 50.0}}});
        let cool = json!({"sensors": {"temp": {"celsius": 30.0}}});
        assert!(cond.evaluate(&hot));
        assert!(!cond.evaluate(&cool));

        match &rule.action {
            Action::Command { node, command, params } => {
                assert_eq!(node, "sprinkler-controller");
                assert_eq!(command, "activate");
                assert_eq!(params["zone"], "garden");
                assert_eq!(params["duration_s"], 300);
            }
            _ => panic!("Expected Command action"),
        }
    }

    #[test]
    fn test_multiple_rules_same_trigger() {
        let yaml = r#"
rules:
  - name: "warn-temp"
    trigger: "bubbaloop/**/status"
    condition:
      field: "temp"
      operator: ">"
      value: 70
    action:
      type: "log"
      message: "Temperature warning"
      level: "warn"
  - name: "critical-temp"
    trigger: "bubbaloop/**/status"
    condition:
      field: "temp"
      operator: ">"
      value: 90
    action:
      type: "log"
      message: "Temperature CRITICAL"
      level: "error"
"#;
        let config: RuleConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.rules.len(), 2);
        assert_eq!(config.rules[0].trigger, config.rules[1].trigger);

        let cond0 = config.rules[0].condition.as_ref().unwrap();
        let cond1 = config.rules[1].condition.as_ref().unwrap();

        let data = json!({"temp": 85});
        assert!(cond0.evaluate(&data));  // > 70
        assert!(!cond1.evaluate(&data)); // not > 90

        let critical_data = json!({"temp": 95});
        assert!(cond0.evaluate(&critical_data));
        assert!(cond1.evaluate(&critical_data));
    }

    #[test]
    fn test_rule_no_condition_always_triggers() {
        let yaml = r#"
rules:
  - name: "log-everything"
    trigger: "bubbaloop/**/data"
    action:
      type: "log"
      message: "Data received"
"#;
        let config: RuleConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.rules[0].condition.is_none());
        // No condition = always triggers (tested in evaluate_rules_for_sample)
    }

    #[test]
    fn test_agent_status_serialization() {
        let status = AgentStatus {
            rule_count: 2,
            rules: vec!["rule-a".to_string(), "rule-b".to_string()],
            recent_triggers: HashMap::from([(
                "rule-a".to_string(),
                RuleTriggerLog {
                    last_triggered_ms: 1234567890,
                    trigger_key: "bubbaloop/local/m1/sensor/status".to_string(),
                    trigger_count: 5,
                },
            )]),
            active_overrides: 1,
        };
        let json = serde_json::to_string(&status).unwrap();
        let deserialized: AgentStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.rule_count, 2);
        assert_eq!(deserialized.rules.len(), 2);
        assert_eq!(deserialized.active_overrides, 1);
        assert_eq!(deserialized.recent_triggers["rule-a"].trigger_count, 5);
    }

    #[test]
    fn test_log_action_default_level() {
        let yaml = r#"
rules:
  - name: "test"
    trigger: "test"
    action:
      type: "log"
      message: "hello"
"#;
        let config: RuleConfig = serde_yaml::from_str(yaml).unwrap();
        match &config.rules[0].action {
            Action::Log { level, .. } => assert_eq!(level, "warn"),
            _ => panic!("Expected Log action"),
        }
    }
}
