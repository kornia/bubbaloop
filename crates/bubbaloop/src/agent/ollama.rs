use crate::agent::claude::{ApiResponse, ContentBlock, Message, ToolDefinition, Usage};
use serde::{Deserialize, Serialize};

pub const DEFAULT_MODEL: &str = "llama3.2";

#[derive(Debug, thiserror::Error)]
pub enum OllamaError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("API error (status {status}): {message}")]
    Api { status: u16, message: String },
}

#[derive(Debug)]
pub struct OllamaClient {
    client: reqwest::Client,
    model: String,
    endpoint: String,
}

#[derive(Serialize)]
struct OllamaMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct OllamaRequest<'a> {
    model: &'a str,
    messages: Vec<OllamaMessage>,
    stream: bool,
}

#[derive(Deserialize)]
struct OllamaResponse {
    message: match_message::OllamaMessageResp,
    prompt_eval_count: Option<u32>,
    eval_count: Option<u32>,
}

mod match_message {
    use serde::Deserialize;
    #[derive(Deserialize)]
    pub struct OllamaMessageResp {
        pub content: String,
    }
}

impl OllamaClient {
    pub fn from_env(model: Option<&str>) -> Result<Self, OllamaError> {
        let endpoint =
            std::env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://localhost:11434".to_string());
        Ok(Self {
            client: reqwest::Client::new(),
            model: model.unwrap_or(DEFAULT_MODEL).to_string(),
            endpoint,
        })
    }

    pub async fn send(
        &self,
        system: Option<&str>,
        messages: &[Message],
        // Tool-use is intentionally passed through and dropped here.
        // Ollama handles simple chat completions while gracefully ignoring tool metadata.
        _tools: &Vec<ToolDefinition>,
    ) -> Result<ApiResponse, OllamaError> {
        let mut ollama_msgs = Vec::new();

        if let Some(sys) = system {
            ollama_msgs.push(OllamaMessage {
                role: "system".to_string(),
                content: sys.to_string(),
            });
        }

        for msg in messages {
            ollama_msgs.push(OllamaMessage {
                role: msg.role.clone(),
                content: msg.text(),
            });
        }

        let body = OllamaRequest {
            model: &self.model,
            messages: ollama_msgs,
            stream: false,
        };

        let url = format!("{}/api/chat", self.endpoint.trim_end_matches('/'));
        let response = self.client.post(&url).json(&body).send().await?;

        let status = response.status();
        if !status.is_success() {
            let message = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            return Err(OllamaError::Api {
                status: status.as_u16(),
                message,
            });
        }

        let resp: OllamaResponse = response.json().await?;

        // Format a unique message ID to fulfill Anthropic's type-checker requirements
        // without introducing real coupling. Agent memory ignores this ID safely.
        let id_str = format!("ollama-{}", uuid::Uuid::new_v4());

        // Map Ollama's response fields exactly to the Claude ApiResponse schema
        // so the core REPL loop can consume it natively without traits.
        Ok(ApiResponse {
            id: id_str,
            content: vec![ContentBlock::Text {
                text: resp.message.content,
            }],
            stop_reason: Some("end_turn".to_string()),
            usage: Usage {
                input_tokens: resp.prompt_eval_count.unwrap_or(0),
                output_tokens: resp.eval_count.unwrap_or(0),
            },
        })
    }
}
