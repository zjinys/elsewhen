pub mod chat_proposals;
pub mod chat_runs;
pub mod conversations;
pub mod db_maintenance;
pub mod entities;
pub mod event_stats;
pub mod fonts;
pub mod goals;
pub mod import;
pub mod knowledge;
pub mod knowledge_digest;
pub mod knowledge_workflows;
pub mod mcp;
pub mod page_marks;
pub mod provider_config;
pub mod provider_reachability;
pub mod relations;
pub mod review_report;
pub mod rules;
pub mod sync;
pub mod theme;
pub mod todos;
pub mod tweet;
pub mod user_skills;
pub mod web_search;
pub mod wiki;
pub mod wiki_chat;
use crate::ai::memory::ContextMessage;
use crate::ai::provider::{AiProvider, OpenAiCompatibleConfig, OpenAiCompatibleProvider};
use crate::event::NewEvent;
use crate::storage::Store;
use crate::storage::rule_triggers::PendingChatHit;
use anyhow::{Context, Result};
pub use chat_proposals::*;
pub use chat_runs::*;
pub use conversations::*;
pub use entities::*;
pub use fonts::*;
pub use import::*;
pub use knowledge_digest::*;
pub use mcp::*;
pub use provider_config::*;
pub use relations::*;
pub use rules::*;
use serde::{Deserialize, Serialize};
pub use theme::*;
pub use todos::*;
pub use tweet::*;
pub use user_skills::*;
pub use web_search::*;
pub use wiki::*;
pub use wiki_chat::*;

/// 现行分析结果契约标识（D1 起升 v3：新增 rule_confirmations；
/// guidance 段已在 D4 启用（prompt 可选要求 + wash 护栏）；emotion 分片待 D5）。
/// v1/v2 为历史读兼容标识；「event-analysis」（无后缀）为 v3 之前的现行写法。
const EVENT_ANALYSIS_VERSION: &str = "event-analysis-v3";
const PRE_V3_EVENT_ANALYSIS: &str = "event-analysis";
const LEGACY_EVENT_ANALYSIS_V1: &str = "event-analysis-v1";
const LEGACY_EVENT_ANALYSIS_V2: &str = "event-analysis-v2";
const DAILY_REVIEW_VERSION: &str = "daily-review-v1";
/// 单次队列排空的总墙钟时间上限（秒）。provider 单次超时可达 60s，
/// 一批最多 50 条，若无上限会长时间占住分析 worker（P2 worker 限时）。
const ANALYSIS_BATCH_MAX_SECS: u64 = 120;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct EventAnalysisV1 {
    schema_version: String,
    event_type: String,
    confidence: f64,
    summary: String,
    clarifications: Vec<String>,
    people: Vec<String>,
    projects: Vec<String>,
    #[serde(default)]
    activities: Vec<String>,
    follow_ups: Vec<String>,
    #[serde(default = "default_recordable")]
    recordable: bool,
    #[serde(default = "default_event_kind")]
    kind: String,
    /// D1：规则预筛判定的回填（命中规则进 prompt，模型判定适用性/置信）。
    #[serde(default)]
    rule_confirmations: Vec<EventRuleConfirmation>,
    /// D4 启用：指导维度（结构就位，prompt 未要求时为 None）。
    #[serde(default)]
    guidance: Option<EventGuidance>,
    /// D5 启用：情绪标注（结构就位，prompt 未要求时为 None）。
    #[serde(default)]
    emotion: Option<Vec<EventEmotionTag>>,
}

/// 规则适用性判定（D1）。rule_id 必须是 prompt 附带的 pending 命中之 id。
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct EventRuleConfirmation {
    rule_id: String,
    applicable: bool,
    confidence: f64,
}

/// 指导维度（D4 落地用）。related 指向知识页 slug，受引用校验约束。
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct EventGuidance {
    #[serde(default)]
    related: Vec<EventGuidanceRef>,
    #[serde(default)]
    lessons: Vec<String>,
    #[serde(default)]
    follow_ups_refined: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct EventGuidanceRef {
    slug: String,
    note: String,
}

/// 情绪标注（D5 落地用）。target 为人物/项目 slug 或原文提及；无 target =
/// 对事件本身的感受。只有文本，无数值评分（红线：不是情绪分析报告）。
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct EventEmotionTag {
    #[serde(default)]
    target: Option<String>,
    feeling: String,
}

fn default_recordable() -> bool {
    true
}
fn default_event_kind() -> String {
    "event".to_string()
}

impl EventAnalysisV1 {
    fn parse(raw: &str) -> Result<Self> {
        let mut result: Self = serde_json::from_str(raw)?;
        if result.schema_version != EVENT_ANALYSIS_VERSION
            && result.schema_version != PRE_V3_EVENT_ANALYSIS
            && result.schema_version != LEGACY_EVENT_ANALYSIS_V1
            && result.schema_version != LEGACY_EVENT_ANALYSIS_V2
        {
            anyhow::bail!("schema_version 必须是 event-analysis（v1/v2/v3）");
        }
        if result.schema_version == EVENT_ANALYSIS_VERSION {
            for confirmation in &result.rule_confirmations {
                anyhow::ensure!(
                    confirmation.confidence.is_finite()
                        && (0.0..=1.0).contains(&confirmation.confidence),
                    "rule_confirmations.confidence 必须在 0..1"
                );
                anyhow::ensure!(
                    !confirmation.rule_id.trim().is_empty(),
                    "rule_confirmations.rule_id 不能为空"
                );
            }
            if let Some(guidance) = &result.guidance {
                anyhow::ensure!(
                    guidance.related.iter().all(|r| !r.slug.trim().is_empty()),
                    "guidance.related.slug 不能为空"
                );
            }
            if let Some(emotions) = &result.emotion {
                anyhow::ensure!(
                    emotions.iter().all(|tag| !tag.feeling.trim().is_empty()),
                    "emotion.feeling 不能为空"
                );
            }
        } else {
            // v3 之前的写法无新字段；归一到无后缀历史标识保持 prompt_version 列保真。
            result.schema_version = PRE_V3_EVENT_ANALYSIS.to_string();
        }
        result.event_type = result.event_type.trim().to_string();
        result.summary = result.summary.trim().to_string();
        if result.event_type.is_empty() || result.summary.is_empty() {
            anyhow::bail!("event_type 和 summary 不能为空");
        }
        if !result.confidence.is_finite() || !(0.0..=1.0).contains(&result.confidence) {
            anyhow::bail!("confidence 必须在 0..1 范围内");
        }
        if !matches!(
            result.kind.as_str(),
            "event" | "discussion" | "chitchat" | "meta"
        ) {
            anyhow::bail!("kind 必须是 event/discussion/chitchat/meta");
        }
        if !result.recordable && result.kind == "event" {
            anyhow::bail!("不可记录结果不能标记为 event");
        }
        normalize_strings(&mut result.clarifications);
        normalize_strings(&mut result.people);
        normalize_strings(&mut result.projects);
        normalize_strings(&mut result.activities);
        normalize_strings(&mut result.follow_ups);
        Ok(result)
    }
}

fn normalize_strings(values: &mut Vec<String>) {
    let mut normalized = Vec::with_capacity(values.len());
    for value in values.drain(..) {
        let value = value.trim();
        if !value.is_empty() && !normalized.iter().any(|seen| seen == value) {
            normalized.push(value.to_string());
        }
    }
    *values = normalized;
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DailyReviewItemV1 {
    text: String,
    source_event_ids: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DailyReviewV1 {
    schema_version: String,
    date: String,
    accomplishments: Vec<DailyReviewItemV1>,
    ideas_decisions: Vec<DailyReviewItemV1>,
    people_projects: Vec<DailyReviewItemV1>,
    follow_ups: Vec<DailyReviewItemV1>,
}

impl DailyReviewV1 {
    fn parse(raw: &str, expected_date: &str, allowed_sources: &[String]) -> Result<Self> {
        let mut review: Self = serde_json::from_str(raw)?;
        if review.schema_version != DAILY_REVIEW_VERSION {
            anyhow::bail!("schema_version 必须是 {DAILY_REVIEW_VERSION}");
        }
        if review.date != expected_date {
            anyhow::bail!("daily review 日期与查询日期不一致");
        }
        for item in review
            .accomplishments
            .iter_mut()
            .chain(review.ideas_decisions.iter_mut())
            .chain(review.people_projects.iter_mut())
            .chain(review.follow_ups.iter_mut())
        {
            item.text = item.text.trim().to_string();
            normalize_strings(&mut item.source_event_ids);
            if item.text.is_empty() || item.source_event_ids.is_empty() {
                anyhow::bail!("daily review 每条结论必须包含文本和来源");
            }
            if item
                .source_event_ids
                .iter()
                .any(|id| !allowed_sources.iter().any(|allowed| allowed == id))
            {
                anyhow::bail!("daily review 包含未声明的来源事件");
            }
        }
        Ok(review)
    }
}

/// Event data transfer object for Flutter
#[derive(Clone, Debug)]
pub struct EventDto {
    pub id: String,
    pub raw_text: String,
    pub recorded_at: String,
    pub occurred_at: String,
    pub source: String,
    pub status: String,
}

/// Analysis result DTO
#[derive(Clone, Debug)]
pub struct AnalysisDto {
    pub event_type: String,
    pub confidence: f64,
    pub summary: String,
    pub clarifications: Vec<String>,
}

/// Durable event-analysis queue counts for operational visibility.
#[derive(Clone, Debug)]
pub struct AnalysisJobStatsDto {
    pub pending: i64,
    pub running: i64,
    pub retry: i64,
    pub succeeded: i64,
    pub failed: i64,
}

#[derive(Clone, Debug)]
pub struct InputRecordDto {
    pub id: String,
    pub raw_text: String,
    pub source: String,
    pub route_status: String,
    pub idempotency_key: Option<String>,
    pub event_id: Option<String>,
    pub message_id: Option<String>,
    pub wiki_page_slug: Option<String>,
    pub todo_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug)]
pub struct DailyEntryDto {
    pub event_id: String,
    pub input_id: Option<String>,
    pub message_id: Option<String>,
    pub raw_text: String,
    pub source: String,
    pub event_status: String,
    pub recorded_at: String,
}

/// guidance 派生层展示（D4）：related 指知识库页，lessons/精炼待办是纯文本。
/// 只从 event_analyses.result_json 派生，不写事件真源。
#[derive(Clone, Debug, serde::Serialize)]
pub struct GuidanceDto {
    pub related: Vec<GuidanceRefDto>,
    pub lessons: Vec<String>,
    pub follow_ups_refined: Vec<String>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct GuidanceRefDto {
    pub slug: String,
    pub note: String,
}

/// 情绪标注展示（D5）：文本标注、无数值（红线：不是情绪分析报告）。
#[derive(Clone, Debug, serde::Serialize)]
pub struct EmotionTagDto {
    pub target: Option<String>,
    pub feeling: String,
}

/// 情绪条目列表行（面板用）：analysis_id 是删除定位键。
#[derive(Clone, Debug, serde::Serialize)]
pub struct EventEmotionDto {
    pub id: String,
    pub event_id: String,
    pub recorded_at: String,
    pub target: Option<String>,
    pub feeling: String,
}

#[derive(Clone, Debug)]
pub struct EventAnalysisDetailDto {
    pub event_id: String,
    pub raw_text: String,
    pub source: String,
    pub recorded_at: String,
    pub event_status: String,
    pub job_status: String,
    pub attempts: i64,
    pub last_error: Option<String>,
    pub available_at: String,
    pub prompt_version: Option<String>,
    pub analysis_created_at: Option<String>,
    pub schema_version: Option<String>,
    pub event_type: Option<String>,
    pub confidence: Option<f64>,
    pub summary: Option<String>,
    pub clarifications: Vec<String>,
    pub people: Vec<String>,
    pub projects: Vec<String>,
    pub activities: Vec<String>,
    pub follow_ups: Vec<String>,
    pub guidance: Option<GuidanceDto>,
    pub emotion: Option<Vec<EmotionTagDto>>,
    pub recordable: Option<bool>,
    pub kind: Option<String>,
    pub effective_recordable: bool,
    pub effective_kind: String,
    pub recordability_source: String,
}

#[derive(Clone, Debug)]
pub struct MessageRecordabilityDto {
    pub message_id: String,
    pub event_id: String,
    pub recordable: bool,
    pub kind: String,
    pub source: String,
    pub job_status: String,
    pub summary: Option<String>,
}

#[derive(Clone, Debug)]
pub struct DailyReviewItemDto {
    pub text: String,
    pub source_event_ids: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct DailyReviewDto {
    pub id: String,
    pub date: String,
    pub prompt_version: String,
    pub created_at: String,
    pub accomplishments: Vec<DailyReviewItemDto>,
    pub ideas_decisions: Vec<DailyReviewItemDto>,
    pub people_projects: Vec<DailyReviewItemDto>,
    pub follow_ups: Vec<DailyReviewItemDto>,
}

#[derive(Clone, Debug)]
pub struct DailyOverviewDto {
    pub date: String,
    pub entries: Vec<DailyEntryDto>,
    pub review: Option<DailyReviewDto>,
    pub todos: Vec<TodoDto>,
    /// 当日触发的规则提醒与聚合（D6 回顾勾连：只呈现，不推断）。
    pub reminder_hits: Vec<crate::api::rules::RuleHitDto>,
    /// 共同记忆回引（D6）：约一周前的条目原文，呈现「你上周说过……」。
    /// 原文快照，不评价；没有合素材时为空。
    pub memory_echo: Vec<MemoryEchoDto>,
}

/// 回顾里的共同记忆回引条目。
#[derive(Clone, Debug)]
pub struct MemoryEchoDto {
    pub event_id: String,
    pub recorded_at: String,
    pub raw_text: String,
}

#[derive(Clone, Debug)]
pub struct EntityFactDto {
    pub id: String,
    pub entity_kind: String,
    pub entity_slug: String,
    pub fact_text: String,
    pub occurred_at: String,
    pub confidence: i64,
    pub source_event_id: String,
    pub created_at: String,
    pub last_seen_at: String,
}

impl From<crate::storage::InputRecord> for InputRecordDto {
    fn from(record: crate::storage::InputRecord) -> Self {
        Self {
            id: record.id,
            raw_text: record.raw_text,
            source: record.source,
            route_status: record.route_status,
            idempotency_key: record.idempotency_key,
            event_id: record.event_id,
            message_id: record.message_id,
            wiki_page_slug: record.wiki_page_slug,
            todo_id: record.todo_id,
            created_at: record.created_at,
            updated_at: record.updated_at,
        }
    }
}

/// 触发 AI 分析的桥接结果：结构化状态取代字符串契约（P2-9），
/// 避免 Dart 侧解析 "no_provider"/"processed:{n}" 字符串。
#[derive(Clone, Debug)]
pub enum AnalysisTriggerResult {
    /// 尚未配置 AI provider：不做分析，等待配置后再唤醒
    NoProvider,
    /// 本轮成功处理了 count 条事件（含 0：队列为空或全部等待重试）
    Processed { count: i64 },
}

/// 生成每日回顾的桥接结果。
#[derive(Clone, Debug)]
pub enum DailyReviewResult {
    NoProvider,
    /// 当天没有可记录内容，未生成回顾
    NoEntries,
    Created {
        id: String,
    },
}

/// Initialize the bridge with database path
pub fn init_bridge(database_path: Option<String>) -> Result<String> {
    // 库路径在进程内钉死（见 config::AppConfig::pin）：早前这里是
    // `std::env::set_var("ELSEWHEN_DATA_DIR", ...)`，而每个 API 函数都重读该环境变量，
    // 于是运行时任何代码都能改写数据源，且 `flutter test` 下并发 suite 会互相覆盖
    // 路径、打开同一个库并随机撞出 SQLITE_BUSY。
    let config = crate::config::AppConfig::pin(database_path.as_deref())
        .map_err(|e| anyhow::anyhow!("Error resolving data directory: {e}"))?;
    Store::open(&config.database_path)
        .and_then(|store| {
            if !store.has_sync_authority()? && store.sync_identity()?.is_none() {
                if let Some(_guard) = crate::sync::runtime::initialization_guard(&config)? {
                    store.recover_interrupted_analysis_jobs()?;
                    store.recover_interrupted_digest_jobs()?;
                }
            }
            store.recover_chat_proposal_decisions()?;
            store.recover_expired_chat_runs()?;
            // 用户技能对账（P3 分发）：远端到达的落盘、本地领先的回写台账。
            // 放在 init 同一层——db 已迁移完毕，失败不阻断启动（技能非核心路径）。
            if let Err(e) = crate::ai::skills::reconcile_with_store(&config.data_dir, &store) {
                eprintln!("[skills] reconcile failed: {e:#}");
            }
            // 出站代理策略（migration 43）：启动时从库加载（off/custom 即时生效）。
            crate::http_proxy::load_policy_from_store(&store);
            Store::mark_initialized(&config.database_path)?;
            Ok(())
        })
        .map_err(|e| anyhow::anyhow!("Error recovering background queues: {e}"))?;
    Ok(config.database_path.display().to_string())
}

/// Record a new event
pub fn record_event(raw_text: String) -> Result<EventDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    // GUI 快录路径：source 显式对齐 DTO（"flutter_gui"），避免与 capture/hotkey
    // 窗口路径（NewEvent::now 默认 "capture"）语义混同——list_events 如实读库后
    // event_card 的 source badge 才能正确显示 GUI。
    let new_event = NewEvent {
        raw_text: &raw_text,
        occurred_at: chrono::Utc::now(),
        recorded_at: chrono::Utc::now(),
        source: "flutter_gui",
    };
    // 先取出 DTO 字段再 move 进 insert_event（否则 move 后无法读取）。
    // 不再全表扫描 + 按 raw_text 回找：相同文本重复记录时会取到错误行，
    // 且 list_events 是 O(n) 全表遍历。
    let raw_text_owned = new_event.raw_text.to_string();
    let recorded_at = new_event.recorded_at.to_rfc3339();
    let occurred_at = new_event.occurred_at.to_rfc3339();
    let id = store.insert_event(new_event)?;

    Ok(EventDto {
        id,
        raw_text: raw_text_owned,
        recorded_at,
        occurred_at,
        source: "flutter_gui".to_string(),
        status: "pending".to_string(),
    })
}

/// Save a plain personal input without waiting for AI/network.
/// Reusing an idempotency key returns the original routed result.
pub fn submit_input(
    raw_text: String,
    source: String,
    idempotency_key: Option<String>,
) -> Result<InputRecordDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    Ok(store
        .submit_input_as_event(&raw_text, &source, idempotency_key.as_deref())?
        .into())
}

/// Start routing a URL input without creating a personal event. The raw URL is
/// durable before any network fetch begins and remains awaiting confirmation
/// until the preview is explicitly saved.
pub fn begin_url_input(
    raw_text: String,
    idempotency_key: Option<String>,
) -> Result<InputRecordDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let record = store.create_input_record(&raw_text, "url_import", idempotency_key.as_deref())?;
    if record.route_status != "pending" {
        return Ok(record.into());
    }
    Ok(store
        .update_input_route(&record.id, "needs_confirmation", None, None, None, None)?
        .into())
}

/// Complete or fail the URL preview route while preserving the original input.
pub fn finish_url_input(
    input_id: String,
    wiki_page_slug: Option<String>,
    failed: bool,
) -> Result<InputRecordDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let status = if failed { "failed" } else { "routed" };
    if !failed && wiki_page_slug.as_deref().map_or(true, str::is_empty) {
        anyhow::bail!("完成 URL 输入路由时必须提供 wiki_page_slug");
    }
    Ok(store
        .update_input_route(
            &input_id,
            status,
            None,
            None,
            wiki_page_slug.as_deref(),
            None,
        )?
        .into())
}

pub fn submit_conversation_input(
    conversation_id: String,
    raw_text: String,
    idempotency_key: Option<String>,
) -> Result<InputRecordDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let record =
        store.submit_conversation_input(&conversation_id, &raw_text, idempotency_key.as_deref())?;

    if let Some(title) = crate::storage::derive_conversation_title(&raw_text) {
        let untitled = store
            .get_conversation(&conversation_id)?
            .and_then(|conversation| conversation.title)
            .map_or(true, |current| current.trim().is_empty());
        if untitled {
            store.rename_conversation(&conversation_id, &title)?;
        }
    }
    Ok(record.into())
}

/// List one local calendar day's complete personal record stream.
pub fn list_daily_entries(date: String) -> Result<Vec<DailyEntryDto>> {
    let date = chrono::NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d")
        .map_err(|_| anyhow::anyhow!("日期必须是 YYYY-MM-DD"))?;
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    Ok(store
        .daily_entries(date)?
        .into_iter()
        .filter(|entry| entry_is_recordable(&store, &entry.event_id))
        .into_iter()
        .map(|entry| DailyEntryDto {
            event_id: entry.event_id,
            input_id: entry.input_id,
            message_id: entry.message_id,
            raw_text: entry.raw_text,
            source: entry.source,
            event_status: entry.event_status,
            recorded_at: entry.recorded_at,
        })
        .collect())
}

fn entry_is_recordable(store: &Store, event_id: &str) -> bool {
    if let Ok(Some(decision)) = store.latest_event_recordability_decision(event_id) {
        return decision.recordable;
    }
    store
        .event_analysis_detail(event_id)
        .ok()
        .flatten()
        .and_then(|detail| detail.result_json)
        .and_then(|raw| EventAnalysisV1::parse(&raw).ok())
        .map(|analysis| analysis.recordable)
        .unwrap_or(true)
}

fn effective_event_recordability(
    store: &Store,
    event_id: &str,
    analysis: Option<&EventAnalysisV1>,
) -> Result<(bool, String, String)> {
    if let Some(decision) = store.latest_event_recordability_decision(event_id)? {
        return Ok((decision.recordable, decision.kind, "manual".to_string()));
    }
    if let Some(analysis) = analysis {
        return Ok((
            analysis.recordable,
            analysis.kind.clone(),
            "analysis".to_string(),
        ));
    }
    Ok((true, "event".to_string(), "default".to_string()))
}

/// 回顾页周/月拼装统计（R1）：本地 SQL 实时聚合，无 AI、无持久化。
/// 口径：窗口内事件按 recordability 判定（manual 决策覆盖），与
/// daily_overview 的 entries 同口径；待办完成日以 updated_at 为准
/// （todos 无 completed_at，如实近似，见 storage::review）。
#[derive(Clone, Debug)]
pub struct NamedCountDto {
    pub label: String,
    pub count: i64,
}

#[derive(Clone, Debug)]
pub struct DayCountDto {
    pub date: String,
    pub count: i64,
}

#[derive(Clone, Debug)]
pub struct PeriodStatsDto {
    pub period_type: String,
    pub start_date: String,
    pub end_date: String,
    pub event_count: i64,
    pub todo_done_count: i64,
    pub by_event_type: Vec<NamedCountDto>,
    pub by_person: Vec<NamedCountDto>,
    pub by_project: Vec<NamedCountDto>,
    pub by_activity: Vec<NamedCountDto>,
    pub daily_counts: Vec<DayCountDto>,
}

/// 聚合列表上限（确定性排序后截断；UI 层如需可再收）。
const PERIOD_TOP_N: usize = 12;

pub fn get_period_stats(period_type: String, date: String) -> Result<PeriodStatsDto> {
    use chrono::Datelike;
    let period_type = period_type.trim().to_string();
    anyhow::ensure!(
        period_type == "week" || period_type == "month",
        "不支持的时间粒度：仅 week / month"
    );
    let date = chrono::NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d")
        .map_err(|_| anyhow::anyhow!("日期必须是 YYYY-MM-DD"))?;
    let (start, end) = if period_type == "week" {
        (
            date - chrono::Duration::days(date.weekday().num_days_from_monday() as i64),
            date + chrono::Duration::days(6 - date.weekday().num_days_from_monday() as i64),
        )
    } else {
        let next_month = if date.month() == 12 {
            chrono::NaiveDate::from_ymd_opt(date.year() + 1, 1, 1).unwrap()
        } else {
            chrono::NaiveDate::from_ymd_opt(date.year(), date.month() + 1, 1).unwrap()
        };
        (
            chrono::NaiveDate::from_ymd_opt(date.year(), date.month(), 1).unwrap(),
            next_month.pred_opt().unwrap(),
        )
    };
    let days = (end - start).num_days() + 1;
    anyhow::ensure!(days <= 62, "统计窗口不得超过 62 天");
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let mut per_type: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    let mut per_person: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    let mut per_project: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    let mut per_activity: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    let mut per_day: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
    for day in 0..days {
        per_day.insert((start + chrono::Duration::days(day)).to_string(), 0);
    }

    let mut event_count = 0i64;
    for event in store.events_between(start, end)? {
        // 与 daily_overview 的 entries 同一判定链（manual 决策 → 分析 → 默认）。
        if !entry_is_recordable(&store, &event.id) {
            continue;
        }
        event_count += 1;
        // 本地日界归属：rfc3339 转本地时区取日期；解析失败取头 10 字符兜底。
        let day = chrono::DateTime::parse_from_rfc3339(&event.recorded_at)
            .map(|dt| dt.with_timezone(&chrono::Local).date_naive().to_string())
            .unwrap_or_else(|_| event.recorded_at.chars().take(10).collect());
        *per_day.entry(day).or_insert(0) += 1;
        let parsed = store
            .event_analysis_detail(&event.id)?
            .and_then(|detail| detail.result_json)
            .and_then(|raw| EventAnalysisV1::parse(&raw).ok());
        if let Some(analysis) = parsed {
            if !analysis.event_type.trim().is_empty() {
                *per_type
                    .entry(analysis.event_type.trim().to_string())
                    .or_insert(0) += 1;
            }
            for person in analysis.people {
                let key = person.trim();
                if !key.is_empty() {
                    *per_person.entry(key.to_string()).or_insert(0) += 1;
                }
            }
            for project in analysis.projects {
                let key = project.trim();
                if !key.is_empty() {
                    *per_project.entry(key.to_string()).or_insert(0) += 1;
                }
            }
            for activity in analysis.activities {
                let key = activity.trim();
                if !key.is_empty() {
                    *per_activity.entry(key.to_string()).or_insert(0) += 1;
                }
            }
        }
    }

    let rank = |map: std::collections::HashMap<String, i64>| {
        let mut rows: Vec<(String, i64)> = map.into_iter().collect();
        // 确定性排序：次数降序、同次数字典序；top N 截断。
        rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        rows.truncate(PERIOD_TOP_N);
        rows.into_iter()
            .map(|(label, count)| NamedCountDto { label, count })
            .collect::<Vec<_>>()
    };

    Ok(PeriodStatsDto {
        period_type,
        start_date: start.to_string(),
        end_date: end.to_string(),
        event_count,
        todo_done_count: store.todos_done_between(start, end)?,
        by_event_type: rank(per_type),
        by_person: rank(per_person),
        by_project: rank(per_project),
        by_activity: rank(per_activity),
        daily_counts: per_day
            .into_iter()
            .map(|(date, count)| DayCountDto { date, count })
            .collect(),
    })
}

pub fn get_daily_overview(date: String) -> Result<DailyOverviewDto> {
    let date = chrono::NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d")
        .map_err(|_| anyhow::anyhow!("日期必须是 YYYY-MM-DD"))?;
    let date_text = date.format("%Y-%m-%d").to_string();
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let entries = store
        .daily_entries(date)?
        .into_iter()
        .filter(|entry| entry_is_recordable(&store, &entry.event_id))
        .into_iter()
        .map(|entry| DailyEntryDto {
            event_id: entry.event_id,
            input_id: entry.input_id,
            message_id: entry.message_id,
            raw_text: entry.raw_text,
            source: entry.source,
            event_status: entry.event_status,
            recorded_at: entry.recorded_at,
        })
        .collect::<Vec<_>>();
    let review = store
        .latest_daily_review(date)?
        .map(|record| {
            let parsed =
                DailyReviewV1::parse(&record.result_json, &date_text, &record.source_event_ids)?;
            let map_items = |items: Vec<DailyReviewItemV1>| {
                items
                    .into_iter()
                    .map(|item| DailyReviewItemDto {
                        text: item.text,
                        source_event_ids: item.source_event_ids,
                    })
                    .collect()
            };
            Ok::<_, anyhow::Error>(DailyReviewDto {
                id: record.id,
                date: record.date,
                prompt_version: record.prompt_version,
                created_at: record.created_at,
                accomplishments: map_items(parsed.accomplishments),
                ideas_decisions: map_items(parsed.ideas_decisions),
                people_projects: map_items(parsed.people_projects),
                follow_ups: map_items(parsed.follow_ups),
            })
        })
        .transpose()?;
    let todos = store
        .list_todos(None)?
        .into_iter()
        .filter(|todo| {
            todo.related_event_id
                .as_deref()
                .is_some_and(|event_id| entries.iter().any(|entry| entry.event_id == event_id))
                || todo
                    .due_at
                    .as_deref()
                    .is_some_and(|due_at| due_at.starts_with(&date_text))
        })
        .map(TodoDto::from)
        .collect();
    // D6 回顾勾连 1：当日命中的提醒与聚合（同款本地日界，只读）。
    let reminder_hits = store
        .rule_hits_on(date)?
        .into_iter()
        .map(|hv| crate::api::rules::RuleHitDto {
            id: hv.id,
            rule_id: hv.rule_id,
            rule_content: hv.rule_content,
            status: hv.status,
            created_at: hv.created_at,
            feedback: hv.feedback,
        })
        .collect::<Vec<_>>();
    // D6 回顾勾连 2：共同记忆回引（逻辑在 storage 层，可测）。
    let memory_echo = store
        .memory_echo_for(date, 3)?
        .into_iter()
        .map(|(event_id, recorded_at, raw_text)| MemoryEchoDto {
            event_id,
            recorded_at,
            raw_text,
        })
        .collect();
    Ok(DailyOverviewDto {
        date: date_text,
        entries,
        review,
        todos,
        reminder_hits,
        memory_echo,
    })
}

pub fn save_daily_review(
    date: String,
    result_json: String,
    prompt_version: String,
    source_event_ids: Vec<String>,
) -> Result<String> {
    let date = chrono::NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d")
        .map_err(|_| anyhow::anyhow!("日期必须是 YYYY-MM-DD"))?;
    if prompt_version.trim() != DAILY_REVIEW_VERSION {
        anyhow::bail!("prompt_version 必须是 {DAILY_REVIEW_VERSION}");
    }
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let allowed_sources = store
        .daily_entries(date)?
        .into_iter()
        .filter(|entry| entry_is_recordable(&store, &entry.event_id))
        .into_iter()
        .map(|entry| entry.event_id)
        .collect::<Vec<_>>();
    let parsed = DailyReviewV1::parse(
        &result_json,
        &date.format("%Y-%m-%d").to_string(),
        &allowed_sources,
    )?;
    let referenced = parsed
        .accomplishments
        .iter()
        .chain(parsed.ideas_decisions.iter())
        .chain(parsed.people_projects.iter())
        .chain(parsed.follow_ups.iter())
        .flat_map(|item| item.source_event_ids.iter())
        .collect::<std::collections::HashSet<_>>();
    if referenced
        .iter()
        .any(|event_id| !source_event_ids.iter().any(|source| source == *event_id))
    {
        anyhow::bail!("daily review source_event_ids 未覆盖结论引用");
    }
    store.save_daily_review(date, DAILY_REVIEW_VERSION, &result_json, &source_event_ids)
}

pub fn generate_daily_review(date: String) -> Result<DailyReviewResult> {
    let date = chrono::NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d")
        .map_err(|_| anyhow::anyhow!("日期必须是 YYYY-MM-DD"))?;
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let Some(provider_config) = store.active_ai_provider_config()? else {
        return Ok(DailyReviewResult::NoProvider);
    };
    let provider = OpenAiCompatibleProvider::new(OpenAiCompatibleConfig {
        base_url: provider_config.base_url,
        api_key: provider_config.api_key,
        model: provider_config.model,
        temperature: provider_config.temperature as f32,
        max_tokens: provider_config.max_tokens.map(|value| value as u32),
        context_window: provider_config.context_window.map(|v| v as u32),
    })?;
    generate_daily_review_with_provider(&store, date, &provider)
}

fn generate_daily_review_with_provider(
    store: &Store,
    date: chrono::NaiveDate,
    provider: &dyn AiProvider,
) -> Result<DailyReviewResult> {
    let entries = store
        .daily_entries(date)?
        .into_iter()
        .filter(|entry| entry_is_recordable(store, &entry.event_id))
        .collect::<Vec<_>>();
    if entries.is_empty() {
        return Ok(DailyReviewResult::NoEntries);
    }
    let date_text = date.format("%Y-%m-%d").to_string();
    let source_event_ids = entries
        .iter()
        .map(|entry| entry.event_id.clone())
        .collect::<Vec<_>>();
    let facts = entries
        .iter()
        .map(|entry| {
            let analysis = store.event_analysis_detail(&entry.event_id)?;
            let analysis_text = analysis
                .and_then(|detail| detail.result_json)
                .unwrap_or_else(|| "null".to_string());
            Ok(format!(
                "event_id={}\nrecorded_at={}\nsource={}\nraw_text={}\nanalysis={}",
                entry.event_id, entry.recorded_at, entry.source, entry.raw_text, analysis_text
            ))
        })
        .collect::<Result<Vec<_>>>()?
        .join("\n\n");
    let prompt = format!(
        "根据以下 {date_text} 的个人事实生成每日回顾。只返回 JSON 对象，不要 Markdown，不要增加字段。schema_version 固定为 daily-review-v1，date 固定为 {date_text}。字段 accomplishments、ideas_decisions、people_projects、follow_ups 都是数组；每项必须是 {{\"text\":string,\"source_event_ids\":[string]}}，来源 ID 必须来自输入。没有可靠内容的分类返回空数组，不要推测。\n\n{facts}"
    );
    let candidates = crate::knowledge::select_knowledge(store, &facts, "daily_review", 2500)?;
    let mut review_context = vec![
        ContextMessage::new("system", crate::knowledge::CITATION_INSTRUCTIONS),
        ContextMessage::new("system", crate::knowledge::context_text(&candidates)?),
        ContextMessage::new("user", prompt),
    ];
    crate::knowledge::bound_model_context(&mut review_context, 24000)?;
    let reply = provider.generate_reply(review_context)?;
    let mut parsed = DailyReviewV1::parse(&reply.content, &date_text, &source_event_ids)?;
    let current = crate::knowledge::current_candidates(store, &candidates)?;
    let mut cited = Vec::new();
    for item in parsed
        .accomplishments
        .iter_mut()
        .chain(parsed.ideas_decisions.iter_mut())
        .chain(parsed.people_projects.iter_mut())
        .chain(parsed.follow_ups.iter_mut())
    {
        let (text, used) = crate::knowledge::validate_answer_citations(&item.text, &current);
        item.text = text;
        for citation in used {
            if !cited
                .iter()
                .any(|c: &crate::knowledge::KnowledgeCitation| c.page_slug == citation.page_slug)
            {
                cited.push(citation);
            }
        }
    }
    let normalized = serde_json::to_string(&parsed)?;
    let id = store.save_daily_review(date, DAILY_REVIEW_VERSION, &normalized, &source_event_ids)?;
    crate::knowledge::record_usage(store, "daily_review", &id, &candidates, &cited)?;
    Ok(DailyReviewResult::Created { id })
}

/// 知识库页聚合视图（D4）：哪些事件的分析 guidance 引用了此页。
#[derive(Debug, serde::Serialize)]
pub struct PageGuidanceDto {
    pub analysis_id: String,
    pub event_id: String,
    pub recorded_at: String,
    pub note: Option<String>,
    pub lessons: Vec<String>,
    pub follow_ups_refined: Vec<String>,
    pub feedback: Option<String>,
}

pub fn list_page_guidance(slug: String) -> Result<Vec<PageGuidanceDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    Ok(store
        .page_guidance_items(slug.trim())?
        .into_iter()
        .map(|item| PageGuidanceDto {
            analysis_id: item.analysis_id,
            event_id: item.event_id,
            recorded_at: item.recorded_at,
            note: item.note,
            lessons: item.lessons,
            follow_ups_refined: item.follow_ups_refined,
            feedback: item.feedback,
        })
        .collect())
}

/// 设置单条 guidance 条目的采纳反馈（遗留收口切片 B，只记录不推断）；
/// feedback None = 清除（toggle off）。未知值 fail-closed 报错。
pub fn set_guidance_feedback(
    analysis_id: String,
    slug: String,
    note: Option<String>,
    feedback: Option<String>,
) -> Result<bool> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    Ok(store.set_guidance_feedback(
        analysis_id.trim(),
        slug.trim(),
        note.as_deref().map(str::trim).filter(|n| !n.is_empty()),
        feedback.as_deref().map(str::trim).filter(|f| !f.is_empty()),
    )?)
}

/// 情绪标注列表（D5 用户模型面板）：最近分析的 emotion 派生。
pub fn list_event_emotions() -> Result<Vec<EventEmotionDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    Ok(store
        .list_event_emotions(100)?
        .into_iter()
        .map(|item| EventEmotionDto {
            id: item.analysis_id,
            event_id: item.event_id,
            recorded_at: item.recorded_at,
            target: item.target,
            feeling: item.feeling,
        })
        .collect())
}

/// 删除一条情绪标注：只编辑分析产物的 emotion 字段，事件真源不动。
pub fn delete_event_emotion(id: String, target: Option<String>, feeling: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let removed = store.delete_event_emotion(id.trim(), target.as_deref(), feeling.trim())?;
    anyhow::ensure!(removed, "情绪标注不存在或已被删除");
    Ok(())
}

/// 情感回访偏好（D6：开关键 + 频率上限；面板可见可改）。
#[derive(Clone, Debug)]
pub struct CompanionPrefsDto {
    pub revisit_enabled: bool,
    pub revisit_interval_days: i64,
}

/// 一次回访素材（D6）：被记住的事件 +（若有）当时情绪。
#[derive(Clone, Debug)]
pub struct CompanionRevisitDto {
    pub event_id: String,
    pub recorded_at: String,
    pub excerpt: String,
    pub feeling: Option<String>,
}

/// 读取回访偏好。
pub fn get_companion_prefs() -> Result<CompanionPrefsDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let settings = store.companion_settings()?;
    Ok(CompanionPrefsDto {
        revisit_enabled: settings.revisit_enabled,
        revisit_interval_days: settings.revisit_interval_days,
    })
}

/// 更新回访偏好（间隔即频率下限；关 = 之后 check 零查询零成本）。
pub fn set_companion_prefs(enabled: bool, interval_days: i64) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.set_companion_settings(enabled, interval_days)?;
    Ok(())
}

/// 打开应用时的一次回访检查：返回可展示的温和回引（有则记账本次展示）；
/// 关闭/未到期/无素材都返回 NULL。
pub fn check_companion_revisit() -> Result<Option<CompanionRevisitDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    Ok(store
        .check_companion_revisit()?
        .map(|item| CompanionRevisitDto {
            event_id: item.event_id,
            recorded_at: item.recorded_at,
            excerpt: item.excerpt,
            feeling: item.feeling,
        }))
}

pub fn get_event_analysis_detail(event_id: String) -> Result<Option<EventAnalysisDetailDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let Some(detail) = store.event_analysis_detail(event_id.trim())? else {
        return Ok(None);
    };
    let analysis = detail
        .result_json
        .as_deref()
        .and_then(|raw| EventAnalysisV1::parse(raw).ok());
    let (effective_recordable, effective_kind, recordability_source) =
        effective_event_recordability(&store, &detail.event_id, analysis.as_ref())?;
    Ok(Some(EventAnalysisDetailDto {
        event_id: detail.event_id,
        raw_text: detail.raw_text,
        source: detail.source,
        recorded_at: detail.recorded_at,
        event_status: detail.event_status,
        job_status: detail.job_status,
        attempts: detail.attempts,
        last_error: detail.last_error,
        available_at: detail.available_at,
        prompt_version: detail.prompt_version,
        analysis_created_at: detail.analysis_created_at,
        schema_version: analysis.as_ref().map(|value| value.schema_version.clone()),
        event_type: analysis.as_ref().map(|value| value.event_type.clone()),
        confidence: analysis.as_ref().map(|value| value.confidence),
        summary: analysis.as_ref().map(|value| value.summary.clone()),
        clarifications: analysis
            .as_ref()
            .map_or_else(Vec::new, |value| value.clarifications.clone()),
        people: analysis
            .as_ref()
            .map_or_else(Vec::new, |value| value.people.clone()),
        projects: analysis
            .as_ref()
            .map_or_else(Vec::new, |value| value.projects.clone()),
        activities: analysis
            .as_ref()
            .map_or_else(Vec::new, |value| value.activities.clone()),
        follow_ups: analysis
            .as_ref()
            .map_or_else(Vec::new, |value| value.follow_ups.clone()),
        guidance: analysis.as_ref().and_then(|value| {
            value.guidance.as_ref().map(|g| GuidanceDto {
                related: g
                    .related
                    .iter()
                    .map(|r| GuidanceRefDto {
                        slug: r.slug.clone(),
                        note: r.note.clone(),
                    })
                    .collect(),
                lessons: g.lessons.clone(),
                follow_ups_refined: g.follow_ups_refined.clone(),
            })
        }),
        emotion: analysis.as_ref().and_then(|value| {
            value.emotion.as_ref().map(|tags| {
                tags.iter()
                    .map(|tag| EmotionTagDto {
                        target: tag.target.clone(),
                        feeling: tag.feeling.clone(),
                    })
                    .collect()
            })
        }),
        recordable: analysis.as_ref().map(|value| value.recordable),
        kind: analysis.as_ref().map(|value| value.kind.clone()),
        effective_recordable,
        effective_kind,
        recordability_source,
    }))
}

pub fn get_message_recordability(message_id: String) -> Result<Option<MessageRecordabilityDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let Some(event_id) = store.event_id_for_message(message_id.trim())? else {
        return Ok(None);
    };
    let detail = store
        .event_analysis_detail(&event_id)?
        .context("关联事件不存在")?;
    let analysis = detail
        .result_json
        .as_deref()
        .and_then(|raw| EventAnalysisV1::parse(raw).ok());
    let (recordable, kind, source) =
        effective_event_recordability(&store, &event_id, analysis.as_ref())?;
    Ok(Some(MessageRecordabilityDto {
        message_id,
        event_id,
        recordable,
        kind,
        source,
        job_status: detail.job_status,
        summary: analysis.map(|value| value.summary),
    }))
}

pub fn set_event_recordability(
    event_id: String,
    recordable: bool,
) -> Result<MessageRecordabilityDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.set_event_recordability(event_id.trim(), recordable, "manual-ui")?;
    let detail = store
        .event_analysis_detail(event_id.trim())?
        .context("事件不存在")?;
    let analysis = detail
        .result_json
        .as_deref()
        .and_then(|raw| EventAnalysisV1::parse(raw).ok());
    let (recordable, kind, source) =
        effective_event_recordability(&store, event_id.trim(), analysis.as_ref())?;
    Ok(MessageRecordabilityDto {
        message_id: String::new(),
        event_id,
        recordable,
        kind,
        source,
        job_status: detail.job_status,
        summary: analysis.map(|value| value.summary),
    })
}

pub fn reanalyze_event(event_id: String) -> Result<bool> {
    let config = crate::config::AppConfig::load()?;
    Store::open(&config.database_path)?.requeue_event_analysis(event_id.trim())
}

/// 事件摘要 DTO（遗留收口切片 C：事件列表卡；详情走
/// get_event_analysis_detail，列表不拖分析结果）。
#[derive(Clone, Debug)]
pub struct EventSummaryDto {
    pub id: String,
    pub raw_text: String,
    pub recorded_at: String,
    pub source: String,
    pub status: String,
}

/// 版本可溯源：设置页展示「这次构建对应哪个 git 提交」。pubspec 版本从
/// 未动过（恒 1.0.0）无区分度，编译期抓短 hash 才有报障价值。
pub fn build_hash() -> String {
    env!("ELSEWHEN_BUILD_HASH", "build.rs 未注入构建 hash").to_string()
}

/// 最近事件读面（事件详情视图列表）：days 天窗口内按记录时间倒序 limit 条。
pub fn list_recent_events(days: i64, limit: i64) -> Result<Vec<EventSummaryDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let days = days.clamp(0, 3650);
    let limit = limit.clamp(1, 200) as usize;
    Ok(store
        .recent_events(days, limit)?
        .into_iter()
        .map(|e| EventSummaryDto {
            id: e.id,
            raw_text: e.raw_text,
            recorded_at: e.recorded_at,
            source: e.source,
            status: e.status,
        })
        .collect())
}

/// List all events
pub fn list_events() -> Result<Vec<EventDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let events = store.list_events()?;

    Ok(events
        .into_iter()
        .map(|e| EventDto {
            id: e.id,
            raw_text: e.raw_text,
            recorded_at: e.recorded_at.clone(),
            occurred_at: e.recorded_at,
            source: e.source,
            status: e.status,
        })
        .collect())
}

/// List completed analyses
pub fn list_analyses() -> Result<Vec<AnalysisDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let analyses = store.list_analyses()?;

    Ok(analyses
        .into_iter()
        .map(|a| AnalysisDto {
            event_type: a.event_type,
            confidence: a.confidence,
            summary: a.raw_text,
            clarifications: serde_json::from_str(&a.clarifications).unwrap_or_default(),
        })
        .collect())
}

pub fn get_analysis_job_stats() -> Result<AnalysisJobStatsDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let stats = store.analysis_job_stats()?;
    Ok(AnalysisJobStatsDto {
        pending: stats.pending,
        running: stats.running,
        retry: stats.retry,
        succeeded: stats.succeeded,
        failed: stats.failed,
    })
}

/// Trigger AI analysis for pending events
/// Returns `AnalysisTriggerResult`（结构化状态，取代字符串契约）。
pub fn trigger_analysis() -> Result<AnalysisTriggerResult> {
    let config = crate::config::AppConfig::load()?;
    // 后台路径不做启动写入：库待升级时本轮直接跳过，交由 init 完成迁移。
    let Some(store) = Store::open_background(&config.database_path)? else {
        return Ok(AnalysisTriggerResult::Processed { count: 0 });
    };
    let Some(_sync_guard) = crate::sync::runtime::background_guard(&config, &store)? else {
        return Ok(AnalysisTriggerResult::Processed { count: 0 });
    };
    if !store.background_processing_allowed()? {
        return Ok(AnalysisTriggerResult::Processed { count: 0 });
    }
    let provider_config = store.active_ai_provider_config()?;
    let Some(provider_config) = provider_config else {
        return Ok(AnalysisTriggerResult::NoProvider);
    };
    let provider = OpenAiCompatibleProvider::new(OpenAiCompatibleConfig {
        base_url: provider_config.base_url,
        api_key: provider_config.api_key,
        model: provider_config.model,
        temperature: provider_config.temperature as f32,
        max_tokens: provider_config.max_tokens.map(|v| v as u32),
        context_window: provider_config.context_window.map(|v| v as u32),
    })?;
    // 每次 worker tick 先重新清扫遗留的 running 任务：GUI 只在启动时恢复一次，
    // 若上一次队列运行中途退出，被 claim 的任务会孤悬到下次启动。
    crate::storage::with_busy_retry(|| {
        store
            .recover_interrupted_analysis_jobs()
            .context("analysis worker: recover interrupted jobs")
    })?;
    let processed = process_analysis_queue(&store, &provider)?;
    // 对话影子校验判定阶段（迁移 54）：预筛在聊天生成链已落 pending，
    // 这里用同一 provider 逐条判定适用性。失败不档其它队列（下轮重试）。
    let judged = process_chat_rule_check_queue(&store, &provider)?;
    Ok(AnalysisTriggerResult::Processed {
        count: processed + judged,
    })
}

/// 与 process_analysis_queue 同形态：封顶 20 条/轮、单条失败记更新不阻塞。
/// 注：confidence 解析失败按 None（字段可选）。
fn process_chat_rule_check_queue(store: &Store, provider: &dyn AiProvider) -> Result<i64> {
    let Ok(hits) = store.pending_chat_hits(20) else {
        return Ok(0);
    };
    let mut judged = 0i64;
    for hit in hits {
        let (applicable, confidence) = match build_judge_context(store, &hit)
            .and_then(|context| {
                provider
                    .generate_reply(context)
                    .map_err(|e| anyhow::anyhow!("影子判定调用失败：{e}"))
            })
            .and_then(|reply| parse_shadow_judgement(&reply.content))
        {
            Ok(pair) => pair,
            Err(_error) => continue, // 下轮重试：pending 不动，不毒化其它轮次。
        };
        if store
            .resolve_chat_hit(&hit.id, applicable, confidence)
            .is_ok()
        {
            judged += 1;
        }
    }
    Ok(judged)
}

/// 组装判定上下文（极简：系统纪律 + 规则 + 消息原文，封顶 4K tokens）。
fn build_judge_context(_store: &Store, hit: &PendingChatHit) -> Result<Vec<ContextMessage>> {
    let mut context = vec![
        ContextMessage::new(
            "system",
            "你只输出严格 JSON 对象（含 applicable 布尔与 confidence 数字 0..1），不要 Markdown 围栏。判定不确定时宁可 applicable=false（宁缺毋滥）。",
        ),
        ContextMessage::new(
            "user",
            format!(
                "判定下面的对话消息是否需要遵守这条个人经验规则。需要遵守/适用为 true；无关、纯属闲聊或规则不适用为 false。\n\n规则：{}\n\n消息：{}",
                hit.rule_content, hit.message_text
            ),
        ),
    ];
    crate::knowledge::bound_model_context(&mut context, 4000)?;
    Ok(context)
}

/// 解析影子判定回包（宽松：confidence 缺失按 None；applicable 缺失报错重试）。
fn parse_shadow_judgement(content: &str) -> Result<(bool, Option<f64>)> {
    let value: serde_json::Value = serde_json::from_str(content.trim())?;
    let applicable = value
        .get("applicable")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| anyhow::anyhow!("影子判定缺少 applicable 字段"))?;
    let confidence = value.get("confidence").and_then(serde_json::Value::as_f64);
    Ok((applicable, confidence))
}

fn process_analysis_queue(store: &Store, provider: &dyn AiProvider) -> Result<i64> {
    let mut processed = 0;
    // Bound each invocation even when producers keep adding work or retries
    // become available while a slow provider is processing other records.
    let stats = store.analysis_job_stats()?;
    // 总墙钟时间上限：provider 单次超时可到 60s，若一批 50 条全部超时，
    // 无上限会占住 worker 约 50 分钟且中途无中断（P2 worker 限时）。
    // 每完成一条记录后检查，超限即停，余量留待下次 tick。
    let batch_start = std::time::Instant::now();
    for _ in 0..(stats.pending + stats.retry).min(50) {
        if batch_start.elapsed().as_secs() >= ANALYSIS_BATCH_MAX_SECS {
            break;
        }
        let Some(job) = crate::storage::with_busy_retry(|| {
            store
                .claim_analysis_job()
                .context("analysis worker: claim job")
        })?
        else {
            break;
        };
        // 上下文构建属于分析任务的一部分：失败不能把已 claim 的 job
        // 孤悬为 running（否则要等下次启动才 recover），记入 last_error 放回重试。
        let (context, candidates) = match decision_support_material(store, &job.raw_text) {
            Ok(context) => context,
            Err(error) => {
                store.fail_analysis(&job, &format!("决策上下文构建失败：{error}"))?;
                continue;
            }
        };
        // 规则命中判定段（D1）：pending 命中随本次分析一并判定，不新增调用面。
        let pending_hits = store.pending_rule_hits_for_event(&job.event_id)?;
        let rules_section = if pending_hits.is_empty() {
            String::new()
        } else {
            let listed = pending_hits
                .iter()
                .map(|hit| format!("- rule_id={}：{}", hit.rule_id, hit.rule_content))
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                "\n\n以下规则在入库预筛时命中本记录，请逐条判定其是否真正适用本记录，并在输出的 rule_confirmations 数组中逐条回填（rule_id 原样返回；confidence 0..1 表示适用置信度）：\n{listed}\n"
            )
        };
        let prompt = format!(
            "分析以下个人记录，只返回 JSON 对象，不要 Markdown，也不要增加字段。schema_version 固定为 event-analysis-v3。字段必须包含 schema_version、recordable(boolean)、kind(event/discussion/chitchat/meta)、event_type(string)、confidence(number 0..1)、summary(string)、clarifications(array of strings)、people(array of strings)、projects(array of strings)、activities(array of strings)、follow_ups(array of strings)；仅当下方附有规则命中时才包含 rule_confirmations(array of {{rule_id: string, applicable: boolean, confidence: number 0..1}})。情绪走红线：仅当记录客观存在对具体人或事的情绪反应时才可选包含 emotion 数组：[{{\"target\": string, \"feeling\": string}}]——feeling 用简短文本描述感受（如「被理解」「焦虑」「挫败」），绝对禁止打分或生成情绪分析报告；target 填相关的人或事（与 people/projects 一致，对事件本身的感受填空字符串）；没有则整个省略 emotion 字段。只有当决策辅助上下文与你确有关联时才可选包含 guidance 对象：{{\"related\":[{{\"slug\":string,\"note\":string}}],\"lessons\":[string],\"follow_ups_refined\":[string]}}：related[].slug 只能取自决策辅助上下文中出现的知识库页 slug，note 写为什么相关/可复用什么（一句话）；lessons 写可复用经验（不是重复记录内容）；follow_ups_refined 是对 follow_ups 的精炼合并；无关联时整个省略 guidance 字段，不得杜撰 slug。projects 只填写明确的长期项目/产品/组织；网站改版、联调、任务、沟通、会议等动作或事项必须放入 activities，不要放入 projects。activities 提取简短常见的动词短语（如 散步、跑步、阅读、写代码），不要用描述性长句——统计按词匹配，词汇不一致就数不到。只有客观经历、决定、行动或进展 recordable=true/kind=event；对 AI 回复评价、闲聊、纯提问或元对话 recordable=false，并保留简短 summary。\n\n决策辅助上下文（只作参考，不能据此臆测新事实）：\n{}\n\n记录：{}{}",
            context, job.raw_text, rules_section
        );
        let mut analysis_context = vec![
            ContextMessage::new("system", crate::knowledge::CITATION_INSTRUCTIONS),
            ContextMessage::new("user", prompt),
        ];
        if let Err(error) = crate::knowledge::bound_model_context(&mut analysis_context, 24000) {
            store.fail_analysis(&job, &error.to_string())?;
            continue;
        }
        match provider.generate_reply(analysis_context) {
            Ok(reply) => match EventAnalysisV1::parse(&reply.content) {
                Ok(mut value) => {
                    // guidance 护栏（D4）：杜撰 slug 丢弃、条数与文本封顶；
                    // 清洗后为空则整字段省略——派生层不存垃圾。
                    wash_guidance(store, &mut value);
                    wash_emotion(&mut value);
                    let current = crate::knowledge::current_candidates(store, &candidates)?;
                    let mut cited = Vec::new();
                    for field in std::iter::once(&mut value.summary)
                        .chain(value.clarifications.iter_mut())
                        .chain(value.follow_ups.iter_mut())
                    {
                        let (text, used) =
                            crate::knowledge::validate_answer_citations(field, &current);
                        *field = text;
                        for citation in used {
                            if !cited.iter().any(|c: &crate::knowledge::KnowledgeCitation| {
                                c.page_slug == citation.page_slug
                            }) {
                                cited.push(citation);
                            }
                        }
                    }
                    store.complete_analysis(
                        &job,
                        &value.schema_version,
                        &serde_json::to_string(&value)?,
                    )?;
                    // 规则判定回填（D1）：失败不影响已入库分析；pending 命中留待
                    // 下次 reanalyze 补判（幂等——只处理 pending 行）。
                    if !value.rule_confirmations.is_empty() {
                        let confirmations: Vec<crate::storage::rule_triggers::RuleConfirmation> =
                            value
                                .rule_confirmations
                                .iter()
                                .map(|c| crate::storage::rule_triggers::RuleConfirmation {
                                    rule_id: c.rule_id.clone(),
                                    applicable: c.applicable,
                                    confidence: c.confidence,
                                })
                                .collect();
                        let _ = store.apply_rule_confirmations(&job.event_id, &confirmations);
                    }
                    crate::knowledge::record_usage(
                        store,
                        "analysis",
                        &job.event_id,
                        &candidates,
                        &cited,
                    )?;
                    // 用户模型增量维护（D2）：@/# 双向标注的组合是用户写死的
                    // 权威实体，双方页面都已存在时直接增量入库（不用等确认）；
                    // 其余组合保留草拟确认卡（低置信才草拟，防打扰）。
                    if value.recordable && !value.people.is_empty() && !value.projects.is_empty() {
                        let annotations = crate::event::parse_annotations_many(std::iter::once(
                            job.raw_text.as_str(),
                        ));
                        let mut draft_pairs: Vec<(String, String)> = Vec::new();
                        for person in &value.people {
                            let person_authoritative =
                                annotations.people.iter().any(|p| p == person);
                            for target in &value.projects {
                                let target_authoritative =
                                    annotations.targets.iter().any(|t| t == target);
                                let authoritative = person_authoritative && target_authoritative;
                                if authoritative
                                    && try_auto_upsert_annotated_relation(
                                        store,
                                        person,
                                        target,
                                        &job.event_id,
                                    )
                                    .unwrap_or(false)
                                {
                                    continue; // 自动入库成功，不再出草拟卡
                                }
                                draft_pairs.push((person.clone(), target.clone()));
                            }
                        }
                        if !draft_pairs.is_empty() {
                            if let Some(conversation_id) =
                                store.conversation_id_for_event(&job.event_id)?
                            {
                                let exists = store.action_exists_for_event(
                                    &conversation_id,
                                    "propose_people_relations",
                                    &job.event_id,
                                )?;
                                if !exists {
                                    let people: Vec<_> = draft_pairs
                                        .iter()
                                        .map(|(person, _)| serde_json::json!({"name": person}))
                                        .collect();
                                    let relations: Vec<_> = draft_pairs
                                        .iter()
                                        .map(|(person, target)| {
                                            serde_json::json!({
                                                "person": person,
                                                "target": target,
                                                "relation": "参与"
                                            })
                                        })
                                        .collect();
                                    let args = serde_json::json!({
                                        "people": people,
                                        "relations": relations,
                                        "source_event_id": job.event_id
                                    });
                                    store.create_pending_action(
                                        &conversation_id,
                                        "propose_people_relations",
                                        &args.to_string(),
                                    )?;
                                }
                            }
                        }
                    }
                    processed += 1;
                }
                Err(error) => store.fail_analysis(
                    &job,
                    &format!("AI 返回不符合 {EVENT_ANALYSIS_VERSION}: {error}"),
                )?,
            },
            Err(error) => store.fail_analysis(&job, &error.to_string())?,
        }
    }
    Ok(processed as i64)
}

/// D2 增量合并的自动入库窄集：person/target 双双来自 @/# 标注（用户写死的
/// 权威实体），且对应页面都已存在时，直接以「参与」关系入库（confidence 4、
/// 记 source_event_id）。任何前置不满足（页面缺失等）都返回 false，走草拟卡。
/// 返回 Ok(false) 不算错误——只是「这组关系不适合自动入库」。
fn try_auto_upsert_annotated_relation(
    store: &Store,
    person: &str,
    target: &str,
    event_id: &str,
) -> Result<bool> {
    // D3 归一管道：标题/别名命中即写 canonical slug；查不到或多义则降级草拟。绝
    // 不盲写 slugify 直译 slug——那正是重复实体的成因。
    let (Some(from_slug), Some(to_slug)) = (
        store.resolve_entity_slug("person", person)?,
        store.resolve_entity_slug("topic", target)?,
    ) else {
        return Ok(false);
    };
    let from = store.get_wiki_page(&from_slug)?;
    let to = store.get_wiki_page(&to_slug)?;
    let (Some(from), Some(to)) = (from, to) else {
        return Ok(false);
    };
    store.upsert_relation(&crate::storage::RelationDraft {
        from_slug,
        from_kind: from.kind,
        to_slug,
        to_kind: to.kind,
        relation: "参与".to_string(),
        note: None,
        confidence: 4,
        source_conversation_id: None,
        source_event_id: Some(event_id.to_string()),
    })?;
    Ok(true)
}

/// Build bounded, read-only context for Phase 4C. This makes existing rules and
/// knowledge visible at the decision point while leaving all writes behind the
/// existing confirmation gates.
/// 决策辅助上下文（D4 扩展：用户模型关系边 + 近期记录条目）。
/// 段级 fail-soft：任何一段检索失败只省略该段，绝不把已入库记录拒之分析门外；
/// 知识库主材料组装失败（select_knowledge 本身）仍向上报错。
fn decision_support_material(
    store: &Store,
    query: &str,
) -> Result<(String, Vec<crate::knowledge::KnowledgeCitation>)> {
    let mut lines = Vec::new();
    for rule in store.list_active_rules()?.into_iter().take(8) {
        lines.push(format!(
            "- 已确认规则：{}",
            rule.content.chars().take(500).collect::<String>()
        ));
    }
    let candidates = crate::knowledge::select_knowledge(store, query, "analysis", 2500)?;
    lines.push(crate::knowledge::context_text(&candidates)?);
    // 用户模型关系边（D2 累积）：最多 10 条，条目本身短语级不截断。
    if let Ok(relations) = store.list_relations() {
        let edges = relations
            .iter()
            .take(10)
            .map(|r| format!("- 关系：{} ——{}—— {}", r.from_slug, r.relation, r.to_slug))
            .collect::<Vec<_>>();
        if !edges.is_empty() {
            lines.push(format!("你的既有关系账本：\n{}", edges.join("\n")));
        }
    }
    // 近期记录条目：最多 5 条、每条 120 字，供「这和最近的事有什么关系」参照。
    if let Ok(recent) = store.recent_events(7, 5) {
        let entries = recent
            .iter()
            .map(|e| {
                format!(
                    "- 近期记录（{}）：{}",
                    e.recorded_at.chars().take(16).collect::<String>(),
                    e.raw_text.chars().take(120).collect::<String>()
                )
            })
            .collect::<Vec<_>>();
        if !entries.is_empty() {
            lines.push(format!("近期记录：\n{}", entries.join("\n")));
        }
    }
    Ok((lines.join("\n\n"), candidates))
}

/// guidance 清洗与护栏（D4）：相关页 slug 必须真实存在，否则丢弃该项；
/// 每类条数封顶 10、每条文本 500 字——分析输出不得无限膨胀。
/// 情绪标注护栏（D5）：只有文本无数值；空 feeling 丢弃、重复归并、
/// 每条 200 字封顶、条数封 8。清洗后为空则整字段省略。
fn wash_emotion(value: &mut EventAnalysisV1) {
    let Some(tags) = value.emotion.as_mut() else {
        return;
    };
    tags.retain(|tag| !tag.feeling.trim().is_empty());
    let mut seen: Vec<(Option<String>, String)> = Vec::new();
    tags.retain(|tag| {
        let feeling: String = tag.feeling.trim().chars().take(200).collect();
        let target = tag
            .target
            .as_ref()
            .map(|t| t.trim().to_owned())
            .filter(|t| !t.is_empty());
        if seen.iter().any(|(t, f)| *t == target && f == &feeling) {
            return false;
        }
        seen.push((target.clone(), feeling.clone()));
        true
    });
    for tag in tags.iter_mut() {
        tag.feeling = tag.feeling.trim().chars().take(200).collect();
        tag.target = tag
            .target
            .as_ref()
            .map(|t| t.trim().chars().take(200).collect::<String>())
            .filter(|t| !t.is_empty());
    }
    tags.truncate(8);
    if tags.is_empty() {
        value.emotion = None;
    }
}

fn wash_guidance(store: &Store, value: &mut EventAnalysisV1) {
    let Some(guidance) = value.guidance.as_mut() else {
        return;
    };
    guidance.related.retain(|r| {
        store
            .get_wiki_page(r.slug.trim())
            .map(|page| page.is_some())
            .unwrap_or(false)
    });
    guidance.related.truncate(10);
    for r in &mut guidance.related {
        r.note = r.note.chars().take(500).collect();
    }
    guidance.lessons = guidance
        .lessons
        .iter()
        .take(10)
        .map(|l| l.chars().take(500).collect())
        .collect();
    guidance.follow_ups_refined = guidance
        .follow_ups_refined
        .iter()
        .take(10)
        .map(|f| f.chars().take(500).collect())
        .collect();
    if guidance.related.is_empty()
        && guidance.lessons.is_empty()
        && guidance.follow_ups_refined.is_empty()
    {
        value.guidance = None;
    }
}

#[cfg(test)]
#[path = "user_model_tests.rs"]
mod user_model_tests;

#[cfg(test)]
#[path = "analysis_tests.rs"]
mod analysis_tests;

#[cfg(test)]
#[path = "daily_review_tests.rs"]
mod daily_review_tests;

#[cfg(test)]
#[path = "guidance_tests.rs"]
mod guidance_tests;

#[cfg(test)]
#[path = "emotion_wash_tests.rs"]
mod emotion_wash_tests;

#[cfg(test)]
#[path = "record_event_tests.rs"]
mod record_event_tests;

#[cfg(test)]
#[path = "period_stats_tests.rs"]
mod period_stats_tests;
