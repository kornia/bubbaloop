//! Role-Based Access Control for MCP tools.
//!
//! Three tiers: viewer (read-only), operator (day-to-day), admin (system).
//! Token file format: `<token>:<tier>` (e.g., `bb_abc123:admin`).
//! Default tier if unspecified: `operator`.

use serde::{Deserialize, Serialize};

/// Authorization tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Viewer = 0,
    Operator = 1,
    Admin = 2,
}

impl Tier {
    /// Check if this tier has at least the required level.
    pub fn has_permission(self, required: Tier) -> bool {
        (self as u8) >= (required as u8)
    }
}

impl std::fmt::Display for Tier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Tier::Viewer => write!(f, "viewer"),
            Tier::Operator => write!(f, "operator"),
            Tier::Admin => write!(f, "admin"),
        }
    }
}

impl std::str::FromStr for Tier {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "viewer" => Ok(Tier::Viewer),
            "operator" => Ok(Tier::Operator),
            "admin" => Ok(Tier::Admin),
            _ => Err(format!(
                "Unknown tier '{}' — must be viewer, operator, or admin",
                s
            )),
        }
    }
}

/// Tool-to-tier mapping.
pub fn required_tier(tool_name: &str) -> Tier {
    match tool_name {
        // Viewer tools (read-only)
        "list_nodes" | "get_node_health" | "get_node_schema" | "get_stream_info"
        | "list_topics" | "get_system_status" | "get_machine_info" | "doctor"
        | "discover_nodes" | "get_node_manifest" | "list_commands"
        | "discover_capabilities" => Tier::Viewer,

        // Operator tools (day-to-day operations)
        "start_node" | "stop_node" | "restart_node" | "get_node_config" | "set_node_config"
        | "read_sensor" | "send_command" | "get_node_logs" => Tier::Operator,

        // Admin tools (system modification)
        "install_node"
        | "remove_node"
        | "build_node"
        | "create_node_instance"
        | "set_system_config"
        | "query_zenoh" => Tier::Admin,

        // Unknown tools default to admin (principle of least privilege)
        _ => Tier::Admin,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tier_ordering() {
        assert!(Tier::Admin.has_permission(Tier::Viewer));
        assert!(Tier::Admin.has_permission(Tier::Operator));
        assert!(Tier::Admin.has_permission(Tier::Admin));
        assert!(Tier::Operator.has_permission(Tier::Viewer));
        assert!(Tier::Operator.has_permission(Tier::Operator));
        assert!(!Tier::Operator.has_permission(Tier::Admin));
        assert!(Tier::Viewer.has_permission(Tier::Viewer));
        assert!(!Tier::Viewer.has_permission(Tier::Operator));
    }

    #[test]
    fn test_tier_parse() {
        assert_eq!("viewer".parse::<Tier>().unwrap(), Tier::Viewer);
        assert_eq!("operator".parse::<Tier>().unwrap(), Tier::Operator);
        assert_eq!("admin".parse::<Tier>().unwrap(), Tier::Admin);
        assert!("unknown".parse::<Tier>().is_err());
    }

    #[test]
    fn test_required_tier_viewer_tools() {
        assert_eq!(required_tier("list_nodes"), Tier::Viewer);
        assert_eq!(required_tier("get_node_health"), Tier::Viewer);
        assert_eq!(required_tier("discover_nodes"), Tier::Viewer);
    }

    #[test]
    fn test_required_tier_operator_tools() {
        assert_eq!(required_tier("start_node"), Tier::Operator);
        assert_eq!(required_tier("send_command"), Tier::Operator);
    }

    #[test]
    fn test_required_tier_admin_tools() {
        assert_eq!(required_tier("query_zenoh"), Tier::Admin);
        assert_eq!(required_tier("install_node"), Tier::Admin);
    }

    #[test]
    fn test_unknown_tool_requires_admin() {
        assert_eq!(required_tier("nonexistent_tool"), Tier::Admin);
    }
}
