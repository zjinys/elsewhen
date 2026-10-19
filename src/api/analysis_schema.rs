//! 事件分析 schema（V1 解析/规范化）与对外 DTO 定义。

use super::*;

pub(crate) const EVENT_ANALYSIS_VERSION: &str = "event-analysis-v3";
pub(crate) const PRE_V3_EVENT_ANALYSIS: &str = "event-analysis";
pub(crate) const LEGACY_EVENT_ANALYSIS_V1: &str = "event-analysis-v1";
pub(crate) const LEGACY_EVENT_ANALYSIS_V2: &str = "event-analysis-v2";
pub(crate) const DAILY_REVIEW_VERSION: &str = "daily-review-v1";
/// 单次队列排空的总墙钟时间上限（秒）。provider 单次超时可达 60s，
/// 一批最多 50 条，若无上限会长时间占住分析 worker（P2 worker 限时）。
pub(crate) const ANALYSIS_BATCH_MAX_SECS: u64 = 120;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EventAnalysisV1 {
    pub(crate) schema_version: String,
    pub(crate) event_type: String,
    pub(crate) confidence: f64,
    pub(crate) summary: String,
    pub(crate) clarifications: Vec<String>,
    pub(crate) people: Vec<String>,
    pub(crate) projects: Vec<String>,
    #[serde(default)]
    pub(crate) activities: Vec<String>,
    pub(crate) follow_ups: Vec<String>,
    #[serde(default = "default_recordable")]
    pub(crate) recordable: bool,
    #[serde(default = "default_event_kind")]
    pub(crate) kind: String,
    /// D1：规则预筛判定的回填（命中规则进 prompt，模型判定适用性/置信）。
    #[serde(default)]
    pub(crate) rule_confirmations: Vec<EventRuleConfirmation>,
    /// D4 启用：指导维度（结构就位，prompt 未要求时为 None）。
    #[serde(default)]
    pub(crate) guidance: Option<EventGuidance>,
    /// D5 启用：情绪标注（结构就位，prompt 未要求时为 None）。
    #[serde(default)]
    pub(crate) emotion: Option<Vec<EventEmotionTag>>,
}

/// 规则适用性判定（D1）。rule_id 必须是 prompt 附带的 pending 命中之 id。
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EventRuleConfirmation {
    pub(crate) rule_id: String,
    pub(crate) applicable: bool,
    pub(crate) confidence: f64,
}

/// 指导维度（D4 落地用）。related 指向知识页 slug，受引用校验约束。
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EventGuidance {
    #[serde(default)]
    pub(crate) related: Vec<EventGuidanceRef>,
    #[serde(default)]
    pub(crate) lessons: Vec<String>,
    #[serde(default)]
    pub(crate) follow_ups_refined: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EventGuidanceRef {
    pub(crate) slug: String,
    pub(crate) note: String,
}

/// 情绪标注（D5 落地用）。target 为人物/项目 slug 或原文提及；无 target =
/// 对事件本身的感受。只有文本，无数值评分（红线：不是情绪分析报告）。
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EventEmotionTag {
    #[serde(default)]
    pub(crate) target: Option<String>,
    pub(crate) feeling: String,
}

pub(crate) fn default_recordable() -> bool {
    true
}
pub(crate) fn default_event_kind() -> String {
    "event".to_string()
}

impl EventAnalysisV1 {
    pub(crate) fn parse(raw: &str) -> Result<Self> {
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

pub(crate) fn normalize_strings(values: &mut Vec<String>) {
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
pub(crate) struct DailyReviewItemV1 {
    pub(crate) text: String,
    pub(crate) source_event_ids: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DailyReviewV1 {
    pub(crate) schema_version: String,
    pub(crate) date: String,
    pub(crate) accomplishments: Vec<DailyReviewItemV1>,
    pub(crate) ideas_decisions: Vec<DailyReviewItemV1>,
    pub(crate) people_projects: Vec<DailyReviewItemV1>,
    pub(crate) follow_ups: Vec<DailyReviewItemV1>,
}

impl DailyReviewV1 {
    pub(crate) fn parse(
        raw: &str,
        expected_date: &str,
        allowed_sources: &[String],
    ) -> Result<Self> {
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
