pub mod fonts;
pub mod todos;
pub mod theme;
pub mod wiki;
pub mod relations;
pub mod tweet;
pub mod import;
pub mod wiki_chat;
pub mod provider_config;
pub mod entities;
pub mod rules;
pub mod conversations;
pub use fonts::*;
pub use todos::*;
pub use theme::*;
pub use wiki::*;
pub use relations::*;
pub use tweet::*;
pub use import::*;
pub use wiki_chat::*;
pub use provider_config::*;
pub use entities::*;
pub use rules::*;
pub use conversations::*;
use crate::ai::memory::ContextMessage;
use crate::ai::provider::{AiProvider, OpenAiCompatibleConfig, OpenAiCompatibleProvider};
use crate::event::NewEvent;
use crate::storage::Store;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const EVENT_ANALYSIS_VERSION: &str = "event-analysis";
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
            && result.schema_version != LEGACY_EVENT_ANALYSIS_V1
            && result.schema_version != LEGACY_EVENT_ANALYSIS_V2
        {
            anyhow::bail!("schema_version 必须是 event-analysis");
        }
        result.schema_version = EVENT_ANALYSIS_VERSION.to_string();
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
    Created { id: String },
}

/// Initialize the bridge with database path
pub fn init_bridge(database_path: Option<String>) -> Result<String> {
    // Bridge API calls independently open Store instances through AppConfig.
    // Persist an explicit override in the process environment so every later
    // call uses the same database selected at initialization. This is also the
    // isolation boundary used by Flutter integration tests.
    if let Some(path) = database_path.as_deref() {
        std::env::set_var("ELSEWHEN_DATA_DIR", path);
    }

    let config = crate::config::AppConfig::load()
        .map_err(|e| anyhow::anyhow!("Error loading config: {e}"))?;
    Store::open(&config.database_path)
        .and_then(|store| store.recover_interrupted_analysis_jobs().map(|_| ()))
        .map_err(|e| anyhow::anyhow!("Error recovering analysis queue: {e}"))?;
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
    Ok(DailyOverviewDto {
        date: date_text,
        entries,
        review,
        todos,
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
    let prompt = format!("根据以下 {date_text} 的个人事实生成每日回顾。只返回 JSON 对象，不要 Markdown，不要增加字段。schema_version 固定为 daily-review-v1，date 固定为 {date_text}。字段 accomplishments、ideas_decisions、people_projects、follow_ups 都是数组；每项必须是 {{\"text\":string,\"source_event_ids\":[string]}}，来源 ID 必须来自输入。没有可靠内容的分类返回空数组，不要推测。\n\n{facts}");
    let reply = provider.generate_reply(vec![ContextMessage::new("user", prompt)])?;
    let parsed = DailyReviewV1::parse(&reply.content, &date_text, &source_event_ids)?;
    let normalized = serde_json::to_string(&parsed)?;
    let id = store.save_daily_review(date, DAILY_REVIEW_VERSION, &normalized, &source_event_ids)?;
    Ok(DailyReviewResult::Created { id })
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
    let store = Store::open(&config.database_path)?;
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
    })?;
    // 每次 worker tick 先重新清扫遗留的 running 任务：GUI 只在启动时恢复一次，
    // 若上一次队列运行中途退出，被 claim 的任务会孤悬到下次启动。
    store.recover_interrupted_analysis_jobs()?;
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
        let Some(job) = store.claim_analysis_job()? else {
            break;
        };
        // 上下文构建属于分析任务的一部分：失败不能把已 claim 的 job
        // 孤悬为 running（否则要等下次启动才 recover），记入 last_error 放回重试。
        let context = match decision_support_context(store) {
            Ok(context) => context,
            Err(error) => {
                store.fail_analysis(&job, &format!("决策上下文构建失败：{error}"))?;
                continue;
            }
        };
        let prompt = format!(
            "分析以下个人记录，只返回 JSON 对象，不要 Markdown，也不要增加字段。schema_version 固定为 event-analysis。字段必须包含 schema_version、recordable(boolean)、kind(event/discussion/chitchat/meta)、event_type(string)、confidence(number 0..1)、summary(string)、clarifications(array of strings)、people(array of strings)、projects(array of strings)、activities(array of strings)、follow_ups(array of strings)。projects 只填写明确的长期项目/产品/组织；付款流程、联调、任务、沟通、会议等动作或事项必须放入 activities，不要放入 projects。只有客观经历、决定、行动或进展 recordable=true/kind=event；对 AI 回复评价、闲聊、纯提问或元对话 recordable=false，并保留简短 summary。\n\n决策辅助上下文（只作参考，不能据此臆测新事实）：\n{}\n\n记录：{}",
            context,
            job.raw_text
        );
        match provider.generate_reply(vec![ContextMessage::new("user", prompt)]) {
            Ok(reply) => match EventAnalysisV1::parse(&reply.content) {
                Ok(value) => {
                    store.complete_analysis(
                        &job,
                        &value.schema_version,
                        &serde_json::to_string(&value)?,
                    )?;
                    if value.recordable && !value.people.is_empty() && !value.projects.is_empty() {
                        if let Some(conversation_id) =
                            store.conversation_id_for_event(&job.event_id)?
                        {
                            let exists = store.action_exists_for_event(
                                &conversation_id,
                                "propose_people_relations",
                                &job.event_id,
                            )?;
                            if !exists {
                                let people: Vec<_> = value
                                    .people
                                    .iter()
                                    .map(|name| serde_json::json!({"name": name}))
                                    .collect();
                                let relations: Vec<_> = value
                                    .people
                                    .iter()
                                    .flat_map(|person| {
                                        value.projects.iter().map(move |project| {
                                            serde_json::json!({
                                                "person": person,
                                                "target": project,
                                                "relation": "参与"
                                            })
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

/// Build bounded, read-only context for Phase 4C. This makes existing rules and
/// knowledge visible at the decision point while leaving all writes behind the
/// existing confirmation gates.
fn decision_support_context(store: &Store) -> Result<String> {
    let mut lines = Vec::new();
    for rule in store.list_active_rules()?.into_iter().take(8) {
        lines.push(format!("- 已确认规则：{}", rule.content));
    }
    for page in store.list_wiki_pages(None, None)?.into_iter().take(8) {
        lines.push(format!(
            "- 知识页 [{}]：{} — {}",
            page.kind, page.title, page.summary
        ));
    }
    if lines.is_empty() {
        Ok("（暂无已确认规则或知识页）".to_string())
    } else {
        Ok(lines.join("\n"))
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
        assert_eq!(
            process_analysis_queue(&store, &provider).unwrap(),
            2
        );
        assert_eq!(store.analysis_job_stats().unwrap().succeeded, 2);
        assert_eq!(store.list_analyses().unwrap().len(), 2);
        assert_eq!(
            process_analysis_queue(&store, &provider).unwrap(),
            0
        );
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
        assert_eq!(parsed.schema_version, EVENT_ANALYSIS_VERSION);
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
        assert!(store
            .latest_daily_review(chrono::Local::now().date_naive())
            .unwrap()
            .is_some());
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn event_analysis_v2_classifies_non_recordable_discussion() {
        let raw = r#"{"schema_version":"event-analysis-v2","recordable":false,"kind":"meta","event_type":"conversation","confidence":0.9,"summary":"用户在评价助手回复","clarifications":[],"people":[],"projects":[],"follow_ups":[]}"#;
        let parsed = EventAnalysisV1::parse(raw).unwrap();
        assert!(!parsed.recordable);
        assert_eq!(parsed.kind, "meta");
        assert_eq!(parsed.schema_version, EVENT_ANALYSIS_VERSION);
    }

    #[test]
    fn event_analysis_v1_defaults_to_recordable_event() {
        let raw = r#"{"schema_version":"event-analysis-v1","event_type":"note","confidence":0.8,"summary":"旧结果","clarifications":[],"people":[],"projects":[],"follow_ups":[]}"#;
        let parsed = EventAnalysisV1::parse(raw).unwrap();
        assert!(parsed.recordable);
        assert_eq!(parsed.kind, "event");
        assert_eq!(parsed.schema_version, EVENT_ANALYSIS_VERSION);
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
        let context = decision_support_context(&store).unwrap();
        assert!(context.contains("先确认付款方再推进"));
        assert!(context.contains("付款检查"));
        drop(store);
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod record_event_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// 仅在测试内把 ELSEWHEN_DATA_DIR 指向临时目录，Drop 时恢复原值，
    /// 避免并行测试读到被污染的全局环境变量。Rust 测试默认并行运行。
    struct DataDirGuard(Option<String>);
    impl DataDirGuard {
        fn set(dir: &std::path::Path) -> Self {
            let prev = std::env::var("ELSEWHEN_DATA_DIR").ok();
            std::env::set_var("ELSEWHEN_DATA_DIR", dir);
            Self(prev)
        }
    }
    impl Drop for DataDirGuard {
        fn drop(&mut self) {
            match &self.0 {
                Some(v) => std::env::set_var("ELSEWHEN_DATA_DIR", v),
                None => std::env::remove_var("ELSEWHEN_DATA_DIR"),
            }
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
        assert_eq!(dto.source, "flutter_gui", "record_event DTO 应声明 GUI 快录路径");
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
