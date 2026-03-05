//! Ollama provider — local LLM inference via `localhost:11434/api/chat`.

use super::{ContentBlock, Message, ModelProvider, ModelResponse, ProviderError, Usage};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const DEFAULT_MODEL: &str = "llama3.2";

/// Ollama provider for local LLM inference.
#[derive(Debug)]
pub struct OllamaProvider {
    client: reqwest::Client,
    model: String,
    endpoint: String,
}

#[derive(Serialize)]
struct OllamaRequest<'a> {
    model: &'a str,
    messages: Vec<OllamaMsg>,
    stream: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<Value>,
}

#[derive(Serialize)]
struct OllamaMsg {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OllamaToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OllamaToolCall {
    function: OllamaFnCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OllamaFnCall {
    name: String,
    arguments: Value,
}

#[derive(Deserialize)]
struct OllamaResp {
    message: OllamaRespMsg,
    prompt_eval_count: Option<u32>,
    eval_count: Option<u32>,
}

#[derive(Deserialize)]
struct OllamaRespMsg {
    #[serde(default)]
    content: String,
    #[serde(default)]
    tool_calls: Vec<OllamaToolCall>,
}

impl OllamaProvider {
    pub fn from_env(model: Option<&str>) -> super::Result<Self> {
        let endpoint =
            std::env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://localhost:11434".to_string());
        Ok(Self {
            client: reqwest::Client::new(),
            model: model.unwrap_or(DEFAULT_MODEL).to_string(),
            endpoint,
        })
    }
}

/// Convert Claude-style content blocks to Ollama's flat message format.
fn to_ollama_messages(messages: &[Message]) -> Vec<OllamaMsg> {
    let mut out = Vec::new();
    for msg in messages {
        // ToolResult blocks → separate "tool" role messages
        let tool_results: Vec<_> = msg
            .content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::ToolResult {
                    content,
                    tool_use_id,
                    ..
                } => Some((tool_use_id.clone(), content.clone())),
                _ => None,
            })
            .collect();

        if !tool_results.is_empty() {
            for (name, content) in tool_results {
                out.push(OllamaMsg {
                    role: "tool".to_string(),
                    content,
                    tool_calls: None,
                    tool_name: Some(name),
                });
            }
            continue;
        }

        // ToolUse blocks → tool_calls array on assistant messages
        let tool_calls: Vec<OllamaToolCall> = msg
            .content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::ToolUse { name, input, .. } => Some(OllamaToolCall {
                    function: OllamaFnCall {
                        name: name.clone(),
                        arguments: input.clone(),
                    },
                }),
                _ => None,
            })
            .collect();

        out.push(OllamaMsg {
            role: msg.role.clone(),
            content: msg.text(),
            tool_calls: if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls)
            },
            tool_name: None,
        });
    }
    out
}

impl ModelProvider for OllamaProvider {
    async fn generate(
        &self,
        system: Option<&str>,
        messages: &[Message],
        tools: &[super::ToolDefinition],
    ) -> super::Result<ModelResponse> {
        let mut msgs = Vec::new();
        if let Some(sys) = system {
            msgs.push(OllamaMsg {
                role: "system".to_string(),
                content: sys.to_string(),
                tool_calls: None,
                tool_name: None,
            });
        }
        msgs.extend(to_ollama_messages(messages));

        let ollama_tools: Vec<Value> = tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.input_schema,
                    }
                })
            })
            .collect();

        let body = OllamaRequest {
            model: &self.model,
            messages: msgs,
            stream: false,
            tools: ollama_tools,
        };

        let url = format!("{}/api/chat", self.endpoint.trim_end_matches('/'));
        let response = self.client.post(&url).json(&body).send().await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let message = response.text().await.unwrap_or_default();
            return Err(ProviderError::Api { status, message });
        }

        let resp: OllamaResp = response.json().await?;

        let mut content = Vec::new();
        if !resp.message.content.is_empty() {
            content.push(ContentBlock::Text {
                text: resp.message.content,
            });
        }

        let has_tool_calls = !resp.message.tool_calls.is_empty();
        for tc in resp.message.tool_calls {
            content.push(ContentBlock::ToolUse {
                id: format!("ollama-{}", uuid::Uuid::new_v4()),
                name: tc.function.name,
                input: tc.function.arguments,
            });
        }

        Ok(ModelResponse {
            content,
            usage: Usage {
                input_tokens: resp.prompt_eval_count.unwrap_or(0),
                output_tokens: resp.eval_count.unwrap_or(0),
            },
            stop_reason: Some(
                if has_tool_calls {
                    "tool_use"
                } else {
                    "end_turn"
                }
                .to_string(),
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_from_env() {
        let p = OllamaProvider::from_env(None).unwrap();
        assert_eq!(p.model, DEFAULT_MODEL);
        assert!(p.endpoint.contains("11434"));
    }

    #[test]
    fn provider_custom_model() {
        let p = OllamaProvider::from_env(Some("mistral")).unwrap();
        assert_eq!(p.model, "mistral");
    }

    #[test]
    fn request_tools_serialized() {
        let schema = serde_json::json!({"type": "object", "properties": {}});
        let req = OllamaRequest {
            model: "llama3.2",
            messages: vec![],
            stream: false,
            tools: vec![serde_json::json!({
                "type": "function",
                "function": { "name": "test", "description": "d", "parameters": schema }
            })],
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["tools"][0]["function"]["name"], "test");
    }

    #[test]
    fn request_empty_tools_omitted() {
        let req = OllamaRequest {
            model: "llama3.2",
            messages: vec![],
            stream: false,
            tools: vec![],
        };
        let json = serde_json::to_value(&req).unwrap();
        assert!(json.get("tools").is_none());
    }

    #[test]
    fn response_with_tool_calls() {
        let json = r#"{
            "message": { "content": "", "tool_calls": [{ "function": { "name": "list_nodes", "arguments": {} } }] },
            "prompt_eval_count": 50, "eval_count": 10
        }"#;
        let resp: OllamaResp = serde_json::from_str(json).unwrap();
        assert_eq!(resp.message.tool_calls.len(), 1);
        assert_eq!(resp.message.tool_calls[0].function.name, "list_nodes");
    }

    #[test]
    fn response_text_only() {
        let resp: OllamaResp =
            serde_json::from_str(r#"{ "message": { "content": "Hello!" } }"#).unwrap();
        assert_eq!(resp.message.content, "Hello!");
        assert!(resp.message.tool_calls.is_empty());
    }

    #[test]
    fn convert_messages_text() {
        let ollama = to_ollama_messages(&[Message::user("hello")]);
        assert_eq!(ollama.len(), 1);
        assert_eq!(ollama[0].role, "user");
        assert_eq!(ollama[0].content, "hello");
    }

    #[test]
    fn convert_messages_tool_results() {
        let msgs = vec![Message::tool_results(vec![(
            "list_nodes".to_string(),
            "2 nodes".to_string(),
            None,
        )])];
        let ollama = to_ollama_messages(&msgs);
        assert_eq!(ollama[0].role, "tool");
        assert_eq!(ollama[0].tool_name.as_deref(), Some("list_nodes"));
    }

    #[test]
    fn convert_messages_assistant_tool_use() {
        let msgs = vec![Message {
            role: "assistant".to_string(),
            content: vec![
                ContentBlock::Text {
                    text: "Checking.".to_string(),
                },
                ContentBlock::ToolUse {
                    id: "t1".to_string(),
                    name: "list_nodes".to_string(),
                    input: serde_json::json!({}),
                },
            ],
        }];
        let ollama = to_ollama_messages(&msgs);
        assert_eq!(
            ollama[0].tool_calls.as_ref().unwrap()[0].function.name,
            "list_nodes"
        );
    }
}
