//! Shared input validation for MCP, CLI, and daemon boundaries.

/// Validate a node name: 1-64 chars, `[a-zA-Z0-9_-]` only.
pub fn validate_node_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > 64 {
        return Err(format!(
            "Node name must be 1-64 characters, got {}",
            name.len()
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(
            "Node name may only contain alphanumeric characters, hyphens, and underscores"
                .to_string(),
        );
    }
    Ok(())
}

/// Validate a rule name: same constraints as node names.
pub fn validate_rule_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > 64 {
        return Err(format!(
            "Rule name must be 1-64 characters, got {}",
            name.len()
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(
            "Rule name may only contain alphanumeric characters, hyphens, and underscores"
                .to_string(),
        );
    }
    Ok(())
}

/// Validate a publish topic: must start with `bubbaloop/`, no wildcards, max 256 chars.
pub fn validate_publish_topic(topic: &str) -> Result<(), String> {
    if topic.is_empty() || topic.len() > 256 {
        return Err(format!(
            "Publish topic must be 1-256 characters, got {}",
            topic.len()
        ));
    }
    if !topic.starts_with("bubbaloop/") {
        return Err("Publish topic must start with 'bubbaloop/'".to_string());
    }
    if topic.contains('*') {
        return Err("Publish topic must not contain wildcards".to_string());
    }
    if !topic.chars().all(|c| c.is_alphanumeric() || "/-_.".contains(c)) {
        return Err("Publish topic contains invalid characters".to_string());
    }
    Ok(())
}

/// Validate a trigger pattern: must start with `bubbaloop/`.
pub fn validate_trigger_pattern(trigger: &str) -> Result<(), String> {
    if !trigger.starts_with("bubbaloop/") {
        return Err("Trigger pattern must start with 'bubbaloop/'".to_string());
    }
    if trigger.len() > 256 {
        return Err("Trigger pattern must be at most 256 characters".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_node_name_valid() {
        assert!(validate_node_name("rtsp-camera").is_ok());
        assert!(validate_node_name("my_node").is_ok());
        assert!(validate_node_name("node123").is_ok());
        assert!(validate_node_name("a").is_ok());
        assert!(validate_node_name(&"a".repeat(64)).is_ok());
    }

    #[test]
    fn test_validate_node_name_invalid() {
        assert!(validate_node_name("").is_err());
        assert!(validate_node_name(&"a".repeat(65)).is_err());
        assert!(validate_node_name("node/slash").is_err());
        assert!(validate_node_name("node with spaces").is_err());
        assert!(validate_node_name("**").is_err());
        assert!(validate_node_name("node;rm").is_err());
        assert!(validate_node_name("../../../etc").is_err());
    }

    #[test]
    fn test_validate_rule_name_valid() {
        assert!(validate_rule_name("high-temp-alert").is_ok());
        assert!(validate_rule_name("cpu_monitor").is_ok());
    }

    #[test]
    fn test_validate_rule_name_invalid() {
        assert!(validate_rule_name("").is_err());
        assert!(validate_rule_name("rule with spaces").is_err());
        assert!(validate_rule_name(&"x".repeat(65)).is_err());
    }

    #[test]
    fn test_validate_publish_topic_valid() {
        assert!(validate_publish_topic("bubbaloop/local/jetson1/my-node/data").is_ok());
        assert!(validate_publish_topic("bubbaloop/scope/machine/node/resource").is_ok());
    }

    #[test]
    fn test_validate_publish_topic_invalid() {
        assert!(validate_publish_topic("").is_err());
        assert!(validate_publish_topic("other/topic").is_err());
        assert!(validate_publish_topic("bubbaloop/**/all").is_err());
        assert!(validate_publish_topic("bubbaloop/bad topic!").is_err());
    }

    #[test]
    fn test_validate_trigger_pattern_valid() {
        assert!(validate_trigger_pattern("bubbaloop/**/telemetry/status").is_ok());
        assert!(validate_trigger_pattern("bubbaloop/local/node1/metrics").is_ok());
    }

    #[test]
    fn test_validate_trigger_pattern_invalid() {
        assert!(validate_trigger_pattern("**").is_err());
        assert!(validate_trigger_pattern("other/namespace/topic").is_err());
        assert!(validate_trigger_pattern("").is_err());
    }
}
