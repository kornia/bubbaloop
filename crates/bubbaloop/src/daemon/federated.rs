//! Federated world state sharing — gossip between machines via Zenoh.

/// Configuration for federated world state gossip.
pub struct FederatedConfig {
    /// How often to publish world state diffs (default 30s)
    pub publish_interval_secs: u64,
    /// Maximum age of entries to publish (stale entries skipped)
    pub max_stale_secs: u64,
}

impl Default for FederatedConfig {
    fn default() -> Self {
        Self {
            publish_interval_secs: 30,
            max_stale_secs: 300,
        }
    }
}

/// Build the Zenoh topic for publishing world state from this machine.
/// Format: `bubbaloop/{scope}/{machine_id}/agent/{agent_id}/world_state`
pub fn world_state_topic(scope: &str, machine_id: &str, agent_id: &str) -> String {
    format!("bubbaloop/{scope}/{machine_id}/agent/{agent_id}/world_state")
}

/// Build the subscription pattern for remote world states (other machines).
/// Format: `bubbaloop/**/agent/*/world_state`
pub fn remote_world_state_pattern() -> &'static str {
    "bubbaloop/**/agent/*/world_state"
}

/// Extract machine_id from a remote world state topic.
/// Topic format: `bubbaloop/{scope}/{machine_id}/agent/{agent_id}/world_state`
/// Returns None if format doesn't match.
pub fn extract_machine_id(topic: &str) -> Option<&str> {
    let parts: Vec<&str> = topic.split('/').collect();
    // [bubbaloop, {scope}, {machine_id}, agent, {agent_id}, world_state]
    if parts.len() == 6
        && parts[0] == "bubbaloop"
        && parts[3] == "agent"
        && parts[5] == "world_state"
    {
        Some(parts[2])
    } else {
        None
    }
}

/// Add the `remote:{machine_id}.` prefix to a world state key when merging remote entries.
pub fn prefix_remote_key(machine_id: &str, key: &str) -> String {
    format!("remote:{machine_id}.{key}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_state_topic_format() {
        let topic = world_state_topic("prod", "jetson-1", "rex");
        assert_eq!(topic, "bubbaloop/prod/jetson-1/agent/rex/world_state");
    }

    #[test]
    fn extract_machine_id_valid_topic() {
        let topic = "bubbaloop/prod/jetson-1/agent/rex/world_state";
        assert_eq!(extract_machine_id(topic), Some("jetson-1"));
    }

    #[test]
    fn extract_machine_id_invalid_topic() {
        assert_eq!(extract_machine_id("bubbaloop/prod/world_state"), None);
        assert_eq!(extract_machine_id("other/path"), None);
    }

    #[test]
    fn prefix_remote_key_format() {
        let key = prefix_remote_key("jetson-2", "dog.location");
        assert_eq!(key, "remote:jetson-2.dog.location");
    }

    #[test]
    fn remote_world_state_pattern_is_wildcard() {
        let pattern = remote_world_state_pattern();
        assert!(pattern.contains("**"));
        assert!(pattern.ends_with("world_state"));
    }

    #[test]
    fn default_config_values() {
        let config = FederatedConfig::default();
        assert_eq!(config.publish_interval_secs, 30);
        assert_eq!(config.max_stale_secs, 300);
    }
}
