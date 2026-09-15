use crate::storage::Store;
use anyhow::{Context, Result};
use super::memory::{estimate_tokens, ContextMessage, MemoryProvider, SimpleMemory, SlidingWindowMemory};
use super::provider::{AiProvider, OpenAiCompatibleProvider, OllamaProvider};

/// Configuration for AI conversation
pub struct ConversationConfig {
    pub memory_type: MemoryType,
    pub provider_type: ProviderType,
}

#[derive(Debug, Clone)]
pub enum MemoryType {
    Simple { max_messages: usize },
    SlidingWindow { max_tokens: usize },
}

#[derive(Debug, Clone)]
pub enum ProviderType {
    OpenAiCompatible,
    Ollama,
}

impl Default for ConversationConfig {
    fn default() -> Self {
        Self {
            memory_type: MemoryType::SlidingWindow { max_tokens: 4096 },
            provider_type: ProviderType::OpenAiCompatible,
        }
    }
}

/// Generate AI reply for a conversation
pub fn generate_conversation_reply(
    conversation_id: &str,
    store: &Store,
    config: Option<ConversationConfig>,
) -> Result<String> {
    let config = config.unwrap_or_default();

    // Create memory provider
    let memory: Box<dyn MemoryProvider> = match config.memory_type {
        MemoryType::Simple { max_messages } => Box::new(SimpleMemory::new(max_messages)),
        MemoryType::SlidingWindow { max_tokens } => Box::new(SlidingWindowMemory::new(max_tokens)),
    };

    // Prepare context
    let context = memory.prepare_context(conversation_id, store)?;

    // Provider 未返回 usage 时的本地兜底：prompt 按上下文估算
    let prompt_tokens_estimate: u64 = context
        .iter()
        .map(|m| estimate_tokens(&m.content) as u64)
        .sum();

    // Create AI provider from database config
    let ai_provider: Box<dyn AiProvider> = match config.provider_type {
        ProviderType::OpenAiCompatible => {
            let ai_config = store
                .active_ai_provider_config()?
                .context("No active AI provider configuration")?;

            let provider_config = super::provider::OpenAiCompatibleConfig {
                base_url: ai_config.base_url,
                api_key: ai_config.api_key,
                model: ai_config.model,
                temperature: 0.7,
                max_tokens: None,
            };
            Box::new(OpenAiCompatibleProvider::new(provider_config)?)
        }
        ProviderType::Ollama => {
            // For Ollama, use environment variables or defaults
            let base_url = std::env::var("OLLAMA_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:11434".to_string());
            let model = std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama2".to_string());

            let provider_config = super::provider::OllamaConfig {
                base_url,
                model,
                temperature: 0.7,
            };
            Box::new(OllamaProvider::new(provider_config)?)
        }
    };

    // Generate reply
    let reply = ai_provider.generate_reply(context)?;
    let content = reply.content;

    // 记录 token 用量：优先 provider 返回的 usage，缺失则本地估算
    let usage = match reply.usage {
        Some(u) => u,
        None => {
            let completion_tokens = estimate_tokens(&content) as u64;
            super::provider::TokenUsage {
                prompt_tokens: prompt_tokens_estimate,
                completion_tokens,
                total_tokens: prompt_tokens_estimate + completion_tokens,
            }
        }
    };
    store.record_token_usage(
        Some(conversation_id),
        usage.prompt_tokens as i64,
        usage.completion_tokens as i64,
        usage.total_tokens as i64,
        reply.model.as_deref(),
    )?;

    // Save reply to database (no parent_message_id for AI replies in main thread)
    store.send_message(conversation_id, "assistant", &content, None)?;

    Ok(content)
}

/// 内容对话的一条临时消息（不入库）
#[derive(Debug, Clone)]
pub struct ContentChatMessage {
    pub role: String,
    pub content: String,
}

/// 针对一段抓取内容做一次性对话回复（不写库）。
/// 用于知识库导入前的临时讨论：上下文 = 基础角色设定 + 抓取内容（system）+ 临时消息历史。
/// 仍会记录 token 用量（conversation_id 为空，计入每日统计）。
pub fn generate_content_chat(
    content: &str,
    messages: &[ContentChatMessage],
    store: &Store,
) -> Result<String> {
    let mut context = vec![super::memory::build_content_system_prompt(content)];
    for m in messages {
        context.push(ContextMessage {
            role: m.role.clone(),
            content: m.content.clone(),
        });
    }

    // Provider 未返回 usage 时的本地兜底：prompt 按上下文估算
    let prompt_tokens_estimate: u64 = context
        .iter()
        .map(|m| estimate_tokens(&m.content) as u64)
        .sum();

    // 内容对话跟随应用当前的默认 provider（OpenAI 兼容）
    let ai_config = store
        .active_ai_provider_config()?
        .context("No active AI provider configuration")?;
    let provider_config = super::provider::OpenAiCompatibleConfig {
        base_url: ai_config.base_url,
        api_key: ai_config.api_key,
        model: ai_config.model,
        temperature: 0.7,
        max_tokens: None,
    };
    let provider = OpenAiCompatibleProvider::new(provider_config)?;

    let reply = provider.generate_reply(context)?;
    let completion_tokens_estimate = estimate_tokens(&reply.content) as u64;
    let usage = match reply.usage {
        Some(u) => u,
        None => super::provider::TokenUsage {
            prompt_tokens: prompt_tokens_estimate,
            completion_tokens: completion_tokens_estimate,
            total_tokens: prompt_tokens_estimate + completion_tokens_estimate,
        },
    };
    store.record_token_usage(
        None,
        usage.prompt_tokens as i64,
        usage.completion_tokens as i64,
        usage.total_tokens as i64,
        reply.model.as_deref(),
    )?;

    Ok(reply.content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_uses_sliding_window() {
        let config = ConversationConfig::default();
        matches!(config.memory_type, MemoryType::SlidingWindow { .. });
    }

    #[test]
    fn default_config_uses_openai_compatible() {
        let config = ConversationConfig::default();
        matches!(config.provider_type, ProviderType::OpenAiCompatible);
    }
}
