#[cfg(test)]
mod acceptance_tests;
mod continuity;
mod supplement_tests;

use super::tool::{ToolCall, ToolPolicy, ToolRegistry, ToolResultMsg, dispatch_with_knowledge};
use crate::ai::provider::{AiProvider, AiReply};
use anyhow::Result;

use super::memory::{
    ContextMessage, MemoryProvider, OmissionPolicy, SegmentOrigin, SegmentPurpose, SimpleMemory,
    estimate_tokens,
};
use super::*;
use crate::storage::Store;
/// 调试日志宏（ELSEWHEN_DEBUG 开启才输出）。这些日志每轮对话都会触发（含工具结果摘录与 provider 错误串，可能带个人数据）。
macro_rules! debug_eprintln {
    ($($arg:tt)*) => {
        if std::env::var("ELSEWHEN_DEBUG").is_ok() {
            eprintln!($($arg)*);
        }
    };
}

/// 把相关知识摘要包成可选背景段；无命中返回 None（不挂空没损失）。
/// 段在引用校验/usage 里与 search 结果一视同仁，因为沿用同一个
/// 「知识选材：」识别前缀。
pub(crate) fn relevant_knowledge_mount(
    store: &Store,
    query: &str,
    max_pages: usize,
) -> Option<ContextMessage> {
    let (mounted, slugs) = crate::knowledge::summary_mount(store, query, max_pages).ok()?;
    if mounted.is_empty() {
        return None;
    }
    // 投递留痕：健康巡检的 mount_events_recent / mount_pages_recent 依
    // 赖于此（run 身份未就绪时 event 自然空操作）。
    let _ = super::generation::event(
        store,
        "knowledge_mounted",
        serde_json::json!({ "pages": slugs }),
    );
    Some(ContextMessage::new("system", mounted).with_segment(
        SegmentPurpose::SelectedEvidence,
        SegmentOrigin::KnowledgeSelection,
        None,
        OmissionPolicy::OptionalBackground,
    ))
}
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
            memory_type: MemoryType::SlidingWindow { max_tokens: 0 },
            provider_type: ProviderType::OpenAiCompatible,
        }
    }
}
/// 单次生成最多四轮，最后一轮只组织答复，不再执行新工具。
const MAX_TOOL_ROUNDS: usize = 4;

/// 文本协议的工具调用标记（AI 独占一行输出）
const TOOL_CALL_MARKER: &str = "[工具调用]";
/// 协议修复预算：疑似调用但格式未知时的最大回炉次数。
/// 与 MAX_TOOL_ROUNDS 独立计数，双保险防死循环。
const MAX_PROTOCOL_REPAIRS: usize = 2;

/// 原生协议失败后回落到纯文本协议前的固定小退避（毫秒）。
const PROTOCOL_FALLBACK_SLEEP_MS: u64 = 300;
/// 普通空回复的重试预算；协议切换和收尾也计入 MAX_TOOL_ROUNDS，
/// 每轮恰好一次请求，不再在轮内隐式重发。
const MAX_EMPTY_RETRIES: usize = 1;

/// 回炉提示：只讲唯一合法格式，不解释、不啰嗦，让模型重发一行调用。
const PROTOCOL_REPAIR_NUDGE: &str = "系统提示：你上一轮回复疑似包含一次工具调用，但格式无法识别，没有执行。工具调用必须独占一行，只允许唯一格式（其他任何格式都会被丢弃）：\n[工具调用]{\"name\":\"工具名\",\"arguments\":{...}}\n请重新只输出这一行调用，不要输出其他内容。";
/// 回炉提示：模型预告了未来动作却没动手。承诺不是答案。
/// 真实案例（2026-09-29 知识页「Omni flash 视频 prompt」）：用户贴入新 prompt，
/// 模型回「我准备一个更新版本…我将为你草拟更新后的版本，稍等片刻。」——零工具调用、
/// 零写入，用户等来的是一句预告。承诺检测抓的就是这一类。
const PROMISE_REPAIR_NUDGE: &str = "系统提示：你上一轮只预告了接下来要做什么（稍等片刻／我准备…），但没有调用任何工具，用户实际上什么都没拿到。承诺不是答案：要么现在调用工具把事情做完，要么直接把你已经能给出的内容说清楚。不要预告等待。";

/// 回炉提示：上游返回空内容。空回复等价于把对话交给用户自己猜。
const EMPTY_REPLY_NUDGE: &str = "系统提示：你上一轮返回了空内容，用户什么也没看到。请直接输出给用户看的文字；需要写入知识库就现在调用对应工具。不要返回空回复。";
const TEXT_PROTOCOL_NUDGE: &str = "系统提示：本轮改用文本工具协议，后续不再发送原生 tools。需要执行工具时，在正文中独占一行输出 [工具调用]{\"name\":\"工具名\",\"arguments\":{...}}，等待真实结果后再回答。缺失调用内容的那次响应没有执行任何操作；之前已返回的工具结果仍然有效，不要重复执行。能直接回答就给出结果，不要承诺稍后回复。";

/// Generate AI reply for a conversation
///
/// 流程：确认门（规则提议 + 待确认写动作）→ 构建上下文 → Agent 循环
/// （原生 tool-calling 优先，模型不支持时自动回落文本信封协议）→
/// 规则提议解析 → token 记账 → 落库。
pub fn generate_conversation_reply(
    conversation_id: &str,
    store: &Store,
    config: Option<ConversationConfig>,
) -> Result<String> {
    super::generation::generate_sync(conversation_id, store, config)
}
/// P1 追赶修复（2026-10-19 摘要事故诊断 option b）：把生产生成链路的
/// 「主 provider + 摘要参数」复刻成后台预热直接消费的形态。
/// provider_basis 必须与 generate_with_provider_configs 的 summary_configs
/// 完全一致——摘要 build 的缓存键按 basis 过滤，错位的 basis 会建出
/// 一套生产命不中的节点，白烧请求。返回 (provider, name, basis, model, budget)。
/// 只取 OpenAI-compatible 且首序配置（生产 failover 的第一尝试）；
/// Ollama 主渠道不适合后台静默预热（拉本地模型），跳过。
pub(crate) fn primary_summary_provider(
    store: &Store,
) -> Result<
    Option<(
        Box<dyn super::provider::AiProvider>,
        String,
        String,
        String,
        usize,
    )>,
> {
    let Some(config) = store
        .list_ai_provider_configs_for_runtime()?
        .into_iter()
        .find(|p| p.provider_type == "openai-compatible" && p.is_active)
    else {
        return Ok(None);
    };
    let basis = crate::storage::knowledge::content_hash(&format!(
        "{}|{}|{}|{:?}|{:?}|{}",
        config.id,
        config.base_url,
        config.model,
        config.max_tokens,
        config.context_window,
        config.temperature
    ));
    let window = config
        .context_window
        .map(|v| v as usize)
        .unwrap_or_else(|| super::budget::default_window(&config.model));
    let output = config.max_tokens.unwrap_or(4096) as usize;
    // 生产 selection_budget = window - output - window/20 - tool_reserve；
    // tool_reserve 是 21 个工具 schema 的实测规模（≈40K）。budget 只影响
    // 触发线与 allowance 校验，不进缓存键，近似无害。
    const TOOL_RESERVE: usize = 40_000;
    let budget = window
        .saturating_sub(output + window / 20 + TOOL_RESERVE)
        .max(1);
    let provider =
        super::provider::OpenAiCompatibleProvider::new(super::provider::OpenAiCompatibleConfig {
            base_url: config.base_url,
            api_key: config.api_key,
            model: config.model.clone(),
            temperature: config.temperature as f32,
            max_tokens: config.max_tokens.map(|v| v as u32),
            context_window: config.context_window.map(|v| v as u32),
        })?;
    Ok(Some((
        Box::new(provider),
        config.name,
        basis,
        config.model,
        budget,
    )))
}
pub(crate) fn generate_conversation_reply_core(
    conversation_id: &str,
    store: &Store,
    config: Option<ConversationConfig>,
) -> Result<String> {
    generate_with_provider_configs(
        conversation_id,
        store,
        config,
        store.list_ai_provider_configs_for_runtime()?,
    )
}
/// Provider 已兼容解析新旧调用字段后，仍声明调用工具却缺少调用内容。
/// 这能证明响应不完整，不能单凭 usage 断定是哪层丢失了内容。
fn missing_tool_call_response(reply: &AiReply) -> bool {
    reply.tool_calls.is_empty()
        && reply.content.trim().is_empty()
        && matches!(
            reply.finish_reason.as_deref(),
            Some("function_call") | Some("tool_calls")
        )
}
fn dispatch_for_turn(
    call: &ToolCall,
    registry: &ToolRegistry,
    store: &Store,
    conversation_id: &str,
    status_only: bool,
    allow_record_event: bool,
    knowledge_candidates: &[crate::knowledge::KnowledgeCitation],
    enabled_tools: &std::collections::HashSet<String>,
) -> Result<ToolResultMsg> {
    super::generation::check(store)?;
    // 延迟加载守卫：未启用的延迟工具拒绝执行，指引模型先 enable_tools。
    if registry.defer_tools()
        && registry.is_deferred(&call.name)
        && !enabled_tools.contains(&call.name)
    {
        return Ok(ToolResultMsg::err(
            call,
            format!(
                "工具「{}」尚未启用：先调用 enable_tools 并把它加入 tools 数组，下一轮即可使用。",
                call.name
            ),
        ));
    }
    if (call.name == "record_event" && !allow_record_event)
        || (status_only
            && registry
                .get(&call.name)
                .is_some_and(|tool| tool.policy() != ToolPolicy::Read))
    {
        return Ok(ToolResultMsg::err(
            call,
            "用户只在询问进展，请说明已有结果或待确认内容，不重复创建或执行操作",
        ));
    }
    let policy = registry
        .get(&call.name)
        .map(|t| t.policy())
        .unwrap_or(ToolPolicy::Read);
    super::generation::event(
        store,
        "tool_started",
        serde_json::json!({"call_id":call.id,"name":call.name}),
    )?;
    // Directory analysis may call a provider. It stages a proposal, never writes
    // the imported source here; its final staging call checks the run separately.
    let local_write = policy != ToolPolicy::Read && call.name != "import_directory_as_project";
    let transaction = if local_write {
        store.begin_write_transaction()?
    } else {
        None
    };
    super::generation::check(store)?;
    let result =
        dispatch_with_knowledge(call, registry, store, conversation_id, knowledge_candidates);
    super::generation::check(store)?;
    if !result.success {
        drop(transaction);
        super::generation::event(
            store,
            "tool_result",
            super::generation::tool_result_payload(call, &result, policy),
        )?;
    } else {
        super::generation::event(
            store,
            "tool_result",
            super::generation::tool_result_payload(call, &result, policy),
        )?;
        if let Some(transaction) = transaction {
            transaction.commit()?;
        }
    }
    Ok(result)
}
/// 写声明核验结论（以库内事实为准）。
#[derive(Debug)]
pub(crate) enum ClaimVerdict {
    /// 声明与事实一致（本轮确有写入），放行。
    Clean,
    /// 声称的实体只在待确认草稿里（草拟≠保存），需纠正并求确认。
    DraftOnly(Vec<String>),
    /// 声称的实体查无此页、是旧页面、或根本没指明——与事实不符，需纠正。
    Unproven(Vec<String>),
}

pub(crate) use continuity::failure_notice;
mod agent;
mod claim_detection;
mod content_chat;
mod generate_pipeline;
mod parse_tools;
mod tool_protocol;

#[cfg(test)]
pub(crate) use agent::AgentOutcome;
pub(crate) use agent::run_agent_loop;
pub(crate) use claim_detection::{
    future_promise_detected, turn_grounding, user_text_wants_write, write_claim_detected,
    write_claim_repair_nudge,
};
#[cfg(test)]
pub(crate) use content_chat::trim_noise;
pub use content_chat::{ContentChatMessage, generate_content_chat};
pub(crate) use content_chat::{
    is_confirmation, is_declination, parse_rule_proposal, pending_review_context,
};
pub(crate) use generate_pipeline::generate_with_provider_configs;
#[cfg(test)]
pub(crate) use generate_pipeline::jitter_backoff_ms;
pub(crate) use parse_tools::{
    extract_claimed_titles, parse_tool_call_envelope, strip_tool_protocol_residue,
    tool_call_attempt_detected, touched_this_turn, verify_claimed_writes,
};
pub(crate) use tool_protocol::{
    ToolProtocol, direct_query, is_transient_provider_error, text_protocol_history,
};

// 仅供测试链路使用的转发（release 构建不产生 unused 警告）
#[cfg(test)]
pub(crate) use agent::{assistant_continuation, run_agent_loop_inner};
#[cfg(test)]
pub(crate) use claim_detection::broad_write_claim;
#[cfg(test)]
pub(crate) use parse_tools::{
    extract_one_tool_call, parse_dsml_tool_calls, parse_xml_tool_calls, strip_dsml_artifacts,
};

#[cfg(test)]
#[path = "conversation_tests/mod.rs"]
mod tests;
