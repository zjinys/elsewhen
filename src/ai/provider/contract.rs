//! 公共契约：TokenUsage、AiReply、ProviderEvent 与 AiProvider trait。

use super::*;

#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    /// Provider 报告的前缀缓存命中 token 数（deepseek
    /// prompt_cache_hit_tokens / OpenAI cached_tokens）。None 表示渠道未报。
    pub prompt_cached_tokens: Option<u64>,
}

/// AI provider reply: content plus optional tool calls, usage and model metadata
#[derive(Debug, Clone)]
pub struct AiReply {
    /// 最终文本；若本轮回调了工具，则可能为空
    pub content: String,
    /// 原生 tool-calling：模型要求调用的工具列表（空 = 未调用）
    pub tool_calls: Vec<ToolCall>,
    pub model: Option<String>,
    pub usage: Option<TokenUsage>,
    pub reasoning_content: Option<String>,
    /// OpenAI 兼容接口的 `choices[0].finish_reason`（`stop` / `length` /
    /// `function_call` / `tool_calls`）。
    ///
    /// 用于区分正常结束、输出耗尽和声明调用工具却缺少调用内容的响应。
    /// `function_call` 也可能对应旧版合法调用，须先解析旧字段再诊断。
    /// 该字段和 usage 本身不能证明内容在哪一层丢失。
    pub finish_reason: Option<String>,
}

impl AiReply {
    /// 构造一个纯文本回复
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            tool_calls: Vec::new(),
            model: None,
            usage: None,
            reasoning_content: None,
            finish_reason: None,
        }
    }
}

/// Distinct channels keep local diagnostics out of model prose and checkpoints.
#[derive(Debug)]
pub enum ProviderEvent<'a> {
    Poll,
    TextDelta(&'a str),
    Budget {
        attempt: u32,
        trigger: &'static str,
        report: &'a crate::ai::budget::BudgetReport,
    },
}

/// AI provider trait for generating replies
pub trait AiProvider {
    /// 生成回复；`tools` 非空时走原生 tool-calling（服务端支持才生效）
    fn generate_reply_with_tools(
        &self,
        messages: Vec<ContextMessage>,
        tools: Option<&[ToolSpec]>,
    ) -> Result<AiReply>;

    /// Observable generation. Providers without streaming still preserve request-boundary cancellation.
    fn generate_observed(
        &self,
        messages: Vec<ContextMessage>,
        tools: Option<&[ToolSpec]>,
        observer: &mut dyn FnMut(ProviderEvent<'_>) -> Result<()>,
    ) -> Result<AiReply> {
        observer(ProviderEvent::Poll)?;
        let reply = self.generate_reply_with_tools(messages, tools)?;
        observer(ProviderEvent::Poll)?;
        Ok(reply)
    }

    /// Conservative preflight for explicit task supplements. Unknown provider
    /// protocols retain the input in the queue rather than dropping context.
    fn accepts_supplement(
        &self,
        _messages: &[ContextMessage],
        _tools: Option<&[ToolSpec]>,
    ) -> Result<bool> {
        Ok(false)
    }

    /// 纯文本请求（不带 tools）
    fn generate_reply(&self, messages: Vec<ContextMessage>) -> Result<AiReply> {
        self.generate_reply_with_tools(messages, None)
    }
}
