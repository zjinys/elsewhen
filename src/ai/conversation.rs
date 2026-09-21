use super::memory::{
    estimate_tokens, ContextMessage, MemoryProvider, SimpleMemory, SlidingWindowMemory,
};
use super::provider::{AiProvider, OllamaProvider, OpenAiCompatibleProvider, TokenUsage};
use super::tool::{dispatch, execute_pending_action, ToolCall, ToolRegistry};
use crate::storage::{RuleStatus, Store};
use anyhow::{Context, Result};
use serde_json::Value;

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

/// 单次生成中工具循环的最大轮数（防止死循环）
const MAX_TOOL_ROUNDS: usize = 4;

/// 文本协议的工具调用标记（AI 独占一行输出）
const TOOL_CALL_MARKER: &str = "[工具调用]";

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
    let config = config.unwrap_or_default();

    // 1) 确认门：规则提议确认/丢弃 + 写类工具待确认动作执行/丢弃
    let promoted_rules = handle_rule_proposal_confirmation(store, conversation_id)?;
    let executed = handle_pending_action_confirmation(store, conversation_id)?;

    // 2) 构建上下文
    let memory: Box<dyn MemoryProvider> = match config.memory_type {
        MemoryType::Simple { max_messages } => Box::new(SimpleMemory::new(max_messages)),
        MemoryType::SlidingWindow { max_tokens } => Box::new(SlidingWindowMemory::new(max_tokens)),
    };
    // Provider 未返回 usage 时的本地兜底：prompt 按上下文估算
    let mut context = memory.prepare_context(conversation_id, store)?;

    // 用户确认后生效的个人规则：注入，避免 AI 重复提议同一条规则
    if let Some(rules) = promoted_rules {
        context.push(ContextMessage::new(
            "system",
            format!("（内部记录）你刚才提议的个人规则已被用户确认并加入规则库：\n{rules}"),
        ));
    }

    // 用户确认后执行成功的写操作：以内部信息注入，让 AI 在回复中确认结果
    if let Some(summary) = executed {
        context.push(ContextMessage::new(
            "system",
            format!("（内部记录）你刚才提议的写操作已被用户确认并执行成功：\n{summary}"),
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
                store.create_pending_action(
                    conversation_id,
                    "propose_people_relations",
                    &args.to_string(),
                )?;
                return Ok(format!("已识别人物 {} 和事项 {}，并草拟了关系保存内容。回复「好」确认保存，回复「不要」取消。", annotations.people.join("、"), annotations.targets.join("、")));
            }
        }
    }
    let direct_query_result: Option<(String, String)> =
        last_user.and_then(|m| direct_query(store, &m.content));
    if let Some((label, data)) = &direct_query_result {
        context.push(ContextMessage::new(
            "system",
            format!(
                "（本机数据直查：系统已直接查好「{label}」的真实数据，请直接引用回答，无需再调用工具）\n{data}"
            ),
        ));
    }

    // 知识页处理会话：注入当前页内容，AI 围绕该页作答/修订
    if let Some(conversation) = store.get_conversation(conversation_id)? {
        if let Some(slug) = conversation.wiki_page_slug {
            if let Some(page) = store.get_wiki_page(&slug)? {
                context.push(ContextMessage::new(
                    "system",
                    format!(
                        "你正在协助用户处理知识库页面「{}」（slug={}，类型：{}）：
用户会请你总结/改述/补充/提取要点等，需要修改页面时调用 save_wiki_revision 工具（保持 slug 不变、在旧内容基础上修订、不丢失已有事实）。
标签：{}

（处理页面：当前完整内容）
---
{}
---
",
                        page.title,
                        page.slug,
                        page.kind,
                        page.tags.join("、"),
                        page.content_md
                    ),
                ));
            }
        }
    }

    // 3) 创建 AI provider
    let ai_providers: Vec<(Option<String>, Box<dyn AiProvider>)> = match config.provider_type {
        ProviderType::OpenAiCompatible => {
            let configs = store.list_ai_provider_configs_for_runtime()?;
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
                    };
                    Ok((
                        Some(id),
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
                base_url,
                model,
                temperature: 0.7,
            };
            vec![(None, Box::new(OllamaProvider::new(provider_config)?))]
        }
    };

    // 4) Agent 循环（可调工具）
    let registry = ToolRegistry::default();
    let mut errors = Vec::new();
    let mut outcome = None;
    for (provider_id, provider) in ai_providers {
        let mut attempt_context = context.clone();
        match run_agent_loop(
            &*provider,
            &mut attempt_context,
            &registry,
            store,
            conversation_id,
        ) {
            Ok(result) => {
                if let Some(id) = provider_id {
                    store.set_active_ai_provider_config(&id)?;
                }
                context = attempt_context;
                outcome = Some(result);
                break;
            }
            Err(error) => errors.push(error.to_string()),
        }
    }
    let outcome = outcome.context(format!("所有 AI provider 均失败：{}", errors.join(" | ")))?;
    let raw = outcome.content;

    // 5) 解析规则提议：若 AI 在末尾提交了一条规则，剥离标记转为友好提示展示，
    //    并把规则文本暂存为「待确认」，等待用户下一条消息确认后入库生效
    let (mut content, proposed_rule) = parse_rule_proposal(&raw);
    if let Some(rule) = proposed_rule {
        store.add_rule(&rule, RuleStatus::Pending, Some(conversation_id))?;
    }

    // 空回复兜底：绝不把空消息存进对话。
    // 若命中过本机直查，直接把真实数据作为答复；否则给出明确的重试提示。
    if content.trim().is_empty() {
        content = match &direct_query_result {
            Some((label, data)) => {
                format!("（AI 未能生成回复，以下是系统直接查到的「{label}」数据）\n{data}")
            }
            None => "抱歉，模型没有返回内容，请重试一次。".to_string(),
        };
    }

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

    // Save reply to database (no parent_message_id for AI replies in main thread)
    store.send_message(conversation_id, "assistant", &content, None)?;

    Ok(content)
}

/// 工具通信协议：原生 tool-calling 或文本信封
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolProtocol {
    Native,
    Text,
}

/// 本机数据直查：对高频统计/清单类问题直接在本地查库并注入上下文，
/// 不依赖模型的 tool-calling 能力（数据本就存在本地数据库）。
/// 命中返回（查询主题, 已查好的数据文本）。
fn direct_query(store: &Store, user_message: &str) -> Option<(String, String)> {
    let msg = user_message.trim().to_lowercase();

    // token 用量
    let wants_token = msg.contains("token")
        || msg.contains("用量")
        || (msg.contains("统计") && (msg.contains("token") || msg.contains("用量")));
    if wants_token {
        let daily = store.daily_token_usage(7).ok()?;
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
        return Some(("token 用量".to_string(), out));
    }

    // 规则清单
    let wants_rules = msg.contains("规则库")
        || msg.contains("有哪些规则")
        || msg.contains("什么规则")
        || msg.contains("我的规则")
        || msg.contains("规则清单")
        || msg.contains("看看规则");
    if wants_rules {
        let rules = store.list_active_rules().ok()?;
        let mut out = String::from("已生效的个人规则：\n");
        if rules.is_empty() {
            out.push_str("（规则库为空，还没有沉淀过规则）");
        } else {
            for r in rules {
                out.push_str(&format!("- {}\n", r.content));
            }
        }
        return Some(("个人规则库".to_string(), out));
    }

    None
}

/// Agent 循环结果
struct AgentOutcome {
    content: String,
    prompt_tokens: u64,
    completion_tokens: u64,
    model: Option<String>,
}

/// Agent 循环：最多 MAX_TOOL_ROUNDS 轮。
/// - 首轮尝试原生 tool-calling（带 tools 清单）；失败则回落文本协议重试一次。
/// - 原生返回 tool_calls → 回传 assistant(tool_calls) + 追加 tool 结果，进入下一轮。
/// - 纯文本但含 `[工具调用]` 信封 → 追加 assistant + system(结果)，进入下一轮。
/// - 无工具调用 → 收敛，返回最终文本。
fn run_agent_loop(
    provider: &dyn AiProvider,
    context: &mut Vec<ContextMessage>,
    registry: &ToolRegistry,
    store: &Store,
    conversation_id: &str,
) -> Result<AgentOutcome> {
    let mut protocol: Option<ToolProtocol> = None;
    let mut content = String::new();
    let mut prompt_tokens: u64 = 0;
    let mut completion_tokens: u64 = 0;
    let mut model: Option<String> = None;
    let mut last_raw = String::new();

    for _round in 0..MAX_TOOL_ROUNDS {
        let wants_native = match &protocol {
            Some(ToolProtocol::Native) => true,
            Some(ToolProtocol::Text) => false,
            None => true,
        };
        let tools = if wants_native {
            Some(registry.provider_specs())
        } else {
            None
        };

        let reply = match provider.generate_reply_with_tools(context.clone(), tools.as_deref()) {
            Ok(r) => r,
            Err(e) => {
                // 原生协议失败（首次尝试 or 已锁定原生）→ 回落到纯文本协议重试一次：
                // 首轮可能是模型/网关不支持 tools 字段；后续轮可能是原生 tool_calls
                // 回传校验失败（如 arguments 字节不一致）。历史里已有上下文，文本模式仍能组织最终回答。
                if !matches!(protocol, Some(ToolProtocol::Text)) {
                    protocol = Some(ToolProtocol::Text);
                    eprintln!("[agent] 请求失败({e})，回落纯文本协议重试");
                    provider.generate_reply(context.clone())?
                } else {
                    eprintln!("[agent] 纯文本协议请求也失败：{e}");
                    return Err(e);
                }
            }
        };
        // 兼容兜底：部分模型不支持 tools 字段但不报错——返回空 content 且无 tool_calls。
        // 视作原生不可用，去掉 tools 重试一次（数据类问题随后也会走本机直查兜底）。
        let reply = if reply.content.trim().is_empty()
            && reply.tool_calls.is_empty()
            && protocol.is_none()
        {
            eprintln!("[agent] round {_round}: 模型返回空 content 且无 tool_calls，判定不支持原生 tools，无 tools 重试");
            provider.generate_reply(context.clone())?
        } else {
            reply
        };
        last_raw = reply.content.clone();
        if let Some(u) = &reply.usage {
            prompt_tokens += u.prompt_tokens;
            completion_tokens += u.completion_tokens;
        }
        if reply.model.is_some() {
            model = reply.model.clone();
        }
        eprintln!(
            "[agent] round={_round} protocol={:?} content_len={} tool_calls={} model={:?}",
            protocol,
            reply.content.chars().count(),
            reply.tool_calls.len(),
            reply.model
        );

        // 原生 tool-calls：回传 assistant(tool_calls) + 工具结果
        if !reply.tool_calls.is_empty() {
            protocol = Some(ToolProtocol::Native);
            context.push(ContextMessage::assistant_with_tool_calls(
                reply.tool_calls.clone(),
            ));
            for call in &reply.tool_calls {
                let result = dispatch(call, registry, store, conversation_id);
                eprintln!(
                    "[agent] dispatch tool={} -> {}",
                    call.name,
                    &result.content.chars().take(80).collect::<String>()
                );
                context.push(ContextMessage::tool_result(call.id.clone(), result.content));
            }
            continue;
        }

        // 文本信封：assistant 原文 + 工具结果（走 system 角色，避免原生协议对 tool 角色的约束）
        let (clean, calls) = parse_tool_call_envelope(&reply.content);
        if !calls.is_empty() {
            protocol = Some(ToolProtocol::Text);
            context.push(ContextMessage::new("assistant", reply.content));
            for call in &calls {
                let result = dispatch(&call, registry, store, conversation_id);
                eprintln!(
                    "[agent] text-protocol tool={} -> {}",
                    call.name,
                    &result.content.chars().take(80).collect::<String>()
                );
                context.push(ContextMessage::new(
                    "system",
                    format!("工具「{}」执行结果：{}", call.name, result.content),
                ));
            }
            continue;
        }

        // 收敛
        content = clean;
        break;
    }

    if content.is_empty() {
        // 轮数用尽仍未收敛：退回最后一段文本（去掉工具信封残留）
        content = parse_tool_call_envelope(&last_raw).0;
    }

    Ok(AgentOutcome {
        content,
        prompt_tokens,
        completion_tokens,
        model,
    })
}

/// 解析文本信封中的工具调用：返回（剥离信封后的文本, 解析出的调用列表）。
/// 支持一行内多个标记，标记所在行会从文本中移除。
fn parse_tool_call_envelope(content: &str) -> (String, Vec<ToolCall>) {
    if !content.contains(TOOL_CALL_MARKER) {
        return (content.to_string(), Vec::new());
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
    (cur, calls)
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

    Ok(reply.content)
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
    // 兼容「确认一下」「记下来吧」这类短确认句式（确认词≥2 字 + 整体很短）
    t.chars().count() <= 6
        && WORDS
            .iter()
            .any(|w| w.chars().count() >= 2 && t.starts_with(w))
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

/// 处理本会话的规则提议：确认则转正生效并返回规则文本（供注入 AI 上下文）；
/// 明确拒绝则丢弃；其他消息一律不动——避免中间插一句话就把草稿误删。
/// 规则归属会话（v13 起），其他会话的 pending 不受影响。
fn handle_rule_proposal_confirmation(
    store: &Store,
    conversation_id: &str,
) -> Result<Option<String>> {
    let pending = store.list_rules(Some(RuleStatus::Pending), Some(conversation_id))?;
    if pending.is_empty() {
        return Ok(None);
    }
    let last_user = store
        .list_messages(conversation_id)?
        .into_iter()
        .rev()
        .find(|m| m.role == "user");
    match last_user.as_ref().map(|m| m.content.as_str()) {
        Some(msg) if is_confirmation(msg) => {
            store.promote_pending_rules(conversation_id)?;
            let promoted = pending
                .iter()
                .map(|r| r.content.clone())
                .collect::<Vec<_>>()
                .join("\n");
            Ok(Some(promoted))
        }
        Some(msg) if is_declination(msg) => {
            store.discard_pending_rules(conversation_id)?;
            Ok(None)
        }
        _ => Ok(None),
    }
}

/// 处理本会话的待确认写动作：确认则执行（写真源），明确拒绝则丢弃；
/// 其他消息保留待确认（之后回「好」仍可生效）。
/// 返回执行成功后的摘要（供下一轮回复确认用）。
fn handle_pending_action_confirmation(
    store: &Store,
    conversation_id: &str,
) -> Result<Option<String>> {
    let pendings = store.pending_actions_for_conversation(conversation_id)?;
    if pendings.is_empty() {
        return Ok(None);
    }
    let last_user = store
        .list_messages(conversation_id)?
        .into_iter()
        .rev()
        .find(|m| m.role == "user");
    let confirmed = matches!(&last_user, Some(m) if is_confirmation(&m.content));
    let declined = matches!(&last_user, Some(m) if is_declination(&m.content));

    let mut summaries = Vec::new();
    for pa in &pendings {
        if confirmed {
            match execute_pending_action(store, pa) {
                Ok(s) => summaries.push(s),
                Err(e) => summaries.push(format!("「{}」执行失败：{e}", pa.action)),
            }
        }
        if confirmed {
            store.delete_pending_action(&pa.id)?;
        } else if declined {
            if pa.action == "propose_people_relations" {
                store.decline_pending_action(&pa.id)?;
            } else {
                store.delete_pending_action(&pa.id)?;
            }
        }
        // 其他消息：保留待确认
    }
    if summaries.is_empty() {
        Ok(None)
    } else {
        Ok(Some(summaries.join("\n")))
    }
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
    let hint = format!("（建议沉淀成一条个人规则：「{rule}」—— 回复「好」即可加入规则库）");
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
            "确认一下",
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
        // 标记行不是合法 JSON（被换行打断）→ 只剥离能解析的部分，不 panic
        let content = "[工具调用]{\"name\":\"get_wiki_page\"";
        let (clean, calls) = parse_tool_call_envelope(content);
        assert!(calls.is_empty(), "非法 JSON 不应被解析");
        assert_eq!(clean, content);
    }

    #[test]
    fn parse_tool_call_envelope_no_marker() {
        let (clean, calls) = parse_tool_call_envelope("随便聊聊");
        assert!(calls.is_empty());
        assert_eq!(clean, "随便聊聊");
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
            }),
            Ok(AiReply::text("规则库查好了。")),
        ]);

        let outcome = run_agent_loop(&provider, &mut context, &registry, &store, &conv).unwrap();
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

        let outcome = run_agent_loop(&provider, &mut context, &registry, &store, &conv).unwrap();
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
        let outcome = run_agent_loop(&provider, &mut context, &registry, &store, &conv).unwrap();
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

        let outcome = run_agent_loop(&provider, &mut context, &registry, &store, &conv).unwrap();
        assert_eq!(outcome.content, "最近没有 AI 调用记录。");
        assert!(provider.saw_tools_on_first_call(), "首轮仍应尝试原生工具");
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn pending_action_confirmation_executes_on_yes() {
        let (store, path) = temporary_database();
        let conv = store.create_conversation(Some("t"), None).unwrap();
        store
            .create_pending_action(
                &conv,
                "record_event",
                &json!({"raw_text": "和张玮确认双链路付款的分工"}).to_string(),
            )
            .unwrap();
        // 用户确认
        store.send_message(&conv, "user", "好", None).unwrap();

        let summary = handle_pending_action_confirmation(&store, &conv).unwrap();
        let summary = summary.expect("确认后应返回执行摘要");
        assert!(summary.contains("已保存事件"), "{summary}");
        // 真源已写入，pending 已清空
        assert_eq!(store.list_events().unwrap().len(), 1);
        assert!(store
            .pending_actions_for_conversation(&conv)
            .unwrap()
            .is_empty());
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn pending_action_kept_on_interleaved_message() {
        let (store, path) = temporary_database();
        let conv = store.create_conversation(Some("t"), None).unwrap();
        store
            .create_pending_action(
                &conv,
                "record_event",
                &json!({"raw_text": "草拟的事件"}).to_string(),
            )
            .unwrap();
        // 用户夹了一条普通消息，草稿不应被误删
        store
            .send_message(&conv, "user", "看看我今天都做了什么", None)
            .unwrap();

        let summary = handle_pending_action_confirmation(&store, &conv).unwrap();
        assert!(summary.is_none(), "未确认不应执行");
        assert!(store.list_events().unwrap().is_empty(), "真源不应被写入");
        assert_eq!(
            store.pending_actions_for_conversation(&conv).unwrap().len(),
            1,
            "穿插的普通消息不应删除待确认动作"
        );
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn pending_action_confirmed_after_interleaved_message() {
        // 回归：草拟 → 中间夹一条普通消息 → 再回「好」，仍应正确执行。
        // 这正是「回了好却像没识别」的根因场景。
        let (store, path) = temporary_database();
        let conv = store.create_conversation(Some("t"), None).unwrap();
        store
            .create_pending_action(
                &conv,
                "record_event",
                &json!({"raw_text": "等对方回电如果当天没回复第二天发消息跟进"}).to_string(),
            )
            .unwrap();
        store
            .send_message(&conv, "user", "中间插了一句别的事情", None)
            .unwrap();
        store.send_message(&conv, "user", "好", None).unwrap();

        let summary = handle_pending_action_confirmation(&store, &conv).unwrap();
        let summary = summary.expect("确认后应执行");
        assert!(summary.contains("已保存事件"), "{summary}");
        assert_eq!(store.list_events().unwrap().len(), 1);
        assert!(store
            .pending_actions_for_conversation(&conv)
            .unwrap()
            .is_empty());
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn pending_action_discards_only_on_declination() {
        let (store, path) = temporary_database();
        let conv = store.create_conversation(Some("t"), None).unwrap();
        store
            .create_pending_action(
                &conv,
                "record_event",
                &json!({"raw_text": "草拟的事件"}).to_string(),
            )
            .unwrap();
        store
            .send_message(&conv, "user", "不用了，先放着", None)
            .unwrap();

        let summary = handle_pending_action_confirmation(&store, &conv).unwrap();
        assert!(summary.is_none());
        assert!(
            store.list_events().unwrap().is_empty(),
            "拒绝后不应写入真源"
        );
        assert!(
            store
                .pending_actions_for_conversation(&conv)
                .unwrap()
                .is_empty(),
            "拒绝应清掉草稿"
        );
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn rule_confirmation_promotes_own_conversation_only() {
        let (store, path) = temporary_database();
        let conv_a = store.create_conversation(Some("a"), None).unwrap();
        let conv_b = store.create_conversation(Some("b"), None).unwrap();
        store
            .add_rule(
                "A 会话的规则",
                crate::storage::RuleStatus::Pending,
                Some(&conv_a),
            )
            .unwrap();
        store
            .add_rule(
                "B 会话的规则",
                crate::storage::RuleStatus::Pending,
                Some(&conv_b),
            )
            .unwrap();
        store.send_message(&conv_a, "user", "好", None).unwrap();

        let promoted = handle_rule_proposal_confirmation(&store, &conv_a).unwrap();
        let promoted = promoted.expect("A 会话确认应转正规则");
        assert!(promoted.contains("A 会话的规则"), "{promoted}");
        assert!(
            !promoted.contains("B 会话的规则"),
            "不应包含别会话的规则: {promoted}"
        );

        let still_pending = store
            .list_rules(Some(crate::storage::RuleStatus::Pending), None)
            .unwrap();
        assert_eq!(still_pending.len(), 1, "B 会话的规则不应被误转正");
        assert_eq!(still_pending[0].content, "B 会话的规则");
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn rule_confirmation_kept_on_interleaved_message() {
        // 回归：提议 → 中间夹普通消息（旧逻辑会误删 pending）→ 之后再回「好」仍应转正
        let (store, path) = temporary_database();
        let conv = store.create_conversation(Some("t"), None).unwrap();
        store
            .add_rule(
                "暂存待确认的规则",
                crate::storage::RuleStatus::Pending,
                Some(&conv),
            )
            .unwrap();
        store
            .send_message(&conv, "user", "我看看今天的安排", None)
            .unwrap();

        let promoted = handle_rule_proposal_confirmation(&store, &conv).unwrap();
        assert!(promoted.is_none(), "普通消息不应触发转正");
        assert_eq!(
            store
                .list_rules(Some(crate::storage::RuleStatus::Pending), Some(&conv))
                .unwrap()
                .len(),
            1,
            "穿插消息不应误删待确认规则"
        );

        store.send_message(&conv, "user", "好", None).unwrap();
        let promoted = handle_rule_proposal_confirmation(&store, &conv).unwrap();
        assert!(promoted.is_some(), "之后回「好」仍应转正");
        assert!(store
            .list_rules(Some(crate::storage::RuleStatus::Pending), Some(&conv))
            .unwrap()
            .is_empty());
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn direct_query_matches_token_usage_question() {
        let (store, path) = temporary_database();
        store
            .record_token_usage(None, 100, 50, 150, Some("fake-model"))
            .unwrap();
        let hit = direct_query(&store, "我最近 token 用了多少？").expect("应命中 token 直查");
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
        let hit = direct_query(&store, "我的规则库里现在有哪些规则？").expect("应命中规则直查");
        assert_eq!(hit.0, "个人规则库");
        assert!(hit.1.contains("留痕"), "{}", hit.1);
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn direct_query_ignores_normal_questions() {
        let (store, path) = temporary_database();
        for q in [
            "张玮有和太极沟通吗",
            "帮我记录一下今天的事",
            "把这条存进知识库",
        ] {
            assert!(direct_query(&store, q).is_none(), "不应命中: {q}");
        }
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn agent_loop_retries_without_tools_on_empty_reply() {
        let (store, path) = temporary_database();
        let conv = store.create_conversation(Some("t"), None).unwrap();
        let mut context = vec![ContextMessage::new("user", "统计一下 token 用量")];
        let registry = ToolRegistry::default();

        // 第一轮（带 tools）：模型不支持 tools 但不报错 → 空 content 无 tool_calls；
        // 第二轮（无 tools）：正常文本
        let provider = ScriptedProvider::new(vec![
            Ok(AiReply::text("")),
            Ok(AiReply::text("最近 7 天没有 AI 调用记录。")),
        ]);

        let outcome = run_agent_loop(&provider, &mut context, &registry, &store, &conv).unwrap();
        assert_eq!(outcome.content, "最近 7 天没有 AI 调用记录。");
        assert!(provider.saw_tools_on_first_call(), "首轮仍应尝试原生工具");
        drop(store);
        let _ = std::fs::remove_file(path);
    }
}
