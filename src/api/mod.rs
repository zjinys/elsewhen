pub mod chat_proposals;
pub mod chat_runs;
pub mod conversations;
pub mod entities;
pub mod event_stats;
pub mod fonts;
pub mod goals;
pub mod import;
pub mod knowledge;
pub mod knowledge_digest;
pub mod knowledge_workflows;
pub mod provider_config;
pub mod provider_reachability;
pub mod relations;
pub mod rules;
pub mod sync;
pub mod theme;
pub mod todos;
pub mod tweet;
pub mod web_search;
pub mod wiki;
pub mod wiki_chat;
use crate::ai::memory::ContextMessage;
use crate::ai::provider::{AiProvider, OpenAiCompatibleConfig, OpenAiCompatibleProvider};
use crate::event::NewEvent;
use crate::storage::Store;
use anyhow::{Context, Result};
pub use chat_proposals::*;
pub use chat_runs::*;
pub use conversations::*;
pub use entities::*;
pub use fonts::*;
pub use import::*;
pub use knowledge_digest::*;
pub use provider_config::*;
pub use relations::*;
pub use rules::*;
use serde::{Deserialize, Serialize};
pub use theme::*;
pub use todos::*;
pub use tweet::*;
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
    Ok(AnalysisTriggerResult::Processed { count: processed })
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
mod user_model_tests {
    use super::*;
    use crate::ai::provider::AiReply;
    use crate::ai::tool::ToolSpec;
    use crate::event::NewEvent;
    use crate::storage::Store;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_database() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "elsewhen-user-model-test-{}.db",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    struct AnalysisStub(&'static str);

    impl crate::ai::provider::AiProvider for AnalysisStub {
        fn generate_reply_with_tools(
            &self,
            _: Vec<ContextMessage>,
            tools: Option<&[ToolSpec]>,
        ) -> Result<AiReply> {
            assert!(tools.is_none());
            Ok(AiReply::text(self.0))
        }
    }

    fn reply(people: &[&str], projects: &[&str]) -> String {
        format!(
            r#"{{"schema_version":"event-analysis-v3","recordable":true,"kind":"event","event_type":"note","confidence":0.9,"summary":"s","clarifications":[],"people":{p},"projects":{q},"activities":[],"follow_ups":[]}}"#,
            p = serde_json::to_string(people).unwrap(),
            q = serde_json::to_string(projects).unwrap(),
        )
    }

    fn page_for_test(store: &Store, kind: &str, title: &str) {
        let slug = crate::wiki::slugify(title);
        store
            .upsert_wiki_page(
                &crate::storage::WikiPageDraft {
                    slug,
                    kind: kind.to_string(),
                    title: title.to_string(),
                    summary: String::new(),
                    content_md: format!("# {title}"),
                    tags: vec![],
                    source_event_ids: vec![],
                    status: "active".to_string(),
                    reason: "test".to_string(),
                    source_url: None,
                },
                crate::storage::ContentPolicy::Always,
            )
            .unwrap();
    }

    #[test]
    fn annotated_pair_auto_upserts_without_draft_card() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        page_for_test(&store, "person", "李四");
        let slug = crate::wiki::slugify("网站改版");
        page_for_test(&store, "topic", "网站改版");
        let _ = slug;
        // @/# 双标注：权威组合，页面齐备 → 直接增量入库
        store
            .insert_event(NewEvent::now("@李四 负责 #网站改版 上线了"))
            .unwrap();
        let provider = AnalysisStub(Box::leak(reply(&["李四"], &["网站改版"]).into_boxed_str()));
        assert_eq!(process_analysis_queue(&store, &provider).unwrap(), 1);
        let relations = store.list_relations().unwrap();
        let stats = store.analysis_job_stats().unwrap();
        eprintln!("DEBUG stats={stats:?} relations={relations:?}");
        {
            let mut stmt = store
                .connection
                .prepare("SELECT event_id, COALESCE(last_error,'') FROM analysis_jobs")
                .unwrap();
            let errs: Vec<(String, String)> = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
                .map(|x| x.unwrap())
                .collect();
            eprintln!("DEBUG jobs={errs:?}");
        }
        assert_eq!(relations.len(), 1, "标注组合应自动入库");
        assert_eq!(relations[0].from_slug, crate::wiki::slugify("李四"));
        assert_eq!(relations[0].to_slug, crate::wiki::slugify("网站改版"));
        assert_eq!(relations[0].confidence, 4);
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn alias_hit_writes_canonical_slug_and_survives_merge_replay() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        // canonical 页「王大山」，分析输出用别名「小王」——归一管道写 canonical
        page_for_test(&store, "person", "王大山");
        store
            .add_entity_alias("person", &crate::wiki::slugify("王大山"), "小王")
            .unwrap();
        page_for_test(&store, "topic", "网站改版");
        store
            .insert_event(NewEvent::now("@小王 把 #网站改版 上线了"))
            .unwrap();
        let provider = AnalysisStub(Box::leak(reply(&["小王"], &["网站改版"]).into_boxed_str()));
        assert_eq!(process_analysis_queue(&store, &provider).unwrap(), 1);
        let relations = store.list_relations().unwrap();
        assert_eq!(relations.len(), 1, "别名命中应自动入库");
        assert_eq!(
            relations[0].from_slug,
            crate::wiki::slugify("王大山"),
            "必须写 canonical"
        );
        assert_eq!(relations[0].confidence, 4);

        // 归并后重放（新事件、同别名）：alias 已迁到目标页 → 边继续写新 canonical，
        // 不复活已合并页、不产生分裂的第二条边。
        page_for_test(&store, "person", "王大山二号");
        store
            .merge_entity(
                "person",
                &crate::wiki::slugify("王大山"),
                &crate::wiki::slugify("王大山二号"),
            )
            .unwrap();
        store
            .insert_event(NewEvent::now("@小王 又推动了 #网站改版 一步"))
            .unwrap();
        let provider2 = AnalysisStub(Box::leak(reply(&["小王"], &["网站改版"]).into_boxed_str()));
        assert_eq!(process_analysis_queue(&store, &provider2).unwrap(), 1);
        let relations = store.list_relations().unwrap();
        assert_eq!(relations.len(), 1, "重放不得另立第二条边");
        assert_eq!(
            relations[0].from_slug,
            crate::wiki::slugify("王大山二号"),
            "重放边应指向合并后的 canonical"
        );
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn unannotated_or_missing_pages_do_not_auto_upsert() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        page_for_test(&store, "person", "王五");
        page_for_test(&store, "person", "赵六");
        // 无标注：即使页面存在也不自动入库（低置信走草拟）
        store
            .insert_event(NewEvent::now("和王五 赵六 开会聊了网站改版"))
            .unwrap();
        let provider = AnalysisStub(Box::leak(
            reply(&["王五", "赵六"], &["网站改版"]).into_boxed_str(),
        ));
        assert_eq!(process_analysis_queue(&store, &provider).unwrap(), 1);
        assert!(
            store.list_relations().unwrap().is_empty(),
            "未标注组合不得自动入库"
        );
        drop(store);
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod analysis_tests {
    use super::*;
    use crate::ai::{provider::AiReply, tool::ToolSpec};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_database() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "elsewhen-api-test-{}.db",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    struct StubProvider(Option<&'static str>);

    impl AiProvider for StubProvider {
        fn generate_reply_with_tools(
            &self,
            _: Vec<ContextMessage>,
            tools: Option<&[ToolSpec]>,
        ) -> Result<AiReply> {
            assert!(tools.is_none());
            match self.0 {
                Some(content) => Ok(AiReply::text(content)),
                None => anyhow::bail!("simulated timeout"),
            }
        }
    }

    #[test]
    fn queue_persists_multiple_results_without_reprocessing() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        store.insert_event(NewEvent::now("first")).unwrap();
        store.insert_event(NewEvent::now("second")).unwrap();
        let provider = StubProvider(Some(
            r#"{"schema_version":"event-analysis-v1","event_type":"note","confidence":0.8,"summary":"test","clarifications":[],"people":[],"projects":[],"follow_ups":[]}"#,
        ));
        assert_eq!(process_analysis_queue(&store, &provider).unwrap(), 2);
        assert_eq!(store.analysis_job_stats().unwrap().succeeded, 2);
        assert_eq!(store.list_analyses().unwrap().len(), 2);
        assert_eq!(process_analysis_queue(&store, &provider).unwrap(), 0);
        assert_eq!(store.list_analyses().unwrap().len(), 2);
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn invalid_json_and_provider_failure_preserve_records_for_retry() {
        for reply in [
            Some("invalid"),
            Some("[]"),
            Some(
                r#"{"schema_version":"event-analysis-v1","event_type":"note","confidence":2,"summary":"bad","clarifications":[],"people":[],"projects":[],"follow_ups":[]}"#,
            ),
            Some(
                r#"{"schema_version":"event-analysis-v1","event_type":"note","confidence":0.5,"summary":"extra","clarifications":[],"people":[],"projects":[],"follow_ups":[],"unexpected":true}"#,
            ),
            None,
        ] {
            let path = temporary_database();
            let store = Store::open(&path).unwrap();
            store.insert_event(NewEvent::now("original")).unwrap();
            assert_eq!(
                process_analysis_queue(&store, &StubProvider(reply)).unwrap(),
                0
            );
            let stats = store.analysis_job_stats().unwrap();
            assert_eq!(stats.retry, 1);
            assert_eq!(stats.running, 0);
            assert!(store.list_analyses().unwrap().is_empty());
            assert_eq!(store.list_events().unwrap()[0].raw_text, "original");
            drop(store);
            let _ = std::fs::remove_file(path);
        }
    }

    #[test]
    fn analysis_schema_normalizes_repeated_and_blank_strings() {
        let parsed = EventAnalysisV1::parse(
            r#"{"schema_version":"event-analysis-v1","event_type":" note ","confidence":0.5,"summary":" summary ","clarifications":[" ask ","","ask"],"people":[" Ada ","Ada"],"projects":[],"follow_ups":[]}"#,
        )
        .unwrap();
        assert_eq!(parsed.event_type, "note");
        assert_eq!(parsed.summary, "summary");
        assert_eq!(parsed.clarifications, ["ask"]);
        assert_eq!(parsed.people, ["Ada"]);
        // v1 输入归一到无后缀历史标识（prompt_version 列保真），不是现行 v3。
        assert_eq!(parsed.schema_version, PRE_V3_EVENT_ANALYSIS);
    }
}

#[cfg(test)]
mod daily_review_tests {
    use super::*;
    use crate::ai::provider::AiReply;
    use crate::ai::tool::ToolSpec;
    use crate::event::NewEvent;
    use crate::storage::{ContentPolicy, RuleStatus, Store};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct StubProvider(&'static str);
    impl AiProvider for StubProvider {
        fn generate_reply_with_tools(
            &self,
            _: Vec<ContextMessage>,
            _: Option<&[ToolSpec]>,
        ) -> Result<AiReply> {
            Ok(AiReply::text(self.0))
        }
    }

    fn temporary_database() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "elsewhen-daily-review-api-{}.db",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn daily_review_requires_per_item_sources_from_declared_set() {
        let valid = r#"{
          "schema_version":"daily-review-v1",
          "date":"2026-09-18",
          "accomplishments":[{"text":"完成分析闭环","source_event_ids":["event-1"]}],
          "ideas_decisions":[],
          "people_projects":[],
          "follow_ups":[{"text":"继续每日聚合","source_event_ids":["event-1"]}]
        }"#;
        let parsed = DailyReviewV1::parse(valid, "2026-09-18", &["event-1".to_string()]).unwrap();
        assert_eq!(parsed.accomplishments[0].text, "完成分析闭环");

        let missing_source = valid.replace(
            r#""source_event_ids":["event-1"]"#,
            r#""source_event_ids":[]"#,
        );
        assert!(
            DailyReviewV1::parse(&missing_source, "2026-09-18", &["event-1".to_string()],).is_err()
        );

        let unknown_source = valid.replace("event-1", "event-other");
        assert!(
            DailyReviewV1::parse(&unknown_source, "2026-09-18", &["event-1".to_string()],).is_err()
        );
    }

    #[test]
    fn generate_daily_review_with_stub_provider_appends_valid_version() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        let event_id = store
            .insert_event(NewEvent::now("完成 Phase 3 数据契约"))
            .unwrap();
        let reply = format!(
            r#"{{"schema_version":"daily-review-v1","date":"{}","accomplishments":[{{"text":"完成数据契约","source_event_ids":["{}"]}}],"ideas_decisions":[],"people_projects":[],"follow_ups":[]}}"#,
            chrono::Local::now().date_naive(),
            event_id
        );
        let result = generate_daily_review_with_provider(
            &store,
            chrono::Local::now().date_naive(),
            &StubProvider(Box::leak(reply.into_boxed_str())),
        )
        .unwrap();
        assert!(
            matches!(result, DailyReviewResult::Created { .. }),
            "应返回 Created，实际：{result:?}"
        );
        assert!(
            store
                .latest_daily_review(chrono::Local::now().date_naive())
                .unwrap()
                .is_some()
        );
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn daily_review_keeps_only_actual_knowledge_citations() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        let event = store
            .insert_event(NewEvent::now("实践 SQLite 事务"))
            .unwrap();
        let page = crate::wiki::save_text_page(
            "SQLite 事务需要验证提交边界",
            Some("SQLite 事务"),
            &[],
            &store,
        )
        .unwrap();
        let reply=serde_json::json!({"schema_version":"daily-review-v1","date":chrono::Local::now().date_naive().to_string(),
            "accomplishments":[{"text":format!("完成 SQLite 实践 [[kb:{}]] [[kb:missing]]",page.slug),"source_event_ids":[event]}],
            "ideas_decisions":[],"people_projects":[],"follow_ups":[]}).to_string();
        let result = generate_daily_review_with_provider(
            &store,
            chrono::Local::now().date_naive(),
            &StubProvider(Box::leak(reply.into_boxed_str())),
        )
        .unwrap();
        let DailyReviewResult::Created { id } = result else {
            panic!("review not created")
        };
        let citations = crate::knowledge::latest_citations(&store, "daily_review", &id).unwrap();
        assert_eq!(citations.len(), 1);
        assert_eq!(citations[0].page_slug, page.slug);
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn event_analysis_v2_classifies_non_recordable_discussion() {
        let raw = r#"{"schema_version":"event-analysis-v2","recordable":false,"kind":"meta","event_type":"conversation","confidence":0.9,"summary":"用户在评价助手回复","clarifications":[],"people":[],"projects":[],"follow_ups":[]}"#;
        let parsed = EventAnalysisV1::parse(raw).unwrap();
        assert!(!parsed.recordable);
        assert_eq!(parsed.kind, "meta");
        assert_eq!(parsed.schema_version, PRE_V3_EVENT_ANALYSIS);
    }

    #[test]
    fn event_analysis_v1_defaults_to_recordable_event() {
        let raw = r#"{"schema_version":"event-analysis-v1","event_type":"note","confidence":0.8,"summary":"旧结果","clarifications":[],"people":[],"projects":[],"follow_ups":[]}"#;
        let parsed = EventAnalysisV1::parse(raw).unwrap();
        assert!(parsed.recordable);
        assert_eq!(parsed.kind, "event");
        assert_eq!(parsed.schema_version, PRE_V3_EVENT_ANALYSIS);
    }

    #[test]
    fn decision_support_context_contains_only_confirmed_reference_material() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        store
            .add_rule("先确认付款方再推进", RuleStatus::Active, None)
            .unwrap();
        let draft = crate::storage::WikiPageDraft {
            slug: "topic/payment-check".to_string(),
            kind: "topic".to_string(),
            title: "付款检查".to_string(),
            summary: "付款前确认责任人".to_string(),
            content_md: "付款前确认责任人".to_string(),
            tags: vec!["decision".to_string()],
            source_event_ids: vec![],
            status: "active".to_string(),
            reason: "test".to_string(),
            source_url: None,
        };
        store
            .upsert_wiki_page(&draft, ContentPolicy::Always)
            .unwrap();
        let (context, _) = decision_support_material(&store, "付款检查").unwrap();
        assert!(context.contains("先确认付款方再推进"));
        assert!(
            !context.contains("付款检查"),
            "无来源的页面不能当作分析证据"
        );
        drop(store);
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod guidance_tests {
    use super::*;
    use crate::event::NewEvent;
    use crate::storage::Store;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_database() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "elsewhen-guidance-test-{}.db",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn page_for_test(store: &Store, kind: &str, title: &str) {
        let slug = crate::wiki::slugify(title);
        store
            .upsert_wiki_page(
                &crate::storage::WikiPageDraft {
                    slug,
                    kind: kind.to_string(),
                    title: title.to_string(),
                    summary: String::new(),
                    content_md: format!("# {title}"),
                    tags: vec![],
                    source_event_ids: vec![],
                    status: "active".to_string(),
                    reason: "test".to_string(),
                    source_url: None,
                },
                crate::storage::ContentPolicy::Always,
            )
            .unwrap();
    }

    #[test]
    fn wash_guidance_drops_invented_slugs_and_caps_counts() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        page_for_test(&store, "topic", "网站改版");
        let mut value =
            EventAnalysisV1::parse(r#"{"schema_version":"event-analysis-v3","recordable":true,"kind":"event","event_type":"note","confidence":0.9,"summary":"s","clarifications":[],"people":[],"projects":[],"activities":[],"follow_ups":[],"guidance":{"related":[{"slug":"网站改版","note":"相关经验"},{"slug":"杜撰页","note":"不存在的页"}],"lessons":["经验一"],"follow_ups_refined":["精炼待办"]}}"#).unwrap();
        wash_guidance(&store, &mut value);
        let guidance = value.guidance.as_ref().unwrap();
        assert_eq!(guidance.related.len(), 1, "杜撰 slug 必须被丢弃");
        assert_eq!(guidance.related[0].slug, "网站改版");
        assert_eq!(guidance.lessons[0], "经验一");
        // 全空后整字段清除
        let mut empty =
            EventAnalysisV1::parse(r#"{"schema_version":"event-analysis-v3","recordable":true,"kind":"event","event_type":"note","confidence":0.9,"summary":"s","clarifications":[],"people":[],"projects":[],"activities":[],"follow_ups":[],"guidance":{"related":[{"slug":"杜撰页","note":"n"}],"lessons":[],"follow_ups_refined":[]}}"#).unwrap();
        wash_guidance(&store, &mut empty);
        assert!(empty.guidance.is_none(), "清洗后为空应整字段省略");
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn decision_material_includes_relations_and_recent_entries() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        page_for_test(&store, "person", "李四");
        page_for_test(&store, "topic", "网站改版");
        store
            .upsert_relation(&crate::storage::RelationDraft {
                from_slug: crate::wiki::slugify("李四"),
                from_kind: "person".to_string(),
                to_slug: crate::wiki::slugify("网站改版"),
                to_kind: "topic".to_string(),
                relation: "参与".to_string(),
                note: None,
                confidence: 3,
                source_conversation_id: None,
                source_event_id: None,
            })
            .unwrap();
        store
            .insert_event(NewEvent::now("和李四讨论网站改版"))
            .unwrap();
        let (context, _) = decision_support_material(&store, "网站改版").unwrap();
        assert!(context.contains("关系："), "决策材料应含用户模型关系边");
        assert!(context.contains("近期记录："), "决策材料应含近期记录条目");
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn guidance_flows_to_detail_and_page_aggregation() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        page_for_test(&store, "topic", "网站改版");
        store.insert_event(NewEvent::now("网站改版上线了")).unwrap();
        // 直接注入带 guidance 的分析结果（绕过 provider，聚焦派生层读面）
        let value =
            EventAnalysisV1::parse(r#"{"schema_version":"event-analysis-v3","recordable":true,"kind":"event","event_type":"note","confidence":0.9,"summary":"上线","clarifications":[],"people":[],"projects":[],"activities":[],"follow_ups":[],"guidance":{"related":[{"slug":"网站改版","note":"此前联调经验可复用"}],"lessons":["先灰度再全量"],"follow_ups_refined":["观察一周报错率"]}}"#).unwrap();
        let detail = store
            .event_analysis_detail(&guidance_event_id(&store))
            .unwrap()
            .unwrap();
        store
            .complete_analysis(
                &crate::storage::AnalysisJob {
                    id: job_id(&store),
                    event_id: detail.event_id.clone(),
                    raw_text: String::new(),
                    attempts: 0,
                },
                &value.schema_version,
                &serde_json::to_string(&value).unwrap(),
            )
            .unwrap();
        let items = store
            .page_guidance_items(&crate::wiki::slugify("网站改版"))
            .unwrap();
        assert_eq!(items.len(), 1, "页聚合应引用到 guidance");
        assert_eq!(items[0].lessons, vec!["先灰度再全量".to_string()]);
        assert_eq!(
            items[0].follow_ups_refined,
            vec!["观察一周报错率".to_string()]
        );
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn emotion_flows_list_and_delete_leaves_event_untouched() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        store
            .insert_event(NewEvent::now("和家长会老师聊了成绩"))
            .unwrap();
        let value =
            EventAnalysisV1::parse(r#"{"schema_version":"event-analysis-v3","recordable":true,"kind":"event","event_type":"note","confidence":0.9,"summary":"s","clarifications":[],"people":[],"projects":[],"activities":[],"follow_ups":[],"emotion":[{"target":"家长会老师","feeling":"被理解"},{"target":"","feeling":"焦虑"}]}"#).unwrap();
        store
            .complete_analysis(
                &crate::storage::AnalysisJob {
                    id: job_id(&store),
                    event_id: guidance_event_id(&store),
                    raw_text: String::new(),
                    attempts: 0,
                },
                &value.schema_version,
                &serde_json::to_string(&value).unwrap(),
            )
            .unwrap();
        // 列表面（派生读，target 空串归一为 None=对事件本身）
        let items = store.list_event_emotions(100).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].target.as_deref(), Some("家长会老师"));
        assert_eq!(items[0].feeling, "被理解");
        assert_eq!(items[1].target, None, "空 target = 对事件本身");

        // 删除只动 result_json 的 emotion 字段：事件原文与其它分析字段不动
        let before: String = store
            .connection
            .query_row(
                "SELECT raw_text FROM events WHERE id=?1",
                [guidance_event_id(&store)],
                |r| r.get(0),
            )
            .unwrap();
        let removed = store
            .delete_event_emotion(&items[0].analysis_id, Some("家长会老师"), "被理解")
            .unwrap();
        assert!(removed);
        let after_raw: String = store
            .connection
            .query_row(
                "SELECT raw_text FROM events WHERE id=?1",
                [guidance_event_id(&store)],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(before, after_raw, "事件真源必须分毫不动");
        let detail = store
            .event_analysis_detail(&guidance_event_id(&store))
            .unwrap()
            .unwrap();
        let value = EventAnalysisV1::parse(&detail.result_json.unwrap()).unwrap();
        let emotions = value.emotion.unwrap();
        assert_eq!(emotions.len(), 1);
        assert_eq!(emotions[0].feeling, "焦虑");
        // 清空后整字段移除（避免残留空数组垃圾）
        store
            .delete_event_emotion(&items[0].analysis_id, Some(""), "焦虑")
            .unwrap();
        let value = EventAnalysisV1::parse(
            &store
                .event_analysis_detail(&guidance_event_id(&store))
                .unwrap()
                .unwrap()
                .result_json
                .unwrap(),
        )
        .unwrap();
        assert!(value.emotion.is_none(), "空数组应整字段省略");
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    fn guidance_event_id(store: &Store) -> String {
        store
            .connection
            .query_row("SELECT id FROM events LIMIT 1", [], |r| r.get(0))
            .unwrap()
    }

    fn job_id(store: &Store) -> String {
        store
            .connection
            .query_row("SELECT id FROM analysis_jobs LIMIT 1", [], |r| r.get(0))
            .unwrap()
    }

    fn rule_for_test(store: &Store) -> String {
        let id = format!("rule-{}", uuid::Uuid::new_v4());
        store
            .connection
            .execute(
                "INSERT INTO rules (id, content, status, created_at, updated_at)
                 VALUES (?1, '发布前先跑测试', 'active', strftime('%Y-%m-%dT%H:%M:%fZ','now'), strftime('%Y-%m-%dT%H:%M:%fZ','now'))",
                rusqlite::params![id],
            )
            .unwrap();
        id
    }

    fn hit_for_test(
        store: &Store,
        rule_id: &str,
        event_id: &str,
        status: &str,
        days_ago: i64,
    ) -> String {
        let id = format!("hit-{}", uuid::Uuid::new_v4());
        store
            .connection
            .execute(
                "INSERT INTO rule_hits (id, rule_id, event_id, status, confidence, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, 0.9, ?5, ?5)",
                rusqlite::params![
                    id,
                    rule_id,
                    event_id,
                    status,
                    (chrono::Utc::now() - chrono::Duration::days(days_ago)).to_rfc3339()
                ],
            )
            .unwrap();
        id
    }

    /// D6 采纳反馈：记录 / 清除（toggle）/ fail-closed。
    /// 走 store 层（api ::set_rule_hit_feedback 会经 AppConfig::load 打开
    /// 真实用户库，测试里一律不碰；api 层只做 trim/转发）。
    #[test]
    fn rule_hit_feedback_records_clears_and_fails_closed() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        let event_id = store
            .insert_event(NewEvent::now("踩了发布流程的坑"))
            .unwrap();
        let rule_id = rule_for_test(&store);
        let hit_id = hit_for_test(&store, &rule_id, &event_id, "notified", 0);

        // 记录采纳
        store
            .set_rule_hit_feedback(&hit_id, Some("helpful"))
            .unwrap();
        let hits = store.recent_rule_hits(10).unwrap();
        assert_eq!(
            hits.iter()
                .find(|h| h.id == hit_id)
                .unwrap()
                .feedback
                .as_deref(),
            Some("helpful")
        );

        // 换一条（同事件同规则 UNIQUE，换事件）：反馈 not_relevant
        let event2_id = store.insert_event(NewEvent::now("又一次提醒")).unwrap();
        let hit2 = hit_for_test(&store, &rule_id, &event2_id, "aggregated", 0);
        store
            .set_rule_hit_feedback(&hit2, Some("not_relevant"))
            .unwrap();
        let hits = store.recent_rule_hits(10).unwrap();
        assert_eq!(
            hits.iter()
                .find(|h| h.id == hit2)
                .unwrap()
                .feedback
                .as_deref(),
            Some("not_relevant")
        );

        // 清除 = feedback NULL
        store.set_rule_hit_feedback(&hit_id, None).unwrap();
        let hits = store.recent_rule_hits(10).unwrap();
        assert!(
            hits.iter()
                .find(|h| h.id == hit_id)
                .unwrap()
                .feedback
                .is_none()
        );

        // fail-closed：未知值报错；不存在的 id 返回 false（api 层转 bail）
        assert!(
            store
                .set_rule_hit_feedback(&hit_id, Some("自动推断"))
                .is_err()
        );
        assert!(
            !store
                .set_rule_hit_feedback("no-such-hit", Some("helpful"))
                .unwrap()
        );
        // pending（未处置）命中不可反馈——度量只面向已送达用户的提醒
        let event3_id = store.insert_event(NewEvent::now("还没提醒的")).unwrap();
        let hit3 = hit_for_test(&store, &rule_id, &event3_id, "pending", 0);
        assert!(!store.set_rule_hit_feedback(&hit3, Some("helpful")).unwrap());
        let _ = event_id;
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    /// D6 回顾勾连拼装等价验证（store 层；api ::get_daily_overview 直接
    /// 调 store.rule_hits_on + memory_echo_for，经 AppConfig 会开真实库，
    /// 测试不碰）。
    #[test]
    fn daily_overview_parts_available_from_store() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        let today = chrono::Local::now().date_naive();
        let today_id = store.insert_event(NewEvent::now("今天的进展")).unwrap();
        let rule_id = rule_for_test(&store);
        hit_for_test(&store, &rule_id, &today_id, "aggregated", 0);
        let hits = store.rule_hits_on(today).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].rule_content, "发布前先跑测试");
        assert_eq!(hits[0].status, "aggregated");
        // 昨天的命中不该混进今天
        let yesterday = store
            .insert_event(NewEvent {
                raw_text: "昨天的事",
                occurred_at: chrono::Utc::now() - chrono::Duration::days(1),
                recorded_at: chrono::Utc::now() - chrono::Duration::days(1),
                source: "capture",
            })
            .unwrap();
        let _ = hit_for_test(&store, &rule_id, &yesterday, "notified", 1);
        // 同规则+同事件会 UNIQUE 冲突，这里只是构造不同事件——用新规则
        let rule2 = rule_for_test(&store);
        let _ = hit_for_test(&store, &rule2, &yesterday, "notified", 1);
        assert_eq!(store.rule_hits_on(today).unwrap().len(), 1);
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    /// D6 回顾勾连 storage 联测：当日命中 + 一周回引同库成立。
    #[test]
    fn memory_echo_and_day_hits_are_store_level_queries() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        let today = chrono::Local::now().date_naive();
        assert!(store.memory_echo_for(today, 3).unwrap().is_empty());
        let days_ago = chrono::Utc::now() - chrono::Duration::days(8);
        store
            .insert_event(NewEvent {
                raw_text: "去年秋天 mountain trip",
                occurred_at: days_ago,
                recorded_at: days_ago,
                source: "capture",
            })
            .unwrap();
        let echo = store.memory_echo_for(today, 3).unwrap();
        assert_eq!(echo.len(), 1);
        assert!(echo[0].2.contains("mountain trip"));
        // 超过 max 截断
        let days_ago2 = chrono::Utc::now() - chrono::Duration::days(7);
        store
            .insert_event(NewEvent {
                raw_text: "第二条回引素材",
                occurred_at: days_ago2,
                recorded_at: days_ago2,
                source: "capture",
            })
            .unwrap();
        assert_eq!(store.memory_echo_for(today, 1).unwrap().len(), 1);
    }
}

#[cfg(test)]
mod emotion_wash_tests {
    use super::*;

    #[test]
    fn wash_emotion_drops_empty_merges_duplicates_and_caps() {
        let mut value =
            EventAnalysisV1::parse(r#"{"schema_version":"event-analysis-v3","recordable":true,"kind":"event","event_type":"note","confidence":0.9,"summary":"s","clarifications":[],"people":[],"projects":[],"activities":[],"follow_ups":[],"emotion":[{"target":"甲","feeling":"焦虑"},{"target":"甲","feeling":"焦虑"},{"feeling":"挫败"},{"target":"乙","feeling":"被理解"}]}"#).unwrap();
        wash_emotion(&mut value);
        let tags = value.emotion.unwrap();
        assert_eq!(tags.len(), 3, "重复归并、其余保留");
        assert_eq!(
            tags[1].target.as_deref(),
            None,
            "缺 target 保留为对事件本身"
        );
        assert_eq!(tags[1].feeling, "挫败");
        // 全空 → 整字段清空
        // 条数封顶 8
        let mut many_json = String::from(
            r#"{"schema_version":"event-analysis-v3","recordable":true,"kind":"event","event_type":"note","confidence":0.9,"summary":"s","clarifications":[],"people":[],"projects":[],"activities":[],"follow_ups":[],"emotion":["#,
        );
        for i in 0..10 {
            if i > 0 {
                many_json.push(',');
            }
            many_json.push_str(&format!(r#"{{"target":"person{i}","feeling":"感受{i}"}}"#));
        }
        many_json.push_str("]}");
        let mut many = EventAnalysisV1::parse(&many_json).unwrap();
        wash_emotion(&mut many);
        assert_eq!(many.emotion.unwrap().len(), 8, "条数封顶 8");
    }
}

#[cfg(test)]
mod record_event_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// 持 config 测试锁，将库路径钉死到临时目录；Drop 时清掉钉死值。
    /// 不修改进程环境变量，避免与并行测试中的环境读取发生竞争。
    struct DataDirGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
    }
    impl DataDirGuard {
        fn set(dir: &std::path::Path) -> Self {
            let lock = crate::config::test_env_guard();
            crate::config::unpin();
            crate::config::AppConfig::pin(Some(dir.to_str().unwrap())).unwrap();
            Self { _lock: lock }
        }
    }
    impl Drop for DataDirGuard {
        fn drop(&mut self) {
            crate::config::unpin();
        }
    }

    #[test]
    fn record_event_persists_dto_source_so_badge_renders_consistently() {
        let dir = std::env::temp_dir().join(format!(
            "elsewhen-record-event-{}.db",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let guard = DataDirGuard::set(&dir);

        let dto = record_event("GUI 快录一条事件".to_string()).unwrap();
        assert_eq!(
            dto.source, "flutter_gui",
            "record_event DTO 应声明 GUI 快录路径"
        );
        assert_eq!(dto.status, "pending");

        let rows = list_events().unwrap();
        let row = rows
            .iter()
            .find(|e| e.id == dto.id)
            .expect("应能读回刚插入的事件");
        assert_eq!(
            row.source, "flutter_gui",
            "落库 source 应与 DTO 对齐（list_events 如实读库后 event_card 的 source badge 才能显示 GUI）"
        );

        drop(guard);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod period_stats_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// 与 record_event_tests 同款：pin 数据目录使 AppConfig::load 指向临时库。
    struct DataDirGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
    }
    impl DataDirGuard {
        fn set(dir: &std::path::Path) -> Self {
            let lock = crate::config::test_env_guard();
            crate::config::unpin();
            crate::config::AppConfig::pin(Some(dir.to_str().unwrap())).unwrap();
            Self { _lock: lock }
        }
    }
    impl Drop for DataDirGuard {
        fn drop(&mut self) {
            crate::config::unpin();
        }
    }

    /// 本地日历日正午（归属唯一、不贴边），转 UTC 供 event 插入。
    fn local_noon_utc(date: chrono::NaiveDate) -> chrono::DateTime<chrono::Utc> {
        use chrono::TimeZone;
        let naive = date.and_hms_opt(12, 0, 0).unwrap();
        chrono::Local
            .from_local_datetime(&naive)
            .single()
            .expect("本地正午无 DST 歧义")
            .with_timezone(&chrono::Utc)
    }

    fn analysis_json(
        event_type: &str,
        people: &[&str],
        projects: &[&str],
        activities: &[&str],
        recordable: bool,
    ) -> String {
        // parse 校验：不可记录结果不能标 kind=event（真实链路里默认弃记
        // 的闲聊就是 chitchat），造数必须同律，否则 parse 失败口径会漂。
        let kind = if recordable { "event" } else { "chitchat" };
        format!(
            r#"{{"schema_version":"{EVENT_ANALYSIS_VERSION}","event_type":"{event_type}",
             "confidence":0.9,"summary":"s","clarifications":[],
             "people":{people},"projects":{projects},"activities":{activities},
             "follow_ups":[],"recordable":{recordable},"kind":"{kind}"}}"#,
            people = serde_json::to_string(people).unwrap(),
            projects = serde_json::to_string(projects).unwrap(),
            activities = serde_json::to_string(activities).unwrap(),
        )
    }

    /// 造一条带分析结果的事件，返回 event_id。
    fn seed_event(
        store: &Store,
        text: &str,
        recorded_at: chrono::DateTime<chrono::Utc>,
        result_json: &str,
    ) -> String {
        let id = store
            .insert_event(crate::event::NewEvent {
                raw_text: text,
                occurred_at: recorded_at,
                recorded_at,
                source: "capture",
            })
            .unwrap();
        store
            .connection
            .execute(
                "INSERT INTO event_analyses (id, event_id, prompt_version, result_json, created_at)
                 VALUES (?1, ?2, 'p', ?3, ?4)",
                rusqlite::params![
                    format!("analysis-{id}"),
                    id,
                    result_json,
                    recorded_at.to_rfc3339()
                ],
            )
            .unwrap();
        id
    }

    /// 手动可记录性决策（覆盖口径用的最高优先级源）。
    fn seed_decision(store: &Store, event_id: &str, recordable: bool) {
        store
            .connection
            .execute(
                "INSERT INTO event_recordability_decisions
                 (id,event_id,recordable,kind,reason,created_at)
                 VALUES (?1,?2,?3,'event','manual',?4)",
                rusqlite::params![
                    format!("decision-{event_id}"),
                    event_id,
                    recordable as i64,
                    chrono::Utc::now().to_rfc3339()
                ],
            )
            .unwrap();
    }

    fn open_pinned_store() -> (DataDirGuard, Store) {
        let dir = std::env::temp_dir().join(format!(
            "elsewhen-period-stats-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let guard = DataDirGuard::set(&dir);
        let config = crate::config::AppConfig::load().unwrap();
        (guard, Store::open(&config.database_path).unwrap())
    }

    #[test]
    fn period_stats_rejects_invalid_input() {
        let (_guard, _store) = open_pinned_store();
        assert!(get_period_stats("year".into(), "2026-10-14".into()).is_err());
        assert!(get_period_stats("week".into(), "2026/10/14".into()).is_err());
    }

    #[test]
    fn week_and_month_windows_are_calendar_aligned() {
        let (_guard, store) = open_pinned_store();
        // 2026-10-12 是周一：锚定周三（10-14）的周窗口应为 10-12..10-18。
        let monday = chrono::NaiveDate::from_ymd_opt(2026, 10, 12).unwrap();
        seed_events_with_dates(
            &store,
            &[
                ("周五", monday + chrono::Duration::days(4)),
                ("下周一", monday + chrono::Duration::days(7)),
            ],
            true,
        );
        let week = get_period_stats("week".into(), "2026-10-14".into()).unwrap();
        assert_eq!(week.start_date, "2026-10-12");
        assert_eq!(week.end_date, "2026-10-18");
        assert_eq!(week.event_count, 1, "下周一的事件不得混入本周");
        assert_eq!(week.daily_counts.len(), 7, "日分布必须铺满整周");

        // 月窗口：10 月锚点 → 10-01..10-31，跨月事件隔离。
        let oct1 = chrono::NaiveDate::from_ymd_opt(2026, 10, 1).unwrap();
        let sep30 = chrono::NaiveDate::from_ymd_opt(2026, 9, 30).unwrap();
        seed_events_with_dates(&store, &[("九月末", sep30), ("月初", oct1)], true);
        let month = get_period_stats("month".into(), "2026-10-14".into()).unwrap();
        assert_eq!(month.start_date, "2026-10-01");
        assert_eq!(month.end_date, "2026-10-31");
        assert_eq!(month.daily_counts.len(), 31, "日分布必须铺满整月");
        assert_eq!(
            month.event_count, 3,
            "周五/下周一(10-19)/月初 属十月；九月末不属于"
        );
    }

    #[test]
    fn aggregation_ranks_and_counts_deterministically() {
        let (_guard, store) = open_pinned_store();
        let monday = chrono::NaiveDate::from_ymd_opt(2026, 10, 12).unwrap();
        let seed = |store: &Store, text: &str, json: String| {
            seed_event(store, text, local_noon_utc(monday), &json)
        };
        seed(
            &store,
            "a",
            analysis_json("work", &["amma"], &["p1"], &["coding"], true),
        );
        seed(
            &store,
            "b",
            analysis_json("work", &["zoe", "amma"], &["p1"], &["coding"], true),
        );
        seed(
            &store,
            "c",
            analysis_json("idea", &["amma"], &["p2"], &["coding"], true),
        );
        // 待办两件本周完成。
        for tid in ["t1", "t2"] {
            store
                .connection
                .execute(
                    "INSERT INTO todos (id,title,status,updated_at,created_at)
                     VALUES (?1,'t','done',?2,?2)",
                    rusqlite::params![
                        tid,
                        local_noon_utc(monday + chrono::Duration::days(1)).to_rfc3339()
                    ],
                )
                .unwrap();
        }
        store
            .connection
            .execute(
                "INSERT INTO todos (id,title,status,updated_at,created_at)
                 VALUES ('t-todo-open','t','open',?1,?1)",
                rusqlite::params![local_noon_utc(monday).to_rfc3339()],
            )
            .unwrap();

        let stats = get_period_stats("week".into(), "2026-10-14".into()).unwrap();
        assert_eq!(stats.event_count, 3);
        assert_eq!(stats.todo_done_count, 2, "open 待办不属于完成数");
        // 次数降序、同次数字典序（work=2 先于 idea=1；amma=3 稳居首）。
        assert_eq!(stats.by_event_type[0].label, "work");
        assert_eq!(stats.by_event_type[0].count, 2);
        assert_eq!(stats.by_event_type[1].label, "idea");
        assert_eq!(stats.by_person[0].label, "amma");
        assert_eq!(stats.by_person[0].count, 3);
        assert_eq!(stats.by_project[0].label, "p1");
        assert_eq!(stats.by_activity[0].label, "coding");
        assert_eq!(stats.by_activity[0].count, 3);
        // 同次数字典序：zoe(1) 排 amma 之外尾段
        let zoe = stats
            .by_person
            .iter()
            .find(|r| r.label == "zoe")
            .expect("zoe 应在榜内");
        assert_eq!(zoe.count, 1);
        let day_key = monday.to_string();
        let day_count = &stats
            .daily_counts
            .iter()
            .find(|d| d.date == day_key)
            .expect("当日必有分布条目")
            .count;
        assert_eq!(*day_count, 3, "三件事都是本地周一 → 当日分布=3");
    }

    #[test]
    fn non_recordable_events_are_excluded_from_stats() {
        let (_guard, store) = open_pinned_store();
        let monday = chrono::NaiveDate::from_ymd_opt(2026, 10, 12).unwrap();
        // 分析判定不可记录 → 排除。
        seed_event(
            &store,
            "chitchat 默认不记",
            local_noon_utc(monday),
            &analysis_json("chitchat", &["zoe"], &[], &[], false),
        );
        // 分析判不可记录但手动决策翻案 → 计入（manual 覆盖）。
        let revived = seed_event(
            &store,
            "手动翻案",
            local_noon_utc(monday),
            &analysis_json("work", &["amma"], &["p1"], &["coding"], false),
        );
        seed_decision(&store, &revived, true);
        let stats = get_period_stats("week".into(), "2026-10-14".into()).unwrap();
        assert_eq!(stats.event_count, 1, "只有翻案事件计入");
        assert_eq!(stats.by_person.len(), 1);
        assert_eq!(stats.by_person[0].label, "amma");
    }

    fn seed_events_with_dates(
        store: &Store,
        events: &[(&str, chrono::NaiveDate)],
        recordable: bool,
    ) {
        for (text, date) in events {
            seed_event(
                store,
                text,
                local_noon_utc(*date),
                &analysis_json("work", &[], &[], &[], recordable),
            );
        }
    }
}
