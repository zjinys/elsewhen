//! 自 wiki.rs 机械分段平移（行为零变化）。

use crate::ai::memory::ContextMessage;
use crate::ai::provider::AiProvider;
use crate::event::{AnnotationSet, EventSummary};
use crate::storage::{EventRecord, Store, WikiPageDraft};
use anyhow::{Context, Result};
use serde::Deserialize;

use crate::wiki::{
    WIKI_KINDS, build_index_md, call_provider, redirect_synonym_creates,
    select_context_pages_for_events, validate_digest_kind, validate_slug,
};
// ── 批量提取：历史事件 → 联系人/关系草拟（@/# 标注为权威，AI 补全） ──────────

/// 单次批量提取最多喂给 LLM 的事件条数（提示词体积可控）
const BATCH_EXTRACT_MAX_EVENTS: usize = 120;
/// 单条事件进入提示词的最大字符数
const BATCH_EXTRACT_EVENT_CHARS: usize = 220;

/// 批量提取：扫描事件库，`@人名` / `#事情` 标注视为权威实体，再让 LLM 根据事件上下文
/// 补全联系人 role_note 与「联系人 ↔ 事情/项目」关系。
/// 只产草拟、不落库，返回可直接进入待确认动作的：
/// `{ "people": [{name,role_note}], "relations": [{person,target,relation,note}] }`
/// （落库仍走 execute_pending_action → apply_people_relations，用户确认后才写）。
pub fn propose_people_relations_from_events(store: &Store) -> Result<serde_json::Value> {
    use crate::event::{AnnotationSet, parse_annotations_many};

    let events = store.list_events()?;
    if events.is_empty() {
        anyhow::bail!("事件库为空，没有可提取的内容");
    }

    let _scope = store.prompt_scope(
        "background_task",
        &format!("people-relations:{}", uuid::Uuid::new_v4()),
    )?;
    // @/# 标注：用户在事件里明确写死的实体，权威且必须全部纳入
    let annotations: AnnotationSet =
        parse_annotations_many(events.iter().map(|e| e.raw_text.as_str()));

    let total = events.len();
    let sampled: Vec<&EventSummary> = events.iter().take(BATCH_EXTRACT_MAX_EVENTS).collect();
    let mut event_lines = String::new();
    for (idx, e) in sampled.iter().enumerate() {
        let text: String = e.raw_text.chars().take(BATCH_EXTRACT_EVENT_CHARS).collect();
        event_lines.push_str(&format!("{}| {}\n", idx + 1, text.trim()));
    }
    let mut annotated = String::new();
    if !annotations.people.is_empty() {
        annotated.push_str(&store.render_knowledge_instruction(
            "relations_people",
            &[("people", annotations.people.join("、"))],
        )?);
    }
    if !annotations.targets.is_empty() {
        annotated.push_str(&store.render_knowledge_instruction(
            "relations_targets",
            &[("targets", annotations.targets.join("、"))],
        )?);
    }

    let system = format!(
        "{}\n{}",
        store.render_knowledge_instruction("relations_system", &[])?,
        store.output_language_directive(false)?
    );
    let user = store.render_knowledge_instruction(
        "relations_input",
        &[
            ("total", total.to_string()),
            ("count", sampled.len().to_string()),
            ("events", event_lines),
            ("annotations", annotated),
        ],
    )?;

    let reply = call_provider(store, &system, &user, 4000)?;
    let parsed = parse_json_value(&reply).map_err(|_| {
        anyhow::anyhow!(
            "批量提取失败：AI 返回无法解析——{}…",
            reply.chars().take(200).collect::<String>()
        )
    })?;
    Ok(normalize_extraction(&parsed, &annotations))
}

/// 把 LLM 返回的提取结果归一化：
/// - 确保所有 @/# 标注实体都出现在 people（LLM 漏掉也兜底补上，role_note 留空）；
/// - relations 里的 person/target 若不在实体集里则跳过（LLM 幻觉过滤）。
pub fn normalize_extraction(parsed: &serde_json::Value, ann: &AnnotationSet) -> serde_json::Value {
    let mut people: Vec<serde_json::Value> = parsed
        .get("people")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    for p in &ann.people {
        let known = people.iter().any(|v| {
            v.get("name")
                .and_then(|n| n.as_str())
                .map(|s| s.trim() == p.as_str())
                .unwrap_or(false)
        });
        if !known {
            people.push(serde_json::json!({ "name": p, "role_note": "" }));
        }
    }
    let mut relations: Vec<serde_json::Value> = parsed
        .get("relations")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    relations.retain(|r| {
        let person = r
            .get("person")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .unwrap_or("");
        let target = r
            .get("target")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .unwrap_or("");
        !person.is_empty()
            && !target.is_empty()
            && people.iter().any(|v| {
                v.get("name")
                    .and_then(|n| n.as_str())
                    .map(|s| s.trim() == person)
                    .unwrap_or(false)
            })
    });
    serde_json::json!({ "people": people, "relations": relations })
}

/// 宽容解析 LLM 返回的 JSON 对象：容忍 ``` 围栏与前后杂质文字。
fn parse_json_value(reply: &str) -> Result<serde_json::Value> {
    let trimmed = reply.trim();
    let inner = if trimmed.starts_with("```") {
        trimmed
            .lines()
            .filter(|l| !l.starts_with("```"))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string()
    } else {
        trimmed.to_string()
    };
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&inner) {
        return Ok(v);
    }
    if let Some(start) = inner.find('{') {
        if let Some(end) = inner.rfind('}') {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&inner[start..=end]) {
                return Ok(v);
            }
        }
    }
    anyhow::bail!("无法解析 AI 返回的 JSON")
}

/// LLM 返回的页面变更提议
#[derive(Debug, Clone, Deserialize)]
pub struct DigestProposal {
    #[serde(default)]
    pub op: String, // "create" | "update"（缺失则按 slug 是否已存在推断）
    pub kind: String,
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub summary: String,
    pub content: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub source_event_ids: Vec<String>, // 事件编号（用户消息里的 [N]）
    #[serde(default)]
    pub reason: Option<String>,
}

/// 单批最多事件数与事件原文字符预算（提示词里还会附带索引与相关页全文）。
const DIGEST_BATCH_MAX_EVENTS: usize = 20;
const DIGEST_BATCH_MAX_CHARS: usize = 6000;
// Auxiliary knowledge must not grow with the entire library. Count UTF-8
// bytes here, matching the conservative estimator used for unknown models.
const DIGEST_INDEX_MAX_BYTES: usize = 8000;
const DIGEST_INDEX_MAX_PAGES: usize = 24;

fn utf8_prefix(text: &str, max_bytes: usize) -> &str {
    let mut end = text.len().min(max_bytes);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// 后台知识消化一次 tick 的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DigestTick {
    /// Persistent reading advanced, is waiting, or failed independently.
    Reading {
        event_id: String,
        state: String,
        error: Option<String>,
    },
    /// 没有到期可消化的事件
    Idle,
    /// 一批事件消化成功（页面变更已与任务确认同事务落库）
    Processed {
        events: usize,
        outcome: crate::storage::DigestBatchOutcome,
    },
    /// 本批失败：任务已按退避回队，原因已写入运行日志
    Failed { events: usize, error: String },
}

fn build_digest_user_prompt(
    events: &[EventRecord],
    store: &Store,
    max_chars: usize,
    index_bytes: usize,
    ranked: &[crate::storage::WikiPage],
    reading_material: Option<&str>,
) -> Result<String> {
    let mut out = store.render_processing_instruction("digest_events", &[])?;
    for (i, e) in events.iter().enumerate() {
        let body = if i == 0 {
            reading_material.unwrap_or(&e.raw_text)
        } else {
            &e.raw_text
        };
        out.push_str(&format!("[{}] {} | {}\n", i + 1, e.recorded_at, body));
    }

    let mut index_pages = Vec::new();
    for page in ranked.iter().take(DIGEST_INDEX_MAX_PAGES) {
        let mut compact = page.clone();
        compact.summary = utf8_prefix(&compact.summary, 480).to_string();
        index_pages.push(compact);
    }
    let mut index = String::new();
    for line in build_index_md(&index_pages).lines() {
        if index.len() + line.len() + 1 > index_bytes {
            break;
        }
        index.push_str(line);
        index.push('\n');
    }
    out.push_str(&store.render_processing_instruction("digest_index", &[])?);
    out.push_str(&index);

    let mut used = 0;
    let mut wrote_header = false;
    for page in ranked {
        let body = utf8_prefix(&page.content_md, page.content_md.len().min(4000));
        let detail = format!("\n--- {} ---\n{}", page.slug, body);
        if used + detail.len() > max_chars {
            continue;
        }
        if !wrote_header {
            out.push_str(&store.render_processing_instruction("digest_pages", &[])?);
            wrote_header = true;
        }
        out.push_str(&detail);
        used += detail.len();
    }

    out.push_str(&store.render_processing_instruction("digest_output", &[])?);
    Ok(out)
}

pub(crate) fn parse_proposals(reply: &str) -> Result<Vec<DigestProposal>> {
    let trimmed = reply.trim();
    let inner = if trimmed.starts_with("```") {
        trimmed
            .lines()
            .filter(|l| !l.starts_with("```"))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string()
    } else {
        trimmed.to_string()
    };
    if let Ok(list) = serde_json::from_str::<Vec<DigestProposal>>(&inner) {
        return Ok(list);
    }
    if let Some(start) = inner.find('[') {
        if let Some(end) = inner.rfind(']') {
            if let Ok(list) = serde_json::from_str::<Vec<DigestProposal>>(&inner[start..=end]) {
                return Ok(list);
            }
        }
    }
    anyhow::bail!(
        "无法解析 digest 返回的 JSON。返回内容前 200 字：{}",
        &reply.chars().take(200).collect::<String>()
    )
}

/// 后台知识消化的一次 tick：整理队列 → 领取一批 → 模型提议 → 整批校验 → 单事务落库。
///
/// 由 GUI 的后台 worker 周期调用（见 `api::trigger_knowledge_digest`），没有手动触发入口。
/// 模型失败、JSON 非法或任一提议不合法时整批不写入，任务按退避回队并记入运行日志。
pub fn process_digest_queue(
    store: &Store,
    provider: &dyn AiProvider,
    model: Option<&str>,
) -> Result<DigestTick> {
    store.settle_digest_queue()?;
    if !store.background_processing_allowed()? {
        return Ok(DigestTick::Idle);
    }
    if store.prefer_event_reading()? {
        if let Some(tick) = scheduling::reading_tick(store, provider, model)? {
            return Ok(tick);
        }
    }
    let Some(mut batch) =
        store.claim_digest_batch(DIGEST_BATCH_MAX_EVENTS, DIGEST_BATCH_MAX_CHARS, model)?
    else {
        return Ok(scheduling::reading_tick(store, provider, model)?.unwrap_or(DigestTick::Idle));
    };
    store.set_digest_lane(false)?;
    let bundle = store.digest_prompt_bundle(&batch.id)?;
    let _prompt_scope = store.use_prompt_bundle(&bundle);
    let prepared_count = std::cell::Cell::new(0usize);
    let applied = digest_batch_drafts(store, provider, &batch.events, None, None, &prepared_count)
        .and_then(|(drafts, selected)| {
            store.resize_digest_batch(&mut batch, selected)?;
            store.apply_digest_batch(&batch, &drafts)
        });
    if applied
        .as_ref()
        .err()
        .is_some_and(|error| error.is::<scheduling::NeedsEventReading>())
    {
        store.resize_digest_batch(&mut batch, 1)?;
        let task = match store.ensure_event_reading(&batch) {
            Ok(task) => task,
            Err(error) => {
                let message = format!("{error:#}");
                store.fail_digest_batch(&batch, &message)?;
                return Ok(DigestTick::Failed {
                    events: 1,
                    error: message,
                });
            }
        };
        store.park_digest_for_reading(&batch)?;
        return Ok(DigestTick::Reading {
            event_id: task.event_id,
            state: "preparing".into(),
            error: None,
        });
    }
    if applied.is_err() {
        store.resize_digest_batch(&mut batch, prepared_count.get().max(1))?;
    }
    let events = batch.events.len();
    match applied {
        Ok(outcome) => Ok(DigestTick::Processed { events, outcome }),
        Err(error) => {
            let message = format!("{error:#}");
            store.fail_digest_batch(&batch, &message)?;
            Ok(DigestTick::Failed {
                events,
                error: message,
            })
        }
    }
}

/// 调模型取提议并整批校验，转换为页面草案；任一提议不合法即整批报错。
fn digest_batch_drafts(
    store: &Store,
    provider: &dyn AiProvider,
    events: &[EventRecord],
    reading_material: Option<&str>,
    final_claim: Option<(
        &crate::storage::event_readings::FinalReadingLease,
        &crate::storage::DigestBatch,
    )>,
    selected: &std::cell::Cell<usize>,
) -> Result<(Vec<WikiPageDraft>, usize)> {
    let system = format!(
        "{}\n{}\n{}",
        store.prompt_for_run("digest.policy")?,
        store.prompt_for_run("digest.fixed_contract")?,
        store.output_language_directive(false)?
    );
    // One library snapshot per tick; no repeated full scans for token probes.
    let pages = store.list_wiki_pages(None, None)?;
    let kinds: Vec<&str> = WIKI_KINDS
        .iter()
        .copied()
        .filter(|kind| *kind != "memo")
        .collect();
    let ranked_cache = std::cell::RefCell::new(None::<(usize, Vec<crate::storage::WikiPage>)>);
    let readable_overflow = std::cell::Cell::new(false);
    // Preparation is local and repeated for each actual failover candidate.
    let prepare = |fits: &crate::ai::provider::RequestFit<'_>| {
        let request = |count: usize,
                       detail_bytes: usize,
                       index_bytes: usize|
         -> Result<Vec<ContextMessage>> {
            let mut cache = ranked_cache.borrow_mut();
            if (detail_bytes > 0 || index_bytes > 0)
                && cache.as_ref().map(|(cached, _)| *cached) != Some(count)
            {
                *cache = Some((
                    count,
                    select_context_pages_for_events(
                        &events[..count],
                        pages.clone(),
                        &kinds,
                        usize::MAX,
                    ),
                ));
            }
            let ranked = if detail_bytes == 0 && index_bytes == 0 {
                &[][..]
            } else {
                cache
                    .as_ref()
                    .expect("ranked material prepared")
                    .1
                    .as_slice()
            };
            Ok(vec![
                ContextMessage::new("system", system.clone()),
                ContextMessage::new(
                    "user",
                    build_digest_user_prompt(
                        &events[..count],
                        store,
                        detail_bytes,
                        index_bytes,
                        ranked,
                        reading_material,
                    )?,
                ),
            ])
        };
        // Required input is admitted first. Never trim an event to make it fit.
        let mut count = events.len();
        loop {
            let required = request(count, 0, 0)?;
            if fits(&required)? {
                break;
            }
            if count == 1 {
                if reading_material.is_none() {
                    let mut empty = events[0].clone();
                    empty.raw_text.clear();
                    let fixed = vec![
                        ContextMessage::new("system", system.clone()),
                        ContextMessage::new(
                            "user",
                            build_digest_user_prompt(&[empty], store, 0, 0, &[], None)?,
                        ),
                    ];
                    if fits(&fixed)? {
                        readable_overflow.set(true);
                    }
                }
                return Err(crate::ai::budget::RequestBudgetError(
                    "digest_required_input_exceeds_budget: 单条完整事件与固定指令已超出该 Provider 输入预算；事件保留在队列中，未截断或确认为已消化".into()).into());
            }
            count -= 1;
        }
        // Allocate optional relevance context against the exact transport
        // envelope, tokenizer, output reserve and configured context window.
        let mut low = 0;
        let mut high = DIGEST_INDEX_MAX_BYTES;
        let mut best = request(count, 0, 0)?;
        while low < high {
            let size = low + (high - low).div_ceil(2);
            let candidate = request(count, size, size)?;
            if fits(&candidate)? {
                low = size;
                best = candidate;
            } else {
                high = size - 1;
            }
        }
        crate::logging::log_always(&format!(
            "[digest-budget] prepared events={}/{} optional_context_bytes={} messages={}",
            count,
            events.len(),
            low,
            best.len()
        ));
        selected.set(count);
        if let Some((lease, batch)) = final_claim {
            store.renew_event_reading_final(lease, batch, chrono::Utc::now().timestamp())?;
        }
        Ok(best)
    };
    let reply = if reading_material.is_some() {
        provider.generate_budgeted_prepared(&prepare)
    } else {
        provider.generate_prepared(&prepare)
    }
    .map_err(|error| {
        if reading_material.is_none() && selected.get() == 0 && readable_overflow.get() {
            anyhow::Error::new(scheduling::NeedsEventReading)
        } else {
            error.context("调用模型失败")
        }
    })?;
    if reading_material.is_some() {
        if let Some(usage) = &reply.usage {
            store.record_token_usage(
                None,
                usage.prompt_tokens as i64,
                usage.completion_tokens as i64,
                usage.total_tokens as i64,
                reply.model.as_deref(),
            )?;
        }
    }
    let text = if reading_material.is_some() {
        let text = crate::knowledge::structured_text(&reply, "event final digest")?;
        anyhow::ensure!(
            reply.finish_reason.as_deref() == Some("stop"),
            "event_reading_final_finish_reason_invalid"
        );
        anyhow::ensure!(text.len() <= 131072, "event_reading_final_output_limit");
        let value: serde_json::Value = serde_json::from_str(text)?;
        let items = value
            .as_array()
            .context("event_reading_final_expected_array")?;
        anyhow::ensure!(items.len() <= 32, "event_reading_final_proposal_limit");
        for item in items {
            let object = item
                .as_object()
                .context("event_reading_final_expected_proposal")?;
            anyhow::ensure!(
                object.keys().all(|key| [
                    "op",
                    "kind",
                    "slug",
                    "title",
                    "summary",
                    "content",
                    "tags",
                    "source_event_ids",
                    "reason"
                ]
                .contains(&key.as_str())),
                "event_reading_final_unknown_field"
            );
        }
        text
    } else {
        &reply.content
    };
    let selected = selected.get();
    anyhow::ensure!(
        selected > 0 && selected <= events.len(),
        "digest preparation did not select an event batch"
    );
    let events = &events[..selected];
    let mut proposals = parse_proposals(text)?;
    if reading_material.is_some() {
        // Final reading material retains the actual immutable event identity.
        // Accept that exact in-batch identity as an alias for its batch number;
        // unrelated UUIDs and out-of-batch numeric references still fail below.
        for proposal in &mut proposals {
            for id in &mut proposal.source_event_ids {
                if let Some(index) = events.iter().position(|event| event.id == *id) {
                    *id = (index + 1).to_string();
                }
            }
        }
    }

    // 先补全模型漏写的 kind 前缀，再做同义新建重定向。缺前缀的 slug 是
    // 「同一习惯被当成新页」的直接来源：例如 habit 提案给 after-lunch-walk，
    // 归一成 habit/after-lunch-walk 后会命中既有页并走 update。
    normalize_proposal_slugs(store, &mut proposals)?;
    // 提案 B：同义新建改写为对既有页的 update（kind+归一化标题+tag 重合）。
    let proposals = redirect_synonym_creates(store, proposals)?;
    let drafts = proposals
        .iter()
        .map(|p| proposal_to_draft(p, events))
        .collect::<Result<Vec<_>>>()?;
    Ok((drafts, selected))
}

/// 补全 digest 提议里漏写的 kind 前缀（如 `after-lunch-walk` →
/// `habit/after-lunch-walk`）。模型偶尔无视提示词里的 slug 格式；不补前缀
/// 会让同一主题被当成新页，产生重复习惯页。
///
/// 只补「缺失且库中不存在同名无前缀页」的情况：历史 source/note 页有自己的
/// 命名约定，已经存在的无前缀页也不能被改写成带前缀的新页。
fn normalize_proposal_slugs(store: &Store, proposals: &mut [DigestProposal]) -> Result<()> {
    for p in proposals.iter_mut() {
        if !validate_digest_kind(&p.kind)
            || p.slug.contains('/')
            || matches!(p.kind.as_str(), "source" | "note")
        {
            continue;
        }
        if store.get_wiki_page(&p.slug)?.is_some() {
            continue;
        }
        p.slug = format!("{}/{}", p.kind.replace('_', "-"), p.slug);
    }
    Ok(())
}

/// 单条提议 → 页面草案。来源编号必须全部落在本批且非空，禁止零来源页。
fn proposal_to_draft(p: &DigestProposal, events: &[EventRecord]) -> Result<WikiPageDraft> {
    if !validate_digest_kind(&p.kind) || !validate_slug(&p.slug) || p.content.trim().is_empty() {
        anyhow::bail!("提议不合法：kind={} slug={}", p.kind, p.slug);
    }
    if p.source_event_ids.is_empty() {
        anyhow::bail!("提议缺少来源事件编号：{}", p.slug);
    }
    let mut ids: Vec<String> = Vec::new();
    for raw in &p.source_event_ids {
        let event = raw
            .trim()
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<usize>()
            .ok()
            .and_then(|n| n.checked_sub(1))
            .and_then(|i| events.get(i))
            .with_context(|| format!("提议 {} 引用了本批之外的事件编号：{raw}", p.slug))?;
        if !ids.contains(&event.id) {
            ids.push(event.id.clone());
        }
    }
    Ok(WikiPageDraft {
        slug: p.slug.clone(),
        kind: p.kind.clone(),
        title: p.title.clone(),
        summary: p.summary.clone(),
        content_md: p.content.clone(),
        tags: p.tags.clone(),
        source_event_ids: ids,
        status: "active".to_string(),
        reason: p
            .reason
            .clone()
            .unwrap_or_else(|| "digest 消化新事件".to_string()),
        source_url: None,
    })
}

mod finalization;
pub(crate) use finalization::finalize_event_reading;

mod scheduling;
