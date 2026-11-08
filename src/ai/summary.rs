//! Budget-triggered, non-destructive conversation summaries. Fixed source
//! chunks and four-way merge nodes are cached separately so interrupted work
//! can resume without claiming that an incomplete prefix has been summarized.
use super::{
    memory::{ContextMessage, OmissionPolicy, SegmentOrigin, SegmentPurpose},
    provider::{AiProvider, AiReply},
};
use crate::storage::{
    MessageSummary, Store,
    chat_summaries::{HistorySpan, SummaryBody, SummaryNode, message_basis, validate_history_path},
    knowledge::content_hash,
};
use anyhow::{Context, Result, ensure};
use rusqlite::OptionalExtension;
use serde_json::json;
use std::{collections::HashSet, time::Instant};

const STRATEGY: &str = "chat-summary-v2-40cd9207574f1da8";
const CHUNK_CHARS: usize = 2048;
// P1（2026-10-19 摘要事故诊断）：340 条消息冷启动需约 163 个节点，
// 16 请求/轮意味着 11 个生成轮且用户侧全程零可见收益（预案一诊断）。
// 64 让冷启动约 3 轮建出根节点，随后回落稳态——auto mark 滚动后每轮
// 只需个位数调用。后台预热（background_summary_preheat）复用同上限。
const MAX_REQUESTS: usize = 64;
const MAX_SPANS: usize = 8192;
const NODE_TOKEN_LIMIT: usize = 1800;
const GROUP_TOKEN_LIMIT: usize = 12000;

#[derive(Clone)]
struct LeafPart {
    span: HistorySpan,
    role: String,
    created_at: String,
    text: String,
}

impl LeafPart {
    fn data(&self) -> serde_json::Value {
        json!({"message_id":self.span.message_id,"role":self.role,"created_at":self.created_at,
            "start_char":self.span.start_char,"end_char":self.span.end_char,"original_text":self.text})
    }
}

fn split_history(messages: &[MessageSummary]) -> Result<Vec<LeafPart>> {
    let mut parts = Vec::new();
    for message in messages {
        let chars: Vec<char> = message.content.chars().collect();
        let basis = message_basis(message);
        // Even an empty original has an explicit covered identity.
        let chunks = chars.chunks(CHUNK_CHARS).count().max(1);
        for index in 0..chunks {
            ensure!(parts.len() < MAX_SPANS, "历史摘要范围超出本轮有界选材上限");
            let start = index * CHUNK_CHARS;
            let end = (start + CHUNK_CHARS).min(chars.len());
            parts.push(LeafPart {
                span: HistorySpan {
                    message_id: message.id.clone(),
                    start_char: start,
                    end_char: end,
                    message_basis: basis.clone(),
                },
                role: message.role.clone(),
                created_at: message.created_at.clone(),
                text: chars[start..end].iter().collect(),
            });
        }
    }
    Ok(parts)
}

fn parse_body(reply: &AiReply, model: &str) -> Result<SummaryBody> {
    ensure!(reply.tool_calls.is_empty(), "摘要请求不能调用工具");
    ensure!(
        !matches!(
            reply.finish_reason.as_deref(),
            Some("length" | "tool_calls" | "function_call")
        ),
        "摘要响应未完整结束"
    );
    ensure!(reply.content.len() <= 65536, "摘要响应过大");
    let body: SummaryBody =
        serde_json::from_str(reply.content.trim()).context("摘要响应不是完整结构化 JSON")?;
    body.validate()?;
    ensure!(
        super::budget::text_tokens(model, &serde_json::to_string(&body)?) <= NODE_TOKEN_LIMIT,
        "摘要响应超过节点预算"
    );
    Ok(body)
}

struct Builder<'a> {
    store: &'a Store,
    conversation: &'a str,
    provider: &'a dyn AiProvider,
    provider_name: &'a str,
    provider_basis: &'a str,
    model: &'a str,
    requests: usize,
    /// 整次摘要的时间预算（issue 2026-10-31 手机实测：507 条会话冷启动摘要
    /// 要几十个请求，手机网络下每个数十秒，生成路径上现场摘要拖过 8 分钟，
    /// 用户只看到「没有回复」。预算超时放弃本轮摘要（回复照常，带降级标注），
    /// 让后台预热慢慢把节点补齐，下次生成就能命中缓存。
    started: std::time::Instant,
    cache_hits: usize,
}

impl Builder<'_> {
    fn strategy(&self) -> Result<String> {
        Ok(format!(
            "{STRATEGY}:{}",
            self.store.prompt_dependency(&[
                "summary.policy",
                "summary.fixed_contract",
                "task.processing.fixed_contract"
            ])?
        ))
    }

    fn node(
        &mut self,
        spans: Vec<HistorySpan>,
        children: Vec<String>,
        input: serde_json::Value,
        compact: bool,
    ) -> Result<SummaryNode> {
        super::generation::check(self.store)?;
        // 时间预算止损（issue 2026-10-31）：生成路径上现场摘要若被慢网络
        // 拖住，主回复会跟着无限等待。超预算即放弃本轮摘要，回复照常
        // 生成并带降级标注；后台预热会继续补节点，下次命中缓存。
        const SUMMARY_TIME_BUDGET: std::time::Duration = std::time::Duration::from_secs(45);
        if self.started.elapsed() > SUMMARY_TIME_BUDGET {
            return Err(anyhow::anyhow!(
                "摘要时间预算已到（{:?}，{} 请求），放弃本轮等待后台预热",
                self.started.elapsed(),
                self.requests
            ));
        }
        let basis_hash =
            content_hash(&json!({"spans":spans,"children":children,"compact":compact,"prompt_dependency":self.store.prompt_dependency(&["summary.policy","summary.fixed_contract","task.processing.fixed_contract"])?}).to_string());
        let strategy = self.strategy()?;
        if let Some(node) = self.store.summary_node(
            self.conversation,
            &basis_hash,
            &strategy,
            self.provider_basis,
        )? {
            self.cache_hits += 1;
            return Ok(node);
        }
        ensure!(
            self.requests < MAX_REQUESTS,
            "本轮摘要请求上限已到，完整覆盖尚未就绪"
        );
        self.requests += 1;
        let max_chars = if compact {
            200
        } else if super::budget::text_tokens(self.model, &"测".repeat(100)) > 200 {
            350
        } else {
            900
        };
        let messages = vec![
            ContextMessage::new(
                "system",
                format!(
                    "{}\n{}\n{}\n{}",
                    self.store.prompt_for_run("summary.policy")?,
                    self.store.prompt_for_run("summary.fixed_contract")?,
                    self.store.render_processing_instruction(
                        "summary_limit",
                        &[("max", max_chars.to_string())]
                    )?,
                    self.store.output_language_directive(false)?
                ),
            ),
            ContextMessage::new("user", serde_json::to_string(&input)?),
        ];
        let started = Instant::now();
        let reply = super::generation::auxiliary_request(
            self.store,
            self.provider,
            self.provider_name,
            messages,
        )
        .inspect_err(|error| {
            crate::logging::log_always(&format!(
                "[summary] 请求 #{} 失败（{:?}）：{error:#}",
                self.requests,
                started.elapsed()
            ));
        })?;
        crate::logging::log_always(&format!(
            "[summary] 请求 #{} 完成，耗时 {:?}",
            self.requests,
            started.elapsed()
        ));
        let body = parse_body(&reply, self.model)?;
        let node = SummaryNode {
            id: content_hash(
                &json!([self.conversation, basis_hash, strategy, self.provider_basis]).to_string(),
            ),
            conversation_id: self.conversation.into(),
            basis_hash,
            spans,
            children,
            strategy,
            provider_basis: self.provider_basis.into(),
            model: reply.model,
            body,
            prompt_tokens: reply.usage.as_ref().map(|usage| usage.prompt_tokens),
            completion_tokens: reply.usage.as_ref().map(|usage| usage.completion_tokens),
            elapsed_ms: started.elapsed().as_millis().min(i64::MAX as u128) as u64,
        };
        self.store.commit_summary_node(&node)?;
        Ok(node)
    }

    fn build(&mut self, messages: &[MessageSummary]) -> Result<SummaryNode> {
        let parts = split_history(messages)?;
        let mut nodes = Vec::new();
        let mut group = Vec::new();
        let mut group_tokens = 0;
        for part in parts {
            let cost = super::budget::text_tokens(self.model, &part.data().to_string());
            if !group.is_empty() && (group.len() == 4 || group_tokens + cost > GROUP_TOKEN_LIMIT) {
                nodes.push(self.leaf(std::mem::take(&mut group))?);
                group_tokens = 0;
            }
            group_tokens += cost;
            group.push(part);
        }
        if !group.is_empty() {
            nodes.push(self.leaf(group)?);
        }
        ensure!(!nodes.is_empty(), "没有可摘要的历史");
        while nodes.len() > 1 {
            let mut merged = Vec::new();
            for group in nodes.chunks(4) {
                if group.len() == 1 {
                    merged.push(group[0].clone());
                } else {
                    merged.push(self.merge(group, false)?);
                }
            }
            nodes = merged;
        }
        Ok(nodes.remove(0))
    }

    fn leaf(&mut self, group: Vec<LeafPart>) -> Result<SummaryNode> {
        self.node(group.iter().map(|part| part.span.clone()).collect(), Vec::new(),
            json!({"kind":"original_history_spans","parts":group.iter().map(LeafPart::data).collect::<Vec<_>>()}), false)
    }

    fn merge(&mut self, nodes: &[SummaryNode], compact: bool) -> Result<SummaryNode> {
        let strategy = self.strategy()?;
        ensure!(
            nodes.iter().all(|node| node.strategy == strategy),
            "summary_language_strategy_mismatch"
        );
        self.node(nodes.iter().flat_map(|node| node.spans.clone()).collect(),
            nodes.iter().map(|node| node.id.clone()).collect(),
            json!({"kind":"generated_summaries_not_original_text","parts":nodes.iter().map(|node| json!({"summary_id":node.id,"body":node.body})).collect::<Vec<_>>()}), compact)
    }
}

fn summary_context(store: &Store, node: &SummaryNode) -> Result<ContextMessage> {
    let first = node.spans.first().context("摘要范围为空")?;
    let last = node.spans.last().context("摘要范围为空")?;
    Ok(ContextMessage::new(
        "system",
        store.render_execution_instruction(
            "summary_context",
            &[
                ("id", node.id.clone()),
                ("first", first.message_id.clone()),
                ("last", last.message_id.clone()),
                ("count", node.spans.len().to_string()),
                ("body", serde_json::to_string(&node.body)?),
            ],
        )?,
    )
    .with_segment(
        SegmentPurpose::Summary,
        SegmentOrigin::Derived,
        Some(&node.id),
        OmissionPolicy::Required,
    ))
}

/// 翻篇摘要的确认前预览/编辑渲染：override 优先，节点原文本兜底。
/// 返回 (注入文本, SummaryBody JSON)。span 对活体消息校验失败即报错——
/// 摘要与历史配套是硬前提，不配套宁可让装配退回原文。
pub(crate) fn render_mark_text(
    store: &Store,
    mark: &crate::storage::PageMark,
) -> Result<(String, String)> {
    let node = store
        .summary_node_by_id(&mark.conversation_id, &mark.summary_node_id)?
        .context("翻篇标记引用的摘要节点不存在")?;
    store.validate_history_spans(&mark.conversation_id, &node.spans)?;
    let body = match &mark.override_json {
        Some(json) => {
            let body: SummaryBody =
                serde_json::from_str(json).context("标记覆盖不是合法摘要 JSON")?;
            body.validate().context("标记覆盖未通过摘要校验")?;
            body
        }
        None => node.body.clone(),
    };
    let body_json = serde_json::to_string(&body).context("摘要序列化失败")?;
    let first = node.spans.first().context("摘要范围为空")?;
    let last = node.spans.last().context("摘要范围为空")?;
    let text = store.render_execution_instruction(
        "summary_mark",
        &[
            ("id", node.id.clone()),
            ("first", first.message_id.clone()),
            ("last", last.message_id.clone()),
            ("count", node.spans.len().to_string()),
            ("body", body_json.clone()),
        ],
    )?;
    Ok((text, body_json))
}

/// 手动/自动翻篇：走 chat-summary-v1 同一条生成与缓存管道产出边界摘要
/// 摘要节点，再落 PageMark。boundary 及更早的消息被标记覆盖。
#[allow(clippy::too_many_arguments)]
pub(crate) fn generate_page_mark(
    store: &Store,
    conversation: &str,
    boundary_message_id: &str,
    provider: &dyn AiProvider,
    provider_name: &str,
    provider_basis: &str,
    model: &str,
    kind: &str,
) -> Result<String> {
    let history = store.list_messages(conversation)?;
    let dependency = store.prompt_dependency(&[
        "summary.policy",
        "summary.fixed_contract",
        "task.processing.fixed_contract",
    ])?;
    let _prompt_scope = store.prompt_scope(
        "background_task",
        &format!(
            "summary:{conversation}:{provider_basis}:{STRATEGY}:{}:{dependency}",
            history.last().map(|m| m.id.as_str()).unwrap_or("empty")
        ),
    )?;
    validate_history_path(&history)?;
    let Some(position) = history.iter().position(|m| m.id == boundary_message_id) else {
        anyhow::bail!("边界消息不在当前会话中");
    };
    let selected = &history[..=position];
    ensure!(
        selected.iter().any(|m| m.role == "user"),
        "翻篇边界前没有用户消息，无可摘要内容"
    );
    let mut builder = Builder {
        store,
        conversation,
        provider,
        provider_name,
        provider_basis,
        model,
        started: std::time::Instant::now(),
        requests: 0,
        cache_hits: 0,
    };
    let node = builder.build(selected)?;
    // 节点覆盖必须与消息链配套（边界前全覆盖、无重叠空洞）。
    store.validate_history_spans(conversation, &node.spans)?;
    let mark_id = store.create_page_mark(conversation, boundary_message_id, &node.id, kind)?;
    super::generation::event(
        store,
        "page_marked",
        json!({"mark_id":mark_id,"boundary":boundary_message_id,"kind":kind,"summary_id":node.id,"requests":builder.requests,"cache_hits":builder.cache_hits,"covered_messages":selected.len()}),
    )?;
    Ok(mark_id)
}

/// Receives uncompressed original history plus dynamic required context. The
/// caller still runs C05 selection and the complete provider payload gate.
/// 一次 apply 的计量快照：请求/命中数与成败。背景预热用它决定「这棵
/// 树还要不要继续推下一批」（P1 修复 c）；生成链路调用方直接丢弃。
#[derive(Debug, Clone, Copy)]
pub(crate) struct SummaryProgress {
    pub requests: usize,
    pub cache_hits: usize,
    /// build 失败（撞顶或结构错误）；失败细节已由 P0-a 事件留痕。
    pub failed: bool,
}

pub(crate) fn apply_summary(
    store: &Store,
    conversation: &str,
    provider: &dyn AiProvider,
    provider_name: &str,
    provider_basis: &str,
    model: &str,
    budget: usize,
    context: &mut Vec<ContextMessage>,
) -> Result<SummaryProgress> {
    let basis = crate::storage::knowledge::content_hash(
        &context
            .iter()
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
    );
    // The history basis is the logical background task. Retries and process
    // restarts resume its immutable bundle; foreground runs inherit their own.
    let _prompt_scope = store.prompt_scope(
        "background_task",
        &format!("summary:{conversation}:{provider_basis}:{STRATEGY}:{basis}"),
    )?;
    if budget == 0 {
        return Ok(SummaryProgress {
            requests: 0,
            cache_hits: 0,
            failed: false,
        });
    }
    super::budget::validate_context_exchanges(context)?;
    // Optional background alone never triggers paid summary work.
    // P3 自动档：80% 触发线提前预热——摘要节点落 PageMark 持久化，
    // 下一轮装配直接读标记跳过边界前原文，免重复摘要成本。
    let tokens: usize = context
        .iter()
        .filter(|message| !message.optional_background)
        .map(|message| super::budget::text_tokens(model, &message.content) + 8)
        .sum();
    let trigger_line = budget * 8 / 10;
    if tokens <= trigger_line {
        return Ok(SummaryProgress {
            requests: 0,
            cache_hits: 0,
            failed: false,
        });
    }
    let mut history = store.list_messages(conversation)?;
    // 已有生效翻篇标记时，本轮摘要只面向边界之后的「当前篇」：边界前
    // 原文已由标记摘要固化，不再重复进入摘要选择（否则 pre-clip 守护
    // 会把标记后的新增长永远挡在摘要之外）。
    if let Some(mark) = store.active_page_mark(conversation)? {
        if let Some(position) = history
            .iter()
            .position(|m| m.id == mark.boundary_message_id)
        {
            history.drain(..=position);
        }
    }
    let users: Vec<_> = history
        .iter()
        .enumerate()
        .filter(|(_, message)| message.role == "user")
        .map(|(index, _)| index)
        .collect();
    if users.len() < 3 {
        return Ok(SummaryProgress {
            requests: 0,
            cache_hits: 0,
            failed: false,
        });
    }
    let cutoff = users[users.len() - 2];
    if cutoff == 0 {
        return Ok(SummaryProgress {
            requests: 0,
            cache_hits: 0,
            failed: false,
        });
    }
    let selected = &history[..cutoff];
    let selected_tokens: usize = selected
        .iter()
        .map(|message| super::budget::text_tokens(model, &message.content) + 8)
        .sum();
    // A tiny prefix cannot pay for a structured summary and its provenance.
    // In particular, an oversized current question must not trigger pointless
    // summary requests for a few short earlier messages.
    if selected_tokens < 512 {
        return Ok(SummaryProgress {
            requests: 0,
            cache_hits: 0,
            failed: false,
        });
    }
    let selected_ids: HashSet<_> = selected
        .iter()
        .map(|message| content_hash(&message.id))
        .collect();
    let complete_originals = selected.iter().all(|original| {
        context.iter().any(|message| {
            message.segment.identity_hash.as_deref() == Some(content_hash(&original.id).as_str())
                && message.role == original.role
                && message.content == original.content
                && message.tool_calls.as_ref().is_none_or(Vec::is_empty)
                && message.tool_call_id.is_none()
        })
    });
    // A pre-clipped or protocol-normalized context is not silently relabelled as
    // full source coverage. The caller must pass uncompressed stored messages.
    if !complete_originals {
        return Ok(SummaryProgress {
            requests: 0,
            cache_hits: 0,
            failed: false,
        });
    }
    let mut builder = Builder {
        store,
        conversation,
        provider,
        provider_name,
        provider_basis,
        model,
        started: std::time::Instant::now(),
        requests: 0,
        cache_hits: 0,
    };
    let mut failed_flag = false;
    super::generation::event(store, "phase", json!({"phase":"summarizing"}))?;
    let result = (|| -> Result<(ContextMessage, String)> {
        validate_history_path(&history)?;
        let mut node = builder.build(selected)?;
        let allowance = (budget / 4).clamp(256, 2400);
        let mut replacement = summary_context(store, &node)?;
        if super::budget::text_tokens(model, &replacement.content) > allowance {
            node = builder.merge(&[node], true)?;
            replacement = summary_context(store, &node)?;
        }
        ensure!(
            super::budget::text_tokens(model, &replacement.content) <= allowance
                && super::budget::text_tokens(model, &replacement.content) < selected_tokens,
            "摘要和覆盖说明超过本轮摘要预算或未缩小历史"
        );
        store.validate_history_spans(conversation, &node.spans)?;
        super::generation::check(store)?;
        Ok((replacement, node.id))
    })();
    super::generation::check(store)?;
    match result {
        Ok((replacement, node_id)) => {
            // failed_flag 已是 false（Ok 分支）
            let mut rebuilt: Vec<_> = context
                .iter()
                .filter(|message| {
                    !message
                        .segment
                        .identity_hash
                        .as_ref()
                        .is_some_and(|id| selected_ids.contains(id))
                })
                .cloned()
                .collect();
            rebuilt.insert(1.min(rebuilt.len()), replacement);
            *context = rebuilt;
            // P3 自动档：把刚生成的摘要固化为 kind=auto 翻篇标记（覆盖
            // 到倒数第二条 user 前），下一轮装配免重做。落标失败不炸
            // 本轮主链路；用户手动撤销（active=0）后 auto 重做属预期。
            let boundary = selected.last().map(|message| message.id.clone());
            if let Some(boundary) = boundary {
                if let Err(error) =
                    store.create_page_mark(conversation, &boundary, &node_id, "auto")
                {
                    super::generation::event(
                        store,
                        "page_mark_stale",
                        json!({"reason":"auto_mark_failed","detail":error.to_string()}),
                    )?;
                }
            }
            super::generation::event(
                store,
                "summary_ready",
                json!({"requests":builder.requests,"cache_hits":builder.cache_hits,"covered_messages":selected.len(),"strategy":STRATEGY}),
            )?;
        }
        Err(error) => {
            failed_flag = true;
            // 事件 detail 同步进日志通道：手机上事件表不可直接查，
            // 没有 logcat 就只能猜（issue 2026-10-31 实测）。
            crate::logging::log_always(&format!("[summary] 摘要不可用：{error:#}"));
            // Completed leaf nodes remain reusable; an incomplete root is never
            // injected. P0-a（2026-10-19 档案）：此前真实错误被丢弃，一个
            // 确定性 bug 伪装成「偶发不可用」每轮重试数周。归并/校验错误
            // 是结构性信息（节点数、span 边界、校验名），截断后入事件供
            // 健康巡检分组定位；不记录用户原文，也不进对话内容。
            let detail: String = {
                let mut text = error.to_string();
                if text.chars().count() > 500 {
                    text = text.chars().take(500).collect();
                }
                text
            };
            context.push(
                ContextMessage::new(
                    "system",
                    store.render_execution_instruction("summary_unavailable", &[])?,
                )
                .with_segment(
                    SegmentPurpose::BudgetNotice,
                    SegmentOrigin::Application,
                    None,
                    OmissionPolicy::Required,
                ),
            );
            super::generation::event(
                store,
                "summary_unavailable",
                json!({"requests":builder.requests,"cache_hits":builder.cache_hits,"reason":"incomplete_or_invalid_basis","detail":detail,"strategy":STRATEGY}),
            )?;
        }
    }
    let failed = failed_flag;
    let progress = SummaryProgress {
        requests: builder.requests,
        cache_hits: builder.cache_hits,
        failed,
    };
    Ok(progress)
}

#[cfg(test)]
mod tests;

mod preheat;

pub(crate) use preheat::background_preheat;
#[cfg(test)]
pub(crate) use preheat::{background_preheat_with, preheat_channel_matches, probe_max_requests};
