use super::memory::ContextMessage;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Token usage reported by an AI provider
#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

/// AI provider reply: content plus optional usage/model metadata
#[derive(Debug, Clone)]
pub struct AiReply {
    pub content: String,
    pub model: Option<String>,
    pub usage: Option<TokenUsage>,
}

/// AI provider trait for generating replies
pub trait AiProvider {
    /// Generate a reply from context messages
    fn generate_reply(&self, messages: Vec<ContextMessage>) -> Result<AiReply>;
}

/// OpenAI-compatible provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiCompatibleConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: Option<u32>,
}

impl Default for OpenAiCompatibleConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: String::new(),
            model: "gpt-3.5-turbo".to_string(),
            temperature: 0.7,
            max_tokens: None,
        }
    }
}

/// OpenAI-compatible provider (OpenAI, DeepSeek, vLLM, etc.)
pub struct OpenAiCompatibleProvider {
    config: OpenAiCompatibleConfig,
    client: reqwest::blocking::Client,
}

impl OpenAiCompatibleProvider {
    pub fn new(config: OpenAiCompatibleConfig) -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self { config, client })
    }
}

#[derive(Serialize)]
struct OpenAiMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct OpenAiRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

#[derive(Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
    model: Option<String>,
    usage: Option<OpenAiUsage>,
}

#[derive(Deserialize)]
struct OpenAiUsage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    total_tokens: Option<u64>,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessageResponse,
}

#[derive(Deserialize)]
struct OpenAiMessageResponse {
    content: String,
}

impl AiProvider for OpenAiCompatibleProvider {
    fn generate_reply(&self, messages: Vec<ContextMessage>) -> Result<AiReply> {
        let openai_messages: Vec<OpenAiMessage> = messages
            .into_iter()
            .map(|m| OpenAiMessage {
                role: m.role,
                content: m.content,
            })
            .collect();

        let request = OpenAiRequest {
            model: self.config.model.clone(),
            messages: openai_messages,
            temperature: self.config.temperature,
            max_tokens: self.config.max_tokens,
        };

        let url = format!("{}/chat/completions", self.config.base_url);

        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .context("Failed to send request to AI provider")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            anyhow::bail!("AI provider returned error {}: {}", status, body);
        }

        let ai_response: OpenAiResponse = response
            .json()
            .context("Failed to parse AI provider response")?;

        let content = ai_response
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .context("AI provider returned no choices")?;

        let usage = ai_response.usage.map(|u| TokenUsage {
            prompt_tokens: u.prompt_tokens.unwrap_or(0),
            completion_tokens: u.completion_tokens.unwrap_or(0),
            total_tokens: u.total_tokens.unwrap_or_else(|| {
                u.prompt_tokens.unwrap_or(0) + u.completion_tokens.unwrap_or(0)
            }),
        });

        Ok(AiReply {
            content,
            model: ai_response.model,
            usage,
        })
    }
}

/// Ollama provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    pub base_url: String,
    pub model: String,
    pub temperature: f32,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:11434".to_string(),
            model: "llama2".to_string(),
            temperature: 0.7,
        }
    }
}

/// Ollama local provider
pub struct OllamaProvider {
    config: OllamaConfig,
    client: reqwest::blocking::Client,
}

impl OllamaProvider {
    pub fn new(config: OllamaConfig) -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self { config, client })
    }
}

#[derive(Serialize)]
struct OllamaMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct OllamaRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    stream: bool,
    options: OllamaOptions,
}

#[derive(Serialize)]
struct OllamaOptions {
    temperature: f32,
}

#[derive(Deserialize)]
struct OllamaResponse {
    message: OllamaMessageResponse,
    model: Option<String>,
    prompt_eval_count: Option<u64>,
    eval_count: Option<u64>,
}

#[derive(Deserialize)]
struct OllamaMessageResponse {
    content: String,
}

impl AiProvider for OllamaProvider {
    fn generate_reply(&self, messages: Vec<ContextMessage>) -> Result<AiReply> {
        let ollama_messages: Vec<OllamaMessage> = messages
            .into_iter()
            .map(|m| OllamaMessage {
                role: m.role,
                content: m.content,
            })
            .collect();

        let request = OllamaRequest {
            model: self.config.model.clone(),
            messages: ollama_messages,
            stream: false,
            options: OllamaOptions {
                temperature: self.config.temperature,
            },
        };

        let url = format!("{}/api/chat", self.config.base_url);

        let response = self.client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .context("Failed to send request to Ollama")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            anyhow::bail!("Ollama returned error {}: {}", status, body);
        }

        let ollama_response: OllamaResponse = response
            .json()
            .context("Failed to parse Ollama response")?;

        let usage = match (ollama_response.prompt_eval_count, ollama_response.eval_count) {
            (None, None) => None,
            (prompt, completion) => {
                let prompt_tokens = prompt.unwrap_or(0);
                let completion_tokens = completion.unwrap_or(0);
                Some(TokenUsage {
                    prompt_tokens,
                    completion_tokens,
                    total_tokens: prompt_tokens + completion_tokens,
                })
            }
        };

        Ok(AiReply {
            content: ollama_response.message.content,
            model: ollama_response.model,
            usage,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_compatible_config_has_defaults() {
        let config = OpenAiCompatibleConfig::default();
        assert_eq!(config.base_url, "https://api.openai.com/v1");
        assert_eq!(config.model, "gpt-3.5-turbo");
        assert_eq!(config.temperature, 0.7);
    }

    #[test]
    fn ollama_config_has_defaults() {
        let config = OllamaConfig::default();
        assert_eq!(config.base_url, "http://localhost:11434");
        assert_eq!(config.model, "llama2");
        assert_eq!(config.temperature, 0.7);
    }

    #[test]
    fn openai_provider_can_be_created() {
        let config = OpenAiCompatibleConfig::default();
        let provider = OpenAiCompatibleProvider::new(config);
        assert!(provider.is_ok());
    }

    #[test]
    fn ollama_provider_can_be_created() {
        let config = OllamaConfig::default();
        let provider = OllamaProvider::new(config);
        assert!(provider.is_ok());
    }

    #[test]
    fn openai_response_parses_usage_and_model() {
        let json = r#"{"choices":[{"message":{"content":"hi"}}],"model":"gpt-4o","usage":{"prompt_tokens":12,"completion_tokens":5,"total_tokens":17}}"#;
        let resp: OpenAiResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.model.as_deref(), Some("gpt-4o"));
        let u = resp.usage.unwrap();
        assert_eq!(u.prompt_tokens, Some(12));
        assert_eq!(u.completion_tokens, Some(5));
        assert_eq!(u.total_tokens, Some(17));
    }

    #[test]
    fn openai_response_without_usage_is_none() {
        // 有些 provider 不返回 usage，应容忍缺失
        let json = r#"{"choices":[{"message":{"content":"hi"}}]}"#;
        let resp: OpenAiResponse = serde_json::from_str(json).unwrap();
        assert!(resp.usage.is_none());
        assert!(resp.model.is_none());
    }

    #[test]
    fn ollama_response_parses_eval_counts() {
        let json = r#"{"message":{"content":"hi"},"model":"llama3","prompt_eval_count":20,"eval_count":8}"#;
        let resp: OllamaResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.model.as_deref(), Some("llama3"));
        assert_eq!(resp.prompt_eval_count, Some(20));
        assert_eq!(resp.eval_count, Some(8));
    }
}
