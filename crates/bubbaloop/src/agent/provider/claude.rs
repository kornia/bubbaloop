//! Claude API provider — implements ModelProvider for Anthropic's Messages API.
//!
//! Carries forward battle-tested auth resolution and request logic from
//! the original `agent/claude.rs`, wrapped in the new ModelProvider trait.

use super::{
    ContentBlock, Message, ModelProvider, ModelResponse, ProviderError, ToolDefinition, Usage,
};
use serde::{Deserialize, Serialize};

// ── Constants ───────────────────────────────────────────────────────

/// Anthropic Messages API endpoint.
const API_URL: &str = "https://api.anthropic.com/v1/messages";

/// API version header value.
const API_VERSION: &str = "2023-06-01";

/// Default model when none is specified.
pub const DEFAULT_MODEL: &str = "claude-sonnet-4-20250514";

/// Default max tokens per response.
const MAX_TOKENS: u32 = 4096;

/// Required beta headers for OAuth tokens (matches Claude Code CLI).
/// Without these, Anthropic returns 401 for setup-token auth.
pub const OAUTH_BETA_HEADERS: &str = "claude-code-20250219,oauth-2025-04-20,fine-grained-tool-streaming-2025-05-14,interleaved-thinking-2025-05-14";

// ── Auth method ─────────────────────────────────────────────────────

/// How the client authenticates with the Anthropic API.
#[derive(Debug)]
enum AuthMethod {
    /// Traditional API key (x-api-key header)
    ApiKey(String),
    /// OAuth bearer token (Authorization: Bearer header)
    OAuthToken(String),
}

// ── API request/response ────────────────────────────────────────────

/// Wire format for the Messages API request.
#[derive(Debug, Serialize)]
struct ApiRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<&'a str>,
    messages: &'a [Message],
    #[serde(skip_serializing_if = "<[ToolDefinition]>::is_empty")]
    tools: &'a [ToolDefinition],
}

/// Wire format for the Messages API response.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApiResponse {
    pub id: String,
    pub content: Vec<ContentBlock>,
    pub stop_reason: Option<String>,
    pub usage: Usage,
}

// ── ClaudeProvider ──────────────────────────────────────────────────

/// Claude API provider implementing the ModelProvider trait.
#[derive(Debug)]
pub struct ClaudeProvider {
    client: reqwest::Client,
    auth: AuthMethod,
    model: String,
}

impl ClaudeProvider {
    /// Create a provider, resolving credentials from (in order):
    ///
    /// 1. `ANTHROPIC_API_KEY` environment variable → ApiKey
    /// 2. `~/.bubbaloop/oauth-credentials.json` file → OAuthToken (if not expired)
    /// 3. `~/.bubbaloop/anthropic-key` file (first line, trimmed) → ApiKey
    pub fn from_env(model: Option<&str>) -> super::Result<Self> {
        // 1. Check env var (always highest priority)
        if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
            return Ok(Self {
                client: reqwest::Client::new(),
                auth: AuthMethod::ApiKey(key),
                model: model.unwrap_or(DEFAULT_MODEL).to_string(),
            });
        }

        // 2. Check OAuth credentials
        if let Some(token) = Self::read_oauth_token() {
            return Ok(Self {
                client: reqwest::Client::new(),
                auth: AuthMethod::OAuthToken(token),
                model: model.unwrap_or(DEFAULT_MODEL).to_string(),
            });
        }

        // 3. Check API key file
        if let Some(key) = Self::read_key_file() {
            return Ok(Self {
                client: reqwest::Client::new(),
                auth: AuthMethod::ApiKey(key),
                model: model.unwrap_or(DEFAULT_MODEL).to_string(),
            });
        }

        Err(ProviderError::MissingCredentials)
    }

    /// Try to read the API key from `~/.bubbaloop/anthropic-key`.
    fn read_key_file() -> Option<String> {
        let path = dirs::home_dir()?.join(".bubbaloop").join("anthropic-key");
        let content = std::fs::read_to_string(path).ok()?;
        let key = content.lines().next()?.trim().to_string();
        if key.is_empty() {
            None
        } else {
            Some(key)
        }
    }

    /// Try to read an OAuth access token from `~/.bubbaloop/oauth-credentials.json`.
    /// Returns None if file doesn't exist or token is expired.
    fn read_oauth_token() -> Option<String> {
        let path = dirs::home_dir()?
            .join(".bubbaloop")
            .join("oauth-credentials.json");
        let content = std::fs::read_to_string(path).ok()?;
        let creds: serde_json::Value = serde_json::from_str(&content).ok()?;

        let access_token = creds.get("access_token")?.as_str()?.to_string();
        let expires_at = creds.get("expires_at")?.as_u64()?;

        // Check if token is still valid (with 5 minute buffer)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_secs();

        if now + 300 < expires_at {
            Some(access_token)
        } else {
            log::warn!("OAuth token expired — run 'bubbaloop login' to refresh");
            None
        }
    }

    /// Get the model name this provider is configured with.
    pub fn model(&self) -> &str {
        &self.model
    }
}

impl ModelProvider for ClaudeProvider {
    async fn generate(
        &self,
        system: Option<&str>,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> super::Result<ModelResponse> {
        let body = ApiRequest {
            model: &self.model,
            max_tokens: MAX_TOKENS,
            system,
            messages,
            tools,
        };

        let mut request = self
            .client
            .post(API_URL)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json");

        // Apply auth based on method
        request = match &self.auth {
            AuthMethod::ApiKey(key) => request.header("x-api-key", key),
            AuthMethod::OAuthToken(token) => request
                .header("authorization", format!("Bearer {}", token))
                .header("user-agent", "claude-cli/2.1.62")
                .header("x-app", "cli")
                .header("anthropic-beta", OAUTH_BETA_HEADERS),
        };

        let response = request.json(&body).send().await?;

        let status = response.status();
        if !status.is_success() {
            let message = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            return Err(ProviderError::Api {
                status: status.as_u16(),
                message,
            });
        }

        let api_response: ApiResponse = response.json().await?;
        Ok(ModelResponse {
            content: api_response.content,
            usage: api_response.usage,
            stop_reason: api_response.stop_reason,
        })
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
        let saved = std::env::var("ANTHROPIC_API_KEY").ok();
        // SAFETY: This test is not run concurrently with other env-mutating tests.
        unsafe { std::env::remove_var("ANTHROPIC_API_KEY") };

        let result = ClaudeProvider::from_env(None);
        match result {
            Err(ProviderError::MissingCredentials) => {}
            Ok(_) => {} // Valid if OAuth or key file exists
            Err(e) => panic!("unexpected error: {e:?}"),
        }

        if let Some(key) = saved {
            // SAFETY: Restoring the env var after test.
            unsafe { std::env::set_var("ANTHROPIC_API_KEY", key) };
        }
    }

    #[test]
    fn default_model_constant() {
        assert_eq!(DEFAULT_MODEL, "claude-sonnet-4-20250514");
    }

    #[test]
    fn auth_method_debug() {
        let auth = AuthMethod::ApiKey("sk-ant-test".to_string());
        let debug = format!("{:?}", auth);
        assert!(debug.contains("ApiKey"));

        let auth = AuthMethod::OAuthToken("sk-ant-oat01-test".to_string());
        let debug = format!("{:?}", auth);
        assert!(debug.contains("OAuthToken"));
    }

    #[test]
    fn model_response_from_api_response() {
        let api = ApiResponse {
            id: "msg_123".to_string(),
            content: vec![ContentBlock::Text {
                text: "hello".to_string(),
            }],
            stop_reason: Some("end_turn".to_string()),
            usage: Usage {
                input_tokens: 10,
                output_tokens: 5,
            },
        };
        let model_resp = ModelResponse {
            content: api.content,
            usage: api.usage,
            stop_reason: api.stop_reason,
        };
        assert_eq!(model_resp.text(), "hello");
        assert!(!model_resp.has_tool_calls());
    }

    #[test]
    fn provider_model_name() {
        // Only test when API key is available (otherwise from_env fails)
        let saved = std::env::var("ANTHROPIC_API_KEY").ok();
        // SAFETY: test env manipulation
        unsafe { std::env::set_var("ANTHROPIC_API_KEY", "test-key") };

        let provider = ClaudeProvider::from_env(Some("claude-haiku-4-5-20251001")).unwrap();
        assert_eq!(provider.model(), "claude-haiku-4-5-20251001");

        // SAFETY: restore env
        unsafe { std::env::remove_var("ANTHROPIC_API_KEY") };
        if let Some(key) = saved {
            unsafe { std::env::set_var("ANTHROPIC_API_KEY", key) };
        }
    }

    #[test]
    fn api_request_serialization() {
        let messages = vec![Message::user("hello")];
        let tools = vec![ToolDefinition {
            name: "test".to_string(),
            description: "A test tool".to_string(),
            input_schema: json!({"type": "object", "properties": {}}),
        }];
        let req = ApiRequest {
            model: "test-model",
            max_tokens: 1024,
            system: Some("You are helpful."),
            messages: &messages,
            tools: &tools,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["model"], "test-model");
        assert_eq!(json["max_tokens"], 1024);
        assert_eq!(json["system"], "You are helpful.");
    }
}
