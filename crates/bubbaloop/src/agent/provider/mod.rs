//! Model provider trait — abstracts LLM backends (Claude, Ollama, etc.).

pub mod claude;
pub mod ollama;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Errors from model provider operations.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("API error (status {status}): {message}")]
    Api { status: u16, message: String },

    #[error("no API key or OAuth credentials configured — run 'bubbaloop login'")]
    MissingCredentials,

    #[error("format error: {0}")]
    Format(String),
}

impl ProviderError {
    /// Whether this error is retryable (transient server/network errors).
    ///
    /// Retryable: 429, 500, 502, 503, 504, network errors.
    /// Non-retryable: 400, 401, 403, format errors, missing credentials.
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Http(_) => true, // network-level errors are transient
            Self::Api { status, .. } => matches!(status, 429 | 500 | 502 | 503 | 504),
            Self::MissingCredentials | Self::Format(_) => false,
        }
    }
}

pub type Result<T> = std::result::Result<T, ProviderError>;

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

// ── Token usage ─────────────────────────────────────────────────────

/// Token usage from the API response.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

// ── Model response ──────────────────────────────────────────────────

/// A tool call extracted from a model response.
#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: Value,
}

/// Response from a model provider.
#[derive(Debug, Clone)]
pub struct ModelResponse {
    /// Raw content blocks from the model.
    pub content: Vec<ContentBlock>,
    /// Token usage.
    pub usage: Usage,
    /// Stop reason (e.g., "end_turn", "tool_use").
    pub stop_reason: Option<String>,
}

impl ModelResponse {
    /// Extract text from all text blocks.
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

    /// Extract tool calls from the response.
    pub fn tool_calls(&self) -> Vec<ToolCall> {
        self.content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::ToolUse { id, name, input } => Some(ToolCall {
                    id: id.clone(),
                    name: name.clone(),
                    input: input.clone(),
                }),
                _ => None,
            })
            .collect()
    }

    /// Whether the response contains tool calls.
    pub fn has_tool_calls(&self) -> bool {
        self.content
            .iter()
            .any(|b| matches!(b, ContentBlock::ToolUse { .. }))
    }
}

// ── Stream events ────────────────────────────────────────────────────

/// Events emitted during streaming response generation.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// Incremental text chunk.
    TextDelta(String),
    /// A complete tool_use block (accumulated from start + deltas).
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    /// Stream complete with usage info.
    Done {
        usage: Usage,
        stop_reason: Option<String>,
    },
    /// Error during streaming.
    Error(String),
}

// ── ModelProvider trait ──────────────────────────────────────────────

/// Trait abstracting LLM provider backends.
///
/// Implementations must be Send + Sync for use in async contexts.
pub trait ModelProvider: Send + Sync {
    /// Generate a response given conversation messages and available tools.
    fn generate(
        &self,
        system: Option<&str>,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> impl std::future::Future<Output = Result<ModelResponse>> + Send;

    /// Streaming variant. Default impl wraps `generate()` for backward compat.
    fn generate_stream(
        &self,
        system: Option<&str>,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> impl std::future::Future<Output = Result<tokio::sync::mpsc::Receiver<StreamEvent>>> + Send
    {
        let system = system.map(|s| s.to_string());
        let messages = messages.to_vec();
        let tools = tools.to_vec();
        async move {
            let response = self.generate(system.as_deref(), &messages, &tools).await?;
            let (tx, rx) = tokio::sync::mpsc::channel(32);
            // Emit text as single delta
            let text = response.text();
            if !text.is_empty() {
                let _ = tx.send(StreamEvent::TextDelta(text)).await;
            }
            // Emit tool uses
            for tc in response.tool_calls() {
                let _ = tx
                    .send(StreamEvent::ToolUse {
                        id: tc.id,
                        name: tc.name,
                        input: tc.input,
                    })
                    .await;
            }
            let _ = tx
                .send(StreamEvent::Done {
                    usage: response.usage,
                    stop_reason: response.stop_reason,
                })
                .await;
            Ok(rx)
        }
    }
}

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
    }

    #[test]
    fn tool_result_message() {
        let msg = Message::tool_results(vec![
            ("tu_1".to_string(), "success".to_string(), None),
            ("tu_2".to_string(), "not found".to_string(), Some(true)),
        ]);
        assert_eq!(msg.role, "user");
        assert_eq!(msg.content.len(), 2);
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
            ContentBlock::ToolUse { id, name, .. } => {
                assert_eq!(id, "toolu_abc");
                assert_eq!(name, "list_nodes");
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
    }

    #[test]
    fn tool_definition_serialization() {
        let tool = ToolDefinition {
            name: "list_nodes".to_string(),
            description: "List all registered nodes".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        };
        let json = serde_json::to_value(&tool).unwrap();
        assert_eq!(json["name"], "list_nodes");
        let parsed: ToolDefinition = serde_json::from_value(json).unwrap();
        assert_eq!(parsed.name, "list_nodes");
    }

    #[test]
    fn model_response_text_extraction() {
        let resp = ModelResponse {
            content: vec![
                ContentBlock::Text {
                    text: "Hello".to_string(),
                },
                ContentBlock::ToolUse {
                    id: "t1".to_string(),
                    name: "test".to_string(),
                    input: json!({}),
                },
            ],
            usage: Usage {
                input_tokens: 10,
                output_tokens: 20,
            },
            stop_reason: Some("tool_use".to_string()),
        };
        assert_eq!(resp.text(), "Hello");
        assert!(resp.has_tool_calls());
        assert_eq!(resp.tool_calls().len(), 1);
        assert_eq!(resp.tool_calls()[0].name, "test");
    }

    #[test]
    fn model_response_no_tool_calls() {
        let resp = ModelResponse {
            content: vec![ContentBlock::Text {
                text: "Done".to_string(),
            }],
            usage: Usage {
                input_tokens: 5,
                output_tokens: 1,
            },
            stop_reason: Some("end_turn".to_string()),
        };
        assert!(!resp.has_tool_calls());
        assert!(resp.tool_calls().is_empty());
    }

    #[test]
    fn stream_event_debug_display() {
        let event = StreamEvent::TextDelta("hello".to_string());
        let debug = format!("{:?}", event);
        assert!(debug.contains("TextDelta"));

        let event = StreamEvent::ToolUse {
            id: "t1".to_string(),
            name: "list_nodes".to_string(),
            input: json!({}),
        };
        let debug = format!("{:?}", event);
        assert!(debug.contains("list_nodes"));

        let event = StreamEvent::Done {
            usage: Usage {
                input_tokens: 10,
                output_tokens: 5,
            },
            stop_reason: Some("end_turn".to_string()),
        };
        let debug = format!("{:?}", event);
        assert!(debug.contains("Done"));

        let event = StreamEvent::Error("test error".to_string());
        let debug = format!("{:?}", event);
        assert!(debug.contains("test error"));
    }

    /// Mock provider for testing the default generate_stream implementation.
    struct MockProvider {
        response: ModelResponse,
    }

    impl ModelProvider for MockProvider {
        async fn generate(
            &self,
            _system: Option<&str>,
            _messages: &[Message],
            _tools: &[ToolDefinition],
        ) -> Result<ModelResponse> {
            Ok(self.response.clone())
        }
    }

    #[tokio::test]
    async fn default_generate_stream_text_only() {
        let provider = MockProvider {
            response: ModelResponse {
                content: vec![ContentBlock::Text {
                    text: "Hello world".to_string(),
                }],
                usage: Usage {
                    input_tokens: 10,
                    output_tokens: 5,
                },
                stop_reason: Some("end_turn".to_string()),
            },
        };

        let mut rx = provider.generate_stream(None, &[], &[]).await.unwrap();

        // Should get TextDelta then Done
        match rx.recv().await.unwrap() {
            StreamEvent::TextDelta(text) => assert_eq!(text, "Hello world"),
            other => panic!("expected TextDelta, got {:?}", other),
        }
        match rx.recv().await.unwrap() {
            StreamEvent::Done { usage, stop_reason } => {
                assert_eq!(usage.input_tokens, 10);
                assert_eq!(stop_reason.as_deref(), Some("end_turn"));
            }
            other => panic!("expected Done, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn default_generate_stream_with_tool_use() {
        let provider = MockProvider {
            response: ModelResponse {
                content: vec![
                    ContentBlock::Text {
                        text: "Let me check.".to_string(),
                    },
                    ContentBlock::ToolUse {
                        id: "t1".to_string(),
                        name: "list_nodes".to_string(),
                        input: json!({"status": "all"}),
                    },
                ],
                usage: Usage {
                    input_tokens: 50,
                    output_tokens: 30,
                },
                stop_reason: Some("tool_use".to_string()),
            },
        };

        let mut rx = provider.generate_stream(None, &[], &[]).await.unwrap();

        match rx.recv().await.unwrap() {
            StreamEvent::TextDelta(text) => assert_eq!(text, "Let me check."),
            other => panic!("expected TextDelta, got {:?}", other),
        }
        match rx.recv().await.unwrap() {
            StreamEvent::ToolUse { id, name, input } => {
                assert_eq!(id, "t1");
                assert_eq!(name, "list_nodes");
                assert_eq!(input["status"], "all");
            }
            other => panic!("expected ToolUse, got {:?}", other),
        }
        match rx.recv().await.unwrap() {
            StreamEvent::Done { stop_reason, .. } => {
                assert_eq!(stop_reason.as_deref(), Some("tool_use"));
            }
            other => panic!("expected Done, got {:?}", other),
        }
    }

    #[test]
    fn provider_error_retryable_api_5xx() {
        for status in [500, 502, 503, 504] {
            let err = ProviderError::Api {
                status,
                message: "server error".to_string(),
            };
            assert!(err.is_retryable(), "status {} should be retryable", status);
        }
    }

    #[test]
    fn provider_error_retryable_api_429() {
        let err = ProviderError::Api {
            status: 429,
            message: "rate limited".to_string(),
        };
        assert!(err.is_retryable());
    }

    #[test]
    fn provider_error_retryable_api_500() {
        let err = ProviderError::Api {
            status: 500,
            message: "internal server error".to_string(),
        };
        assert!(err.is_retryable());
    }

    #[test]
    fn provider_error_retryable_api_502() {
        let err = ProviderError::Api {
            status: 502,
            message: "bad gateway".to_string(),
        };
        assert!(err.is_retryable());
    }

    #[test]
    fn provider_error_not_retryable_api_401() {
        let err = ProviderError::Api {
            status: 401,
            message: "unauthorized".to_string(),
        };
        assert!(!err.is_retryable());
    }

    #[test]
    fn provider_error_not_retryable_api_400() {
        let err = ProviderError::Api {
            status: 400,
            message: "bad request".to_string(),
        };
        assert!(!err.is_retryable());
    }

    #[test]
    fn provider_error_not_retryable_missing_credentials() {
        let err = ProviderError::MissingCredentials;
        assert!(!err.is_retryable());
    }

    #[test]
    fn provider_error_not_retryable_format() {
        let err = ProviderError::Format("bad json".to_string());
        assert!(!err.is_retryable());
    }
}
