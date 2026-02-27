//! Raw reqwest Claude API client with tool_use support.
//!
//! Provides a minimal client for the Anthropic Messages API, supporting
//! text messages and tool use (function calling). No streaming — single
//! request/response only.

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ── Constants ───────────────────────────────────────────────────────

/// Anthropic Messages API endpoint.
const API_URL: &str = "https://api.anthropic.com/v1/messages";

/// API version header value.
const API_VERSION: &str = "2023-06-01";

/// Default model when none is specified.
const DEFAULT_MODEL: &str = "claude-sonnet-4-20250514";

/// Default max tokens per response.
const MAX_TOKENS: u32 = 4096;

// ── Errors ──────────────────────────────────────────────────────────

/// Errors from Claude API operations.
#[derive(Debug, thiserror::Error)]
pub enum ClaudeError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("API error (status {status}): {message}")]
    Api { status: u16, message: String },

    #[error("ANTHROPIC_API_KEY not set")]
    MissingApiKey,

    #[error("format error: {0}")]
    Format(String),
}

pub type Result<T> = std::result::Result<T, ClaudeError>;

// ── Content blocks ──────────────────────────────────────────────────

/// A single content block in a message (text, tool_use, or tool_result).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },

    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },

    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}

// ── Message ─────────────────────────────────────────────────────────

/// A conversation message with role and content blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: Vec<ContentBlock>,
}

impl Message {
    /// Create a user message from plain text.
    pub fn user(text: &str) -> Self {
        Self {
            role: "user".to_string(),
            content: vec![ContentBlock::Text {
                text: text.to_string(),
            }],
        }
    }

    /// Create a user message containing tool results.
    ///
    /// Each tuple is `(tool_use_id, content, is_error)`.
    pub fn tool_results(results: Vec<(String, String, Option<bool>)>) -> Self {
        Self {
            role: "user".to_string(),
            content: results
                .into_iter()
                .map(
                    |(tool_use_id, content, is_error)| ContentBlock::ToolResult {
                        tool_use_id,
                        content,
                        is_error,
                    },
                )
                .collect(),
        }
    }

    /// Concatenate all text blocks into a single string.
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    /// Extract all tool_use blocks as `(id, name, input)` tuples.
    pub fn tool_uses(&self) -> Vec<(&str, &str, &Value)> {
        self.content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::ToolUse { id, name, input } => {
                    Some((id.as_str(), name.as_str(), input))
                }
                _ => None,
            })
            .collect()
    }
}

// ── Tool definition ─────────────────────────────────────────────────

/// A tool definition for the Claude API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

// ── API request/response ────────────────────────────────────────────

/// Wire format for the Messages API request (not public).
#[derive(Debug, Serialize)]
struct ApiRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<&'a str>,
    messages: &'a [Message],
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: &'a Vec<ToolDefinition>,
}

/// Token usage from the API response.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

/// Wire format for the Messages API response.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApiResponse {
    pub id: String,
    pub content: Vec<ContentBlock>,
    pub stop_reason: Option<String>,
    pub usage: Usage,
}

// ── Client ──────────────────────────────────────────────────────────

/// A minimal Claude API client.
#[derive(Debug)]
pub struct ClaudeClient {
    client: reqwest::Client,
    api_key: String,
    model: String,
}

impl ClaudeClient {
    /// Create a client from the `ANTHROPIC_API_KEY` environment variable.
    ///
    /// Uses `DEFAULT_MODEL` when `model` is `None`.
    pub fn from_env(model: Option<&str>) -> Result<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| ClaudeError::MissingApiKey)?;

        Ok(Self {
            client: reqwest::Client::new(),
            api_key,
            model: model.unwrap_or(DEFAULT_MODEL).to_string(),
        })
    }

    /// Send a message to the Claude API.
    ///
    /// Returns the parsed API response or a `ClaudeError`.
    pub async fn send(
        &self,
        system: Option<&str>,
        messages: &[Message],
        tools: &Vec<ToolDefinition>,
    ) -> Result<ApiResponse> {
        let body = ApiRequest {
            model: &self.model,
            max_tokens: MAX_TOKENS,
            system,
            messages,
            tools,
        };

        let response = self
            .client
            .post(API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let message = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            return Err(ClaudeError::Api {
                status: status.as_u16(),
                message,
            });
        }

        let api_response: ApiResponse = response.json().await?;
        Ok(api_response)
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn message_user_constructor() {
        let msg = Message::user("hello");
        assert_eq!(msg.role, "user");
        assert_eq!(msg.content.len(), 1);
        assert_eq!(msg.text(), "hello");
    }

    #[test]
    fn message_text_concatenation() {
        let msg = Message {
            role: "assistant".to_string(),
            content: vec![
                ContentBlock::Text {
                    text: "Hello ".to_string(),
                },
                ContentBlock::ToolUse {
                    id: "t1".to_string(),
                    name: "get_weather".to_string(),
                    input: json!({}),
                },
                ContentBlock::Text {
                    text: "world".to_string(),
                },
            ],
        };
        assert_eq!(msg.text(), "Hello world");
    }

    #[test]
    fn message_tool_uses_extraction() {
        let msg = Message {
            role: "assistant".to_string(),
            content: vec![
                ContentBlock::Text {
                    text: "Let me check.".to_string(),
                },
                ContentBlock::ToolUse {
                    id: "tu_1".to_string(),
                    name: "list_nodes".to_string(),
                    input: json!({"status": "running"}),
                },
                ContentBlock::ToolUse {
                    id: "tu_2".to_string(),
                    name: "get_health".to_string(),
                    input: json!({}),
                },
            ],
        };

        let uses = msg.tool_uses();
        assert_eq!(uses.len(), 2);
        assert_eq!(uses[0].0, "tu_1");
        assert_eq!(uses[0].1, "list_nodes");
        assert_eq!(uses[0].2, &json!({"status": "running"}));
        assert_eq!(uses[1].0, "tu_2");
        assert_eq!(uses[1].1, "get_health");
    }

    #[test]
    fn tool_result_message() {
        let msg = Message::tool_results(vec![
            ("tu_1".to_string(), "success".to_string(), None),
            ("tu_2".to_string(), "not found".to_string(), Some(true)),
        ]);

        assert_eq!(msg.role, "user");
        assert_eq!(msg.content.len(), 2);

        match &msg.content[0] {
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => {
                assert_eq!(tool_use_id, "tu_1");
                assert_eq!(content, "success");
                assert_eq!(*is_error, None);
            }
            _ => panic!("expected ToolResult"),
        }

        match &msg.content[1] {
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => {
                assert_eq!(tool_use_id, "tu_2");
                assert_eq!(content, "not found");
                assert_eq!(*is_error, Some(true));
            }
            _ => panic!("expected ToolResult"),
        }
    }

    #[test]
    fn content_block_serde_text() {
        let block = ContentBlock::Text {
            text: "hello".to_string(),
        };
        let json = serde_json::to_string(&block).unwrap();
        let parsed: ContentBlock = serde_json::from_str(&json).unwrap();
        assert_eq!(block, parsed);
    }

    #[test]
    fn content_block_serde_tool_use() {
        let json_str = r#"{
            "type": "tool_use",
            "id": "toolu_abc",
            "name": "list_nodes",
            "input": {"filter": "running"}
        }"#;
        let block: ContentBlock = serde_json::from_str(json_str).unwrap();
        match &block {
            ContentBlock::ToolUse { id, name, input } => {
                assert_eq!(id, "toolu_abc");
                assert_eq!(name, "list_nodes");
                assert_eq!(input["filter"], "running");
            }
            _ => panic!("expected ToolUse"),
        }
    }

    #[test]
    fn content_block_serde_tool_result() {
        let block = ContentBlock::ToolResult {
            tool_use_id: "toolu_abc".to_string(),
            content: "done".to_string(),
            is_error: Some(false),
        };
        let json = serde_json::to_string(&block).unwrap();
        let parsed: ContentBlock = serde_json::from_str(&json).unwrap();
        assert_eq!(block, parsed);

        // Verify is_error=None is omitted from serialization
        let block_no_err = ContentBlock::ToolResult {
            tool_use_id: "toolu_xyz".to_string(),
            content: "ok".to_string(),
            is_error: None,
        };
        let json_no_err = serde_json::to_string(&block_no_err).unwrap();
        assert!(!json_no_err.contains("is_error"));
        let parsed_no_err: ContentBlock = serde_json::from_str(&json_no_err).unwrap();
        assert_eq!(block_no_err, parsed_no_err);
    }

    #[test]
    fn api_response_deserialization() {
        let json_str = r#"{
            "id": "msg_01XFDUDYJgAACzvnptvVoYEL",
            "content": [
                {"type": "text", "text": "Hello! How can I help?"}
            ],
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 10, "output_tokens": 25}
        }"#;
        let resp: ApiResponse = serde_json::from_str(json_str).unwrap();
        assert_eq!(resp.id, "msg_01XFDUDYJgAACzvnptvVoYEL");
        assert_eq!(resp.content.len(), 1);
        assert_eq!(resp.stop_reason.as_deref(), Some("end_turn"));
        assert_eq!(resp.usage.input_tokens, 10);
        assert_eq!(resp.usage.output_tokens, 25);
    }

    #[test]
    fn api_response_with_tool_use() {
        let json_str = r#"{
            "id": "msg_02abc",
            "content": [
                {"type": "text", "text": "I'll check that for you."},
                {
                    "type": "tool_use",
                    "id": "toolu_01A",
                    "name": "list_nodes",
                    "input": {"status": "all"}
                }
            ],
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 50, "output_tokens": 100}
        }"#;
        let resp: ApiResponse = serde_json::from_str(json_str).unwrap();
        assert_eq!(resp.stop_reason.as_deref(), Some("tool_use"));
        assert_eq!(resp.content.len(), 2);

        match &resp.content[1] {
            ContentBlock::ToolUse { id, name, input } => {
                assert_eq!(id, "toolu_01A");
                assert_eq!(name, "list_nodes");
                assert_eq!(input["status"], "all");
            }
            _ => panic!("expected ToolUse"),
        }
    }

    #[test]
    fn client_from_env_missing_key() {
        // Ensure the key is not set (test isolation: we cannot unset
        // env vars safely across threads, so we just check that if it
        // happens to be unset the error is correct).
        // Use a temp override approach: save, remove, test, restore.
        let saved = std::env::var("ANTHROPIC_API_KEY").ok();
        std::env::remove_var("ANTHROPIC_API_KEY");

        let result = ClaudeClient::from_env(None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ClaudeError::MissingApiKey),
            "expected MissingApiKey, got: {err:?}"
        );

        // Restore if it was set
        if let Some(key) = saved {
            std::env::set_var("ANTHROPIC_API_KEY", key);
        }
    }

    #[test]
    fn tool_definition_serialization() {
        let tool = ToolDefinition {
            name: "list_nodes".to_string(),
            description: "List all registered nodes".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "status": {
                        "type": "string",
                        "description": "Filter by status"
                    }
                },
                "required": []
            }),
        };

        let json = serde_json::to_value(&tool).unwrap();
        assert_eq!(json["name"], "list_nodes");
        assert_eq!(json["description"], "List all registered nodes");
        assert_eq!(json["input_schema"]["type"], "object");

        // Roundtrip
        let parsed: ToolDefinition = serde_json::from_value(json).unwrap();
        assert_eq!(parsed.name, "list_nodes");
    }
}
