//! 事件与实体抽取 DTO：EventRecord/InputRecord/实体事实与分析明细及行映射。

use super::*;

/// 带 id 的事件记录（digest 需要把事件 id 写进 wiki 页作为溯源）
#[derive(Debug, Clone)]
pub struct EventRecord {
    pub id: String,
    pub recorded_at: String,
    pub raw_text: String,
}

/// 一次用户原始提交及其路由结果。它只负责关联，不取代 event/message/wiki/todo
/// 各自的权威数据；raw_text 保存提交时的原文，后续路由不得覆盖。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputRecord {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyEntry {
    pub event_id: String,
    pub input_id: Option<String>,
    pub message_id: Option<String>,
    pub raw_text: String,
    pub source: String,
    pub event_status: String,
    pub recorded_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyReviewRecord {
    pub id: String,
    pub date: String,
    pub prompt_version: String,
    pub result_json: String,
    pub source_event_ids: Vec<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityFact {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityMergeStatus {
    pub source_slug: String,
    pub target_slug: String,
    pub entity_kind: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct EventAnalysisDetail {
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
    pub result_json: Option<String>,
    pub analysis_created_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PageGuidanceItem {
    pub analysis_id: String,
    pub event_id: String,
    pub recorded_at: String,
    pub note: Option<String>,
    pub lessons: Vec<String>,
    pub follow_ups_refined: Vec<String>,
    /// 本机采纳反馈（guidance_feedback 表注入；None = 未反馈）。
    pub feedback: Option<String>,
}

/// 情绪标注面（D5）：从 event_analyses.result_json 的 `$.emotion` 派生。
/// D0 原定的 `event_emotions` 独立表不建——result_json 本身就是分析产物真源
/// 且随快照同步，独立转写表会引入跨设备删除不一致；编辑情绪 = 编辑
/// result_json 里的 emotion 字段，事件真源（events 表）不动。
pub struct EventEmotionItem {
    /// 所属分析结果 id（删除定位键）。
    pub analysis_id: String,
    pub event_id: String,
    pub recorded_at: String,
    /// None = 对事件本身的感受（D0 语义）。
    pub target: Option<String>,
    pub feeling: String,
}

/// 宽容解析 JSON 字符串数组：坏类型/坏 JSON 一律空数组（派生层不因展示瑕疵报错）。
pub(crate) fn parse_json_string_array(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw).unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventRecordabilityDecision {
    pub event_id: String,
    pub recordable: bool,
    pub kind: String,
    pub reason: String,
    pub created_at: String,
}

pub(crate) fn map_input_record(row: &rusqlite::Row) -> rusqlite::Result<InputRecord> {
    Ok(InputRecord {
        id: row.get(0)?,
        raw_text: row.get(1)?,
        source: row.get(2)?,
        route_status: row.get(3)?,
        idempotency_key: row.get(4)?,
        event_id: row.get(5)?,
        message_id: row.get(6)?,
        wiki_page_slug: row.get(7)?,
        todo_id: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

pub(crate) fn map_wiki_page(row: &rusqlite::Row) -> rusqlite::Result<WikiPage> {
    let tags_raw: String = row.get(6)?;
    let sources_raw: String = row.get(7)?;
    Ok(WikiPage {
        id: row.get(0)?,
        slug: row.get(1)?,
        kind: row.get(2)?,
        title: row.get(3)?,
        summary: row.get(4)?,
        content_md: row.get(5)?,
        tags: serde_json::from_str(&tags_raw).unwrap_or_default(),
        source_event_ids: serde_json::from_str(&sources_raw).unwrap_or_default(),
        evidence_count: row.get(8)?,
        first_seen_at: row.get(9)?,
        last_seen_at: row.get(10)?,
        status: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
        source_url: row.get(14)?,
        area: row.get(15)?,
        based_on: row.get(16)?,
        content_type: row.get(17)?,
        human_edited_at: row.get(18)?,
        opinion: row.get(19)?,
    })
}

/// wiki_pages 行 → WikiPage 的公共列清单。
/// 顺序必须与 `map_wiki_page` 的按位取值（0..=19）严格一致。
pub(crate) const WIKI_PAGE_COLS: &str = "id, slug, kind, title, summary, content_md, tags, source_event_ids, \
     evidence_count, first_seen_at, last_seen_at, status, created_at, updated_at, source_url, \
     COALESCE(area, 'insight'), based_on, content_type, human_edited_at, opinion";
