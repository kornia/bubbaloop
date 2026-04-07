//! Claude API provider — implements ModelProvider for Anthropic's Messages API.
//!
//! Carries forward battle-tested auth resolution and request logic from
//! the original `agent/claude.rs`, wrapped in the new ModelProvider trait.

use super::{
    ContentBlock, Message, ModelProvider, ModelResponse, ProviderError, StreamEvent,
    ToolDefinition, Usage,
};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::time::Duration;

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

/// Maximum retries for transient provider errors (429, 5xx, network).
const MAX_RETRIES: u32 = 3;

/// Base delay for retry backoff (seconds). Backoff: 1s, 4s, 16s.
const RETRY_BASE_SECS: u64 = 1;

// ── Auth method ─────────────────────────────────────────────────────

/// How the client authenticates with the Anthropic API.
enum AuthMethod {
    /// Traditional API key (x-api-key header)
    ApiKey(String),
    /// OAuth bearer token (Authorization: Bearer header)
    OAuthToken(String),
}

impl std::fmt::Debug for AuthMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ApiKey(_) => write!(f, "ApiKey(****)"),
            Self::OAuthToken(_) => write!(f, "OAuthToken(****)"),
        }
    }
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
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stream: bool,
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

    /// Check that a credential file has restrictive permissions (Unix only).
    /// Logs a warning if the file is more permissive than 0o600 but does not
    /// prevent reading — this avoids breaking functionality.
    #[cfg(unix)]
    fn check_credential_permissions(path: &std::path::Path) {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(path) {
            let mode = metadata.permissions().mode() & 0o777;
            if mode & 0o077 != 0 {
                log::warn!(
                    "Credential file {:?} has permissions {:o} — should be 0600 or stricter",
                    path,
                    mode
                );
            }
        }
    }

    /// Try to read the API key from `~/.bubbaloop/anthropic-key`.
    fn read_key_file() -> Option<String> {
        let path = dirs::home_dir()?.join(".bubbaloop").join("anthropic-key");
        #[cfg(unix)]
        Self::check_credential_permissions(&path);
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
        #[cfg(unix)]
        Self::check_credential_permissions(&path);
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

    /// Create a provider directly from an API key (for testing and explicit configuration).
    /// Does not read any environment variables or files.
    pub fn with_api_key(api_key: impl Into<String>, model: Option<&str>) -> Self {
        Self {
            client: reqwest::Client::new(),
            auth: AuthMethod::ApiKey(api_key.into()),
            model: model.unwrap_or(DEFAULT_MODEL).to_string(),
        }
    }

    /// Get the model name this provider is configured with.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Build an authenticated request to the Messages API.
    fn build_request(&self) -> reqwest::RequestBuilder {
        let mut request = self
            .client
            .post(API_URL)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json");

        request = match &self.auth {
            AuthMethod::ApiKey(key) => request.header("x-api-key", key),
            AuthMethod::OAuthToken(token) => request
                .header("authorization", format!("Bearer {}", token))
                .header("user-agent", "claude-cli/2.1.62")
                .header("x-app", "cli")
                .header("anthropic-beta", OAUTH_BETA_HEADERS),
        };

        request
    }

    /// Send a request and return the response, converting HTTP errors to ProviderError.
    async fn send_request(&self, body: &ApiRequest<'_>) -> super::Result<reqwest::Response> {
        let response = self.build_request().json(body).send().await?;

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

        Ok(response)
    }

    /// Send a request with retry for transient errors (429, 5xx, network).
    async fn send_with_retry(&self, body: &ApiRequest<'_>) -> super::Result<reqwest::Response> {
        let mut last_err = None;
        for attempt in 0..MAX_RETRIES {
            match self.send_request(body).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    if !e.is_retryable() || attempt + 1 >= MAX_RETRIES {
                        return Err(e);
                    }
                    // Exponential backoff: base * 4^attempt (1s, 4s, 16s)
                    let delay_secs = RETRY_BASE_SECS * 4u64.pow(attempt);
                    log::warn!(
                        "Provider error (attempt {}/{}), retrying in {}s: {}",
                        attempt + 1,
                        MAX_RETRIES,
                        delay_secs,
                        e
                    );
                    tokio::time::sleep(Duration::from_secs(delay_secs)).await;
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.unwrap_or(ProviderError::Format("retry exhausted".to_string())))
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
            stream: false,
        };

        let response = self.send_with_retry(&body).await?;

        let api_response: ApiResponse = response.json().await?;
        Ok(ModelResponse {
            content: api_response.content,
            usage: api_response.usage,
            stop_reason: api_response.stop_reason,
        })
    }

    async fn generate_stream(
        &self,
        system: Option<&str>,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> super::Result<tokio::sync::mpsc::Receiver<StreamEvent>> {
        let body = ApiRequest {
            model: &self.model,
            max_tokens: MAX_TOKENS,
            system,
            messages,
            tools,
            stream: true,
        };

        let response = self.send_with_retry(&body).await?;

        let (tx, rx) = tokio::sync::mpsc::channel(64);
        let byte_stream = response.bytes_stream();

        tokio::spawn(async move {
            if let Err(e) = process_sse_stream(byte_stream, &tx).await {
                let _ = tx.send(StreamEvent::Error(e)).await;
            }
        });

        Ok(rx)
    }
}

/// Incremental SSE parser that accumulates line-oriented state and emits `StreamEvent`s.
///
/// Holds all mutable parsing state so callers can feed chunks one at a time via
/// `process_chunk` and then call `finish` after the stream ends.
struct SseParser {
    /// Incomplete line data carried over from previous chunks.
    line_buffer: String,
    /// The `event:` field of the SSE block currently being parsed.
    current_event_type: String,
    /// `id` of the tool-use block being accumulated (empty when not in a tool block).
    tool_id: String,
    /// `name` of the tool-use block being accumulated.
    tool_name: String,
    /// Accumulated `input_json_delta` fragments for the current tool block.
    tool_input_json: String,
    /// Whether we are currently inside a `tool_use` content block.
    in_tool_block: bool,
    /// Token usage counters, updated by `message_start` / `message_delta` events.
    usage: Usage,
    /// Stop reason received from the `message_delta` event, if any.
    stop_reason: Option<String>,
}

impl SseParser {
    fn new() -> Self {
        Self {
            line_buffer: String::new(),
            current_event_type: String::new(),
            tool_id: String::new(),
            tool_name: String::new(),
            tool_input_json: String::new(),
            in_tool_block: false,
            usage: Usage {
                input_tokens: 0,
                output_tokens: 0,
            },
            stop_reason: None,
        }
    }

    /// Feed a raw byte chunk into the parser.
    ///
    /// Returns the events produced by all complete SSE lines in `chunk`, plus a
    /// boolean that is `true` when a `message_stop` event was encountered (i.e.
    /// the stream is finished and the caller should stop reading).
    ///
    /// Returns `Err` if the internal line buffer exceeds the 10 MB safety limit.
    fn process_chunk(
        &mut self,
        chunk: &[u8],
    ) -> std::result::Result<(Vec<StreamEvent>, bool), String> {
        const MAX_SSE_BUFFER: usize = 10 * 1024 * 1024; // 10 MB

        let chunk_str = String::from_utf8_lossy(chunk);
        self.line_buffer.push_str(&chunk_str);

        if self.line_buffer.len() > MAX_SSE_BUFFER {
            return Err("SSE stream buffer exceeded 10MB limit".to_string());
        }

        let mut events = Vec::new();
        let mut stream_done = false;

        // Process complete lines (SSE format: lines ending with \n)
        while let Some(newline_pos) = self.line_buffer.find('\n') {
            let line = self.line_buffer[..newline_pos]
                .trim_end_matches('\r')
                .to_string();
            self.line_buffer = self.line_buffer[newline_pos + 1..].to_string();

            if line.is_empty() {
                // Empty line = end of event block, reset event type
                self.current_event_type.clear();
                continue;
            }

            if let Some(event_type) = line.strip_prefix("event: ") {
                self.current_event_type = event_type.to_string();
                continue;
            }

            if let Some(data) = line.strip_prefix("data: ") {
                let parsed: serde_json::Value = match serde_json::from_str(data) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                match self.current_event_type.as_str() {
                    "content_block_start" => {
                        if let Some(block) = parsed.get("content_block") {
                            match block.get("type").and_then(|t| t.as_str()) {
                                Some("tool_use") => {
                                    self.in_tool_block = true;
                                    self.tool_id = block
                                        .get("id")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    self.tool_name = block
                                        .get("name")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    self.tool_input_json.clear();
                                }
                                _ => {
                                    self.in_tool_block = false;
                                }
                            }
                        }
                    }
                    "content_block_delta" => {
                        if let Some(delta) = parsed.get("delta") {
                            match delta.get("type").and_then(|t| t.as_str()) {
                                Some("text_delta") => {
                                    if let Some(text) = delta.get("text").and_then(|t| t.as_str()) {
                                        events.push(StreamEvent::TextDelta(text.to_string()));
                                    }
                                }
                                Some("input_json_delta") => {
                                    if let Some(partial) =
                                        delta.get("partial_json").and_then(|t| t.as_str())
                                    {
                                        self.tool_input_json.push_str(partial);
                                        // Safety limit: tool input JSON should not exceed 1MB
                                        if self.tool_input_json.len() > 1_048_576 {
                                            return Err(
                                                "Tool input JSON exceeded 1MB limit".to_string()
                                            );
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    "content_block_stop" => {
                        if self.in_tool_block {
                            let input: serde_json::Value =
                                serde_json::from_str(&self.tool_input_json)
                                    .unwrap_or_else(|_| serde_json::json!({}));
                            events.push(StreamEvent::ToolUse {
                                id: self.tool_id.clone(),
                                name: self.tool_name.clone(),
                                input,
                            });
                            self.in_tool_block = false;
                            self.tool_input_json.clear();
                        }
                    }
                    "message_delta" => {
                        if let Some(d) = parsed.get("delta") {
                            if let Some(sr) = d.get("stop_reason").and_then(|v| v.as_str()) {
                                self.stop_reason = Some(sr.to_string());
                            }
                        }
                        if let Some(u) = parsed.get("usage") {
                            if let Some(out) = u.get("output_tokens").and_then(|v| v.as_u64()) {
                                self.usage.output_tokens = out as u32;
                            }
                        }
                    }
                    "message_start" => {
                        if let Some(msg) = parsed.get("message") {
                            if let Some(u) = msg.get("usage") {
                                if let Some(inp) = u.get("input_tokens").and_then(|v| v.as_u64()) {
                                    self.usage.input_tokens = inp as u32;
                                }
                            }
                        }
                    }
                    "message_stop" => {
                        events.push(StreamEvent::Done {
                            usage: self.usage.clone(),
                            stop_reason: self.stop_reason.clone(),
                        });
                        stream_done = true;
                        // Stop processing further lines in this chunk.
                        break;
                    }
                    _ => {}
                }
            }
        }

        Ok((events, stream_done))
    }

    /// Produce a terminal `Done` event for streams that end without `message_stop`.
    fn finish(self) -> StreamEvent {
        StreamEvent::Done {
            usage: self.usage,
            stop_reason: self.stop_reason,
        }
    }
}

/// Process an SSE byte stream into StreamEvents.
///
/// Reads SSE-formatted lines from the stream and converts them to `StreamEvent`s.
/// Handles: `message_start`, `content_block_start/delta/stop`, `message_delta`, `message_stop`.
async fn process_sse_stream(
    stream: impl futures::Stream<Item = reqwest::Result<impl AsRef<[u8]>>>,
    tx: &tokio::sync::mpsc::Sender<StreamEvent>,
) -> std::result::Result<(), String> {
    tokio::pin!(stream);

    let mut parser = SseParser::new();

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(|e: reqwest::Error| e.to_string())?;

        let (events, stream_done) = parser.process_chunk(chunk.as_ref())?;

        for event in events {
            if tx.send(event).await.is_err() {
                return Ok(());
            }
        }

        if stream_done {
            return Ok(());
        }
    }

    // If we exit without message_stop, still send Done
    let _ = tx.send(parser.finish()).await;
    Ok(())
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
        // from_env() either succeeds (if credentials exist on this machine)
        // or returns MissingCredentials — both are valid outcomes.
        // We no longer manipulate env vars here to avoid unsafe in multi-threaded tests.
        let result = ClaudeProvider::from_env(None);
        match result {
            Ok(_) | Err(ProviderError::MissingCredentials) => {}
            Err(e) => panic!("unexpected error: {e:?}"),
        }
    }

    #[test]
    fn default_model_constant() {
        assert_eq!(DEFAULT_MODEL, "claude-sonnet-4-20250514");
    }

    #[test]
    fn auth_method_debug_redacted() {
        let auth = AuthMethod::ApiKey("sk-ant-test-secret".to_string());
        let debug = format!("{:?}", auth);
        assert!(debug.contains("ApiKey"));
        assert!(
            !debug.contains("sk-ant-test"),
            "API key must be redacted in Debug output"
        );

        let auth = AuthMethod::OAuthToken("sk-ant-oat01-secret".to_string());
        let debug = format!("{:?}", auth);
        assert!(debug.contains("OAuthToken"));
        assert!(
            !debug.contains("sk-ant-oat01"),
            "OAuth token must be redacted in Debug output"
        );
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
        // Use with_api_key to avoid unsafe env var manipulation in tests.
        let provider = ClaudeProvider::with_api_key("test-key", Some("claude-haiku-4-5-20251001"));
        assert_eq!(provider.model(), "claude-haiku-4-5-20251001");
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
            stream: false,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["model"], "test-model");
        assert_eq!(json["max_tokens"], 1024);
        assert_eq!(json["system"], "You are helpful.");
        // stream=false should be omitted
        assert!(json.get("stream").is_none());
    }

    #[test]
    fn api_request_stream_serialization() {
        let messages = vec![Message::user("hello")];
        let req = ApiRequest {
            model: "test-model",
            max_tokens: 1024,
            system: None,
            messages: &messages,
            tools: &[],
            stream: true,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["stream"], true);
    }

    /// Helper: create a mock SSE byte stream from a string.
    fn mock_sse_stream(data: &str) -> impl futures::Stream<Item = reqwest::Result<Vec<u8>>> {
        let bytes = data.as_bytes().to_vec();
        futures::stream::once(async move { Ok(bytes) })
    }

    #[tokio::test]
    async fn sse_parse_text_delta() {
        let sse_data = "\
event: message_start\n\
data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\
\n\
event: content_block_start\n\
data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\
\n\
event: content_block_delta\n\
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\
\n\
event: content_block_delta\n\
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\" world\"}}\n\
\n\
event: content_block_stop\n\
data: {\"type\":\"content_block_stop\",\"index\":0}\n\
\n\
event: message_delta\n\
data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":5}}\n\
\n\
event: message_stop\n\
data: {\"type\":\"message_stop\"}\n\
\n";

        let stream = mock_sse_stream(sse_data);
        let (tx, mut rx) = tokio::sync::mpsc::channel(32);
        let result = process_sse_stream(stream, &tx).await;
        assert!(result.is_ok());
        drop(tx);

        let mut text = String::new();
        let mut got_done = false;
        while let Some(event) = rx.recv().await {
            match event {
                StreamEvent::TextDelta(t) => text.push_str(&t),
                StreamEvent::Done { usage, stop_reason } => {
                    assert_eq!(usage.input_tokens, 10);
                    assert_eq!(usage.output_tokens, 5);
                    assert_eq!(stop_reason.as_deref(), Some("end_turn"));
                    got_done = true;
                }
                _ => {}
            }
        }
        assert_eq!(text, "Hello world");
        assert!(got_done);
    }

    #[tokio::test]
    async fn sse_parse_tool_use() {
        let sse_data = "\
event: message_start\n\
data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_2\",\"usage\":{\"input_tokens\":20,\"output_tokens\":0}}}\n\
\n\
event: content_block_start\n\
data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_1\",\"name\":\"list_nodes\"}}\n\
\n\
event: content_block_delta\n\
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"status\\\"\"}}\n\
\n\
event: content_block_delta\n\
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\": \\\"all\\\"}\"}}\n\
\n\
event: content_block_stop\n\
data: {\"type\":\"content_block_stop\",\"index\":0}\n\
\n\
event: message_delta\n\
data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":15}}\n\
\n\
event: message_stop\n\
data: {\"type\":\"message_stop\"}\n\
\n";

        let stream = mock_sse_stream(sse_data);
        let (tx, mut rx) = tokio::sync::mpsc::channel(32);
        let result = process_sse_stream(stream, &tx).await;
        assert!(result.is_ok());
        drop(tx);

        let mut got_tool = false;
        let mut got_done = false;
        while let Some(event) = rx.recv().await {
            match event {
                StreamEvent::ToolUse { id, name, input } => {
                    assert_eq!(id, "toolu_1");
                    assert_eq!(name, "list_nodes");
                    assert_eq!(input["status"], "all");
                    got_tool = true;
                }
                StreamEvent::Done { stop_reason, .. } => {
                    assert_eq!(stop_reason.as_deref(), Some("tool_use"));
                    got_done = true;
                }
                _ => {}
            }
        }
        assert!(got_tool, "should have received ToolUse event");
        assert!(got_done, "should have received Done event");
    }
}
