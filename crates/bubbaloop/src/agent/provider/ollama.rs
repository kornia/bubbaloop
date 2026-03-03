//! Ollama provider stub — local LLM integration (future).
//!
//! Implements ModelProvider with `todo!()` body. Ready for future
//! local LLM integration via `localhost:11434/api/chat`.

use super::{Message, ModelProvider, ModelResponse, ToolDefinition};

/// Ollama provider for local LLM inference.
#[derive(Debug)]
pub struct OllamaProvider {
    _endpoint: String,
}

impl OllamaProvider {
    /// Create an Ollama provider targeting the given endpoint.
    ///
    /// Default Ollama endpoint is `http://localhost:11434`.
    #[allow(dead_code)]
    pub fn new(endpoint: &str) -> Self {
        Self {
            _endpoint: endpoint.to_string(),
        }
    }
}

impl ModelProvider for OllamaProvider {
    async fn generate(
        &self,
        _system: Option<&str>,
        _messages: &[Message],
        _tools: &[ToolDefinition],
    ) -> super::Result<ModelResponse> {
        todo!("OllamaProvider not yet implemented — see design doc Section 3")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ollama_provider_creation() {
        let provider = OllamaProvider::new("http://localhost:11434");
        assert_eq!(provider._endpoint, "http://localhost:11434");
    }
}
