use crate::ai::memory::ContextMessage;
use crate::ai::provider::{AiProvider, OpenAiCompatibleConfig, OpenAiCompatibleProvider};
use crate::event::NewEvent;
use crate::storage::{ContentPolicy, RelationDraft, RuleStatus, Store};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const EVENT_ANALYSIS_VERSION: &str = "event-analysis";
const LEGACY_EVENT_ANALYSIS_V1: &str = "event-analysis-v1";
const LEGACY_EVENT_ANALYSIS_V2: &str = "event-analysis-v2";
const DAILY_REVIEW_VERSION: &str = "daily-review-v1";

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

/// Initialize the bridge with database path
pub fn init_bridge(database_path: Option<String>) -> String {
    // Bridge API calls independently open Store instances through AppConfig.
    // Persist an explicit override in the process environment so every later
    // call uses the same database selected at initialization. This is also the
    // isolation boundary used by Flutter integration tests.
    if let Some(path) = database_path.as_deref() {
        std::env::set_var("ELSEWHEN_DATA_DIR", path);
    }

    let config = match crate::config::AppConfig::load() {
        Ok(c) => c,
        Err(e) => return format!("Error loading config: {}", e),
    };
    match Store::open(&config.database_path)
        .and_then(|store| store.recover_interrupted_analysis_jobs().map(|_| ()))
    {
        Ok(()) => {}
        Err(e) => return format!("Error recovering analysis queue: {}", e),
    }
    config.database_path.display().to_string()
}

/// Record a new event
pub fn record_event(raw_text: String) -> Result<EventDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let new_event = NewEvent::now(&raw_text);
    let id = store.insert_event(new_event)?;

    // Query back the created event
    let events = store.list_events()?;
    let event = events
        .into_iter()
        .find(|e| e.raw_text == raw_text)
        .ok_or_else(|| anyhow::anyhow!("Event not found after insert"))?;

    Ok(EventDto {
        id,
        raw_text: event.raw_text,
        recorded_at: event.recorded_at.clone(),
        occurred_at: event.recorded_at,
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

pub fn generate_daily_review(date: String) -> Result<String> {
    let date = chrono::NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d")
        .map_err(|_| anyhow::anyhow!("日期必须是 YYYY-MM-DD"))?;
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let Some(provider_config) = store.active_ai_provider_config()? else {
        return Ok("no_provider".to_string());
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
) -> Result<String> {
    let entries = store
        .daily_entries(date)?
        .into_iter()
        .filter(|entry| entry_is_recordable(store, &entry.event_id))
        .collect::<Vec<_>>();
    if entries.is_empty() {
        return Ok("no_entries".to_string());
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
    Ok(format!("created:{id}"))
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

pub fn list_entity_facts(entity_kind: String, entity_slug: String) -> Result<Vec<EntityFactDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    Ok(store
        .list_entity_facts(entity_kind.trim(), entity_slug.trim())?
        .into_iter()
        .map(|fact| EntityFactDto {
            id: fact.id,
            entity_kind: fact.entity_kind,
            entity_slug: fact.entity_slug,
            fact_text: fact.fact_text,
            occurred_at: fact.occurred_at,
            confidence: fact.confidence,
            source_event_id: fact.source_event_id,
            created_at: fact.created_at,
            last_seen_at: fact.last_seen_at,
        })
        .collect())
}

pub fn delete_entity_fact(id: String) -> Result<bool> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.delete_entity_fact(id.trim())
}

pub fn list_entity_aliases(entity_kind: String, entity_slug: String) -> Result<Vec<String>> {
    let config = crate::config::AppConfig::load()?;
    Store::open(&config.database_path)?.list_entity_aliases(entity_kind.trim(), entity_slug.trim())
}

pub fn add_entity_alias(entity_kind: String, entity_slug: String, alias: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    Store::open(&config.database_path)?.add_entity_alias(
        entity_kind.trim(),
        entity_slug.trim(),
        alias.trim(),
    )
}

/// Merge two confirmed entities after explicit user confirmation.
pub fn merge_entity(entity_kind: String, source_slug: String, target_slug: String) -> Result<bool> {
    let config = crate::config::AppConfig::load()?;
    Store::open(&config.database_path)?.merge_entity(
        entity_kind.trim(),
        source_slug.trim(),
        target_slug.trim(),
    )
}

#[derive(Clone, Debug)]
pub struct EntityMergeStatusDto {
    pub source_slug: String,
    pub target_slug: String,
    pub entity_kind: String,
    pub created_at: String,
}

pub fn get_entity_merge_status(source_slug: String) -> Result<Option<EntityMergeStatusDto>> {
    let config = crate::config::AppConfig::load()?;
    Ok(Store::open(&config.database_path)?
        .entity_merge_status(source_slug.trim())?
        .map(|m| EntityMergeStatusDto {
            source_slug: m.source_slug,
            target_slug: m.target_slug,
            entity_kind: m.entity_kind,
            created_at: m.created_at,
        }))
}

/// Undo one merge only when every moved row still matches its merge snapshot.
pub fn undo_entity_merge(source_slug: String) -> Result<bool> {
    let config = crate::config::AppConfig::load()?;
    Store::open(&config.database_path)?.undo_entity_merge(source_slug.trim())
}

/// List all events
pub fn list_events() -> Result<Vec<EventDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let events = store.list_events()?;

    Ok(events
        .into_iter()
        .map(|e| EventDto {
            id: uuid::Uuid::new_v4().to_string(), // TODO: Store should return ID
            raw_text: e.raw_text,
            recorded_at: e.recorded_at.clone(),
            occurred_at: e.recorded_at,
            source: "unknown".to_string(),
            status: "completed".to_string(),
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

/// Get active AI provider info
pub fn get_ai_provider() -> Result<Option<String>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    if let Some(provider) = store.active_ai_provider_config()? {
        Ok(Some(format!("{} - {}", provider.model, provider.base_url)))
    } else {
        Ok(None)
    }
}

/// AI provider config DTO for Flutter settings page
#[derive(Clone, Debug)]
pub struct AiProviderConfigDto {
    pub id: String,
    pub name: String,
    pub provider_type: String,
    pub base_url: String,
    pub model: String,
    pub api_key_source: String,
    /// 明文密钥只在保存时上行；读取时不回传（用 api_key_source 判断是否已配置）
    pub api_key: String,
    pub is_active: bool,
    pub temperature: f64,
    pub max_tokens: Option<i64>,
}

fn dto_from_active(p: crate::storage::AiProviderConfig) -> AiProviderConfigDto {
    AiProviderConfigDto {
        id: p.id,
        name: p.name,
        provider_type: p.provider_type,
        base_url: p.base_url,
        model: p.model,
        api_key_source: p.api_key_source,
        api_key: String::new(),
        is_active: p.is_active,
        temperature: p.temperature,
        max_tokens: p.max_tokens,
    }
}

/// Get the active AI provider full config (for settings page prefill)
pub fn get_ai_provider_config() -> Result<Option<AiProviderConfigDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    Ok(store.active_ai_provider_config()?.map(dto_from_active))
}

/// 列出全部 AI provider 配置（多配置，仅一个 is_active=true）
pub fn list_ai_provider_configs() -> Result<Vec<AiProviderConfigDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    Ok(store
        .list_ai_provider_configs()?
        .into_iter()
        .map(|r| AiProviderConfigDto {
            id: r.id,
            name: r.name,
            provider_type: r.provider_type,
            base_url: r.base_url,
            model: r.model,
            api_key_source: r.api_key_source,
            api_key: String::new(),
            is_active: r.is_active,
            temperature: r.temperature,
            max_tokens: r.max_tokens,
        })
        .collect())
}

/// 新增或编辑 AI provider 配置；返回配置 id。
/// provider.id 为空表示新建；api_key 传空串表示保留原 key 不变（新建则必填）。
pub fn save_ai_provider_config(provider: AiProviderConfigDto) -> Result<String> {
    if provider.name.trim().is_empty() {
        anyhow::bail!("配置名称不能为空");
    }
    if provider.base_url.trim().is_empty() || provider.model.trim().is_empty() {
        anyhow::bail!("base_url 和 model 均不能为空");
    }
    if provider.id.trim().is_empty() && provider.api_key.trim().is_empty() {
        anyhow::bail!("新建配置时必须填写 API Key");
    }
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let id = if provider.id.trim().is_empty() {
        None
    } else {
        Some(provider.id.trim())
    };
    store.save_ai_provider_config(
        id,
        provider.name.trim(),
        &provider.provider_type,
        provider.base_url.trim(),
        provider.model.trim(),
        provider.api_key.trim(),
        provider.temperature,
        provider.max_tokens,
    )
}

/// 将指定配置设为激活（唯一激活项）
pub fn set_active_ai_provider_config(id: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.set_active_ai_provider_config(&id)?;
    Ok(())
}

/// 删除一个 AI provider 配置；若删除的是激活项，剩余第一条自动激活
pub fn delete_ai_provider_config(id: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.delete_ai_provider_config(&id)?;
    Ok(())
}

/// Upsert the active AI provider config (settings page save)
pub fn update_ai_provider_config(base_url: String, model: String, api_key: String) -> Result<()> {
    if base_url.trim().is_empty() || model.trim().is_empty() || api_key.trim().is_empty() {
        anyhow::bail!("base_url、model 和 api_key 均不能为空");
    }
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.upsert_ai_provider_config(&base_url, &model, &api_key)?;
    Ok(())
}

/// Trigger AI analysis for pending events
/// Returns "no_provider" or "processed:<successful count>".
pub fn trigger_analysis() -> Result<String> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let provider_config = store.active_ai_provider_config()?;
    let Some(provider_config) = provider_config else {
        return Ok("no_provider".to_string());
    };
    let provider = OpenAiCompatibleProvider::new(OpenAiCompatibleConfig {
        base_url: provider_config.base_url,
        api_key: provider_config.api_key,
        model: provider_config.model,
        temperature: provider_config.temperature as f32,
        max_tokens: provider_config.max_tokens.map(|v| v as u32),
    })?;
    process_analysis_queue(&store, &provider)
}

fn process_analysis_queue(store: &Store, provider: &dyn AiProvider) -> Result<String> {
    let mut processed = 0;
    // Bound each invocation even when producers keep adding work or retries
    // become available while a slow provider is processing other records.
    let stats = store.analysis_job_stats()?;
    for _ in 0..(stats.pending + stats.retry).min(50) {
        let Some(job) = store.claim_analysis_job()? else {
            break;
        };
        let context = decision_support_context(store)?;
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
    Ok(format!("processed:{processed}"))
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
            "processed:2"
        );
        assert_eq!(store.analysis_job_stats().unwrap().succeeded, 2);
        assert_eq!(store.list_analyses().unwrap().len(), 2);
        assert_eq!(
            process_analysis_queue(&store, &provider).unwrap(),
            "processed:0"
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
                "processed:0"
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

/// Conversation DTO for Flutter
#[derive(Clone, Debug)]
pub struct ConversationDto {
    pub id: String,
    pub title: Option<String>,
    pub tag: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: i32,
    pub last_message_preview: Option<String>,
    pub archived: bool,
    pub wiki_page_slug: Option<String>,
}

/// Message DTO for Flutter
#[derive(Clone, Debug)]
pub struct MessageDto {
    pub id: String,
    pub conversation_id: String,
    pub parent_message_id: Option<String>,
    pub role: String,
    pub content: String,
    pub created_at: String,
}

#[derive(Clone, Debug)]
pub struct PendingActionDto {
    pub id: String,
    pub action: String,
    pub args_json: String,
    pub created_at: String,
}

pub fn list_pending_actions(conversation_id: String) -> Result<Vec<PendingActionDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    Ok(store
        .pending_actions_for_conversation(&conversation_id)?
        .into_iter()
        .map(|item| PendingActionDto {
            id: item.id,
            action: item.action,
            args_json: item.args_json,
            created_at: item.created_at,
        })
        .collect())
}

pub fn update_pending_action_args(action_id: String, args_json: String) -> Result<bool> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let _: serde_json::Value = serde_json::from_str(&args_json)?;
    store.update_pending_action_args(&action_id, &args_json)
}

/// Create a new conversation
/// 个人经验规则 DTO
#[derive(Clone, Debug)]
pub struct RuleDto {
    pub id: String,
    pub content: String,
    pub status: String,
    pub created_at: String,
}

/// 列出规则库（含已生效与待确认）
pub fn list_rules() -> Result<Vec<RuleDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let rules = store.list_rules(None, None)?;
    Ok(rules
        .into_iter()
        .map(|r| RuleDto {
            id: r.id,
            content: r.content,
            status: r.status.as_str().to_string(),
            created_at: r.created_at,
        })
        .collect())
}

/// 新增一条规则（手动添加，直接生效）
pub fn add_rule(content: String) -> Result<String> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    store.add_rule(&content, RuleStatus::Active, None)
}

/// 删除一条规则
pub fn delete_rule(rule_id: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    store.delete_rule(&rule_id)?;
    Ok(())
}

pub fn create_conversation(title: Option<String>, tag: Option<String>) -> Result<ConversationDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let title_ref = title.as_deref();
    let tag_ref = tag.as_deref();
    let id = store.create_conversation(title_ref, tag_ref)?;

    let conversation = store
        .get_conversation(&id)?
        .ok_or_else(|| anyhow::anyhow!("Conversation not found after creation"))?;

    Ok(ConversationDto {
        id: conversation.id,
        title: conversation.title,
        tag: conversation.tag,
        created_at: conversation.created_at,
        updated_at: conversation.updated_at,
        message_count: conversation.message_count,
        last_message_preview: conversation.last_message_preview,
        archived: conversation.archived,
        wiki_page_slug: conversation.wiki_page_slug,
    })
}

/// List all non-archived conversations
pub fn list_conversations() -> Result<Vec<ConversationDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let conversations = store.list_conversations()?;

    Ok(conversations
        .into_iter()
        .map(|c| ConversationDto {
            id: c.id,
            title: c.title,
            tag: c.tag,
            created_at: c.created_at,
            updated_at: c.updated_at,
            message_count: c.message_count,
            last_message_preview: c.last_message_preview,
            archived: c.archived,
            wiki_page_slug: c.wiki_page_slug,
        })
        .collect())
}

/// List archived conversations
pub fn list_archived_conversations() -> Result<Vec<ConversationDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let conversations = store.list_archived_conversations()?;

    Ok(conversations
        .into_iter()
        .map(|c| ConversationDto {
            id: c.id,
            title: c.title,
            tag: c.tag,
            created_at: c.created_at,
            updated_at: c.updated_at,
            message_count: c.message_count,
            last_message_preview: c.last_message_preview,
            archived: c.archived,
            wiki_page_slug: c.wiki_page_slug,
        })
        .collect())
}

/// Rename a conversation
pub fn rename_conversation(conversation_id: String, title: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.rename_conversation(&conversation_id, &title)
}

/// Archive or unarchive a conversation
pub fn set_conversation_archived(conversation_id: String, archived: bool) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.set_conversation_archived(&conversation_id, archived)
}

/// Delete an archived ordinary conversation.
pub fn delete_archived_conversation(conversation_id: String) -> Result<bool> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.delete_archived_conversation(&conversation_id)
}

/// Get a specific conversation
pub fn get_conversation(conversation_id: String) -> Result<Option<ConversationDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    if let Some(conversation) = store.get_conversation(&conversation_id)? {
        Ok(Some(ConversationDto {
            id: conversation.id,
            title: conversation.title,
            tag: conversation.tag,
            created_at: conversation.created_at,
            updated_at: conversation.updated_at,
            message_count: conversation.message_count,
            last_message_preview: conversation.last_message_preview,
            archived: conversation.archived,
            wiki_page_slug: conversation.wiki_page_slug,
        }))
    } else {
        Ok(None)
    }
}

/// Send a message in a conversation
pub fn send_message(
    conversation_id: String,
    role: String,
    content: String,
    parent_message_id: Option<String>,
) -> Result<MessageDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let parent_ref = parent_message_id.as_deref();
    let message_id = store.send_message(&conversation_id, &role, &content, parent_ref)?;

    // 首条用户消息到达时自动生成对话标题，避免列表里一屏「新对话」
    if role == "user" {
        if let Some(title) = crate::storage::derive_conversation_title(&content) {
            let untitled = store
                .get_conversation(&conversation_id)?
                .and_then(|c| c.title)
                .map_or(true, |t| t.trim().is_empty());
            if untitled {
                store.rename_conversation(&conversation_id, &title)?;
            }
        }
    }

    let messages = store.list_messages(&conversation_id)?;
    let message = messages
        .into_iter()
        .find(|m| m.id == message_id)
        .ok_or_else(|| anyhow::anyhow!("Message not found after creation"))?;

    Ok(MessageDto {
        id: message.id,
        conversation_id: message.conversation_id,
        parent_message_id: message.parent_message_id,
        role: message.role,
        content: message.content,
        created_at: message.created_at,
    })
}

/// List messages in a conversation
pub fn list_messages(conversation_id: String) -> Result<Vec<MessageDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let messages = store.list_messages(&conversation_id)?;

    Ok(messages
        .into_iter()
        .map(|m| MessageDto {
            id: m.id,
            conversation_id: m.conversation_id,
            parent_message_id: m.parent_message_id,
            role: m.role,
            content: m.content,
            created_at: m.created_at,
        })
        .collect())
}

/// Get child messages of a specific message (for branching conversations)
pub fn get_child_messages(parent_id: String) -> Result<Vec<MessageDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let messages = store.get_child_messages(&parent_id)?;

    Ok(messages
        .into_iter()
        .map(|m| MessageDto {
            id: m.id,
            conversation_id: m.conversation_id,
            parent_message_id: m.parent_message_id,
            role: m.role,
            content: m.content,
            created_at: m.created_at,
        })
        .collect())
}

/// Get the message chain from root to a specific message
pub fn get_message_chain(message_id: String) -> Result<Vec<MessageDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let messages = store.get_message_chain(&message_id)?;

    Ok(messages
        .into_iter()
        .map(|m| MessageDto {
            id: m.id,
            conversation_id: m.conversation_id,
            parent_message_id: m.parent_message_id,
            role: m.role,
            content: m.content,
            created_at: m.created_at,
        })
        .collect())
}

/// Generate AI reply for a conversation
pub fn generate_reply(
    conversation_id: String,
    provider_type: Option<String>,
    memory_type: Option<String>,
    memory_window_size: Option<u32>,
) -> Result<String> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let provider = match provider_type.as_deref().unwrap_or("openai_compatible") {
        "openai_compatible" => crate::ai::ProviderType::OpenAiCompatible,
        "ollama" => crate::ai::ProviderType::Ollama,
        _ => anyhow::bail!("Unsupported provider type"),
    };

    let memory = match memory_type.as_deref().unwrap_or("sliding_window") {
        "simple" => crate::ai::MemoryType::Simple {
            max_messages: memory_window_size.unwrap_or(10) as usize,
        },
        "sliding_window" => crate::ai::MemoryType::SlidingWindow {
            max_tokens: memory_window_size.unwrap_or(4096) as usize,
        },
        _ => anyhow::bail!("Unsupported memory type"),
    };

    let conv_config = crate::ai::ConversationConfig {
        provider_type: provider,
        memory_type: memory,
    };

    crate::ai::generate_conversation_reply(&conversation_id, &store, Some(conv_config))
}

/// Daily token usage DTO for Flutter（每日 token 使用统计）
#[derive(Clone, Debug)]
pub struct DailyTokenUsageDto {
    pub date: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub call_count: i64,
}

/// 获取最近 N 天的每日 token 用量统计（含当天，日期倒序）
pub fn get_daily_token_usage(days: u32) -> Result<Vec<DailyTokenUsageDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let rows = store.daily_token_usage(days)?;
    Ok(rows
        .into_iter()
        .map(|d| DailyTokenUsageDto {
            date: d.date,
            prompt_tokens: d.prompt_tokens,
            completion_tokens: d.completion_tokens,
            total_tokens: d.total_tokens,
            call_count: d.call_count,
        })
        .collect())
}

/// Wiki page data transfer object for Flutter（知识库浏览）
#[derive(Clone, Debug)]
pub struct WikiPageDto {
    pub id: String,
    pub slug: String,
    pub kind: String,
    pub title: String,
    pub summary: String,
    pub content_md: String,
    pub tags: Vec<String>,
    pub source_event_ids: Vec<String>,
    pub evidence_count: i64,
    pub first_seen_at: String,
    pub last_seen_at: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub source_url: Option<String>,
    /// 来源/用途分区：imported（素材库）/ network（人物项目）/ insight（知识沉淀）/ derivative（派生产物）
    pub area: String,
    /// 派生产物指向的原页面 slug（仅 derivative 有值）
    pub based_on: Option<String>,
    /// 派生产物的加工类型（总结/提炼观点/抖音文案…，仅 derivative 有值）
    pub content_type: Option<String>,
    /// 最近一次人工编辑正文的时间（非空 ⇔ 该页由人工持有，digest 不整篇覆盖）
    pub human_edited_at: Option<String>,
    /// 素材页观点评价：Some("endorse")/Some("reject")/None=未表态（缺省认可）
    pub opinion: Option<String>,
}

impl From<crate::storage::WikiPage> for WikiPageDto {
    fn from(p: crate::storage::WikiPage) -> Self {
        Self {
            id: p.id,
            slug: p.slug,
            kind: p.kind,
            title: p.title,
            summary: p.summary,
            content_md: p.content_md,
            tags: p.tags,
            source_event_ids: p.source_event_ids,
            evidence_count: p.evidence_count,
            first_seen_at: p.first_seen_at,
            last_seen_at: p.last_seen_at,
            status: p.status,
            created_at: p.created_at,
            updated_at: p.updated_at,
            source_url: p.source_url,
            area: p.area,
            based_on: p.based_on,
            content_type: p.content_type,
            human_edited_at: p.human_edited_at,
            opinion: p.opinion,
        }
    }
}

/// List wiki pages（主列表）。kind/area 均为 None 时列出全部（不含派生产物）。
/// area：imported（素材库）/ network（人物项目）/ insight（知识沉淀）。
pub fn list_wiki_pages(kind: Option<String>, area: Option<String>) -> Result<Vec<WikiPageDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let pages = store.list_wiki_pages(kind.as_deref(), area.as_deref())?;
    Ok(pages.into_iter().map(WikiPageDto::from).collect())
}

/// 某页的派生产物列表（AI 加工成果，挂在该页详情下，不进主列表）。
pub fn list_wiki_page_derivatives(slug: String) -> Result<Vec<WikiPageDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let pages = store.list_derivatives(&slug)?;
    Ok(pages.into_iter().map(WikiPageDto::from).collect())
}

/// Get a single wiki page by slug
pub fn get_wiki_page(slug: String) -> Result<Option<WikiPageDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let page = store.get_wiki_page(&slug)?;
    Ok(page.map(WikiPageDto::from))
}

/// 更新知识页标签（应用内整理元数据用；传空数组即清空）。返回更新后的页面。
pub fn update_wiki_tags(slug: String, tags: Vec<String>) -> Result<WikiPageDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let page = store.update_wiki_tags(&slug, &tags)?;
    Ok(WikiPageDto::from(page))
}

/// 人类编辑保存一页正文（限可编辑 kind；素材页只读拒绝）。
/// 保存后 `human_edited_at` 置位：该页被 AI digest 视为人工持有，不再整篇覆盖。
/// `expected_updated_at`：乐观锁（§11 Q3）——传加载时的 updated_at（rfc3339），
/// 与当前不一致则报「编辑冲突」，拒绝静默覆盖编辑期间的后台写入；None 跳过校验。
pub fn save_wiki_page_content(
    slug: String,
    content_md: String,
    reason: String,
    expected_updated_at: Option<String>,
) -> Result<WikiPageDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let page = store.save_wiki_page_content(
        &slug,
        &content_md,
        &reason,
        expected_updated_at.as_deref(),
    )?;
    Ok(WikiPageDto::from(page))
}

/// 素材页观点评价（仅 source/note）。opinion：Some("endorse")=认可 / Some("reject")=不认可 /
/// None=清空回未表态（读取按缺省认可处理）。不改变正文、不置位人工编辑保护。
pub fn set_wiki_opinion(slug: String, opinion: Option<String>) -> Result<WikiPageDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let page = store.set_wiki_opinion(&slug, opinion.as_deref())?;
    Ok(WikiPageDto::from(page))
}

/// 一条人物关系 DTO（AI 从对话识别「人 ↔ 事情/项目」，用户确认后保存）
#[derive(Clone, Debug)]
pub struct RelationDto {
    pub id: String,
    pub from_slug: String,
    pub from_kind: String,
    pub to_slug: String,
    pub to_kind: String,
    pub relation: String,
    pub note: Option<String>,
    pub confidence: i64,
    pub created_at: String,
    pub last_seen_at: String,
}

impl From<crate::storage::Relation> for RelationDto {
    fn from(r: crate::storage::Relation) -> Self {
        Self {
            id: r.id,
            from_slug: r.from_slug,
            from_kind: r.from_kind,
            to_slug: r.to_slug,
            to_kind: r.to_kind,
            relation: r.relation,
            note: r.note,
            confidence: r.confidence,
            created_at: r.created_at,
            last_seen_at: r.last_seen_at,
        }
    }
}

/// 与某页相关的人物关系（双向：作为人物方或作为事情/项目方）
pub fn list_relations_for_page(slug: String) -> Result<Vec<RelationDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let rows = store.list_relations_for_page(&slug)?;
    Ok(rows.into_iter().map(RelationDto::from).collect())
}

/// 全部人物关系（备用：未来人物视图）
pub fn list_relations() -> Result<Vec<RelationDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let rows = store.list_relations()?;
    Ok(rows.into_iter().map(RelationDto::from).collect())
}

/// 手动添加/刷新一条人物关系（应用内修复合用；一般由 AI 草拟、用户确认后生成）。
/// 两侧页面必须已存在；返回落库后的关系（含真实 id）。
pub fn add_relation(
    from_slug: String,
    to_slug: String,
    relation: String,
    note: Option<String>,
) -> Result<RelationDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let from = store
        .get_wiki_page(&from_slug)?
        .ok_or_else(|| anyhow::anyhow!("起始页面不存在：{from_slug}"))?;
    let to = store
        .get_wiki_page(&to_slug)?
        .ok_or_else(|| anyhow::anyhow!("目标页面不存在：{to_slug}"))?;
    let rel = store.upsert_relation(&RelationDraft {
        from_slug,
        from_kind: from.kind,
        to_slug,
        to_kind: to.kind,
        relation,
        note,
        confidence: 3,
        source_conversation_id: None,
        source_event_id: None,
    })?;
    Ok(RelationDto::from(rel))
}

/// 删除一条人物关系（修正误识别时用）。返回是否真的删掉了。
pub fn delete_relation(id: String) -> Result<bool> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    Ok(store.delete_relation(&id)?)
}

/// 抓取的推文内容 DTO（只解析，不入库）
#[derive(Clone, Debug)]
pub struct TweetFetchDto {
    pub tweet_id: String,
    pub url: String,
    pub text: String,
    /// 文章型推文的标题（article.title），普通推文为 None
    pub title: Option<String>,
    pub author_name: Option<String>,
    pub screen_name: Option<String>,
}

/// 从 x.com / twitter.com 推文链接抓取长文（只解析 json，不写库）。
/// 是否入库由后续「保存」动作决定。
pub fn fetch_tweet(url: String) -> Result<TweetFetchDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    // 按设置的抓取服务分发（当前仅 fxtwitter）
    let service = store
        .get_meta("tweet_fetch_service")?
        .unwrap_or_else(|| "fxtwitter".to_string());
    if service != "fxtwitter" {
        anyhow::bail!("暂不支持的推文抓取服务: {service}");
    }

    let t = crate::wiki::fetch_tweet_text(&url)?;
    Ok(TweetFetchDto {
        tweet_id: t.tweet_id,
        url,
        text: t.text,
        title: t.title,
        author_name: t.author_name,
        screen_name: t.screen_name,
    })
}

/// 把已抓取的推文内容保存为知识库页面（kind=source）并返回该页。
/// 只有用户点击「保存」才走这里入库存。
pub fn save_tweet_page(
    tweet_id: String,
    text: String,
    title: Option<String>,
    author_name: Option<String>,
    screen_name: Option<String>,
) -> Result<WikiPageDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let t = crate::wiki::TweetText {
        tweet_id,
        text,
        title,
        author_name,
        screen_name,
    };
    let page = crate::wiki::save_tweet_page(
        &t,
        Some(&format!("https://x.com/i/status/{}", t.tweet_id)),
        &store,
    )?;
    Ok(WikiPageDto::from(page))
}

/// 把用户粘贴的纯文本保存为知识库页面（kind=topic），返回该页。
/// content_md 保留全文，不截断；tags 可选（页面保留「note」锚点标签）。
pub fn save_text_page(
    text: String,
    title: Option<String>,
    tags: Vec<String>,
) -> Result<WikiPageDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let page = crate::wiki::save_text_page(&text, title.as_deref(), &tags, &store)?;
    Ok(WikiPageDto::from(page))
}

/// 判断一个网址是否需要走专用抓取 API（当前：x.com/twitter.com 推文 → fxtwitter）。
/// 返回 "tweet" 或 "web"，供导入入口统一分发，避免前端各自猜测。
pub fn guess_import_kind(url: String) -> Result<String> {
    let trimmed = url.trim();
    if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        anyhow::bail!("仅支持 http/https 链接");
    }
    Ok(if crate::wiki::is_tweet_url(trimmed) {
        "tweet".to_string()
    } else {
        "web".to_string()
    })
}

/// 内容对话消息 DTO（临时讨论的一条消息）
#[derive(Clone, Debug)]
pub struct ContentChatMessageDto {
    pub role: String,
    pub content: String,
}

/// 针对一段抓取内容做一次性对话回复（不写库，供保存前与 AI 讨论内容）
pub fn generate_content_chat(
    content: String,
    messages: Vec<ContentChatMessageDto>,
) -> Result<String> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let msgs: Vec<crate::ai::ContentChatMessage> = messages
        .into_iter()
        .map(|m| crate::ai::ContentChatMessage {
            role: m.role,
            content: m.content,
        })
        .collect();
    crate::ai::generate_content_chat(&content, &msgs, &store)
}

/// 当前推文抓取服务（设置页读取；当前仅支持 fxtwitter）
pub fn get_tweet_fetch_service() -> Result<String> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    Ok(store
        .get_meta("tweet_fetch_service")?
        .unwrap_or_else(|| "fxtwitter".to_string()))
}

/// 更新推文抓取服务（设置页保存）
pub fn update_tweet_fetch_service(service: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.set_meta("tweet_fetch_service", &service)
}

/// 按推文链接查知识库是否已存在对应页面（URL 查重）。
/// 已保存过则返回该页 DTO（UI 直接打开、不再抓取）；否则返回 None。
/// 链接格式不合法 / 未找到都算「不存在」。
pub fn find_tweet_source_page(url: String) -> Result<Option<WikiPageDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let Some(id) = crate::wiki::extract_tweet_id(&url) else {
        return Ok(None);
    };
    let slug = format!("tweet-{id}");
    let page = store.get_wiki_page(&slug)?;
    Ok(page.map(WikiPageDto::from))
}

/// 主题偏好 DTO（设置页「外观」：模式 + 预设；存 app_meta）
#[derive(Clone, Debug)]
pub struct ThemePrefsDto {
    /// "light" | "dark" | "system"
    pub mode: String,
    /// 预设名，如 "amber" | "indigo" | "aqua" | "violet"
    pub preset: String,
}

/// 读取主题偏好（默认深色 + 琥珀，保留现有观感）
pub fn get_theme_prefs() -> Result<ThemePrefsDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let mode = store
        .get_meta("theme_mode")?
        .unwrap_or_else(|| "dark".to_string());
    let preset = store
        .get_meta("theme_preset")?
        .unwrap_or_else(|| "amber".to_string());
    Ok(ThemePrefsDto { mode, preset })
}

/// 保存主题偏好（设置页「外观」保存）
pub fn update_theme_prefs(mode: String, preset: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.set_meta("theme_mode", &mode)?;
    store.set_meta("theme_preset", &preset)
}

// ── 个人待办（todo） ──────────────────────────────────────────────────

/// 待办 DTO
#[derive(Clone, Debug)]
pub struct TodoDto {
    pub id: String,
    pub title: String,
    pub status: String,
    pub priority: String,
    pub due_at: Option<String>,
    pub related_event_id: Option<String>,
    pub related_wiki_slug: Option<String>,
    pub note: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<crate::storage::Todo> for TodoDto {
    fn from(t: crate::storage::Todo) -> Self {
        Self {
            id: t.id,
            title: t.title,
            status: t.status.as_str().to_string(),
            priority: t.priority,
            due_at: t.due_at,
            related_event_id: t.related_event_id,
            related_wiki_slug: t.related_wiki_slug,
            note: t.note,
            created_at: t.created_at,
            updated_at: t.updated_at,
        }
    }
}

/// 列出待办（status 过滤：open/done/archived；None 时列出 open+done）
pub fn list_todos(status: Option<String>) -> Result<Vec<TodoDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let todos = store.list_todos(status.as_deref())?;
    Ok(todos.into_iter().map(TodoDto::from).collect())
}

/// 新建待办（用户手动创建，直接生效）
pub fn create_todo(
    title: String,
    due_at: Option<String>,
    priority: Option<String>,
    related_wiki_slug: Option<String>,
    note: Option<String>,
) -> Result<TodoDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    if title.trim().is_empty() {
        anyhow::bail!("待办内容不能为空");
    }
    let t = store.create_todo(
        title.trim(),
        priority.as_deref().unwrap_or("normal"),
        due_at.as_deref(),
        None,
        related_wiki_slug.as_deref(),
        note.as_deref(),
    )?;
    Ok(TodoDto::from(t))
}

/// 更新待办状态（open/done/archived）
pub fn update_todo_status(id: String, status: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.update_todo_status(&id, crate::storage::TodoStatus::parse(&status))
}

/// 打开待办对应的可讨论工作项；无关联页时按需创建并回写关联。
pub fn open_todo_work_item(id: String) -> Result<WikiPageDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let todo = store
        .get_todo(&id)?
        .ok_or_else(|| anyhow::anyhow!("未找到待办（id={id}）"))?;
    if let Some(slug) = todo.related_wiki_slug.as_deref() {
        return store
            .get_wiki_page(slug)?
            .map(WikiPageDto::from)
            .ok_or_else(|| anyhow::anyhow!("待办关联页面不存在（slug={slug}）"));
    }
    let marker = format!("work-item-id:{}", todo.id);
    if let Some(existing) = store
        .list_wiki_pages(None, None)?
        .into_iter()
        .find(|page| page.tags.iter().any(|tag| tag == &marker))
    {
        store.set_todo_related_wiki_slug(&todo.id, &existing.slug)?;
        return Ok(WikiPageDto::from(existing));
    }
    let mut content = format!("# {}\n\n", todo.title);
    content.push_str("## 工作项状态\n\n");
    content.push_str(&format!("- 状态：{}\n", todo.status.as_str()));
    content.push_str(&format!("- 优先级：{}\n", todo.priority));
    if let Some(due_at) = todo.due_at.as_deref() {
        content.push_str(&format!("- 截止：{}\n", due_at));
    }
    if let Some(note) = todo.note.as_deref() {
        content.push_str(&format!("\n## 说明\n\n{}\n", note));
    }
    let page = crate::wiki::save_text_page(
        &content,
        Some(&todo.title),
        &["work-item".to_string(), marker],
        &store,
    )?;
    store.set_todo_related_wiki_slug(&todo.id, &page.slug)?;
    Ok(WikiPageDto::from(page))
}

/// 更新待办的可编辑字段（标题 / 补充 / 优先级 / 截止时间）。
/// 可选字段传 None 表示清除（如结束拖延、去掉截止时间）。
pub fn update_todo(
    id: String,
    title: String,
    note: Option<String>,
    priority: Option<String>,
    due_at: Option<String>,
) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.update_todo(
        &id,
        &title,
        note.as_deref(),
        priority.as_deref(),
        due_at.as_deref(),
    )
}

/// 删除一条待办
pub fn delete_todo(id: String) -> Result<bool> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.delete_todo(&id)
}

// ── 任意 URL 导入 ──────────────────────────────────────────────────────

/// 任意 URL 抓取结果 DTO（推文或普通页面，只解析不入库）
#[derive(Clone, Debug)]
pub struct ImportUrlDto {
    pub source_url: String,
    /// "tweet" | "webpage"
    pub source_kind: String,
    pub title: Option<String>,
    pub content_md: String,
    pub author_name: Option<String>,
    pub screen_name: Option<String>,
}

/// 抓取任意 URL 的内容（推文走 fxtwitter，普通页面走 HTML 文本提取）。
/// 只解析不写库，由后续「保存」动作决定。
pub fn fetch_import_url(url: String) -> Result<ImportUrlDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    if !url.starts_with("http://") && !url.starts_with("https://") {
        anyhow::bail!("仅支持 http/https 链接");
    }
    let service = store
        .get_meta("tweet_fetch_service")?
        .unwrap_or_else(|| "fxtwitter".to_string());
    if service != "fxtwitter" && crate::wiki::is_tweet_url(&url) {
        anyhow::bail!("暂不支持的推文抓取服务: {service}");
    }
    let c = crate::wiki::fetch_import_url(&url)?;
    Ok(ImportUrlDto {
        source_url: c.source_url,
        source_kind: c.source_kind,
        title: c.title,
        content_md: c.content_md,
        author_name: c.author_name,
        screen_name: c.screen_name,
    })
}

/// 把抓取到的 URL 内容保存为知识库页面（kind=source，带 source_url 溯源）。
/// 用户点击「保存」才走这里入库。
pub fn save_imported_page(
    title: String,
    content_md: String,
    source_url: String,
    source_kind: String,
    tags: Vec<String>,
) -> Result<WikiPageDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let trimmed = content_md.trim();
    if trimmed.is_empty() {
        anyhow::bail!("内容为空，无法保存");
    }
    let title = if title.trim().is_empty() {
        "未命名导入".to_string()
    } else {
        title.trim().to_string()
    };
    let summary: String = trimmed.chars().take(120).collect();
    // URL 去重：同一条 source_url 已导入过 → 走更新而不是复制新页
    let existing_slug = store
        .find_wiki_page_by_source_url(&source_url)?
        .map(|p| p.slug);
    let slug = match existing_slug {
        Some(slug) => slug,
        None => format!(
            "{}-{}",
            if source_kind == "tweet" {
                "tweet"
            } else {
                "import"
            },
            &uuid::Uuid::new_v4().to_string()[..8]
        ),
    };
    let mut all_tags = tags;
    all_tags.push("import".to_string());
    if source_kind == "webpage" {
        all_tags.push("web".to_string());
    } else {
        all_tags.push("tweet".to_string());
    }
    all_tags.sort();
    all_tags.dedup();
    let draft = crate::storage::WikiPageDraft {
        slug,
        kind: "source".to_string(),
        title,
        summary,
        content_md: trimmed.to_string(),
        tags: all_tags,
        source_event_ids: vec![],
        status: "active".to_string(),
        reason: format!("从 {source_url} 导入"),
        source_url: Some(source_url),
    };
    let outcome = store.upsert_wiki_page(&draft, ContentPolicy::Always)?;
    Ok(WikiPageDto::from(outcome.page))
}

// ── 知识页内 AI 处理会话 ──────────────────────────────────────────────

/// 获取（不存在则创建）某个知识页的处理会话，返回会话 DTO。
/// 页内 AI 聊天通过该会话进行；生成时自动注入页面内容。
pub fn ensure_wiki_page_chat(page_slug: String) -> Result<ConversationDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let Some(page) = store.get_wiki_page(&page_slug)? else {
        anyhow::bail!("知识页不存在: {page_slug}");
    };
    let conversation_id = match store.find_wiki_chat_conversation(&page_slug)? {
        Some(id) => id,
        None => {
            store.create_wiki_chat_conversation(&page_slug, &format!("[知识页] {}", page.title))?
        }
    };
    let conversation = store
        .get_conversation(&conversation_id)?
        .ok_or_else(|| anyhow::anyhow!("会话创建失败"))?;
    Ok(ConversationDto {
        id: conversation.id,
        title: conversation.title,
        tag: conversation.tag,
        created_at: conversation.created_at,
        updated_at: conversation.updated_at,
        message_count: conversation.message_count,
        last_message_preview: conversation.last_message_preview,
        archived: conversation.archived,
        wiki_page_slug: conversation.wiki_page_slug,
    })
}

/// 删除一个知识页的处理会话（重建时用）
pub fn archive_wiki_page_chat(page_slug: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let Some(conversation_id) = store.find_wiki_chat_conversation(&page_slug)? else {
        return Ok(());
    };
    store.set_conversation_archived(&conversation_id, true)
}

#[cfg(test)]
mod daily_review_tests {
    use super::*;
    use crate::ai::provider::AiReply;
    use crate::ai::tool::ToolSpec;
    use crate::event::NewEvent;
    use crate::storage::Store;
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
        assert!(result.starts_with("created:"));
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
