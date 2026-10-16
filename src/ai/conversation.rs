#[cfg(test)]
mod acceptance_tests;
mod continuity;
#[cfg(test)]
mod supplement_tests;
#[cfg(test)]
use super::tool::dispatch;

use super::memory::{
    ContextMessage, MemoryProvider, OmissionPolicy, SegmentOrigin, SegmentPurpose, SimpleMemory,
    estimate_tokens,
};
use super::provider::{AiProvider, AiReply, OllamaProvider, OpenAiCompatibleProvider, TokenUsage};
use super::tool::{ToolCall, ToolPolicy, ToolRegistry, ToolResultMsg, dispatch_with_knowledge};
use crate::storage::{RuleStatus, Store};
use anyhow::{Context, Result};
use continuity::{FINAL_ANSWER_NUDGE, FollowUp, progress_fallback, unhelpful_failure_reply};
use serde_json::Value;

/// agent loop 诊断日志统一 gate：默认静默，仅当设置 ELSEWHEN_DEBUG 时输出。
/// 这些日志每轮对话都会触发（含工具结果摘录与 provider 错误串，可能带个人数据），
/// 无条件打印既不必要也会把隐私写入终端（与 insight.rs 的既有模式对齐）。
macro_rules! debug_eprintln {
    ($($arg:tt)*) => {
        if std::env::var("ELSEWHEN_DEBUG").is_ok() {
            eprintln!($($arg)*);
        }
    };
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

/// provider failover 轮换的 jittered backoff：第 n 次失败约 400·n ms，
/// 叠加 ±30% 抖动。瞬时全挂时把连环重试拉开，避免 thundering herd。
fn jitter_backoff_ms(attempt: usize) -> u64 {
    let base = 400u64.saturating_mul(attempt as u64);
    // 用单调时间低位做伪随机源，不进 rand 依赖；jitter ∈ [0, 0.3·base]
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0);
    let jitter = nanos % 100 * base / 100 * 3 / 10;
    if nanos % 2 == 0 {
        base.saturating_add(jitter)
    } else {
        base.saturating_sub(jitter)
    }
}

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

// Production always enters through the DB-authoritative wrapper above. Unit
// acceptance can supply credentials in memory without materializing a live key
// into a temporary SQLite database.
fn generate_with_provider_configs(
    conversation_id: &str,
    store: &Store,
    config: Option<ConversationConfig>,
    providers: Vec<crate::storage::AiProviderConfig>,
) -> Result<String> {
    super::generation::check(store)?;
    super::generation::event(store, "phase", serde_json::json!({"phase":"preparing"}))?;
    let mut config = config.unwrap_or_default();
    let selection_model = providers
        .first()
        .map(|p| p.model.clone())
        .unwrap_or_else(|| "unknown".into());
    let registry = ToolRegistry::default();
    let tool_definitions = registry.provider_specs_for(true).into_iter().map(|spec| serde_json::json!({
        "type":"function", "function":{"name":spec.name,"description":spec.description,"parameters":spec.parameters}
    })).collect::<Vec<_>>();
    let tool_reserve =
        super::budget::text_tokens(&selection_model, &serde_json::to_string(&tool_definitions)?);
    if matches!(
        config.memory_type,
        MemoryType::SlidingWindow { max_tokens: 0 }
    ) {
        let provider = providers.first();
        let window = provider
            .as_ref()
            .map(|p| {
                p.context_window
                    .map(|v| v as usize)
                    .unwrap_or_else(|| super::budget::default_window(&p.model))
            })
            .unwrap_or(32768);
        let output = provider.as_ref().and_then(|p| p.max_tokens).unwrap_or(4096) as usize;
        let output = provider.as_ref().and_then(|p| p.max_tokens).unwrap_or(4096) as usize;
        // 注意：不要在此处加“总预算上限”——历史预算 = 上限 - tool_reserve，
        // 在 21 个工具定义 ≈16k tokens 的现状下，20k 上限会把历史预算挤到
        // 负值，记忆整体失效（实测：全部历史被“较早对话已省略”替代）。
        // 提速走：时间戳尾部化吃 provider 前缀缓存 + 工具瘦身腾预算。
        config.memory_type = MemoryType::SlidingWindow {
            max_tokens: window.saturating_sub(output + window / 20),
        };
    }

    // 本轮时间窗起点：写声明核验以库内时间为准（标题→查库→卡时间窗），
    // failover 换 provider 重试也不重置、不丢失，比内存 flag 稳定。
    let turn_start = chrono::Utc::now();

    // 本轮只读取提议/回执；自然语言不构成业务写入授权。

    // 2) 构建上下文
    let selection_budget = match config.memory_type {
        MemoryType::SlidingWindow { max_tokens } => {
            Some(max_tokens.saturating_sub(tool_reserve).max(1))
        }
        MemoryType::Simple { .. } => None,
    };
    // Keep full originals until the provider is available for cancellable derived
    // summaries. Final request budgeting remains in the provider in all paths.
    let mut context = match config.memory_type {
        MemoryType::Simple { max_messages } => {
            SimpleMemory::new(max_messages).prepare_context(conversation_id, store)?
        }
        MemoryType::SlidingWindow { .. } => {
            super::memory::prepare_uncompressed_context(conversation_id, store)?
        }
    };
    super::supplements::restore(store, conversation_id, &mut context)?;
    let feedback = crate::knowledge::workflows::feedback_context(store, conversation_id)?;
    if !feedback.is_empty() {
        context.push(ContextMessage::new("system", feedback).with_segment(
            SegmentPurpose::ReviewState,
            SegmentOrigin::Review,
            Some(conversation_id),
            OmissionPolicy::Required,
        ));
    }
    let followup = FollowUp::load(store, conversation_id)?;

    if let Some(review) = pending_review_context(store, conversation_id)? {
        context.push(review.with_segment(
            SegmentPurpose::ReviewState,
            SegmentOrigin::Review,
            Some(conversation_id),
            OmissionPolicy::Required,
        ));
    }

    // 本机数据直查：统计/清单类问题（token 用量、规则等）直接在本地查库注入，
    // 让 AI 直接引用真实数据作答，不依赖模型的 tool-calling 能力。
    let last_user = store
        .list_messages(conversation_id)?
        .into_iter()
        .rev()
        .find(|m| m.role == "user");
    if let Some(message) = &last_user {
        let annotations = crate::event::parse_annotations(&message.content);
        if !annotations.people.is_empty() && !annotations.targets.is_empty() {
            let pending = store.pending_actions_for_conversation(conversation_id)?;
            let already_pending = pending
                .iter()
                .any(|p| p.action == "propose_people_relations");
            if !already_pending {
                let people: Vec<_> = annotations
                    .people
                    .iter()
                    .map(|name| serde_json::json!({"name": name}))
                    .collect();
                let relations: Vec<_> = annotations.people.iter().flat_map(|person| {
                    annotations.targets.iter().map(move |target| serde_json::json!({"person": person, "target": target, "relation": "参与"}))
                }).collect();
                let args = serde_json::json!({
                    "people": people,
                    "relations": relations,
                    "source_event_id": store.latest_event_id_for_conversation(conversation_id)?
                });
                let transaction = store.begin_write_transaction()?;
                super::generation::check(store)?;
                store.create_pending_action(
                    conversation_id,
                    "propose_people_relations",
                    &args.to_string(),
                )?;
                super::generation::event(
                    store,
                    "proposal_ready",
                    serde_json::json!({"kind":"action"}),
                )?;
                if let Some(transaction) = transaction {
                    transaction.commit()?;
                }
                return Ok(format!(
                    "已识别联系人 {} 和事项 {}，并草拟了关系保存内容。在审阅卡点击接受确认保存，在审阅卡点击拒绝取消。",
                    annotations.people.join("、"),
                    annotations.targets.join("、")
                ));
            }
        }
    }
    let query = followup
        .as_ref()
        .filter(|f| f.unresolved)
        .map(|f| f.request.as_str())
        .or_else(|| last_user.as_ref().map(|m| m.content.as_str()));
    let direct_query_result: Option<(String, String)> = match query {
        Some(query) => direct_query(store, query)?,
        None => None,
    };
    if let Some((label, data)) = &direct_query_result {
        context.push(ContextMessage::new(
            "system",
            format!(
                "（本机数据直查：系统已直接查好「{label}」的真实数据，请直接引用回答，无需再调用工具）\n{data}"
            ),
        ).with_segment(SegmentPurpose::SelectedEvidence, SegmentOrigin::ToolExecution, None, OmissionPolicy::Required));
    }

    let page_task = super::page_tasks::task_for_run(store, conversation_id)?;
    if let (Some(task), Some(input)) = (&page_task, &last_user) {
        // Legacy callers have no display-time task, but even their tools must
        // retain the material version captured at this generation boundary.
        store
            .captured_page_task
            .replace(Some((input.id.clone(), task.clone())));
    }

    // 速度优化（2026-10）：知识不再每轮预载（Candidates 曾固定占 3.2k
    // tokens，对闲聊纯浪费），按需检索的引导写进 search_knowledge_base
    // 的工具描述（消除单独 system 段的 token 开销）；引用规范仍常驻，
    // 保证检索结果可溯源。
    context.push(ContextMessage::new(
        "system",
        crate::knowledge::CITATION_INSTRUCTIONS,
    ));

    // The provider budgets the complete request, including dynamic context and tools.
    // Recover the original task from persisted history even if the memory window
    // only retained the short status question. The per-round bound still applies.
    if let Some(followup) = &followup {
        let has_pending = !store
            .pending_actions_for_conversation(conversation_id)?
            .is_empty();
        context.push(followup.context(has_pending));
    }

    super::time_context::refresh_current_time(&mut context, &super::time_context::SystemClock);

    super::generation::event(store, "provider_basis", serde_json::json!(providers.iter().map(|p| serde_json::json!({
        "id":p.id,"model":p.model,"context_window":p.context_window,
        "version":crate::storage::knowledge::content_hash(&format!("{}|{}|{}|{:?}|{:?}|{}",p.id,p.base_url,p.model,p.max_tokens,p.context_window,p.temperature))
    })).collect::<Vec<_>>()))?;
    let summary_configs: std::collections::HashMap<_, _> = providers
        .iter()
        .map(|p| {
            (
                p.id.clone(),
                (
                    p.model.clone(),
                    crate::storage::knowledge::content_hash(&format!(
                        "{}|{}|{}|{:?}|{:?}|{}",
                        p.id, p.base_url, p.model, p.max_tokens, p.context_window, p.temperature
                    )),
                ),
            )
        })
        .collect();
    // 3) 创建 AI provider
    let ai_providers: Vec<(Option<String>, String, Box<dyn AiProvider>)> =
        match config.provider_type {
            ProviderType::OpenAiCompatible => {
                let configs = providers;
                if configs.is_empty() {
                    anyhow::bail!("No active AI provider configuration");
                }
                configs
                    .into_iter()
                    .map(|ai_config| {
                        let id = ai_config.id.clone();
                        let provider_config = super::provider::OpenAiCompatibleConfig {
                            base_url: ai_config.base_url,
                            api_key: ai_config.api_key,
                            model: ai_config.model,
                            temperature: ai_config.temperature as f32,
                            max_tokens: ai_config.max_tokens.map(|v| v as u32),
                            context_window: ai_config.context_window.map(|v| v as u32),
                        };
                        Ok((
                            Some(id),
                            ai_config.name,
                            Box::new(OpenAiCompatibleProvider::new(provider_config)?)
                                as Box<dyn AiProvider>,
                        ))
                    })
                    .collect::<Result<Vec<_>>>()?
            }
            ProviderType::Ollama => {
                // For Ollama, use environment variables or defaults
                let base_url = std::env::var("OLLAMA_BASE_URL")
                    .unwrap_or_else(|_| "http://localhost:11434".to_string());
                let model = std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama2".to_string());

                let provider_config = super::provider::OllamaConfig {
                    context_window: None,
                    max_tokens: None,
                    base_url,
                    model,
                    temperature: 0.7,
                };
                vec![(
                    None,
                    "Ollama".to_string(),
                    Box::new(OllamaProvider::new(provider_config)?),
                )]
            }
        };

    // 4) Agent 循环（可调工具）
    let mut errors = Vec::new();
    let mut outcome = None;
    for (provider_id, provider_name, provider) in ai_providers {
        let mut attempt_context = context.clone();
        if let Some(budget) = selection_budget {
            let fallback_model = std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama2".into());
            let fallback_basis = crate::storage::knowledge::content_hash(&format!(
                "ollama|{}|{}",
                fallback_model,
                std::env::var("OLLAMA_BASE_URL")
                    .unwrap_or_else(|_| "http://localhost:11434".into())
            ));
            let (model, basis) = provider_id
                .as_ref()
                .and_then(|id| summary_configs.get(id))
                .map(|(model, basis)| (model.as_str(), basis.as_str()))
                .unwrap_or((&fallback_model, &fallback_basis));
            super::summary::apply_summary(
                store,
                conversation_id,
                &*provider,
                &provider_name,
                basis,
                model,
                budget,
                &mut attempt_context,
            )?;
            super::memory::compress_context_for_model(&mut attempt_context, budget, model);
        }
        if let Some(task) = &page_task {
            if task.kind == "full_revision" {
                return super::page_tasks::revise_full(
                    store,
                    conversation_id,
                    task,
                    query.unwrap_or(""),
                    &*provider,
                    &provider_name,
                    &attempt_context,
                );
            }
            attempt_context.push(super::page_tasks::prepare(
                store,
                conversation_id,
                task,
                query.unwrap_or(""),
                &*provider,
                &provider_name,
            )?);
        }
        match run_agent_loop(
            &*provider,
            &provider_name,
            &mut attempt_context,
            &registry,
            store,
            conversation_id,
            turn_start,
        ) {
            Ok(result) => {
                if let Some(id) = provider_id {
                    store.set_active_ai_provider_config(&id)?;
                }
                context = attempt_context;
                outcome = Some(result);
                break;
            }
            Err(error) => {
                super::generation::check(store)?;
                // Consumed supplements belong to this run, not to one provider
                // attempt. A failover must not lose or consume them a second time.
                for supplement in attempt_context
                    .iter()
                    .filter(|message| message.segment.purpose == SegmentPurpose::TaskSupplement)
                {
                    if !context.iter().any(|message| {
                        message.segment.purpose == SegmentPurpose::TaskSupplement
                            && message.segment.identity_hash == supplement.segment.identity_hash
                    }) {
                        context.push(supplement.clone());
                    }
                }
                if error
                    .downcast_ref::<super::budget::RequestBudgetError>()
                    .is_some()
                {
                    return Err(error);
                }
                errors.push(format!("[{provider_name}] {error}"));
                debug_eprintln!("[agent] provider「{provider_name}」失败：{error:#}");
                // 全部 provider 可能临时不可用（网关抖动/限流）：轮换前加
                // jittered backoff，避免每次会话请求都同一瞬间锤向这批端点
                // （P2 retry backoff；Dart 侧 5s timer 只控制 tick，拦不住
                // 同 tick 内的连环 failover）。
                super::generation::pause(
                    store,
                    std::time::Duration::from_millis(jitter_backoff_ms(errors.len())),
                )?;
            }
        }
    }
    let outcome = outcome.context(format!("所有 AI provider 均失败：{}", errors.join(" | ")))?;
    if let Some(task) = &page_task {
        task.validate(store)?;
    }
    let transaction = store.begin_write_transaction()?;
    super::generation::check(store)?;
    crate::knowledge::workflows::record_generation(
        store,
        conversation_id,
        &outcome.content,
        outcome.model.as_deref(),
    )?;
    let raw = outcome.content;
    debug_eprintln!(
        "[agent] finished rounds={} empty_retries={}",
        outcome.rounds_used,
        outcome.empty_retries
    );

    // 5) 解析规则提议：若 AI 在末尾提交了一条规则，剥离标记转为友好提示展示，
    //    并把规则文本暂存为「待确认」，等待用户下一条消息确认后入库生效
    let (mut content, proposed_rule) = parse_rule_proposal(&raw);
    if let Some(rule) = proposed_rule {
        let id = store.add_rule(&rule, RuleStatus::Pending, Some(conversation_id))?;
        super::generation::event(
            store,
            "proposal_ready",
            serde_json::json!({"kind":"rule","proposal_id":id}),
        )?;
    }

    // Report the task and real results; transport diagnostics belong in logs.
    if content.trim().is_empty() {
        content = match &direct_query_result {
            Some((label, data)) => {
                format!("查到的「{label}」数据如下：\n{data}")
            }
            None => progress_fallback(store, conversation_id, &context, &[])?,
        };
    }

    let knowledge_candidates = crate::knowledge::candidates_in_context(&context);
    let current = crate::knowledge::current_candidates(store, &knowledge_candidates)?;
    let (validated_content, cited) =
        crate::knowledge::validate_answer_citations(&content, &current);
    content = validated_content;
    let owner = format!(
        "{conversation_id}:{}",
        crate::storage::knowledge::content_hash(&content)
    );
    crate::knowledge::record_usage(store, "conversation", &owner, &knowledge_candidates, &cited)?;

    // 6) 记录 token 用量：优先 provider 返回的 usage（累计），缺失则本地估算
    let prompt_est: u64 = context
        .iter()
        .map(|m| estimate_tokens(&m.content) as u64)
        .sum();
    let completion_est = estimate_tokens(&content) as u64;
    let prompt = if outcome.prompt_tokens > 0 {
        outcome.prompt_tokens
    } else {
        prompt_est
    };
    let completion = if outcome.completion_tokens > 0 {
        outcome.completion_tokens
    } else {
        completion_est
    };
    store.record_token_usage(
        Some(conversation_id),
        prompt as i64,
        completion as i64,
        (prompt + completion) as i64,
        outcome.model.as_deref(),
    )?;

    super::generation::check(store)?;
    if let Some(transaction) = transaction {
        transaction.commit()?;
    }
    // The runtime commits the verified reply and terminal state together.
    Ok(content)
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

/// 工具通信协议：原生 tool-calling 或文本信封
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolProtocol {
    Native,
    Text,
}

/// A text fallback must also remove native history fields. Merely omitting the
/// tools definition can still trigger the same gateway conversion/validation.
/// Preserve completed calls/results as text; never dispatch historical calls.
fn text_protocol_history(context: &mut [ContextMessage]) {
    for message in context {
        if let Some(calls) = message.tool_calls.take() {
            for call in calls {
                let envelope = serde_json::json!({"name": call.name, "arguments": call.arguments});
                message
                    .content
                    .push_str(&format!("\n{TOOL_CALL_MARKER}{envelope}"));
            }
            message.reasoning_content = None;
        }
        if message.role == "tool" {
            message.role = "system".into();
            message.content = format!("（已执行工具的历史结果）\n{}", message.content);
        }
        message.tool_call_id = None;
    }
}

/// 本机数据直查：对高频统计/清单类问题直接在本地查库并注入上下文，
/// 不依赖模型的 tool-calling 能力（数据本就存在本地数据库）。
/// 命中返回（查询主题, 已查好的数据文本）。
fn direct_query(store: &Store, user_message: &str) -> Result<Option<(String, String)>> {
    let msg = user_message.trim().to_lowercase();

    // token 用量
    let wants_token = msg.contains("token")
        || msg.contains("用量")
        || (msg.contains("统计") && (msg.contains("token") || msg.contains("用量")));
    if wants_token {
        let daily = store.daily_token_usage(7)?;
        let mut out = String::from("每日 token 用量：\n");
        if daily.is_empty() {
            out.push_str("（最近 7 天没有 AI 调用记录）");
        } else {
            for d in daily {
                out.push_str(&format!(
                    "- {}：{} tokens（{} 次调用）\n",
                    d.date, d.total_tokens, d.call_count
                ));
            }
        }
        return Ok(Some(("token 用量".to_string(), out)));
    }

    // 规则清单
    let wants_rules = msg.contains("规则库")
        || msg.contains("有哪些规则")
        || msg.contains("什么规则")
        || msg.contains("我的规则")
        || msg.contains("规则清单")
        || msg.contains("看看规则");
    if wants_rules {
        let rules = store.list_active_rules()?;
        let mut out = String::from("已生效的个人规则：\n");
        if rules.is_empty() {
            out.push_str("（规则库为空，还没有沉淀过规则）");
        } else {
            for r in rules {
                out.push_str(&format!("- {}\n", r.content));
            }
        }
        return Ok(Some(("个人规则库".to_string(), out)));
    }

    // 人生目标/主要任务是本地知识库中的稳定事实，不能依赖模型自行决定
    // 是否调用搜索工具，更不能受当前对话记忆窗口限制。
    let wants_goals = [
        "人生目标",
        "主要目标",
        "当前目标",
        "主要任务",
        "长期目标",
        "我想做什么",
        "搞钱",
    ]
    .iter()
    .any(|keyword| msg.contains(keyword));
    if wants_goals {
        let hits = store.search_knowledge_base("目标", 8)?;
        let mut out = String::from("知识库中的目标相关页面：\n");
        if hits.is_empty() {
            out.push_str("（暂未找到目标页面）");
        } else {
            for hit in hits {
                out.push_str(&format!("- 《{}》：{}\n", hit.title, hit.snippet));
            }
        }
        return Ok(Some(("当前目标".to_string(), out)));
    }

    Ok(None)
}

/// Agent 循环结果
struct AgentOutcome {
    content: String,
    prompt_tokens: u64,
    completion_tokens: u64,
    model: Option<String>,
    /// 诊断：本次循环实际消耗的轮数（1..=MAX_TOOL_ROUNDS），供空回复兜底文案说明重试过几次。
    rounds_used: usize,
    /// 诊断：因上游返回空 content 而额外重发的次数。
    empty_retries: usize,
}

/// Agent 循环：最多 MAX_TOOL_ROUNDS 轮。
/// - 首轮尝试原生 tool-calling（带 tools 清单）；失败则回落文本协议重试一次。
/// - 原生返回 tool_calls → 回传 assistant(tool_calls) + 追加 tool 结果，进入下一轮。
/// - 纯文本但含可解析调用 → 执行并进入下一轮。
/// - 疑似想调但格式未知 → 回炉重发（最多 MAX_PROTOCOL_REPAIRS 次），绝不静默吞掉。
/// - 无工具调用 → 写声明核验（标题→查库→本轮时间窗）→ 收敛，返回最终文本。
/// Provider failover never removes an already committed event. Fixed input
/// identities and durable results make retries idempotent instead of destructive.
fn run_agent_loop(
    provider: &dyn AiProvider,
    provider_name: &str,
    context: &mut Vec<ContextMessage>,
    registry: &ToolRegistry,
    store: &Store,
    conversation_id: &str,
    turn_start: chrono::DateTime<chrono::Utc>,
) -> Result<AgentOutcome> {
    run_agent_loop_inner(
        provider,
        provider_name,
        context,
        registry,
        store,
        conversation_id,
        turn_start,
    )
}

// Native reasoning endpoints require the original opaque reasoning alongside
// assistant messages when a repair continues the same turn. It is never shown
// as answer text; text_protocol_history removes it on an explicit fallback.
fn assistant_continuation(reply: AiReply) -> ContextMessage {
    let mut message = ContextMessage::new("assistant", reply.content);
    message.reasoning_content = reply.reasoning_content;
    message
}

fn run_agent_loop_inner(
    provider: &dyn AiProvider,
    provider_name: &str,
    context: &mut Vec<ContextMessage>,
    registry: &ToolRegistry,
    store: &Store,
    conversation_id: &str,
    turn_start: chrono::DateTime<chrono::Utc>,
) -> Result<AgentOutcome> {
    let conversation = store.get_conversation(conversation_id)?;
    let is_knowledge_mentor = conversation.as_ref().is_some_and(|conversation| {
        conversation.assistant_mode == "knowledge_mentor" || conversation.wiki_page_slug.is_some()
    });
    let input_already_recorded = store
        .latest_event_id_for_conversation(conversation_id)?
        .is_some();
    let followup = FollowUp::load(store, conversation_id)?;
    let has_pending = !store
        .pending_actions_for_conversation(conversation_id)?
        .is_empty();
    let status_only = followup
        .as_ref()
        .is_some_and(|f| !f.unresolved || has_pending);
    let verification_start = followup
        .as_ref()
        .and_then(|f| f.request_started_at)
        .unwrap_or(turn_start);
    let allow_record_event = !is_knowledge_mentor && !input_already_recorded && followup.is_none();
    let mut protocol: Option<ToolProtocol> = None;
    let mut content = String::new();
    let mut prompt_tokens: u64 = 0;
    let mut completion_tokens: u64 = 0;
    let mut model: Option<String> = None;
    let mut results: Vec<crate::ai::tool::ToolResultMsg> = Vec::new();
    let mut must_finalize = false;
    // 协议修复预算：疑似调用但未知格式时的回炉次数（与轮数上限独立，双保险防死循环）
    let mut repairs_used = 0usize;
    // 2026-10-08：写声明核验不再要求「用户先说写意图」前置，
    // writes_requested 变量与 user_requested_write 函数随之删除。
    let mut claim_repaired = false;
    // 收敛前接地重发的一次性闸门（防 finalizing 空转，见收敛段注释）。
    let mut grounding_retried = false;
    // 第二次空头预告直接进入最终答复阶段；不能因修复预算用完而放行预告。
    let mut promise_repaired = false;
    // 空回复重发：上游返回空 content 时消耗一轮预算重试，而不是把空串当答案收敛。
    let mut empty_retries_used = 0usize;
    let mut rounds_used = 0usize;
    let tool_names = registry.names();

    for _round in 0..MAX_TOOL_ROUNDS {
        super::generation::check(store)?;
        if context.iter().any(|m| m.sampled_time.is_some()) {
            super::time_context::refresh_current_time(context, &super::time_context::SystemClock);
        }
        rounds_used = _round + 1;
        let finalizing = must_finalize || _round == MAX_TOOL_ROUNDS - 1;
        if finalizing {
            context.push(ContextMessage::new("system", FINAL_ANSWER_NUDGE));
            // 接地注入（治本）：把本轮行为事实（工具成败 + 库内新增）摆进上下文，
            // 模型看着清单复述，无需靠词表猜它是否撒谎。
            context.push(ContextMessage::new(
                "system",
                turn_grounding(
                    store,
                    registry,
                    conversation_id,
                    &results,
                    verification_start,
                )?,
            ));
        }
        if protocol == Some(ToolProtocol::Text) {
            text_protocol_history(context);
        }
        // The provider checks the full serialized request, including tools, below.
        let wants_native = match &protocol {
            Some(ToolProtocol::Native) => true,
            Some(ToolProtocol::Text) => false,
            None => true,
        };
        let tools = if wants_native && !finalizing {
            let mut specs = registry.provider_specs_for(allow_record_event);
            if status_only {
                specs.retain(|spec| {
                    registry
                        .get(&spec.name)
                        .is_some_and(|t| t.policy() == ToolPolicy::Read)
                });
            }
            Some(specs)
        } else {
            None
        };

        let mut request_context = context.clone();
        if protocol == Some(ToolProtocol::Text) && !finalizing {
            let specs: Vec<_> = registry
                .provider_specs_for(allow_record_event)
                .into_iter()
                .filter(|spec| {
                    !status_only
                        || registry
                            .get(&spec.name)
                            .is_some_and(|t| t.policy() == ToolPolicy::Read)
                })
                .collect();
            request_context.push(ContextMessage::new(
                "system",
                ToolRegistry::prompt_for_specs(&specs),
            ));
        }
        // Only a completed tool exchange creates a steering boundary. Inputs
        // arriving during a plain answer remain queued, never silently appended.
        if _round > 0 && !results.is_empty() {
            if let Some(supplement) = super::supplements::inject_next(
                store,
                conversation_id,
                provider,
                &request_context,
                tools.as_deref(),
            )? {
                request_context.push(supplement.clone());
                context.push(supplement);
            }
        }
        let reply = match super::generation::request(
            store,
            provider,
            provider_name,
            request_context,
            tools.as_deref(),
        ) {
            Ok(r) => r,
            Err(e) => {
                super::generation::check(store)?;
                // Local budget failures are not evidence of protocol incompatibility.
                if e.downcast_ref::<super::budget::RequestBudgetError>()
                    .is_some()
                    || e.downcast_ref::<super::provider::ObservedRequestFailure>()
                        .is_some()
                {
                    return Err(e);
                }
                if finalizing {
                    debug_eprintln!(
                        "[agent] provider「{provider_name}」final response failed, report known progress: {e}"
                    );
                    break;
                }
                // 网关/上游暂时失败时不要把同一上下文改成纯文本协议；
                // 直接交给外层 provider 轮换，避免重复请求和协议状态污染。
                if is_transient_provider_error(&e) {
                    debug_eprintln!(
                        "[agent] provider「{provider_name}」上游暂时失败，交给下一个 provider：{e:#}"
                    );
                    return Err(e);
                }
                // 原生协议失败（首次尝试 or 已锁定原生）→ 回落到纯文本协议重试一次：
                // 首轮可能是模型/网关不支持 tools 字段；后续轮可能是原生 tool_calls
                // 回传校验失败（如 arguments 字节不一致）。历史里已有上下文，文本模式仍能组织最终回答。
                if !matches!(protocol, Some(ToolProtocol::Text)) {
                    protocol = Some(ToolProtocol::Text);
                    context.push(ContextMessage::new("system", TEXT_PROTOCOL_NUDGE));
                    debug_eprintln!(
                        "[agent] provider「{provider_name}」round={_round} 请求失败({e:#})，下一轮使用文本工具协议"
                    );
                    // 回退前小退避：原生协议刚软失败，紧随的文本重试可能命中
                    // 同一瞬限流；仅首次硬失败后触发，成功路径零开销（P2 backoff）。
                    super::generation::pause(
                        store,
                        std::time::Duration::from_millis(PROTOCOL_FALLBACK_SLEEP_MS),
                    )?;
                    continue;
                } else {
                    debug_eprintln!("[agent] provider「{provider_name}」纯文本协议请求也失败：{e:#}");
                    return Err(e);
                }
            }
        };
        // Account and diagnose every HTTP response before choosing a retry.
        if let Some(u) = &reply.usage {
            prompt_tokens += u.prompt_tokens;
            completion_tokens += u.completion_tokens;
        }
        if reply.model.is_some() {
            model = reply.model.clone();
        }
        debug_eprintln!(
            "[agent] provider「{provider_name}」round={_round} protocol={:?} offered_tools={} finalizing={finalizing} content_len={} reasoning_len={} tool_calls={} finish_reason={:?} model={:?} prompt_tokens={} completion_tokens={}",
            protocol,
            tools.as_ref().map_or(0, Vec::len),
            reply.content.chars().count(),
            reply
                .reasoning_content
                .as_ref()
                .map_or(0, |r| r.chars().count()),
            reply.tool_calls.len(),
            reply.finish_reason,
            reply.model,
            reply.usage.as_ref().map_or(0, |u| u.prompt_tokens),
            reply.usage.as_ref().map_or(0, |u| u.completion_tokens)
        );
        if missing_tool_call_response(&reply) {
            debug_eprintln!(
                "[agent] round={_round} 响应声明调用工具但缺少调用内容，本次未执行工具"
            );
            if finalizing {
                break;
            }
            if protocol != Some(ToolProtocol::Text) {
                protocol = Some(ToolProtocol::Text);
                context.push(ContextMessage::new("system", TEXT_PROTOCOL_NUDGE));
                debug_eprintln!("[agent] 下一轮切换文本工具协议，本次循环不再启用原生 tools");
            } else {
                must_finalize = true;
                debug_eprintln!("[agent] 文本工具协议仍返回不完整响应，下一轮只报告实际结果");
            }
            continue;
        }

        // 原生 tool-calls：回传 assistant(tool_calls) + 工具结果
        if !reply.tool_calls.is_empty() {
            // Even a model ignoring the finalization instruction cannot start
            // another action, through either protocol.
            if finalizing {
                break;
            }
            if protocol != Some(ToolProtocol::Text) {
                protocol = Some(ToolProtocol::Native);
            }
            context.push(ContextMessage::assistant_with_tool_calls(
                reply.content.clone(),
                reply.tool_calls.clone(),
                reply.reasoning_content.clone(),
            ));
            for call in &reply.tool_calls {
                if let Some(body) = call.arguments.get("content_md").and_then(|v| v.as_str()) {
                    crate::knowledge::workflows::record_generation(
                        store,
                        conversation_id,
                        body,
                        reply.model.as_deref(),
                    )?;
                }
                let result = dispatch_for_turn(
                    call,
                    registry,
                    store,
                    conversation_id,
                    status_only,
                    allow_record_event,
                    &crate::knowledge::candidates_in_context(context),
                )?;
                debug_eprintln!(
                    "[agent] dispatch tool={} -> {}",
                    call.name,
                    &result.content.chars().take(80).collect::<String>()
                );
                context.push(ContextMessage::tool_result(
                    call.id.clone(),
                    result.content.clone(),
                ));
                results.push(result);
            }
            continue;
        }

        // 文本信封：assistant 原文 + 工具结果（走 system 角色，避免原生协议对 tool 角色的约束）
        let (clean, calls) = parse_tool_call_envelope(&reply.content);
        if !calls.is_empty() {
            if finalizing {
                break;
            }
            protocol = Some(ToolProtocol::Text);
            let reply_model = reply.model.clone();
            context.push(assistant_continuation(reply));
            for call in &calls {
                if let Some(body) = call.arguments.get("content_md").and_then(|v| v.as_str()) {
                    crate::knowledge::workflows::record_generation(
                        store,
                        conversation_id,
                        body,
                        reply_model.as_deref(),
                    )?;
                }
                let result = dispatch_for_turn(
                    &call,
                    registry,
                    store,
                    conversation_id,
                    status_only,
                    allow_record_event,
                    &crate::knowledge::candidates_in_context(context),
                )?;
                debug_eprintln!(
                    "[agent] text-protocol tool={} -> {}",
                    call.name,
                    &result.content.chars().take(80).collect::<String>()
                );
                context.push(ContextMessage::new(
                    "system",
                    format!("工具「{}」执行结果：{}", call.name, result.content),
                ));
                results.push(result);
            }
            continue;
        }

        // 疑似想调工具但严格解析器没认出来 → 回炉重发，而不是静默吞掉。
        // 原则1：该调的必须调。误报最多浪费一轮（有次数上限），漏报则工具永远不执行。
        if tool_call_attempt_detected(&reply.content, &tool_names) {
            if finalizing || repairs_used >= MAX_PROTOCOL_REPAIRS {
                break;
            }
            repairs_used += 1;
            debug_eprintln!("[agent] 疑似工具调用但无法解析，回炉重发 (repair {repairs_used})");
            context.push(assistant_continuation(reply));
            context.push(ContextMessage::new("system", PROTOCOL_REPAIR_NUDGE));
            continue;
        }

        // 写声明核验（辅助层）：回复命中声明词 → 查库验真。治本靠 finalizing 的
        // 接地注入（turn_grounding）；这层只兜「一轮收敛且措辞命中词表」的裸谎。
        // 教学/引用语境（无标题 + 零工具调用 + 无待办）不走回炉，减少误报。
        if write_claim_detected(&clean) {
            let titles = extract_claimed_titles(&clean);
            match verify_claimed_writes(store, conversation_id, &titles, verification_start)? {
                ClaimVerdict::Clean => {}
                verdict => {
                    if finalizing {
                        break;
                    }
                    must_finalize = claim_repaired;
                    claim_repaired = true;
                    debug_eprintln!("[agent] 写声明与库内事实不符，回炉纠正 ({verdict:?})");
                    context.push(assistant_continuation(reply));
                    context.push(ContextMessage::new(
                        "system",
                        write_claim_repair_nudge(store, conversation_id, &verdict)?,
                    ));
                    continue;
                }
            }
        }

        // 承诺回炉：只预告「接下来要做」就收敛 = 用户等一场空。逼它现在动手或直接给结果。
        // 与写声明核验同源不同向：那边抓「谎报已做完」，这边抓「预告还没做」。
        if future_promise_detected(&clean) {
            if finalizing {
                break;
            }
            must_finalize = promise_repaired;
            promise_repaired = true;
            debug_eprintln!("[agent] 只预告未动手，回炉逼它执行 (repair)");
            context.push(assistant_continuation(reply));
            context.push(ContextMessage::new("system", PROMISE_REPAIR_NUDGE));
            continue;
        }

        // 空回复重发：上游给了空 content（200 但无正文，常见于推理模型烧光输出预算）。
        // 原来这里直接 break，空串被当成答案收敛，用户只看到一句无信息兜底。
        if clean.trim().is_empty() || unhelpful_failure_reply(&clean) {
            if finalizing {
                break;
            }
            must_finalize = empty_retries_used >= MAX_EMPTY_RETRIES;
            if !must_finalize {
                empty_retries_used += 1;
            }
            debug_eprintln!(
                "[agent] empty reply; retry={empty_retries_used} finalize_next={must_finalize}"
            );
            context.push(ContextMessage::new("system", EMPTY_REPLY_NUDGE));
            continue;
        }

        // 收敛前最后一道接地（治本版）：判定依据从「AI 说了什么」换成
        // 「用户要什么 + AI 做了什么」——用户明确要求写 + 本轮零工具调用 = 强制重发，
        // 不看 AI 回复措辞。2026-10-08 kimi 模式：用户说「保存为topic」，AI 贴出正文
        // 说「现在重新提交」但 tool_calls=0——它把「展示内容」当成了「执行保存」，
        // 任何词表都拦不住这种认知错误，只能从行为（零工具）+ 意图（用户要写）判定。
        // finalizing 轮不重发：轮首已注入过接地清单，再 continue 会因条件反复命中空转
        // 到轮数耗尽。grounding_retried 一次性闸门：重发后仍不动手就放行，事实留在上下文。
        let user_wants_write = context
            .iter()
            .rev()
            .find(|m| m.role == "user")
            .is_some_and(|m| user_text_wants_write(&m.content));
        if !finalizing && !grounding_retried && results.is_empty() && user_wants_write {
            grounding_retried = true;
            debug_eprintln!("[agent] 用户要求写但本轮零工具调用，强制接地重发");
            context.push(assistant_continuation(reply));
            context.push(ContextMessage::new(
                "system",
                format!(
                    "{}\n用户明确要求你执行写操作（保存/提交/整理），但你本轮没有调用任何工具。\
                     把内容贴给用户看不等于执行——必须调用对应工具（如 save_knowledge_draft），\
                     或明确告诉用户你无法执行及原因。",
                    turn_grounding(
                        store,
                        registry,
                        conversation_id,
                        &results,
                        verification_start
                    )?
                ),
            ));
            must_finalize = true;
            continue;
        }

        // 收敛
        content = clean;
        break;
    }

    if content.trim().is_empty() {
        content = progress_fallback(store, conversation_id, context, &results)?;
    }

    Ok(AgentOutcome {
        content,
        prompt_tokens,
        completion_tokens,
        model,
        rounds_used,
        empty_retries: empty_retries_used,
    })
}

fn dispatch_for_turn(
    call: &ToolCall,
    registry: &ToolRegistry,
    store: &Store,
    conversation_id: &str,
    status_only: bool,
    allow_record_event: bool,
    knowledge_candidates: &[crate::knowledge::KnowledgeCitation],
) -> Result<ToolResultMsg> {
    super::generation::check(store)?;
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

fn is_transient_provider_error(error: &anyhow::Error) -> bool {
    let text = error.to_string().to_ascii_lowercase();
    [
        " 429",
        " 500",
        " 502",
        " 503",
        " 504",
        "failed dependency",
        "bad gateway",
        "timed out",
        "timeout",
    ]
    .iter()
    .any(|marker| text.contains(marker))
}

/// 解析文本信封中的工具调用：返回（剥离信封后的文本, 解析出的调用列表）。
/// 支持一行内多个标记，标记所在行会从文本中移除。
/// 再加一层兜底：任何没被识别的协议残留都会被剥掉，绝不直接见用户。
fn parse_tool_call_envelope(content: &str) -> (String, Vec<ToolCall>) {
    if !content.contains(TOOL_CALL_MARKER) {
        let (cleaned, mut calls) = parse_xml_tool_calls(content);
        calls.extend(parse_dsml_tool_calls(&cleaned));
        return (
            strip_tool_protocol_residue(&strip_dsml_artifacts(&cleaned)),
            calls,
        );
    }
    let mut cur = content.to_string();
    let mut calls = Vec::new();
    loop {
        match extract_one_tool_call(&cur) {
            Some((cleaned, call)) => {
                cur = cleaned;
                calls.push(call);
            }
            None => break,
        }
    }
    if cur.contains("<tool_call") {
        let (cleaned, mut xml_calls) = parse_xml_tool_calls(&cur);
        cur = cleaned;
        calls.append(&mut xml_calls);
    }
    (
        strip_tool_protocol_residue(&strip_dsml_artifacts(&cur)),
        calls,
    )
}

/// 解析 XML 方言的工具调用：
/// `<tool_call>工具名<arg_key>参数名</arg_key><arg_value>参数值</arg_value>…</tool_call>`。
/// 部分模型无视文本协议，按自带习惯吐这种格式；认下来并执行，
/// 而不是让它原样漏给用户。返回（剥离调用块后的文本, 调用列表）。
/// 结构残缺（无名、无参数、无闭合）的块不解析，留给兜底清理。
fn parse_xml_tool_calls(content: &str) -> (String, Vec<ToolCall>) {
    const OPEN: &str = "<tool_call>";
    const CLOSE: &str = "</tool_call>";
    const KEY_OPEN: &str = "<arg_key>";
    const KEY_CLOSE: &str = "</arg_key>";
    const VAL_OPEN: &str = "<arg_value>";
    const VAL_CLOSE: &str = "</arg_value>";
    let mut cur = content.to_string();
    let mut calls = Vec::new();
    loop {
        let Some(start) = cur.find(OPEN) else { break };
        let after_open = start + OPEN.len();
        // 工具名：紧跟其后的文本，直到下一个 `<`
        let name_end_rel = cur[after_open..].find('<');
        let Some(name_end_rel) = name_end_rel else {
            break;
        };
        let name = cur[after_open..after_open + name_end_rel].trim();
        if name.is_empty() {
            break;
        }
        // 参数对：连续的 <arg_key>K</arg_key><arg_value>V</arg_value>
        let mut args = serde_json::Map::new();
        let mut pos = after_open + name_end_rel;
        loop {
            let rest = &cur[pos..];
            let Some(ko) = rest.find(KEY_OPEN) else { break };
            // key 与工具名/上一对 value 之间只允许空白
            if !rest[..ko].trim().is_empty() {
                break;
            }
            let k_start = pos + ko + KEY_OPEN.len();
            let Some(k_end_rel) = cur[k_start..].find(KEY_CLOSE) else {
                break;
            };
            let key = cur[k_start..k_start + k_end_rel].trim().to_string();
            let v_open = k_start + k_end_rel + KEY_CLOSE.len();
            if !cur[v_open..].trim_start().starts_with(VAL_OPEN) {
                break;
            }
            let v_start = v_open + cur[v_open..].find(VAL_OPEN).unwrap_or(0) + VAL_OPEN.len();
            let Some(v_end_rel) = cur[v_start..].find(VAL_CLOSE) else {
                break;
            };
            let value = cur[v_start..v_start + v_end_rel].trim().to_string();
            if key.is_empty() {
                break;
            }
            args.insert(key, Value::String(value));
            pos = v_start + v_end_rel + VAL_CLOSE.len();
        }
        if args.is_empty() {
            break;
        }
        // 闭合标签：参数之后只允许空白
        let tail = &cur[pos..];
        let Some(close_rel) = tail.find(CLOSE) else {
            break;
        };
        if !tail[..close_rel].trim().is_empty() {
            break;
        }
        let end = pos + close_rel + CLOSE.len();
        calls.push(ToolCall::new(name, Value::Object(args)));
        cur.replace_range(start..end, "");
    }
    (cur.trim().to_string(), calls)
}

/// 宽口径“调用意图”检测：模型疑似想调工具时返回 true。
/// 故意比严格解析器宽得多——两者解耦：解析器负责“能执行”，这里负责“别漏调”。
/// 漏报 = 该调的工具永远不执行（不可接受）；误报 = 最多浪费一次回炉（有次数上限）。
/// 判定信号分两档：
/// 1. 任一已知协议的结构标记（与具体方言无关，见一个即命中）；
/// 2. 出现已知工具名 + 调用形态（arguments 字样，或 JSON 名值对）。
fn tool_call_attempt_detected(content: &str, tool_names: &[&str]) -> bool {
    const STRUCTURAL: &[&str] = &[
        "<tool_call",
        "</tool_call>",
        "<arg_key",
        "<arg_value",
        TOOL_CALL_MARKER,
        "<|invoke",
        "invoke name=",
        "parameter name=",
        "\"tool_calls\"",
        "\"function_call\"",
        "<function",
        "</function>",
        "function name=",
    ];
    if STRUCTURAL.iter().any(|m| content.contains(m)) {
        return true;
    }
    if !tool_names.iter().any(|n| content.contains(*n)) {
        return false;
    }
    let lower = content.to_lowercase();
    lower.contains("arguments") || (content.contains("\"name\"") && content.contains('{'))
}

/// 写声明核验结论（以库内事实为准）。
#[derive(Debug)]
enum ClaimVerdict {
    /// 声明与事实一致（本轮确有写入），放行。
    Clean,
    /// 声称的实体只在待确认草稿里（草拟≠保存），需纠正并求确认。
    DraftOnly(Vec<String>),
    /// 声称的实体查无此页、是旧页面、或根本没指明——与事实不符，需纠正。
    Unproven(Vec<String>),
}

/// 从回复中提取被声称的实体标题（「」/『』/《》/""/'' 配对引号）。
/// 本应用的写声明几乎总是引用标题（法医语料全部命中），比动词关键词稳定得多。
fn extract_claimed_titles(content: &str) -> Vec<String> {
    const PAIRS: &[(char, char)] = &[
        ('「', '」'),
        ('『', '』'),
        ('《', '》'),
        ('"', '"'),
        ('\'', '\''),
    ];
    let mut titles = Vec::new();
    for &(open, close) in PAIRS {
        let mut rest = content;
        while let Some(start) = rest.find(open) {
            let after = &rest[start + open.len_utf8()..];
            if let Some(end) = after.find(close) {
                let title = after[..end].trim();
                // 过滤单字虚词（「好」/「不要」等多为正文引用，不是实体标题）
                if title.chars().count() >= 2 && !titles.iter().any(|t: &String| t == title) {
                    titles.push(title.to_string());
                }
                rest = &after[end + close.len_utf8()..];
            } else {
                break;
            }
        }
    }
    titles
}

/// RFC3339 时间戳是否落在 [turn_start, now] 内；解析失败则按字符串兜底比较。
fn touched_this_turn(ts: &str, turn_start: &chrono::DateTime<chrono::Utc>) -> bool {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
        dt.with_timezone(&chrono::Utc) >= *turn_start
    } else {
        ts >= turn_start.to_rfc3339().as_str()
    }
}

/// 核验写声明：标题 → 查库（页/待办）→ 卡本轮时间窗。
/// - 页或待办存在且创建/更新在本轮 → 写定；
/// - 只在待确认草稿参数里 → 草稿中；
/// - 查无此页、是旧页面 → 未证实；
/// - 未指明任何标题 → 本轮有任意新增页/待办即放行，否则未证实。
fn verify_claimed_writes(
    store: &Store,
    conversation_id: &str,
    titles: &[String],
    turn_start: chrono::DateTime<chrono::Utc>,
) -> Result<ClaimVerdict> {
    let mut draft_only = Vec::new();
    let mut unproven = Vec::new();
    // 本轮待确认草稿的参数快照（标题键因动作而异，兜底用全文包含判断）
    let pendings = store.pending_actions_for_conversation(conversation_id)?;

    let check_title = |title: &str| -> Result<Option<bool>> {
        if let Some(page) = store.find_wiki_page_by_title(title)? {
            // 旧页面≠本轮写入：创建和更新都要卡时间窗（补充修订走 updated_at）。
            if touched_this_turn(&page.created_at, &turn_start)
                || touched_this_turn(&page.updated_at, &turn_start)
            {
                return Ok(Some(true));
            }
            return Ok(Some(false));
        }
        for todo in store.list_todos(None)? {
            if todo.title.trim() == title.trim()
                && (touched_this_turn(&todo.created_at, &turn_start)
                    || touched_this_turn(&todo.updated_at, &turn_start))
            {
                return Ok(Some(true));
            }
        }
        Ok(None)
    };
    let in_draft = |title: &str| -> bool {
        pendings.iter().any(|p| {
            serde_json::from_str::<serde_json::Value>(&p.args_json)
                .ok()
                .and_then(|v| {
                    v.get("title")
                        .or_else(|| v.get("project_name"))
                        .and_then(|t| t.as_str())
                        .map(|t| t.trim() == title.trim())
                        .or_else(|| {
                            // 兜底：标题出现在参数全文里（如 content_md 首行）
                            Some(p.args_json.contains(title))
                        })
                })
                .unwrap_or(false)
        })
    };

    if titles.is_empty() {
        // 裸声明（“存好了”但没点名）：本轮有任意新增/更新即放行，否则未证实。
        // NOTE：直接读库扫描，failover 重试也不丢；个人库规模下全量可接受。
        let mut any = false;
        for page in store.list_wiki_pages(None, None)? {
            if touched_this_turn(&page.created_at, &turn_start)
                || touched_this_turn(&page.updated_at, &turn_start)
            {
                any = true;
                break;
            }
        }
        if !any {
            for todo in store.list_todos(None)? {
                if touched_this_turn(&todo.created_at, &turn_start)
                    || touched_this_turn(&todo.updated_at, &turn_start)
                {
                    any = true;
                    break;
                }
            }
        }
        if any {
            return Ok(ClaimVerdict::Clean);
        }
        return Ok(ClaimVerdict::Unproven(vec!["（未指明页面）".to_string()]));
    }

    for title in titles {
        match check_title(title)? {
            Some(true) => {}
            Some(false) => unproven.push(format!("《{title}》是旧页面，本轮未写入")),
            None if in_draft(title) => draft_only.push(format!("《{title}》还在待确认")),
            None => unproven.push(format!("《{title}》库中无此页")),
        }
    }
    if !unproven.is_empty() {
        Ok(ClaimVerdict::Unproven(unproven))
    } else if !draft_only.is_empty() {
        Ok(ClaimVerdict::DraftOnly(draft_only))
    } else {
        Ok(ClaimVerdict::Clean)
    }
}

/// 写完成声明检测：回复声称“已保存/已创建”类动作完成。
/// 排除讲解/检索/否定语境（搜、查、没有、还没、未、失败、疑问），只抓肯定式完成声明。
fn write_claim_detected(content: &str) -> bool {
    const CLAIMS: &[&str] = &[
        "存好了",
        "保存好了",
        "已经保存",
        "已保存",
        "保存成功",
        "创建好了",
        "已创建",
        "新建好了",
        "建好了",
        "记好了",
        "已经记下",
        "归档好了",
        "已归档",
        "已生效",
        "已经在知识库",
        "已经在里面",
        // 2026-10-08 kimi 漏拦补充：模型用变体措辞绕过原词表。
        "已写入",
        "已存入",
        "存进了",
        "存到",
        "收录了",
        "已收录",
        "已入库",
        "放进知识库",
        "放进了知识库",
        "存进了知识库",
        "整理成了知识页",
        "整理成知识页",
        "已整理成",
        "已经整理成",
    ];
    // 守卫故意收窄到多字形态：单字（未/查/没）会误伤“未动/查看/没问题，存好了”等正常表述。
    // 漏拦的代价只是一次回炉纠正，误拦则会放过谎言，所以宁可少拦。
    // 2026-10-08：去掉「？」「?」守卫——模型只要在句尾加一句「需要我再补充吗？」
    // 就整体豁免核验，等于给谎言留了万能后门。否定/未遂语境仍由多字守卫覆盖。
    // 「抱歉/对不起」同理：承认「之前没保存」是诚实认错，但「抱歉，已保存」仍是谎言，
    // 不该豁免（kimi 实测用「抱歉，我刚才说已保存其实没保存」绕过核验后继续不执行）。
    const GUARDS: &[&str] = &[
        "搜",
        "没有",
        "还没",
        "失败",
        "是否",
        "如果",
        "假如",
        "抱歉",
        "对不起",
    ];
    CLAIMS.iter().any(|c| content.contains(c)) && !GUARDS.iter().any(|g| content.contains(g))
}

/// 未完成宣告检测：回复预告了「接下来我要做」，但本轮并没有动手。
///
/// 与 `write_claim_detected` 是两个方向——那边抓「谎报已经做完」，这边抓「预告还没做」。
/// 后者曾真实发生：用户贴入新素材要求合并保存，模型回「我准备一个更新版本…我将为你
/// 草拟更新后的版本，稍等片刻。」，零工具调用、零写入，用户干等。写声明词表一个都
/// 命中不了（没有「已保存」类完成态），于是直接收敛。
fn future_promise_detected(content: &str) -> bool {
    const PROMISES: &[&str] = &[
        "稍等片刻",
        "请稍等",
        "稍等一下",
        "稍等我",
        "请稍候",
        "稍候片刻",
        "马上就好",
        "我这就",
        "这就来",
        "接下来我",
        "我接下来",
        "我准备",
        "我来帮你",
        "让我先",
        "我去处理",
        "我去做",
        "我来处理",
        "我会处理",
        "我去查",
        "我先去",
        "一会给你",
        "一会儿给你",
        "稍后给你",
        "稍后回复",
        "一会回复",
        "回头给你",
        "完成后回复",
        "做好后发给你",
        "等我整理好",
    ];
    // 守卫：已经交付了内容并在求确认 —— 那是有实质答复的征求，不算空头预告。
    // 修复受总轮数限制；交付正文和征求确认不应仅因开场措辞被拦截。
    const GUARDS: &[&str] = &[
        "请确认",
        "确认一下",
        "是否需要",
        "要我现在",
        "草稿如下",
        "结果如下",
        "改进稿如下",
        "具体如下",
        "你看这样",
        "这样可以吗",
    ];
    // Examples and quoted/source code are data, not assistant commitments.
    let mut in_code = false;
    let prose = content
        .lines()
        .filter(|line| {
            let line = line.trim();
            if line.starts_with("```") {
                in_code = !in_code;
                return false;
            }
            !in_code && !line.starts_with('>')
        })
        .collect::<Vec<_>>()
        .join("\n");
    PROMISES.iter().any(|p| prose.contains(p)) && !GUARDS.iter().any(|g| prose.contains(g))
}

/// 用户文本是否表达写意图（存/建/记/归档类）。疑问句式（…吗/？）是在问状态，不是下指令。
fn user_text_wants_write(text: &str) -> bool {
    let t = text.trim();
    if t.ends_with('吗') || t.ends_with('？') || t.ends_with('?') {
        return false;
    }
    // 否定/搁置守卫：「可以不保存」「别保存」「先放着」「不用了」含写意图词但语义是拒绝。
    // 原测试 natural_replies_never_execute 的「好的，别保存」「可以不保存」靠这个排除。
    const NEGATIVE: &[&str] = &[
        "不保存",
        "别保存",
        "不用",
        "别存",
        "不要存",
        "先不",
        "先放着",
        "先别",
        "暂不",
        "取消",
    ];
    if NEGATIVE.iter().any(|n| t.contains(n)) {
        return false;
    }
    // 「提交」补于 2026-10-08：kimi 对「再提交古籍那页」「保存为topic」零工具响应。
    // 「整理」不列入——「完成了整理/整理房间/整理思路」是日常陈述非写指令，误伤面太大。
    const WANT: &[&str] = &[
        "存", "保存", "记住", "记下", "创建", "新建", "归档", "入库", "提交",
    ];
    WANT.iter().any(|w| t.contains(w))
}

/// 宽声明面：比 write_claim_detected 更松的「声称完成」信号，用于收敛前接地重发。
/// 不含守卫——这里不是判定撒谎，只是决定「要不要给模型看一眼事实清单」，误报代价
/// 仅是一轮重发。确认/否决短语（「好」「先放着」）天然不含这些词，不受影响。
fn broad_write_claim(content: &str) -> bool {
    const SIGNALS: &[&str] = &[
        "已保存",
        "存好了",
        "保存好了",
        "已创建",
        "建好了",
        "记好了",
        "已归档",
        "已入库",
        "已收录",
        "已写入",
        "已存入",
        "存进了",
        "已经在知识库",
        "完成了",
        "搞定了",
        "妥了",
        "办好了",
        "处理好了",
        "整理好了",
    ];
    SIGNALS.iter().any(|s| content.contains(s))
}

/// 接地注入（治本）：把本轮「行为事实」结构化摆进模型上下文。
///
/// 设计原则：凡是需要模型诚实报告自己行为的地方，把行为事实直接注入，
/// 而不是从它的输出文本反推（关键词匹配永远追不上措辞创造力）。
/// 事实 = 两层：工具执行结果（成败）+ 库内本轮新增（写定/待确认），后者卡时间窗，
/// 与 verify_claimed_writes 同源。模型看着清单复述，措辞空间被事实封死。
fn turn_grounding(
    store: &Store,
    registry: &ToolRegistry,
    conversation_id: &str,
    results: &[ToolResultMsg],
    turn_start: chrono::DateTime<chrono::Utc>,
) -> Result<String> {
    // 第一层：工具执行清单（含成败），写/读分开说。
    let mut lines = vec!["【本轮事实——回复必须与此一致】".to_string()];
    if results.is_empty() {
        lines.push("本轮未执行任何工具调用，没有任何写入发生。".to_string());
    } else {
        for r in results {
            let policy = registry
                .get(&r.call_name)
                .map(|t| t.policy())
                .unwrap_or(ToolPolicy::Read);
            let kind = match policy {
                ToolPolicy::Read => "查询",
                ToolPolicy::WriteDirect => "写入",
                ToolPolicy::WriteConfirm => "草拟（待确认）",
            };
            let outcome = if r.success { "成功" } else { "失败" };
            lines.push(format!("- {} {}：{}", r.call_name, kind, outcome));
        }
    }
    // 第二层：库内本轮新增（卡时间窗，与写声明核验同一事实源）。
    let mut drafts = Vec::new();
    for pa in store.pending_actions_for_conversation(conversation_id)? {
        if touched_this_turn(&pa.created_at, &turn_start) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&pa.args_json) {
                if let Some(t) = v.get("title").and_then(|x| x.as_str()) {
                    drafts.push(format!("「{t}」（{}）", pa.action));
                }
            }
        }
    }
    if !drafts.is_empty() {
        lines.push(format!("本轮登记的待确认草稿：{}。", drafts.join("、")));
    }
    lines.push(
        "如实说明：什么做成了、什么失败了及原因、哪些仍在等用户确认。\
         未执行的不得说成已完成；失败的要给出原因。"
            .to_string(),
    );
    Ok(lines.join("\n"))
}

/// 写声明回炉提示：把库内事实（缺页/旧页/待确认）摆出来，让模型无法再编。
fn write_claim_repair_nudge(
    store: &Store,
    conversation_id: &str,
    verdict: &ClaimVerdict,
) -> Result<String> {
    let detail = match verdict {
        ClaimVerdict::Clean => String::new(),
        ClaimVerdict::DraftOnly(items) | ClaimVerdict::Unproven(items) => items.join("；"),
    };
    // 草稿标题一并给出（slug 要到执行落定时才生成，此处不编造），模型求确认时能点名。
    let mut draft_slugs = Vec::new();
    for pa in store.pending_actions_for_conversation(conversation_id)? {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&pa.args_json) {
            if let Some(t) = v
                .get("title")
                .or_else(|| v.get("project_name"))
                .and_then(|t| t.as_str())
            {
                draft_slugs.push(format!("《{t}》(待确认，未入库)"));
            }
        }
    }
    let drafts = if draft_slugs.is_empty() {
        "当前没有待确认草稿".to_string()
    } else {
        format!("当前待确认：{}", draft_slugs.join("、"))
    };
    Ok(format!(
        "系统核实（以知识库为准，你的上一句与事实不符）：{detail}。{drafts}。\
         你有两个选择，必须二选一，不得再用文字声称已保存：\
         (1) 现在调用工具实际执行（如 save_knowledge_draft），拿到成功结果后才能说已保存；\
         (2) 明确告诉用户「我还没有保存」，并说明原因或需要用户提供什么。\
         请区分：待确认草稿仅存于对话，尚未入库；确认执行并返回真实结果才算已保存。\
         若已有草稿，告知用户可在“今天”栏打开待入库草稿列表查看并单独保存或删除，\
         不要重新草拟同名页，更不要声称草稿已在知识库。"
    ))
}

/// 兜底清理：解析器没认出来的工具协议残留，绝不直接见用户。
/// 先删整段 `<tool_call>…</tool_call>` 残块，再按行过滤其他协议行
///（`[工具调用]` / DSML invoke 等；协议要求调用独占一行，按行删安全）。
fn strip_tool_protocol_residue(content: &str) -> String {
    const CLOSE: &str = "</tool_call>";
    let mut cur = content.to_string();
    loop {
        let Some(start) = cur.find("<tool_call") else {
            break;
        };
        let Some(rel_end) = cur[start..].find(CLOSE) else {
            break;
        };
        cur.replace_range(start..start + rel_end + CLOSE.len(), "");
    }
    cur.lines()
        .filter(|line| {
            !(line.contains("<tool_call")
                || line.contains(CLOSE)
                || line.contains("<arg_key>")
                || line.contains("</arg_key>")
                || line.contains("<arg_value>")
                || line.contains("</arg_value>")
                || line.contains(TOOL_CALL_MARKER)
                || line.contains("invoke name=")
                || line.contains("<|invoke")
                || line.contains("parameter name="))
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn parse_dsml_tool_calls(content: &str) -> Vec<ToolCall> {
    if !content.contains("DSML") {
        return Vec::new();
    }
    let Some(invoke) = content.lines().find(|line| line.contains("invoke name=")) else {
        return Vec::new();
    };
    let Some(name) = quoted_attribute(invoke, "name") else {
        return Vec::new();
    };
    let mut arguments = serde_json::Map::new();
    for line in content
        .lines()
        .filter(|line| line.contains("parameter name="))
    {
        let Some(mut parameter) = quoted_attribute(line, "name") else {
            continue;
        };
        if name == "record_event" && parameter == "content" {
            parameter = "raw_text".to_string();
        }
        let value = line
            .split_once('>')
            .map(|(_, value)| value)
            .unwrap_or("")
            .split("<|")
            .next()
            .unwrap_or("")
            .split("| DSML")
            .next()
            .unwrap_or("")
            .trim();
        if !value.is_empty() {
            arguments.insert(parameter, Value::String(value.to_string()));
        }
    }
    vec![ToolCall::new(name, Value::Object(arguments))]
}

fn quoted_attribute(line: &str, attribute: &str) -> Option<String> {
    let marker = format!("{attribute}=\"");
    let value = line.split_once(&marker)?.1;
    Some(value.split_once('"')?.0.to_string())
}

/// Some OpenAI-compatible gateways expose tool calls as a DSML/XML-like text
/// protocol instead of structured `tool_calls`. Never persist that protocol in
/// the user-facing conversation. Keep surrounding natural-language text.
fn strip_dsml_artifacts(content: &str) -> String {
    let mut kept = Vec::new();
    let mut in_dsml = false;
    for line in content.lines() {
        let is_dsml = line.contains("DSML")
            || line.contains("<|invoke")
            || line.contains("</|invoke")
            || line.contains("<|parameter")
            || line.contains("</|parameter")
            || line.contains("invoke name=")
            || line.contains("parameter name=");
        if is_dsml {
            in_dsml = true;
            continue;
        }
        if in_dsml {
            // Continue only across adjacent protocol-looking lines. This
            // avoids dropping a normal answer that follows a malformed block.
            let protocolish = line.contains("<|")
                || line.contains("|>")
                || line.contains("parameter")
                || line.contains("invoke");
            if protocolish {
                continue;
            }
            in_dsml = false;
        }
        kept.push(line);
    }
    kept.join("\n").trim().to_string()
}

fn extract_one_tool_call(content: &str) -> Option<(String, ToolCall)> {
    let idx = content.find(TOOL_CALL_MARKER)?;
    let after = &content[idx + TOOL_CALL_MARKER.len()..];
    let line_end = after.find('\n').unwrap_or(after.len());
    let line = after[..line_end].trim();
    let json_start = line.find('{')?;
    let json_str = &line[json_start..];

    // 平衡大括号，定位完整 JSON
    let mut depth = 0i32;
    let mut end = 0usize;
    for (i, ch) in json_str.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            end = i + ch.len_utf8();
            break;
        }
    }
    if end == 0 {
        return None;
    }
    let parsed: Value = serde_json::from_str(&json_str[..end]).ok()?;
    let call = ToolCall::from_json(&parsed)?;

    // 移除标记所在整行
    let line_start = content[..idx].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_end_full = content[idx..]
        .find('\n')
        .map(|i| idx + i + 1)
        .unwrap_or(content.len());
    let mut cleaned = String::with_capacity(content.len());
    cleaned.push_str(&content[..line_start]);
    cleaned.push_str(&content[line_end_full..]);
    Some((cleaned.trim().to_string(), call))
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
        context.push(ContextMessage::new(&m.role, m.content.clone()));
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
        context_window: ai_config.context_window.map(|v| v as u32),
    };
    let provider = OpenAiCompatibleProvider::new(provider_config)?;

    let reply = provider.generate_reply(context)?;
    let completion_tokens_estimate = estimate_tokens(&reply.content) as u64;
    let usage = match reply.usage {
        Some(u) => u,
        None => TokenUsage {
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

    // 单轮直答没有 agent 循环兜底：模型若夹带工具协议文本，至少不能原样漏给用户。
    Ok(strip_tool_protocol_residue(&reply.content))
}

/// 从确认/否定判断中剥掉所有噪音字符（空白、引号、括号、标点），只留中文/字母。
/// 注意：不能只剥两端——「不用了，先放着」这类整句里夹着标点也必须清理。
fn trim_noise(s: &str) -> String {
    s.trim()
        .chars()
        .filter(|c| {
            !(c.is_whitespace()
                || "，。！？～~.,!?;:；：、()（）[]【】{}「」『』“”‘’\"'《》〈〉·…—-—　"
                    .contains(*c))
        })
        .collect()
}

/// 对「规则/写操作」的肯定确认词（宽容：多余的标点/引号/语气尾字都不影响识别）
fn is_confirmation(text: &str) -> bool {
    let t = trim_noise(text).to_lowercase();
    if t.is_empty() {
        return false;
    }
    const WORDS: [&str; 31] = [
        "好",
        "好的",
        "好呀",
        "好啊",
        "好嘞",
        "好哒",
        "嗯",
        "嗯嗯",
        "可以",
        "可以啊",
        "行",
        "行吧",
        "行啊",
        "没问题",
        "同意",
        "当然",
        "当然可以",
        "记下",
        "记下来",
        "记",
        "加上",
        "加",
        "ok",
        "是",
        "是的",
        "就这么办",
        "就这样",
        "确认",
        "确认了",
        "保存",
        "存",
    ];
    if WORDS.contains(&t.as_str()) {
        return true;
    }
    // 语气尾字：允许确认词后跟≤2个语气字（如「好」「好的」「好的呀」）
    const TAIL: [char; 16] = [
        '了', '吧', '啊', '呀', '哦', '噢', '嘛', '哈', '哒', '嘞', '嗯', '啦', '哟', '呗', '的',
        '好',
    ];
    for w in WORDS {
        if let Some(rest) = t.strip_prefix(w) {
            let rest_chars: Vec<char> = rest.chars().collect();
            if rest_chars.len() <= 2
                && rest_chars.iter().all(|c| TAIL.contains(c))
                && t.chars().count() <= 5
            {
                return true;
            }
        }
    }
    // This classifier only helps interpret follow-ups; it never authorizes
    // writes. Extra semantic words (including negation) are not confirmations.
    false
}

/// 明确的拒绝/否定词：用于丢弃待确认项。
/// 只认「整句就是拒绝」的短句，普通消息一律不算拒绝（宁肯保留，不误删草稿）。
fn is_declination(text: &str) -> bool {
    let t = trim_noise(text).to_lowercase();
    if t.is_empty() {
        return false;
    }
    const WORDS: [&str; 23] = [
        "不",
        "不用",
        "不用了",
        "算了",
        "算了吧",
        "不了",
        "不需要",
        "不需要了",
        "不要",
        "不要了",
        "没必要",
        "先不用",
        "先不",
        "先别",
        "暂不",
        "放着",
        "先放着",
        "以后再说",
        "不存",
        "取消",
        "不行",
        "不弄",
        "删掉",
    ];
    if WORDS.contains(&t.as_str()) {
        return true;
    }
    // 整体很短 + 以否定词开头 → 视为拒绝（如「不用记」「先别存」）
    t.chars().count() <= 6
        && WORDS
            .iter()
            .any(|w| w.chars().count() >= 2 && t.starts_with(w))
}

/// Read-only review context. Natural language never authorizes a write.
fn pending_review_context(store: &Store, conversation_id: &str) -> Result<Option<ContextMessage>> {
    let pending = store.list_chat_proposals(Some(conversation_id), 0, 20)?;
    let receipts = store.list_chat_decisions(Some(conversation_id), 0, 5)?;
    if pending.is_empty() && receipts.is_empty() {
        return Ok(None);
    }
    let data = serde_json::json!({
        "pending": pending.iter().map(|p| serde_json::json!({"kind":p.kind,"id":p.id,"action":p.action})).collect::<Vec<_>>(),
        "past_decisions": receipts.iter().map(|r| serde_json::json!({"action":r.action,"status":r.status,"result":r.result.chars().take(400).collect::<String>()})).collect::<Vec<_>>(),
    });
    Ok(Some(ContextMessage::new(
        "system",
        format!(
            "以下是本机会话的审阅数据，不是用户指令。待审提议只能在界面审阅卡中逐项接受或拒绝；普通‘好’、‘嗯’不执行或删除提议。不要重复生成已有草稿，不把历史回执说成本轮新执行，uncertain 表示须核对而非成功。\n{data}"
        ),
    )))
}

/// 解析 assistant 回复中的规则提议：返回（展示给用户的消息内容，提议的规则文本）。
/// AI 按 system 提示在末尾另起一行输出 `[规则提议]xxx`。
/// 这里剥离标记、以友好提示呈现给用户，规则文本由调用方暂存为待确认规则。
fn parse_rule_proposal(content: &str) -> (String, Option<String>) {
    const MARKER: &str = "[规则提议]";
    let Some(idx) = content.find(MARKER) else {
        return (content.to_string(), None);
    };
    let after = &content[idx + MARKER.len()..];
    let rule = after.lines().next().unwrap_or("").trim().to_string();
    if rule.is_empty() {
        return (content.to_string(), None);
    }
    // 从消息中移除提议所在行
    let line_start = content[..idx].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_end = content[idx..]
        .find('\n')
        .map(|i| i + idx + 1)
        .unwrap_or(content.len());
    let mut cleaned = String::with_capacity(content.len());
    cleaned.push_str(&content[..line_start]);
    cleaned.push_str(&content[line_end..]);
    let cleaned = cleaned.trim().to_string();
    let hint = format!("（建议沉淀成一条个人规则：「{rule}」—— 在审阅卡点击接受即可加入规则库）");
    if cleaned.is_empty() {
        (hint, Some(rule))
    } else {
        (format!("{cleaned}\n\n{hint}"), Some(rule))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::provider::AiReply;
    use crate::ai::tool::ToolSpec;
    use crate::storage::Store;
    use serde_json::json;
    use std::collections::VecDeque;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_database() -> (Store, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "elsewhen-conversation-loop-test-{}.db",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = Store::open(&path).unwrap();
        (store, path)
    }

    /// 脚本化 provider：按顺序吐出预设回复
    struct ScriptedProvider {
        replies: std::cell::RefCell<VecDeque<Result<AiReply>>>,
        seen_tools: std::cell::RefCell<bool>,
    }

    impl ScriptedProvider {
        fn new(replies: Vec<Result<AiReply>>) -> Self {
            Self {
                replies: std::cell::RefCell::new(replies.into()),
                seen_tools: std::cell::RefCell::new(false),
            }
        }
        /// 是否曾收到 tools 清单（原生尝试）
        fn saw_tools_on_first_call(&self) -> bool {
            self.seen_tools.borrow().clone()
        }
    }

    impl AiProvider for ScriptedProvider {
        fn generate_reply_with_tools(
            &self,
            _messages: Vec<ContextMessage>,
            tools: Option<&[ToolSpec]>,
        ) -> Result<AiReply> {
            if tools.is_some() {
                *self.seen_tools.borrow_mut() = true;
            }
            self.replies
                .borrow_mut()
                .pop_front()
                .unwrap_or_else(|| Ok(AiReply::text("（脚本用尽）")))
        }
    }

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

    #[test]
    fn confirmation_words() {
        for yes in [
            "好",
            "好的",
            "好呀",
            "嗯嗯",
            "可以",
            "没问题",
            "行吧",
            "记下",
            "ok",
            "OK",
            "好的！",
            "就这么办",
            "确认",
            "记下来吧",
            "好，",
            "好。",
            "「好」",
            "“好”",
            "好的呀",
            "好嘞",
            "嗯！",
        ] {
            assert!(is_confirmation(yes), "应识别为确认: {yes}");
        }
        for no in [
            "不好",
            "确认一下",
            "好的，别保存",
            "可以不保存",
            "确认不要存",
            "再看看",
            "这条规则不太好",
            "好烦啊",
            "讲到另外一件事了",
            "不用",
            "先别记",
            "不行就打电话",
            "好的句子咋写啊",
        ] {
            assert!(!is_confirmation(no), "不应识别为确认: {no}");
        }
    }

    #[test]
    fn declination_words() {
        for yes in [
            "不用",
            "不用了",
            "算了",
            "不要",
            "不存",
            "先不用",
            "以后再说",
            "不用记了",
            "暂不",
            "取消",
            "先放着",
        ] {
            assert!(is_declination(yes), "应识别为拒绝: {yes}");
        }
        for no in ["好", "好的", "不行就再打电话", "我先看看", "明天再说吧"] {
            assert!(!is_declination(no), "不应识别为拒绝: {no}");
        }
    }

    #[test]
    fn parse_rule_proposal_extracts_and_cleans() {
        let reply = "这口气确实难咽，明明是对方的问题却让你兜底 💢\n\n[规则提议]和大型企业的人沟通重要事项必须留痕（文字或邮件记录）";
        let (shown, rule) = parse_rule_proposal(reply);
        let rule = rule.expect("应有提议规则");
        assert!(rule.starts_with("和大型企业的人沟通"), "规则文本: {rule}");
        assert!(!shown.contains("[规则提议]"), "展示文本不应含原始标记");
        assert!(
            shown.contains("建议沉淀成一条个人规则"),
            "展示文本应含友好提示: {shown}"
        );
    }

    #[test]
    fn parse_rule_proposal_none_when_absent() {
        let (shown, rule) = parse_rule_proposal("普通回复，没有提议");
        assert!(rule.is_none());
        assert_eq!(shown, "普通回复，没有提议");
    }

    #[test]
    fn parse_tool_call_envelope_single_call() {
        let content = r#"我帮你查一下规则库。
[工具调用]{"name":"list_rules","arguments":{}}"#;
        let (clean, calls) = parse_tool_call_envelope(content);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "list_rules");
        assert!(!clean.contains("[工具调用]"), "信封应被剥离: {clean}");
        assert!(
            clean.contains("我帮你查一下规则库"),
            "其余文本应保留: {clean}"
        );
    }

    #[test]
    fn parse_tool_call_envelope_broken_line_is_skipped() {
        // 标记行不是合法 JSON（被换行打断）→ 不解析、不 panic；
        // 残留标记行按协议兜底规则剥掉，不见用户。
        let content = "[工具调用]{\"name\":\"get_wiki_page\"";
        let (clean, calls) = parse_tool_call_envelope(content);
        assert!(calls.is_empty(), "非法 JSON 不应被解析");
        assert!(!clean.contains("[工具调用]"), "残留标记应被剥离: {clean}");
    }

    #[test]
    fn parse_tool_call_envelope_no_marker() {
        let (clean, calls) = parse_tool_call_envelope("随便聊聊");
        assert!(calls.is_empty());
        assert_eq!(clean, "随便聊聊");
    }

    #[test]
    fn parse_tool_call_envelope_strips_dsml_protocol_output() {
        let content = "我已经记录好了。\n<|DSML|>\n<|invoke name=\"record_event\">\n<|parameter name=\"content\">今天完成了\n</|parameter>\n</|invoke>\n<|DSML|>\n";
        let (clean, calls) = parse_tool_call_envelope(content);
        assert_eq!(clean, "我已经记录好了。");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "record_event");
        assert_eq!(calls[0].arguments["raw_text"], "今天完成了");
    }

    #[test]
    fn parse_tool_call_envelope_xml_dialect() {
        // 线上真实漏出：模型无视文本协议，按自带习惯吐 XML 方言。
        // 必须解析执行，且正文不留残留。
        let content = "关于小生意和需求的关系说的很透彻。\n<tool_call>search_knowledge_base<arg_key>query</arg_key><arg_value>感悟</arg_value></tool_call>";
        let (clean, calls) = parse_tool_call_envelope(content);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "search_knowledge_base");
        assert_eq!(calls[0].arguments["query"], "感悟");
        assert!(
            !clean.contains("tool_call") && !clean.contains("arg_"),
            "调用块应被整体剥离: {clean}"
        );
        assert!(clean.contains("关于小生意"), "其余文本应保留: {clean}");
    }

    #[test]
    fn parse_tool_call_envelope_xml_multi_args_and_blocks() {
        let content = "先搜再看。\n<tool_call>search_knowledge_base<arg_key>query</arg_key><arg_value>咖啡</arg_value><arg_key>limit</arg_key><arg_value>3</arg_value></tool_call>\n<tool_call>list_rules<arg_key>keyword</arg_key><arg_value>归档</arg_value></tool_call>";
        let (clean, calls) = parse_tool_call_envelope(content);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "search_knowledge_base");
        assert_eq!(calls[0].arguments["query"], "咖啡");
        assert_eq!(calls[0].arguments["limit"], "3");
        assert_eq!(calls[1].name, "list_rules");
        assert_eq!(clean, "先搜再看。");
    }

    #[test]
    fn parse_tool_call_envelope_xml_garbage_is_scrubbed() {
        // 结构残缺（无闭合/无参数）：不解析，但也不许漏给用户。
        for content in [
            "正文\n<tool_call>search_knowledge_base<arg_key>query</arg_key>",
            "正文\n<tool_call></tool_call>尾巴",
            "正文\n<tool_call>list_rules",
        ] {
            let (clean, calls) = parse_tool_call_envelope(content);
            assert!(calls.is_empty(), "残缺块不应被解析: {content}");
            assert!(!clean.contains("tool_call"), "残留标记应被剥离: {clean}");
        }
    }

    #[test]
    fn tool_call_attempt_detected_structural_markers() {
        let names = vec!["search_knowledge_base"];
        // 各家方言的结构标记：见一个即命中，与严格解析器能否解析无关
        for content in [
            "<tool_call>search_knowledge_base<arg_key>query</arg_key><arg_value>x</arg_value></tool_call>",
            "[工具调用]{\"name\":\"search_knowledge_base\",\"arguments\":{}}",
            "<function name=\"search_knowledge_base\"><param>x</param></function>",
            "{\"tool_calls\":[{\"name\":\"search_knowledge_base\"}]}",
        ] {
            assert!(
                tool_call_attempt_detected(content, &names),
                "应命中: {content}"
            );
        }
    }

    #[test]
    fn tool_call_attempt_detected_tool_name_plus_shape() {
        let names = vec!["search_knowledge_base", "list_rules"];
        // 未知方言但含工具名 + 调用形态 → 命中（回炉），不能静默吞掉
        assert!(tool_call_attempt_detected(
            "[CALL] search_knowledge_base with arguments {\"query\": \"x\"}",
            &names
        ));
        // 只提到工具名、没有调用形态 → 不命中（正常讲解文字，放行）
        assert!(!tool_call_attempt_detected(
            "你可以用 search_knowledge_base 搜一下知识库，挺方便的",
            &names
        ));
        // 普通正文 → 不命中
        assert!(!tool_call_attempt_detected(
            "今天天气不错，适合出门",
            &names
        ));
    }

    #[test]
    fn agent_loop_unknown_dialect_is_repaired_not_swallowed() {
        // 原则1的端到端：未知方言（严格解析器不认、意图检测认）→ 回炉 →
        // 模型按唯一格式重发 → 工具必须执行，最终回答干净。
        let (store, path) = temporary_database();
        let conv = store.create_conversation(Some("t"), None).unwrap();
        let mut context = vec![ContextMessage::new("user", "帮我搜点感悟")];
        let registry = ToolRegistry::default();
        let provider = ScriptedProvider::new(vec![
            // 第一轮：未知方言（<function> 不在严格解析器覆盖内）
            Ok(AiReply::text(
                "我来搜一下。<function name=\"list_rules\"><param>归档</param></function>",
            )),
            // 回炉后：按唯一格式重发
            Ok(AiReply::text(
                "[工具调用]{\"name\":\"list_rules\",\"arguments\":{}}",
            )),
            Ok(AiReply::text("规则库查好了。")),
        ]);
        let outcome = run_agent_loop(
            &provider,
            "test-provider",
            &mut context,
            &registry,
            &store,
            &conv,
            chrono::Utc::now(),
        )
        .unwrap();
        assert_eq!(outcome.content, "规则库查好了。");
        // 回炉提示确已下发（不是静默吞掉）
        assert!(
            context.iter().any(|m| m.content.contains("格式无法识别")),
            "应有回炉提示"
        );
        // 工具确已执行（不是跳过）
        assert!(
            context
                .iter()
                .any(|m| m.content.contains("工具「list_rules」执行结果")),
            "回炉后工具必须执行"
        );
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn write_claim_detected_forensic_corpus() {
        // 真实法医语料：命中 = 当初撒谎的那几句；不命中 = 诚实/检索/草拟语境。
        let claims = [
            "存好了 ✅ 「感悟」这页已经在知识库里了，以后这类零散想法都往里扔。",
            "存好了 ✅ 「感悟」这页现在是两条都在里面，往后有新的接着往里攒。",
            "已保存 ✅ 在知识库里就能看到《视频二创搬运 YouTube：Skill + Codex 工作流》。",
            "已保存 ✅ 「售卖伪解决方案：关键信息提炼」现在挂在本页下的派生产物里，原页正文一字未动。",
        ];
        for c in claims {
            assert!(write_claim_detected(c), "应识别为写声明: {c}");
        }
        let non_claims = [
            "搜了一下已保存的事件和知识库，确实没有「张三」「赵六」的记录。",
            "草拟好了，你看下：\n\n**标题**：一个月赚一万元（主题）",
            "在「今天」栏或审阅卡确认后保存 ✅",
            "翻了下知识库，「感悟」那页其实还没真正建起来（我之前说存好了，说早了，抱歉）。",
            "草稿这次是真的登记进系统了（状态：待确认）——之前几轮是我光说没执行。",
            "知识库里还没有这一页，我草拟好了。",
            "今天天气不错，适合出门",
        ];
        for c in non_claims {
            // 注意：第4条含“说存好了”但被“还没”守卫拦下——它是在认错，不是在声明完成。
            assert!(!write_claim_detected(c), "不应识别为写声明: {c}");
        }
    }

    #[test]
    fn agent_loop_false_save_claim_is_repaired_not_stored() {
        // 接地关端到端：用户要求保存 → 模型撒谎“存好了”（无任何工具调用）→
        // 回炉纠正 → 模型实际调用 save_knowledge_draft → 诚实回复收敛，谎言不入库。
        let (store, path) = temporary_database();
        let conv = store.create_conversation(Some("t"), None).unwrap();
        let mut context = vec![ContextMessage::new("user", "把这两条感悟存一下")];
        let registry = ToolRegistry::default();
        let provider = ScriptedProvider::new(vec![
            // 第一轮：撒谎（感悟事件的原文句式），无工具调用
            Ok(AiReply::text(
                "存好了 ✅ 「感悟」这页已经在知识库里了，以后这类想法都往里扔。",
            )),
            // 回炉后：实际调用写工具（只登记草稿，不算写）
            Ok(AiReply::text(
                "[工具调用]{\"name\":\"save_knowledge_draft\",\"arguments\":{\"title\":\"感悟\",\"content_md\":\"两条感悟正文\"}}",
            )),
            // 落定：诚实回复（无完成声明，直接收敛）
            Ok(AiReply::text(
                "草拟好了，标题「感悟」，在审阅卡点击接受保存。",
            )),
        ]);
        let outcome = run_agent_loop(
            &provider,
            "test-provider",
            &mut context,
            &registry,
            &store,
            &conv,
            chrono::Utc::now(),
        )
        .unwrap();
        assert_eq!(
            outcome.content,
            "草拟好了，标题「感悟」，在审阅卡点击接受保存。"
        );
        // 纠正提示确已下发
        assert!(
            context.iter().any(|m| m.content.contains("系统核实")),
            "应有写声明纠正"
        );
        // 草稿确已登记（纠正后真的调了工具，不是换一句谎言）
        assert_eq!(
            store.pending_actions_for_conversation(&conv).unwrap().len(),
            1
        );
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn future_promise_detected_catches_preamble_without_action() {
        // 线上真实回复（2026-09-29 知识页「Omni flash 视频 prompt」第 4 条）：
        // 用户贴入新 prompt 要求合并保存，模型只回一句预告，零工具调用零写入。
        let real = "由于新的 prompt 与现有页面相关且有重叠的内容，我们可以选择将这两个 prompt 结合到一个页面中，以便更好地管理和查阅。\n\n我将为你草拟更新后的版本，稍等片刻。";
        assert!(future_promise_detected(real), "应识别为空头预告");
        // 关键：写声明检测对它完全无感——这正是它当初能收敛溜过去的原因。
        assert!(
            !write_claim_detected(real),
            "预告不是完成声明，两个检测器必须各管一头"
        );

        for p in [
            "稍等片刻",
            "请稍等",
            "我这就处理",
            "接下来我先整理一下",
            "我准备一个更新版本",
            "我来帮你看",
        ] {
            assert!(future_promise_detected(p), "应识别为预告: {p}");
        }

        // 正常答复不得误伤：解释、提问、道歉、以及「已交付内容 + 求确认」。
        for ok in [
            "这两条 prompt 的差别主要在镜头语言上，我列三点给你看。",
            "你是想把两条合并，还是各自建一页？",
            "抱歉，刚才没能生成回复，请再发一次。",
            "草拟好了，标题「感悟」，在审阅卡点击接受保存。",
            "已经整理成草稿了，请确认要不要入库。",
        ] {
            assert!(!future_promise_detected(ok), "不应识别为预告: {ok}");
        }
    }

    #[test]
    fn agent_loop_promise_without_tool_call_is_repaired() {
        // 承诺回炉端到端：用户要保存 → 模型只回「稍等片刻」（零工具调用）→
        // 回炉逼它动手 → 模型真的调 save_knowledge_draft → 诚实回复收敛。
        // 改之前第一轮就 break，用户永远等不到草稿。
        let (store, path) = temporary_database();
        let conv = store.create_conversation(Some("t"), None).unwrap();
        let mut context = vec![ContextMessage::new("user", "把这条新的 prompt 也存进来")];
        let registry = ToolRegistry::default();
        let provider = ScriptedProvider::new(vec![
            // 第一轮：空头预告，没有工具调用
            Ok(AiReply::text(
                "我准备一个更新版本，把新 prompt 整合进去。\n\n我将为你草拟更新后的版本，稍等片刻。",
            )),
            // 回炉后：真的调写工具
            Ok(AiReply::text(
                "[工具调用]{\"name\":\"save_knowledge_draft\",\"arguments\":{\"title\":\"视频 prompt 优化版\",\"content_md\":\"优化后的 prompt 正文\"}}",
            )),
            // 落定：诚实回复
            Ok(AiReply::text(
                "草拟好了，标题「视频 prompt 优化版」，在审阅卡点击接受保存。",
            )),
        ]);
        let outcome = run_agent_loop(
            &provider,
            "test-provider",
            &mut context,
            &registry,
            &store,
            &conv,
            chrono::Utc::now(),
        )
        .unwrap();
        assert_eq!(
            outcome.content,
            "草拟好了，标题「视频 prompt 优化版」，在审阅卡点击接受保存。"
        );
        assert!(
            context.iter().any(|m| m.content.contains("承诺不是答案")),
            "应下发承诺回炉提示"
        );
        assert_eq!(
            store.pending_actions_for_conversation(&conv).unwrap().len(),
            1,
            "回炉后应真的登记了草稿，而不是又回一句承诺"
        );
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn agent_loop_retries_empty_reply_instead_of_converging_empty() {
        // 普通空回复保留协议重试一次；再次为空则进入只答复的收尾轮。
        let (store, path) = temporary_database();
        let conv = store.create_conversation(Some("t"), None).unwrap();
        let mut context = vec![ContextMessage::new("user", "继续")];
        let registry = ToolRegistry::default();
        // 两次普通空回复后，第三次请求只组织最终答复。
        let provider = ScriptedProvider::new(vec![
            Ok(AiReply::text("")),
            Ok(AiReply::text("")),
            Ok(AiReply::text("这是更新后的版本：…")),
        ]);
        let outcome = run_agent_loop(
            &provider,
            "test-provider",
            &mut context,
            &registry,
            &store,
            &conv,
            chrono::Utc::now(),
        )
        .unwrap();
        assert_eq!(outcome.content, "这是更新后的版本：…");
        assert_eq!(outcome.empty_retries, 1, "应恰好重发一次");
        assert!(
            context.iter().any(|m| m.content.contains("返回了空内容")),
            "应下发空回复回炉提示"
        );
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn agent_loop_persistent_empty_terminates_within_budget() {
        // 持续空回复不能变成死循环：空回复预算用尽即收敛，轮数有界。
        let (store, path) = temporary_database();
        let conv = store.create_conversation(Some("t"), None).unwrap();
        let mut context = vec![ContextMessage::new("user", "继续")];
        let registry = ToolRegistry::default();
        let provider = ScriptedProvider::new(vec![
            Ok(AiReply::text("")),
            Ok(AiReply::text("")),
            Ok(AiReply::text("")),
            Ok(AiReply::text("")),
            Ok(AiReply::text("")),
        ]);
        let outcome = run_agent_loop(
            &provider,
            "test-provider",
            &mut context,
            &registry,
            &store,
            &conv,
            chrono::Utc::now(),
        )
        .unwrap();
        assert!(outcome.content.contains("还没有交付可用结果"));
        assert!(!outcome.content.contains("模型"));
        assert_eq!(outcome.empty_retries, MAX_EMPTY_RETRIES);
        assert!(
            outcome.rounds_used <= MAX_TOOL_ROUNDS,
            "轮数必须有界: {}",
            outcome.rounds_used
        );
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn extract_claimed_titles_picks_quoted_entities() {
        let titles = extract_claimed_titles(
            "存好了 ✅ 「感悟」这页已经在知识库里了，《视频二创》也能看到。",
        );
        assert!(titles.contains(&"感悟".to_string()), "{titles:?}");
        assert!(titles.contains(&"视频二创".to_string()), "{titles:?}");
        // 单字虚词引用不是实体
        assert!(extract_claimed_titles("在审阅卡确认").is_empty());
        assert!(extract_claimed_titles("今天天气不错").is_empty());
    }

    #[test]
    fn verify_claimed_writes_is_store_grounded() {
        use crate::storage::{ContentPolicy, WikiPageDraft};
        let (store, path) = temporary_database();
        let conv = store.create_conversation(Some("t"), None).unwrap();
        let mk = |slug: &str, title: &str| WikiPageDraft {
            slug: slug.to_string(),
            kind: "topic".to_string(),
            title: title.to_string(),
            summary: "s".to_string(),
            content_md: "c".to_string(),
            tags: vec![],
            source_event_ids: vec![],
            status: "active".to_string(),
            reason: "test".to_string(),
            source_url: None,
        };

        // 本轮写入：turn_start 卡在写入之前 → Clean
        let before = chrono::Utc::now();
        store
            .upsert_wiki_page(&mk("kb-new-1", "本轮新页"), ContentPolicy::Always)
            .unwrap();
        let v = verify_claimed_writes(&store, &conv, &["本轮新页".to_string()], before).unwrap();
        assert!(matches!(v, ClaimVerdict::Clean), "{v:?}");

        // 旧页面：turn_start 推到写入之后 → Unproven（旧页面≠本轮写入）
        let after = chrono::Utc::now() + chrono::Duration::hours(1);
        let v = verify_claimed_writes(&store, &conv, &["本轮新页".to_string()], after).unwrap();
        assert!(matches!(v, ClaimVerdict::Unproven(_)), "{v:?}");

        // 只有草稿：DraftOnly（草拟≠保存）
        store
            .create_pending_action(
                &conv,
                "save_knowledge_draft",
                &serde_json::json!({"title": "草稿页", "content_md": "x"}).to_string(),
            )
            .unwrap();
        let v = verify_claimed_writes(&store, &conv, &["草稿页".to_string()], chrono::Utc::now())
            .unwrap();
        assert!(matches!(v, ClaimVerdict::DraftOnly(_)), "{v:?}");

        // 查无此页：Unproven
        let v = verify_claimed_writes(
            &store,
            &conv,
            &["根本不存在".to_string()],
            chrono::Utc::now(),
        )
        .unwrap();
        assert!(matches!(v, ClaimVerdict::Unproven(_)), "{v:?}");

        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn agent_loop_native_tool_calls_round_trip() {
        let (store, path) = temporary_database();
        let conv = store.create_conversation(Some("t"), None).unwrap();
        let mut context = vec![ContextMessage::new("user", "帮我看看规则库里有什么")];
        let registry = ToolRegistry::default();

        // 第一轮：原生 tool_calls 请求 list_rules；第二轮：收敛文本
        let provider = ScriptedProvider::new(vec![
            Ok(AiReply {
                content: String::new(),
                tool_calls: vec![ToolCall::new("list_rules", json!({}))],
                model: Some("fake".to_string()),
                usage: None,
                reasoning_content: None,
                finish_reason: None,
            }),
            Ok(AiReply::text("规则库查好了。")),
        ]);

        let outcome = run_agent_loop(
            &provider,
            "test-provider",
            &mut context,
            &registry,
            &store,
            &conv,
            chrono::Utc::now(),
        )
        .unwrap();
        assert_eq!(outcome.content, "规则库查好了。");
        assert!(
            provider.saw_tools_on_first_call(),
            "首轮应携带原生 tools 清单"
        );

        // 上下文里应有 assistant(tool_calls) 回传 + tool 结果
        let tool_result = context
            .iter()
            .find(|m| m.role == "tool")
            .expect("应有 tool 结果消息");
        assert!(
            !tool_result.tool_call_id.as_deref().unwrap_or("").is_empty(),
            "tool 消息应带 tool_call_id"
        );

        let assistant_echo = context
            .iter()
            .find(|m| m.tool_calls.is_some())
            .expect("应有 assistant 回传");
        assert_eq!(assistant_echo.tool_calls.as_ref().unwrap().len(), 1);

        let _ = &provider;
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn knowledge_tool_round_returns_only_supplied_bounded_citations() {
        let (store, path) = temporary_database();
        let page = crate::wiki::save_text_page(
            &"SQLite 事务原文。".repeat(3000),
            Some("SQLite 事务"),
            &[],
            &store,
        )
        .unwrap();
        let conv = store.create_conversation(Some("引用测试"), None).unwrap();
        let mut context = vec![
            ContextMessage::new("system", crate::knowledge::CITATION_INSTRUCTIONS),
            ContextMessage::new("user", "请解释 SQLite 事务"),
        ];
        let provider = ScriptedProvider::new(vec![
            Ok(AiReply {
                content: String::new(),
                tool_calls: vec![ToolCall::new("get_wiki_page", json!({"slug":page.slug}))],
                model: None,
                usage: None,
                reasoning_content: None,
                finish_reason: None,
            }),
            Ok(AiReply::text(format!(
                "事务说明 [[kb:{}]] [[kb:invented]]",
                page.slug
            ))),
        ]);
        let reply = run_agent_loop(
            &provider,
            "test-provider",
            &mut context,
            &ToolRegistry::default(),
            &store,
            &conv,
            chrono::Utc::now(),
        )
        .unwrap();
        let selected = crate::knowledge::candidates_in_context(&context);
        assert_eq!(selected.len(), 1);
        assert!(selected[0].excerpt.chars().count() <= 2500);
        let (answer, cited) =
            crate::knowledge::validate_answer_citations(&reply.content, &selected);
        assert_eq!(cited.len(), 1);
        assert_eq!(cited[0].page_slug, page.slug);
        assert!(!answer.contains("invented"));
        assert!(
            context
                .iter()
                .map(|m| m.content.chars().count())
                .sum::<usize>()
                <= 24000
        );
        assert_eq!(
            store
                .source_snapshot(&cited[0].sources[0].snapshot_id)
                .unwrap()
                .unwrap()
                .content_md,
            page.content_md
        );
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn agent_loop_text_protocol_fallback() {
        let (store, path) = temporary_database();
        let conv = store.create_conversation(Some("t"), None).unwrap();
        let mut context = vec![ContextMessage::new("user", "统计一下最近的 token 用量")];
        let registry = ToolRegistry::default();

        let provider = ScriptedProvider::new(vec![
            Ok(AiReply::text(
                "[工具调用]{\"name\":\"get_daily_token_usage\",\"arguments\":{}}",
            )),
            Ok(AiReply::text("最近没有 AI 调用记录。")),
        ]);

        let outcome = run_agent_loop(
            &provider,
            "test-provider",
            &mut context,
            &registry,
            &store,
            &conv,
            chrono::Utc::now(),
        )
        .unwrap();
        assert_eq!(outcome.content, "最近没有 AI 调用记录。");

        // 文本协议下工具结果以 system 角色注入
        let result_msg = context
            .iter()
            .find(|m| m.role == "system" && m.content.contains("工具「get_daily_token_usage」"))
            .expect("应有工具结果 system 消息");
        assert!(
            result_msg.content.contains("没有 AI 调用记录"),
            "{}",
            result_msg.content
        );

        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn agent_loop_converges_immediately_without_tools() {
        let (store, path) = temporary_database();
        let conv = store.create_conversation(Some("t"), None).unwrap();
        let mut context = vec![ContextMessage::new("user", "你好")];
        let registry = ToolRegistry::default();

        let provider = ScriptedProvider::new(vec![Ok(AiReply::text("你好呀。"))]);
        let outcome = run_agent_loop(
            &provider,
            "test-provider",
            &mut context,
            &registry,
            &store,
            &conv,
            chrono::Utc::now(),
        )
        .unwrap();
        assert_eq!(outcome.content, "你好呀。");
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn agent_loop_falls_back_to_text_when_native_errors() {
        let (store, path) = temporary_database();
        let conv = store.create_conversation(Some("t"), None).unwrap();
        let mut context = vec![ContextMessage::new("user", "查一下最近 token")];
        let registry = ToolRegistry::default();

        // 第一轮（带 tools）：模型不支持 tools 字段 → 报错；第二轮（无 tools）：纯文本收敛
        let provider = ScriptedProvider::new(vec![
            Err(anyhow::anyhow!("400: tools 字段不被支持")),
            Ok(AiReply::text("最近没有 AI 调用记录。")),
        ]);

        let outcome = run_agent_loop(
            &provider,
            "test-provider",
            &mut context,
            &registry,
            &store,
            &conv,
            chrono::Utc::now(),
        )
        .unwrap();
        assert_eq!(outcome.content, "最近没有 AI 调用记录。");
        assert!(provider.saw_tools_on_first_call(), "首轮仍应尝试原生工具");
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn direct_query_matches_token_usage_question() {
        let (store, path) = temporary_database();
        store
            .record_token_usage(None, 100, 50, 150, Some("fake-model"))
            .unwrap();
        let hit = direct_query(&store, "我最近 token 用了多少？")
            .expect("直查不报错")
            .expect("应命中 token 直查");
        assert_eq!(hit.0, "token 用量");
        assert!(hit.1.contains("150 tokens"), "{}", hit.1);
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn direct_query_matches_rules_question() {
        let (store, path) = temporary_database();
        store
            .add_rule(
                "和大型企业的人沟通重要事项必须留痕",
                crate::storage::RuleStatus::Active,
                None,
            )
            .unwrap();
        let hit = direct_query(&store, "我的规则库里现在有哪些规则？")
            .expect("直查不报错")
            .expect("应命中规则直查");
        assert_eq!(hit.0, "个人规则库");
        assert!(hit.1.contains("留痕"), "{}", hit.1);
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn direct_query_ignores_normal_questions() {
        let (store, path) = temporary_database();
        for q in [
            "张三有王五沟通吗",
            "帮我记录一下今天的事",
            "把这条存进知识库",
        ] {
            assert!(
                direct_query(&store, q).expect("直查不报错").is_none(),
                "不应命中: {q}"
            );
        }
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    fn reply_with(fr: Option<&str>, content: &str, tool_calls: Vec<ToolCall>) -> AiReply {
        AiReply {
            content: content.to_string(),
            tool_calls,
            model: Some("gpt-4o".to_string()),
            usage: None,
            reasoning_content: None,
            finish_reason: fr.map(str::to_string),
        }
    }

    /// 声明调用但没有正文或调用字段，与普通空回复分开处理。
    #[test]
    fn missing_tool_call_detects_incomplete_response() {
        assert!(missing_tool_call_response(&reply_with(
            Some("function_call"),
            "",
            vec![]
        )));
        // 部分 provider 用 tool_calls 而非 function_call 表示同一件事
        assert!(missing_tool_call_response(&reply_with(
            Some("tool_calls"),
            "",
            vec![]
        )));
        // 正文其实是空白字符时同样算「没有正文」
        assert!(missing_tool_call_response(&reply_with(
            Some("function_call"),
            "  \n ",
            vec![]
        )));
    }

    /// 误报会让诊断变得不可信——正常的空回复、真实拿到了 tool_calls、
    /// 正常有正文的三种情况都必须安静退出。
    #[test]
    fn missing_tool_call_does_not_misreport_normal_replies() {
        // 模型自己返回空（推理型模型烧光输出预算的常见情形）
        assert!(!missing_tool_call_response(&reply_with(
            Some("stop"),
            "",
            vec![]
        )));
        assert!(!missing_tool_call_response(&reply_with(None, "", vec![])));
        // 真的拿到了工具调用：此时正文为空是正常的
        assert!(!missing_tool_call_response(&reply_with(
            Some("function_call"),
            "",
            vec![ToolCall::new("list_rules", json!({}))]
        )));
        // 有正文就与本签名无关
        assert!(!missing_tool_call_response(&reply_with(
            Some("function_call"),
            "我来帮你查一下。",
            vec![]
        )));
    }

    include!("conversation/continuity_tests.rs");
    include!("conversation/protocol_tests.rs");
    include!("conversation/baseline_tests.rs");
}
