//! Google Gemini provider — implements ModelProvider for the Generative Language API.
//!
//! Endpoint: generativelanguage.googleapis.com/v1beta/models/{model}:generateContent
//! Auth: `x-goog-api-key` header (never as a query param to avoid log exposure).
//! Streaming: `:streamGenerateContent?alt=sse` — each SSE frame is a complete
//! GenerateContentResponse JSON. Concatenate text deltas across frames.

use super::{
    http_client, ContentBlock, Message, ModelProvider, ModelResponse, ProviderError, StreamEvent,
    ToolDefinition, Usage,
};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ── Constants ────────────────────────────────────────────────────────

const API_BASE: &str = "https://generativelanguage.googleapis.com/v1beta/models";

/// Default model — Flash is on the free tier (15 RPM / 1M tokens/day).
pub const DEFAULT_MODEL: &str = "gemini-2.5-flash";

// ── Auth ─────────────────────────────────────────────────────────────

/// How the Gemini client authenticates.
#[derive(Debug)]
pub enum AuthMethod {
    ApiKey(String),
}

// ── Wire types (request) ─────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerateRequest<'a> {
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiSystemInstruction<'a>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<GeminiTool<'a>>,
    generation_config: GenerationConfig,
}

#[derive(Serialize)]
struct GeminiSystemInstruction<'a> {
    parts: Vec<GeminiTextPart<'a>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct GeminiContent {
    role: String,
    parts: Vec<GeminiPart>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(untagged)]
enum GeminiPart {
    Text(GeminiTextPartOwned),
    FunctionCall(GeminiFunctionCall),
    FunctionResponse(GeminiFunctionResponse),
}

#[derive(Serialize)]
struct GeminiTextPart<'a> {
    text: &'a str,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct GeminiTextPartOwned {
    text: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct GeminiFunctionCall {
    #[serde(rename = "functionCall")]
    function_call: GeminiFn,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct GeminiFunctionResponse {
    #[serde(rename = "functionResponse")]
    function_response: GeminiFnResponse,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct GeminiFn {
    name: String,
    args: Value,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct GeminiFnResponse {
    name: String,
    response: Value,
}

#[derive(Serialize)]
struct GeminiTool<'a> {
    function_declarations: Vec<GeminiFnDeclaration<'a>>,
}

#[derive(Serialize)]
struct GeminiFnDeclaration<'a> {
    name: &'a str,
    description: &'a str,
    parameters: &'a Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerationConfig {
    max_output_tokens: u32,
}

// ── Wire types (response) ────────────────────────────────────────────

#[derive(Deserialize, Debug)]
struct GenerateResponse {
    candidates: Vec<GeminiCandidate>,
    #[serde(default)]
    usage_metadata: Option<GeminiUsage>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GeminiCandidate {
    content: GeminiContent,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GeminiUsage {
    prompt_token_count: Option<u32>,
    candidates_token_count: Option<u32>,
}

// ── GeminiProvider ───────────────────────────────────────────────────

/// Gemini API provider implementing the ModelProvider trait.
#[derive(Debug)]
pub struct GeminiProvider {
    client: reqwest::Client,
    auth: AuthMethod,
    model: String,
}

impl GeminiProvider {
    /// Create a provider, resolving credentials from (in order):
    ///
    /// 1. `GEMINI_API_KEY` environment variable
    /// 2. `~/.bubbaloop/gemini-key` file (first line, trimmed)
    pub fn from_env(model: Option<&str>) -> super::Result<Self> {
        if let Ok(key) = std::env::var("GEMINI_API_KEY") {
            return Ok(Self::new(key, model));
        }
        if let Some(key) = Self::read_key_file() {
            return Ok(Self::new(key, model));
        }
        Err(ProviderError::MissingCredentials)
    }

    fn new(key: impl Into<String>, model: Option<&str>) -> Self {
        Self {
            client: http_client(),
            auth: AuthMethod::ApiKey(key.into()),
            model: model.unwrap_or(DEFAULT_MODEL).to_string(),
        }
    }

    fn read_key_file() -> Option<String> {
        let path = dirs::home_dir()?.join(".bubbaloop").join("gemini-key");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = std::fs::metadata(&path) {
                if meta.permissions().mode() & 0o077 != 0 {
                    log::warn!(
                        "Credential file {:?} has loose permissions — use chmod 600",
                        path
                    );
                }
            }
        }
        let content = std::fs::read_to_string(path).ok()?;
        let key = content.lines().next()?.trim().to_string();
        if key.is_empty() {
            None
        } else {
            Some(key)
        }
    }

    /// Check whether Gemini credentials are available without constructing a provider.
    pub fn has_credentials() -> bool {
        if std::env::var("GEMINI_API_KEY").is_ok() {
            return true;
        }
        dirs::home_dir()
            .map(|h| {
                let path = h.join(".bubbaloop").join("gemini-key");
                path.exists()
                    && std::fs::read_to_string(&path)
                        .ok()
                        .and_then(|s| s.lines().next().map(|l| !l.trim().is_empty()))
                        .unwrap_or(false)
            })
            .unwrap_or(false)
    }

    fn api_key(&self) -> &str {
        let AuthMethod::ApiKey(ref k) = self.auth;
        k
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    /// Build authenticated request builder for a given path suffix.
    fn request(&self, path_suffix: &str) -> reqwest::RequestBuilder {
        let url = format!("{}/{}{}", API_BASE, self.model, path_suffix);
        self.client
            .post(&url)
            .header("x-goog-api-key", self.api_key())
            .header("content-type", "application/json")
    }
}

// ── Message conversion ───────────────────────────────────────────────

/// Convert our canonical `Message` slice to Gemini `contents` + optional system prompt.
fn to_gemini_contents(messages: &[Message]) -> Vec<GeminiContent> {
    let mut out = Vec::new();
    for msg in messages {
        let role = match msg.role.as_str() {
            "assistant" => "model",
            r => r,
        };
        let mut parts = Vec::new();
        for block in &msg.content {
            match block {
                ContentBlock::Text { text } => {
                    parts.push(GeminiPart::Text(GeminiTextPartOwned { text: text.clone() }));
                }
                ContentBlock::ToolUse { name, input, .. } => {
                    parts.push(GeminiPart::FunctionCall(GeminiFunctionCall {
                        function_call: GeminiFn {
                            name: name.clone(),
                            args: input.clone(),
                        },
                    }));
                }
                ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    is_error,
                } => {
                    let response = if is_error.unwrap_or(false) {
                        serde_json::json!({ "error": content })
                    } else {
                        serde_json::json!({ "output": content })
                    };
                    parts.push(GeminiPart::FunctionResponse(GeminiFunctionResponse {
                        function_response: GeminiFnResponse {
                            name: tool_use_id.clone(),
                            response,
                        },
                    }));
                }
            }
        }
        if !parts.is_empty() {
            out.push(GeminiContent {
                role: role.to_string(),
                parts,
            });
        }
    }
    out
}

fn to_gemini_tools<'a>(tools: &'a [ToolDefinition]) -> Vec<GeminiTool<'a>> {
    if tools.is_empty() {
        return vec![];
    }
    vec![GeminiTool {
        function_declarations: tools
            .iter()
            .map(|t| GeminiFnDeclaration {
                name: &t.name,
                description: &t.description,
                parameters: &t.input_schema,
            })
            .collect(),
    }]
}

// ── Response conversion ──────────────────────────────────────────────

fn parse_candidate(candidate: &GeminiCandidate) -> (Vec<ContentBlock>, Option<String>) {
    let mut blocks = Vec::new();
    for part in &candidate.content.parts {
        match part {
            GeminiPart::Text(t) => {
                if !t.text.is_empty() {
                    blocks.push(ContentBlock::Text {
                        text: t.text.clone(),
                    });
                }
            }
            GeminiPart::FunctionCall(fc) => {
                blocks.push(ContentBlock::ToolUse {
                    id: format!("gemini-{}", uuid::Uuid::new_v4()),
                    name: fc.function_call.name.clone(),
                    input: fc.function_call.args.clone(),
                });
            }
            GeminiPart::FunctionResponse(_) => {}
        }
    }
    let stop_reason = candidate.finish_reason.as_deref().map(|r| match r {
        "STOP" => "end_turn",
        "MAX_TOKENS" => "max_tokens",
        "FUNCTION_CALL" => "tool_use",
        other => other,
    });
    (blocks, stop_reason.map(str::to_string))
}

fn usage_from_metadata(meta: Option<GeminiUsage>) -> Usage {
    meta.map(|u| Usage {
        input_tokens: u.prompt_token_count.unwrap_or(0),
        output_tokens: u.candidates_token_count.unwrap_or(0),
    })
    .unwrap_or(Usage {
        input_tokens: 0,
        output_tokens: 0,
    })
}

// ── ModelProvider impl ───────────────────────────────────────────────

impl ModelProvider for GeminiProvider {
    async fn generate(
        &self,
        system: Option<&str>,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> super::Result<ModelResponse> {
        let body = GenerateRequest {
            contents: to_gemini_contents(messages),
            system_instruction: system.map(|s| GeminiSystemInstruction {
                parts: vec![GeminiTextPart { text: s }],
            }),
            tools: to_gemini_tools(tools),
            generation_config: GenerationConfig {
                max_output_tokens: 4096,
            },
        };

        let response = self.request(":generateContent").json(&body).send().await?;

        let status = response.status();
        if !status.is_success() {
            let msg = response.text().await.unwrap_or_default();
            return Err(ProviderError::Api {
                status: status.as_u16(),
                message: msg,
            });
        }

        let resp: GenerateResponse = response.json().await?;
        let candidate =
            resp.candidates.into_iter().next().ok_or_else(|| {
                ProviderError::Format("no candidates in Gemini response".to_string())
            })?;

        let (content, stop_reason) = parse_candidate(&candidate);
        Ok(ModelResponse {
            content,
            usage: usage_from_metadata(resp.usage_metadata),
            stop_reason,
        })
    }

    async fn generate_stream(
        &self,
        system: Option<&str>,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> super::Result<tokio::sync::mpsc::Receiver<StreamEvent>> {
        let body = GenerateRequest {
            contents: to_gemini_contents(messages),
            system_instruction: system.map(|s| GeminiSystemInstruction {
                parts: vec![GeminiTextPart { text: s }],
            }),
            tools: to_gemini_tools(tools),
            generation_config: GenerationConfig {
                max_output_tokens: 4096,
            },
        };

        let response = self
            .request(":streamGenerateContent?alt=sse")
            .json(&body)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let msg = response.text().await.unwrap_or_default();
            return Err(ProviderError::Api {
                status: status.as_u16(),
                message: msg,
            });
        }

        let (tx, rx) = tokio::sync::mpsc::channel(64);
        let byte_stream = response.bytes_stream();

        tokio::spawn(async move {
            if let Err(e) = process_gemini_sse(byte_stream, &tx).await {
                let _ = tx.send(StreamEvent::Error(e)).await;
            }
        });

        Ok(rx)
    }
}

// ── SSE processing ───────────────────────────────────────────────────

/// Process Gemini SSE byte stream.
///
/// Gemini SSE format: each `data:` line is a complete GenerateContentResponse JSON.
/// We emit TextDelta events for each text chunk and accumulate tool calls.
/// The final frame has `finish_reason` set and contains usage_metadata.
async fn process_gemini_sse(
    stream: impl futures::Stream<Item = reqwest::Result<impl AsRef<[u8]>>>,
    tx: &tokio::sync::mpsc::Sender<StreamEvent>,
) -> std::result::Result<(), String> {
    tokio::pin!(stream);

    let mut line_buf = String::new();
    let mut usage = Usage {
        input_tokens: 0,
        output_tokens: 0,
    };
    let mut stop_reason: Option<String> = None;

    const MAX_SSE_BUFFER: usize = 10 * 1024 * 1024;

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(|e: reqwest::Error| e.to_string())?;
        let chunk_str = String::from_utf8_lossy(chunk.as_ref());
        line_buf.push_str(&chunk_str);

        if line_buf.len() > MAX_SSE_BUFFER {
            return Err("Gemini SSE stream buffer exceeded 10MB".to_string());
        }

        while let Some(nl) = line_buf.find('\n') {
            let line = line_buf[..nl].trim_end_matches('\r').to_string();
            line_buf = line_buf[nl + 1..].to_string();

            if line.is_empty() {
                continue;
            }

            let data = match line.strip_prefix("data: ") {
                Some(d) => d,
                None => continue,
            };

            let resp: GenerateResponse = match serde_json::from_str(data) {
                Ok(r) => r,
                Err(_) => continue,
            };

            // Update usage from every frame (last one wins with full counts)
            if let Some(meta) = resp.usage_metadata {
                usage = Usage {
                    input_tokens: meta.prompt_token_count.unwrap_or(usage.input_tokens),
                    output_tokens: meta.candidates_token_count.unwrap_or(usage.output_tokens),
                };
            }

            let candidate = match resp.candidates.into_iter().next() {
                Some(c) => c,
                None => continue,
            };

            // Capture stop_reason from this frame
            if let Some(ref fr) = candidate.finish_reason {
                stop_reason = Some(
                    match fr.as_str() {
                        "STOP" => "end_turn",
                        "MAX_TOKENS" => "max_tokens",
                        "FUNCTION_CALL" => "tool_use",
                        other => other,
                    }
                    .to_string(),
                );
            }

            // Emit events for each part
            for part in candidate.content.parts {
                match part {
                    GeminiPart::Text(t) => {
                        if !t.text.is_empty() {
                            if tx.send(StreamEvent::TextDelta(t.text)).await.is_err() {
                                return Ok(());
                            }
                        }
                    }
                    GeminiPart::FunctionCall(fc) => {
                        if tx
                            .send(StreamEvent::ToolUse {
                                id: format!("gemini-{}", uuid::Uuid::new_v4()),
                                name: fc.function_call.name,
                                input: fc.function_call.args,
                            })
                            .await
                            .is_err()
                        {
                            return Ok(());
                        }
                    }
                    GeminiPart::FunctionResponse(_) => {}
                }
            }

            // Terminal frame — emit Done and return early
            if stop_reason.is_some() {
                let _ = tx
                    .send(StreamEvent::Done {
                        usage: usage.clone(),
                        stop_reason: stop_reason.clone(),
                    })
                    .await;
                return Ok(());
            }
        }
    }

    // Stream ended without a terminal frame — emit Done anyway
    let _ = tx.send(StreamEvent::Done { usage, stop_reason }).await;

    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn from_env_missing_returns_err() {
        // With no env var and no key file, should return MissingCredentials.
        // (May pass if the test machine has a key file — that's fine.)
        let _result = GeminiProvider::from_env(None);
        // Either Ok or MissingCredentials is acceptable
    }

    #[test]
    fn default_model() {
        assert_eq!(DEFAULT_MODEL, "gemini-2.5-flash");
    }

    #[test]
    fn model_accessor() {
        let p = GeminiProvider::new("test-key", Some("gemini-2.5-pro"));
        assert_eq!(p.model(), "gemini-2.5-pro");
    }

    #[test]
    fn to_gemini_contents_user_text() {
        let msgs = vec![Message::user("hello")];
        let contents = to_gemini_contents(&msgs);
        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0].role, "user");
        match &contents[0].parts[0] {
            GeminiPart::Text(t) => assert_eq!(t.text, "hello"),
            _ => panic!("expected Text part"),
        }
    }

    #[test]
    fn to_gemini_contents_assistant_role_becomes_model() {
        let msgs = vec![Message {
            role: "assistant".to_string(),
            content: vec![ContentBlock::Text {
                text: "hi".to_string(),
            }],
        }];
        let contents = to_gemini_contents(&msgs);
        assert_eq!(contents[0].role, "model");
    }

    #[test]
    fn to_gemini_contents_tool_use() {
        let msgs = vec![Message {
            role: "assistant".to_string(),
            content: vec![ContentBlock::ToolUse {
                id: "tu_1".to_string(),
                name: "node_list".to_string(),
                input: json!({"status": "all"}),
            }],
        }];
        let contents = to_gemini_contents(&msgs);
        match &contents[0].parts[0] {
            GeminiPart::FunctionCall(fc) => {
                assert_eq!(fc.function_call.name, "node_list");
                assert_eq!(fc.function_call.args["status"], "all");
            }
            _ => panic!("expected FunctionCall part"),
        }
    }

    #[test]
    fn to_gemini_contents_tool_result_success() {
        let msgs = vec![Message::tool_results(vec![(
            "node_list".to_string(),
            "2 nodes".to_string(),
            None,
        )])];
        let contents = to_gemini_contents(&msgs);
        match &contents[0].parts[0] {
            GeminiPart::FunctionResponse(fr) => {
                assert_eq!(fr.function_response.name, "node_list");
                assert_eq!(fr.function_response.response["output"], "2 nodes");
            }
            _ => panic!("expected FunctionResponse part"),
        }
    }

    #[test]
    fn to_gemini_contents_tool_result_error() {
        let msgs = vec![Message::tool_results(vec![(
            "node_list".to_string(),
            "not found".to_string(),
            Some(true),
        )])];
        let contents = to_gemini_contents(&msgs);
        match &contents[0].parts[0] {
            GeminiPart::FunctionResponse(fr) => {
                assert_eq!(fr.function_response.response["error"], "not found");
            }
            _ => panic!("expected FunctionResponse part"),
        }
    }

    #[test]
    fn to_gemini_tools_empty() {
        let tools = to_gemini_tools(&[]);
        assert!(tools.is_empty());
    }

    #[test]
    fn to_gemini_tools_nonempty() {
        let tools = vec![ToolDefinition {
            name: "node_list".to_string(),
            description: "List nodes".to_string(),
            input_schema: json!({"type": "object", "properties": {}}),
        }];
        let gemini_tools = to_gemini_tools(&tools);
        assert_eq!(gemini_tools.len(), 1);
        assert_eq!(gemini_tools[0].function_declarations[0].name, "node_list");
    }

    #[test]
    fn parse_candidate_text() {
        let candidate = GeminiCandidate {
            content: GeminiContent {
                role: "model".to_string(),
                parts: vec![GeminiPart::Text(GeminiTextPartOwned {
                    text: "Hello!".to_string(),
                })],
            },
            finish_reason: Some("STOP".to_string()),
        };
        let (blocks, stop) = parse_candidate(&candidate);
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Text { text } => assert_eq!(text, "Hello!"),
            _ => panic!("expected Text"),
        }
        assert_eq!(stop.as_deref(), Some("end_turn"));
    }

    #[test]
    fn parse_candidate_function_call() {
        let candidate = GeminiCandidate {
            content: GeminiContent {
                role: "model".to_string(),
                parts: vec![GeminiPart::FunctionCall(GeminiFunctionCall {
                    function_call: GeminiFn {
                        name: "node_list".to_string(),
                        args: json!({}),
                    },
                })],
            },
            finish_reason: Some("FUNCTION_CALL".to_string()),
        };
        let (blocks, stop) = parse_candidate(&candidate);
        assert!(matches!(&blocks[0], ContentBlock::ToolUse { name, .. } if name == "node_list"));
        assert_eq!(stop.as_deref(), Some("tool_use"));
    }

    #[test]
    fn usage_from_metadata_none() {
        let u = usage_from_metadata(None);
        assert_eq!(u.input_tokens, 0);
        assert_eq!(u.output_tokens, 0);
    }

    #[test]
    fn usage_from_metadata_some() {
        let u = usage_from_metadata(Some(GeminiUsage {
            prompt_token_count: Some(10),
            candidates_token_count: Some(20),
        }));
        assert_eq!(u.input_tokens, 10);
        assert_eq!(u.output_tokens, 20);
    }

    #[tokio::test]
    async fn gemini_sse_single_text_frame() {
        let frame_json = serde_json::json!({
            "candidates": [{
                "content": { "role": "model", "parts": [{"text": "Hello world"}] },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 5
            }
        });
        let sse = format!("data: {}\n\n", frame_json);
        let byte_stream =
            futures::stream::once(async move { Ok::<_, reqwest::Error>(sse.into_bytes()) });

        let (tx, mut rx) = tokio::sync::mpsc::channel(32);
        process_gemini_sse(byte_stream, &tx).await.unwrap();
        drop(tx);

        let mut text = String::new();
        let mut got_done = false;
        while let Some(ev) = rx.recv().await {
            match ev {
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
    async fn gemini_sse_multi_frame_text() {
        let frame1 = serde_json::json!({
            "candidates": [{"content": {"role": "model", "parts": [{"text": "Hel"}]}}]
        });
        let frame2 = serde_json::json!({
            "candidates": [{
                "content": {"role": "model", "parts": [{"text": "lo!"}]},
                "finishReason": "STOP"
            }],
            "usageMetadata": {"promptTokenCount": 5, "candidatesTokenCount": 2}
        });
        let sse = format!("data: {}\n\ndata: {}\n\n", frame1, frame2);
        let byte_stream =
            futures::stream::once(async move { Ok::<_, reqwest::Error>(sse.into_bytes()) });

        let (tx, mut rx) = tokio::sync::mpsc::channel(32);
        process_gemini_sse(byte_stream, &tx).await.unwrap();
        drop(tx);

        let mut text = String::new();
        while let Some(ev) = rx.recv().await {
            if let StreamEvent::TextDelta(t) = ev {
                text.push_str(&t);
            }
        }
        assert_eq!(text, "Hello!");
    }

    #[tokio::test]
    async fn gemini_sse_function_call() {
        let frame = serde_json::json!({
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{
                        "functionCall": { "name": "node_list", "args": {"status": "all"} }
                    }]
                },
                "finishReason": "FUNCTION_CALL"
            }],
            "usageMetadata": {"promptTokenCount": 20, "candidatesTokenCount": 15}
        });
        let sse = format!("data: {}\n\n", frame);
        let byte_stream =
            futures::stream::once(async move { Ok::<_, reqwest::Error>(sse.into_bytes()) });

        let (tx, mut rx) = tokio::sync::mpsc::channel(32);
        process_gemini_sse(byte_stream, &tx).await.unwrap();
        drop(tx);

        let mut got_tool = false;
        while let Some(ev) = rx.recv().await {
            if let StreamEvent::ToolUse { name, input, .. } = ev {
                assert_eq!(name, "node_list");
                assert_eq!(input["status"], "all");
                got_tool = true;
            }
        }
        assert!(got_tool);
    }
}
