//! MemoryProvider trait 与 Simple/SlidingWindow 实现、token 压缩。

use super::*;

/// Memory provider trait for context management
pub trait MemoryProvider {
    /// Prepare context messages from conversation history
    fn prepare_context(&self, conversation_id: &str, store: &Store) -> Result<Vec<ContextMessage>>;
}

/// Simple memory: last N messages
pub struct SimpleMemory {
    pub(crate) max_messages: usize,
}

impl SimpleMemory {
    pub fn new(max_messages: usize) -> Self {
        Self { max_messages }
    }
}

impl MemoryProvider for SimpleMemory {
    fn prepare_context(&self, conversation_id: &str, store: &Store) -> Result<Vec<ContextMessage>> {
        let messages = store.list_messages(conversation_id)?;

        // The count is a soft limit: keep the whole user exchange that crosses
        // it rather than starting the retained history inside a tool round.
        let desired = messages.len().saturating_sub(self.max_messages);
        let start = messages[..desired.min(messages.len())]
            .iter()
            .rposition(|message| message.role == "user")
            .filter(|_| {
                messages
                    .get(desired)
                    .is_none_or(|message| message.role != "user")
            })
            .unwrap_or(desired);
        let mut context: Vec<ContextMessage> = messages
            .into_iter()
            .skip(start)
            .map(ContextMessage::from_record)
            .collect();

        // 角色设定置于最前，防止 AI 泛化成通用聊天助手
        context.splice(0..0, build_system_context(store, conversation_id)?);
        let mut next_index = 1usize;
        if let Some(background) = recent_background(store, conversation_id)? {
            context.insert(next_index, background);
            next_index += 1;
        }
        if let Some(today) = today_memory_background(store, conversation_id)? {
            context.insert(next_index, today);
        }

        Ok(context)
    }
}

/// Sliding window memory: token-limited context
pub struct SlidingWindowMemory {
    pub(crate) max_tokens: usize,
    pub(crate) model: String,
}

/// Local BPE accounting for memory selection; each provider separately counts
/// the complete serialized request using its tokenizer family.
pub fn estimate_tokens(text: &str) -> usize {
    tiktoken_rs::cl100k_base_singleton()
        .encode_ordinary(text)
        .len()
}

/// Compact older conversation history using a preliminary memory estimate.
/// Required system context and the latest exchange are never truncated; the
/// provider enforces the actual budget on the complete serialized request.
pub fn compress_context(context: &mut Vec<ContextMessage>, max_tokens: usize) {
    compress_context_for_model(context, max_tokens, "gpt-4");
}

/// R5 确定性压形：装不下的较早历史轮降级为「骨架」——assistant 长回复
/// 留头 220 字、工具结果留头 160 字，用户消息与协议字段（tool_calls 的
/// id/name 配对）原样。不做模型调用（前台零延迟）；真正的语义纪要归
/// 翻篇摘要管线（apply_summary → PageMark）。压形后 content 与库内原文
/// 不再逐字一致，摘要管线的 complete_originals 护栏会自然跳过这些轮，
/// 避免把压形文本当全源再摘要。
fn degrade_turn(
    turn: &[ContextMessage],
    estimate_tokens: &dyn Fn(&str) -> usize,
) -> Vec<ContextMessage> {
    turn.iter()
        .map(|message| {
            if message.role == "user" || estimate_tokens(&message.content) < 400 {
                return message.clone();
            }
            let keep = if message.role == "tool" { 160 } else { 220 };
            let mut content: String = message.content.chars().take(keep).collect();
            content.push_str("…[后文已按预算压缩，完整原文见会话记录]");
            let mut degraded = message.clone();
            degraded.content = content;
            degraded
        })
        .collect()
}

pub(crate) fn compress_context_for_model(
    context: &mut Vec<ContextMessage>,
    max_tokens: usize,
    model: &str,
) {
    // Never make malformed native history appear valid by dropping its bad turn.
    // Preserve it intact so the provider's final gate reports the protocol error.
    if crate::ai::budget::validate_context_exchanges(context).is_err() {
        return;
    }
    let estimate_tokens = |text: &str| crate::ai::budget::text_tokens(model, text);
    if max_tokens == 0 {
        return;
    }
    let total: usize = context.iter().map(|m| estimate_tokens(&m.content)).sum();
    if total <= max_tokens {
        return;
    }
    let mut omissions: Vec<_> = context
        .iter_mut()
        .flat_map(|message| std::mem::take(&mut message.selection_omissions))
        .collect();
    // Optional background is always sacrificed before original conversation turns.
    context.retain(|message| {
        if message.optional_background {
            segments::note_omission(
                &mut omissions,
                &message.segment,
                model,
                estimate_tokens(&message.content),
                "preselection_optional_background",
            );
            false
        } else {
            true
        }
    });
    if context
        .iter()
        .map(|message| estimate_tokens(&message.content))
        .sum::<usize>()
        <= max_tokens
    {
        if let Some(first) = context.first_mut() {
            first.selection_omissions = omissions;
        }
        return;
    }

    let mut systems = context.iter().filter(|m| m.role == "system");
    let Some(primary) = systems.next().cloned() else {
        if let Some(first) = context.first_mut() {
            first.selection_omissions = omissions;
        }
        return;
    };
    let mut history: Vec<_> = context
        .iter()
        .filter(|m| m.role != "system")
        .cloned()
        .collect();
    // The latest user request is non-negotiable; never let a long page body
    // or a subsequent system injection displace it.
    let latest_user = history
        .iter()
        .rposition(|m| m.role == "user")
        .unwrap_or(history.len());
    let current = history.split_off(latest_user);
    let current_tokens: usize = current.iter().map(|m| estimate_tokens(&m.content)).sum();
    let mut remaining = max_tokens
        .saturating_sub(estimate_tokens(&primary.content))
        .saturating_sub(current_tokens);
    let mut rebuilt = vec![primary];
    let omitted_marker = "（较早对话因预算已省略）";
    let marker_budget = if history.is_empty() {
        0
    } else {
        estimate_tokens(omitted_marker)
    };
    remaining = remaining.saturating_sub(marker_budget);

    // Never truncate source evidence, rules or execution results. Optional
    // background is removed at the complete, provider-specific request boundary.
    for source in systems {
        remaining = remaining.saturating_sub(estimate_tokens(&source.content));
        rebuilt.push(source.clone());
    }

    // Keep complete user exchanges. An assistant's native calls and their tool
    // results must never be separated by preliminary memory selection.
    //
    // 10-19 档案 P0-b（根因 A）：最近一个完整交换无条件保留（加上当前
    // 轮即 fit 侧「最近两轮不可省略」的同一口径），不参与预算装填。出事
    // 会话里 preselection 只留 7 条，模型不承认 3 分钟前刚做的确认
    // （invisible turn，run 还标 completed）。装不下就整体交给 fit 层
    // fail-fast（诚实失败），好过静默失忆后幻觉。
    let mut recent = Vec::new();
    for _ in 0..1 {
        if history.is_empty() {
            break;
        }
        let start = history.iter().rposition(|m| m.role == "user").unwrap_or(0);
        recent.push(history.split_off(start));
    }
    let mut kept = Vec::new();
    while !history.is_empty() {
        let start = history.iter().rposition(|m| m.role == "user").unwrap_or(0);
        let cost: usize = history[start..]
            .iter()
            .map(|m| {
                estimate_tokens(&m.content)
                    + m.tool_calls.as_ref().map_or(0, |calls| {
                        calls
                            .iter()
                            .map(|call| {
                                estimate_tokens(&call.name)
                                    + estimate_tokens(&call.arguments.to_string())
                                    + estimate_tokens(&call.id)
                            })
                            .sum::<usize>()
                    })
                    + m.reasoning_content
                        .as_ref()
                        .map_or(0, |text| estimate_tokens(text))
            })
            .sum();
        if cost > remaining {
            // R5：先尝试确定性压形再放弃——assistant/工具长消息大头被
            // 摘成骨架后常常能装下，模型在意的「做过什么」维度得以保留；
            // 仍装不下才整体交给 fit 层 fail-fast（诚实失败）。
            let tail = &history[start..];
            let degraded = degrade_turn(tail, &estimate_tokens);
            let degraded_cost: usize = degraded.iter().map(|m| estimate_tokens(&m.content)).sum();
            if degraded_cost >= cost || degraded_cost > remaining {
                break;
            }
            for message in tail {
                segments::note_omission(
                    &mut omissions,
                    &message.segment,
                    model,
                    estimate_tokens(&message.content),
                    "preselection_degraded_turn",
                );
            }
            kept.push(degraded);
            history.truncate(start);
            break;
        }
        remaining -= cost;
        kept.push(history.split_off(start));
    }
    if !history.is_empty() {
        for message in &history {
            segments::note_omission(
                &mut omissions,
                &message.segment,
                model,
                estimate_tokens(&message.content),
                "preselection_earlier_turn",
            );
        }
        // R8B：占位符显式给出被裁规模，模型知道「哪些内容不在」，不凭
        // 印象补全；截选不是语义摘要——语义纪要归 C06 翻篇管线。
        let omitted_marker = format!(
            "（前文省略：{} 条较早的历史消息未装入本轮上下文；只保留了你最近的活动，请勿凭记忆补全这段被省略的内容，被问到时应说明记不全）",
            history.len()
        );
        rebuilt.push(ContextMessage::new("system", omitted_marker).with_segment(
            SegmentPurpose::BudgetNotice,
            SegmentOrigin::Application,
            None,
            OmissionPolicy::Required,
        ));
    }
    for turn in kept.into_iter().rev() {
        rebuilt.extend(turn);
    }
    // recent[0] 是更早的交换、recent[1]（若有）紧贴当前轮，按时间序展开。
    for turn in recent.into_iter().rev() {
        rebuilt.extend(turn);
    }
    rebuilt.extend(current);
    if let Some(first) = rebuilt.first_mut() {
        first.selection_omissions = omissions;
    }
    *context = rebuilt;
}

impl SlidingWindowMemory {
    pub fn new(max_tokens: usize) -> Self {
        Self::for_model(max_tokens, "gpt-4")
    }

    pub(crate) fn for_model(max_tokens: usize, model: &str) -> Self {
        Self {
            max_tokens,
            model: model.to_owned(),
        }
    }

    /// Token 估算（ASCII 约 4 字符/token，CJK 约 2 字符/token）
    pub(crate) fn estimate_tokens(text: &str) -> usize {
        estimate_tokens(text)
    }
}

pub(crate) fn prepare_uncompressed_context(
    conversation_id: &str,
    store: &Store,
) -> Result<Vec<ContextMessage>> {
    let messages = store.list_messages(conversation_id)?;
    // 翻篇（P2 手动 / P3 自动档）：生效标记边界前的原文确定性跳过，
    // 摘要注入 system 之后、历史之前。边界消息若已不在会话中（被删/换
    // 设备截断），标记视为失效——退回全原文并落事件，不静默吞历史。
    let mut page_mark = store.active_page_mark(conversation_id)?;
    if let Some(mark) = page_mark.take() {
        let boundary_present = messages.iter().any(|m| m.id == mark.boundary_message_id);
        if boundary_present {
            page_mark = Some(mark);
        } else {
            crate::ai::generation::event(
                store,
                "page_mark_stale",
                json!({"mark_id":mark.id,"reason":"boundary_message_missing"}),
            )?;
        }
    }
    let history_start = page_mark
        .as_ref()
        .and_then(|mark| {
            messages
                .iter()
                .position(|m| m.id == mark.boundary_message_id)
                .map(|position| position + 1)
        })
        .unwrap_or(0);
    let mut context: Vec<_> = messages
        .into_iter()
        .skip(history_start)
        .map(ContextMessage::from_record)
        .collect();
    context.splice(0..0, build_system_context(store, conversation_id)?);
    if let Some(mark) = &page_mark {
        match crate::ai::summary::render_mark_text(store, mark) {
            Ok((text, _)) => {
                context.insert(
                    1.min(context.len()),
                    ContextMessage::new("system", text).with_segment(
                        SegmentPurpose::Summary,
                        SegmentOrigin::Derived,
                        Some(&mark.summary_node_id),
                        OmissionPolicy::Required,
                    ),
                );
            }
            Err(error) => {
                // 摘要失效（节点被删/覆盖不合法）不炸主链路：退回原文。
                crate::ai::generation::event(
                    store,
                    "page_mark_stale",
                    json!({"mark_id":mark.id,"reason":"summary_unusable","detail":error.to_string()}),
                )?;
            }
        }
    }
    // 动态背景段推到尾部（历史之后）：它们内容每轮都可能变，若插在
    // system 与历史之间，任何变化都会使其后的全部 KV 缓存失效——挪到
    // 尾部后，system+历史成为逐轮可复用的稳定前缀（provider 前缀缓存）。
    if let Some(background) = recent_background(store, conversation_id)? {
        context.push(background);
    }
    if let Some(today) = today_memory_background(store, conversation_id)? {
        context.push(today);
    }
    Ok(context)
}

impl MemoryProvider for SlidingWindowMemory {
    fn prepare_context(&self, conversation_id: &str, store: &Store) -> Result<Vec<ContextMessage>> {
        let messages = store.list_messages(conversation_id)?;

        let mut context: Vec<_> = messages
            .into_iter()
            .map(ContextMessage::from_record)
            .collect();
        context.splice(0..0, build_system_context(store, conversation_id)?);
        // 同 prepare_uncompressed_context：动态背景推尾部，护住前缀缓存。
        if let Some(background) = recent_background(store, conversation_id)? {
            context.push(background);
        }
        if let Some(today) = today_memory_background(store, conversation_id)? {
            context.push(today);
        }
        compress_context_for_model(&mut context, self.max_tokens, &self.model);

        Ok(context)
    }
}
