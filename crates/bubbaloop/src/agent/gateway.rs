//! Gateway wire format and topic builders for multi-agent Zenoh messaging.
//!
//! The Gateway is a convention (topic pair + JSON schema), not a process.
//! Messages flow through Zenoh pub/sub between CLI clients and agent runtimes.

use serde::{Deserialize, Serialize};

// ── Inbox (CLI → Daemon) ─────────────────────────────────────────

/// A message sent to an agent's shared inbox.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentMessage {
    /// Correlation ID (UUID) for matching outbox responses.
    pub id: String,
    /// User's text message.
    pub text: String,
    /// Target agent ID. If None, routes to the default agent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
}

// ── Outbox (Daemon → CLI) ────────────────────────────────────────

/// Event type for outbox messages.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AgentEventType {
    /// Incremental text token from the LLM.
    Delta,
    /// Tool call initiated (text = tool name).
    Tool,
    /// Tool call result.
    ToolResult,
    /// Error message.
    Error,
    /// Turn complete — no more events for this correlation ID.
    Done,
}

/// An event emitted by an agent on its outbox topic.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentEvent {
    /// Correlation ID matching the inbox message.
    pub id: String,
    /// Event type.
    #[serde(rename = "type")]
    pub event_type: AgentEventType,
    /// Event payload (text delta, tool name, error message, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Truncated tool input JSON (only present on Tool events).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
}

impl AgentEvent {
    /// Create a Delta event (streaming text token).
    pub fn delta(id: &str, text: &str) -> Self {
        Self {
            id: id.to_string(),
            event_type: AgentEventType::Delta,
            text: Some(text.to_string()),
            input: None,
        }
    }

    /// Create a Tool event (tool call started).
    ///
    /// `input` is an optional truncated JSON preview of the tool's arguments.
    pub fn tool(id: &str, tool_name: &str, input: Option<&str>) -> Self {
        Self {
            id: id.to_string(),
            event_type: AgentEventType::Tool,
            text: Some(tool_name.to_string()),
            input: input.map(|s| s.to_string()),
        }
    }

    /// Create a ToolResult event.
    pub fn tool_result(id: &str, result: &str) -> Self {
        Self {
            id: id.to_string(),
            event_type: AgentEventType::ToolResult,
            text: Some(result.to_string()),
            input: None,
        }
    }

    /// Create an Error event.
    pub fn error(id: &str, message: &str) -> Self {
        Self {
            id: id.to_string(),
            event_type: AgentEventType::Error,
            text: Some(message.to_string()),
            input: None,
        }
    }

    /// Create a Done event (turn complete).
    pub fn done(id: &str) -> Self {
        Self {
            id: id.to_string(),
            event_type: AgentEventType::Done,
            text: None,
            input: None,
        }
    }
}

// ── Manifest (queryable) ─────────────────────────────────────────

/// Agent manifest — advertises capabilities via Zenoh queryable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentManifest {
    /// Unique agent identifier (e.g., "jean-clawd").
    pub agent_id: String,
    /// Human-readable display name.
    pub name: String,
    /// Capability keywords for routing (e.g., ["camera", "rtsp"]).
    pub capabilities: Vec<String>,
    /// Claude model name.
    pub model: String,
    /// Whether this is the default agent for unaddressed messages.
    pub is_default: bool,
    /// Machine ID where this agent is running.
    #[serde(default)]
    pub machine_id: String,
}

// ── Topic builders ───────────────────────────────────────────────

/// Build the shared agent inbox topic.
///
/// Format: `bubbaloop/{scope}/{machine}/agent/inbox`
pub fn inbox_topic(scope: &str, machine_id: &str) -> String {
    format!("bubbaloop/{}/{}/agent/inbox", scope, machine_id)
}

/// Build a per-agent outbox topic.
///
/// Format: `bubbaloop/{scope}/{machine}/agent/{agent_id}/outbox`
pub fn outbox_topic(scope: &str, machine_id: &str, agent_id: &str) -> String {
    format!(
        "bubbaloop/{}/{}/agent/{}/outbox",
        scope, machine_id, agent_id
    )
}

/// Build a per-agent manifest topic (queryable).
///
/// Format: `bubbaloop/{scope}/{machine}/agent/{agent_id}/manifest`
pub fn manifest_topic(scope: &str, machine_id: &str, agent_id: &str) -> String {
    format!(
        "bubbaloop/{}/{}/agent/{}/manifest",
        scope, machine_id, agent_id
    )
}

/// Build a wildcard pattern for discovering all agent manifests.
///
/// Format: `bubbaloop/{scope}/{machine}/agent/*/manifest`
pub fn manifest_wildcard(scope: &str, machine_id: &str) -> String {
    format!("bubbaloop/{}/{}/agent/*/manifest", scope, machine_id)
}

/// Build a wildcard pattern for discovering agents on ALL machines.
///
/// Format: `bubbaloop/{scope}/*/agent/*/manifest`
pub fn manifest_wildcard_all(scope: &str) -> String {
    format!("bubbaloop/{}/*/agent/*/manifest", scope)
}

/// Build a wildcard pattern for subscribing to all agent outboxes.
///
/// Format: `bubbaloop/{scope}/{machine}/agent/*/outbox`
pub fn outbox_wildcard(scope: &str, machine_id: &str) -> String {
    format!("bubbaloop/{}/{}/agent/*/outbox", scope, machine_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_message_serde_roundtrip() {
        let msg = AgentMessage {
            id: "abc-123".to_string(),
            text: "list my nodes".to_string(),
            agent: Some("camera-expert".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: AgentMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, parsed);
    }

    #[test]
    fn agent_message_no_agent_field() {
        let msg = AgentMessage {
            id: "abc-123".to_string(),
            text: "hello".to_string(),
            agent: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(!json.contains("agent"));
        let parsed: AgentMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, parsed);
    }

    #[test]
    fn agent_event_delta_serde() {
        let event = AgentEvent::delta("id-1", "Hello");
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"delta\""));
        let parsed: AgentEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, parsed);
    }

    #[test]
    fn agent_event_tool_serde() {
        let event = AgentEvent::tool("id-1", "list_nodes", None);
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"tool\""));
        assert!(json.contains("list_nodes"));
    }

    #[test]
    fn agent_event_done_no_text() {
        let event = AgentEvent::done("id-1");
        let json = serde_json::to_string(&event).unwrap();
        assert!(!json.contains("\"text\""));
    }

    #[test]
    fn agent_event_all_types_roundtrip() {
        let events = vec![
            AgentEvent::delta("id", "token"),
            AgentEvent::tool("id", "get_health", None),
            AgentEvent::tool_result("id", "ok"),
            AgentEvent::error("id", "429 rate limit"),
            AgentEvent::done("id"),
        ];
        for event in events {
            let json = serde_json::to_string(&event).unwrap();
            let parsed: AgentEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(event, parsed);
        }
    }

    #[test]
    fn agent_manifest_serde_roundtrip() {
        let manifest = AgentManifest {
            agent_id: "jean-clawd".to_string(),
            name: "Jean-Clawd".to_string(),
            capabilities: vec!["general".to_string()],
            model: "claude-sonnet-4-20250514".to_string(),
            is_default: true,
            machine_id: "jetson01".to_string(),
        };
        let json = serde_json::to_string(&manifest).unwrap();
        let parsed: AgentManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(manifest, parsed);
    }

    #[test]
    fn inbox_topic_format() {
        assert_eq!(
            inbox_topic("local", "jetson01"),
            "bubbaloop/local/jetson01/agent/inbox"
        );
    }

    #[test]
    fn outbox_topic_format() {
        assert_eq!(
            outbox_topic("local", "jetson01", "jean-clawd"),
            "bubbaloop/local/jetson01/agent/jean-clawd/outbox"
        );
    }

    #[test]
    fn manifest_topic_format() {
        assert_eq!(
            manifest_topic("local", "jetson01", "camera-expert"),
            "bubbaloop/local/jetson01/agent/camera-expert/manifest"
        );
    }

    #[test]
    fn manifest_wildcard_format() {
        assert_eq!(
            manifest_wildcard("local", "jetson01"),
            "bubbaloop/local/jetson01/agent/*/manifest"
        );
    }

    #[test]
    fn outbox_wildcard_format() {
        assert_eq!(
            outbox_wildcard("local", "jetson01"),
            "bubbaloop/local/jetson01/agent/*/outbox"
        );
    }

    #[test]
    fn manifest_wildcard_all_format() {
        assert_eq!(
            manifest_wildcard_all("local"),
            "bubbaloop/local/*/agent/*/manifest"
        );
    }

    #[test]
    fn agent_manifest_machine_id_default() {
        // machine_id should default to empty string for backward compat
        let json =
            r#"{"agent_id":"a","name":"A","capabilities":[],"model":"m","is_default":false}"#;
        let manifest: AgentManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.machine_id, "");
    }
}
