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
        annotated.push_str(&format!("联系人标注：{}\n", annotations.people.join("、")));
    }
    if !annotations.targets.is_empty() {
        annotated.push_str(&format!(
            "事情/项目标注：{}",
            annotations.targets.join("、")
        ));
    }

    let system = r#"你是 elsewhen 个人知识库的「联系人关系」批量提取器。任务：阅读用户的事件，输出「联系人 + 联系人↔事情/项目关系」。

抽取规则：
- 事件里用 @人名 标注的一定是人、#事情 标注的一定是事情/项目（如「@李四 负责 #网站改版」）。已标注实体必须全部纳入 people / relations；同名但备注不同（「李四（市场部）」「李四（设计）」）是不同的人，不能合并。
- 未标注的联系人：只提取信息具体、对用户重要的人（有称呼、有身份或参与的明确事情），不为随口一提的名字建条目。
- relations 的 person/target 必须来自 people 或已标注实体；relation 用 负责/参与/合作/对接/跟进/顾问 等 2~4 字动词。
- role_note 一句话身份/背景，来自事件上下文；没有可写信息就留空字符串。
- 宁缺毋滥：没有把握的关系不要编造。

严格输出 JSON（不要代码块围栏、不要其他任何文字）：
{"people":[{"name":"姓名","role_note":"身份备注"}],"relations":[{"person":"姓名","target":"事情/项目","relation":"负责","note":"补充说明"}]}"#;
    let user = format!(
        "共 {total} 条事件（展示最近 {} 条）：\n\n{event_lines}\n{annotated}",
        sampled.len()
    );

    let reply = call_provider(store, system, &user, 4000)?;
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

const DIGEST_SYSTEM_PROMPT: &str = r#"你是 elsewhen 个人知识库的 wiki 维护者。任务：读新事件，把它们承载的"持久事实"提炼并写进 wiki 页面。

页面 kind 枚举（必须严格使用其一）：
- profile：关于用户身份、背景、状态的基本事实
- person：对话/事件中出现的重要联系人（姓名、身份、TA 参与或负责的事情/项目），一人一页，跨事件合并，不重复建档
- recurring_cost：反复出现的固定支出或反复动作（通勤、固定费用、例行事务）
- capability：用户掌握的技能/能力（含正在学习的）
- asset：用户拥有但可能闲置/未充分利用的资产（设备、空间、时间块、关系）
- project：用户参与的项目
- relationship：重要人际关系/合作
- decision：重要决策及理由（尤其涉及钱、时间、取舍的）
- habit：规律习惯
- constraint：约束条件
- topic：其他值得长期记住的主题

写页规则：
- 只提炼事件里【有依据】的事实，绝不编造；不确定就在内容里标注（待确认）。
- slug：小写 kebab-case，格式 <kind 去下划线>/<简短名>，例如 recurring-cost/dongguan-huizhou-commute。
- 已有页面需要更新时 op=update，给出【全文新内容】（基于旧内容增量修补，不丢失旧事实）。
- 若事件没有产生任何新事实，返回空数组。
- 索引或「相关现有页面全文」中已有同主题页时【必须 op=update】，不得新建同义
  slug；凡标题或标签命中的候选页，逐页判断是否需要更新，并在 reason 里说明
  为什么改或不改。
- 每个页面变更的 source_event_ids 必须列出支撑它的事件编号（至少一个，只能用本次给出的编号）。
- 宁缺毋滥：一条事件 0~2 个页面变更，允许 0；与已有页相关时优先更新而不是新建。"#;

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
/// 单条事件在提示词里的展示上限（超长事件单独成批时防止上下文失控）。
const DIGEST_EVENT_MAX_CHARS: usize = 4000;

/// 后台知识消化一次 tick 的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DigestTick {
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
) -> Result<String> {
    let mut out = String::from("这是等待消化的新事件（[编号] 时间 | 内容）：\n");
    for (i, e) in events.iter().enumerate() {
        let text: String = e.raw_text.chars().take(DIGEST_EVENT_MAX_CHARS).collect();
        out.push_str(&format!("[{}] {} | {}\n", i + 1, e.recorded_at, text));
    }

    let pages = store.list_wiki_pages(None, None)?;
    let index = build_index_md(&pages);
    out.push_str("\n当前 wiki 索引：\n");
    out.push_str(&index);

    // memo 全文不进 digest 上下文（只留给「知识引用式」按需挂载，评审 D3）。
    let kinds: Vec<&str> = WIKI_KINDS
        .iter()
        .copied()
        .filter(|k| *k != "memo")
        .collect();
    // 提案 A：相关性选页——本批事件提到的页必然拿到正文，不再只看证据数。
    let context = select_context_pages_for_events(events, pages, &kinds, max_chars);
    if !context.is_empty() {
        out.push_str("\n相关现有页面全文（合并/更新时参考）：\n");
        for p in &context {
            let cost = p.content_md.len().min(4000);
            out.push_str(&format!(
                "\n--- {} ---\n{}",
                p.slug,
                p.content_md.chars().take(cost).collect::<String>()
            ));
        }
    }

    out.push_str(
        "\n\n输出 JSON 数组，不要输出任何其他文字、不要代码块围栏：\n\
         [{\"op\":\"create|update\",\"kind\":\"...\",\"slug\":\"...\",\"title\":\"...\",\n\
         \"summary\":\"一行摘要\",\"content\":\"markdown 正文，用 - 要点\",\n\
         \"tags\":[\"...\"],\"source_event_ids\":[\"编号\"],\"reason\":\"为什么建/改\"}]",
    );
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
    let Some(batch) =
        store.claim_digest_batch(DIGEST_BATCH_MAX_EVENTS, DIGEST_BATCH_MAX_CHARS, model)?
    else {
        return Ok(DigestTick::Idle);
    };
    let events = batch.events.len();
    let applied = digest_batch_drafts(store, provider, &batch.events)
        .and_then(|drafts| store.apply_digest_batch(&batch, &drafts));
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
) -> Result<Vec<WikiPageDraft>> {
    let user = build_digest_user_prompt(events, store, 8000)?;
    let reply = provider
        .generate_reply(vec![
            ContextMessage::new("system", DIGEST_SYSTEM_PROMPT.to_string()),
            ContextMessage::new("user", user),
        ])
        .context("调用模型失败")?
        .content;
    let mut proposals = parse_proposals(&reply)?;
    // 先补全模型漏写的 kind 前缀，再做同义新建重定向。缺前缀的 slug 是
    // 「同一习惯被当成新页」的直接来源：例如 habit 提案给 after-lunch-walk，
    // 归一成 habit/after-lunch-walk 后会命中既有页并走 update。
    normalize_proposal_slugs(store, &mut proposals)?;
    // 提案 B：同义新建改写为对既有页的 update（kind+归一化标题+tag 重合）。
    let proposals = redirect_synonym_creates(store, proposals)?;
    proposals
        .iter()
        .map(|p| proposal_to_draft(p, events))
        .collect()
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
