//! 日报/页面指引/情绪洗涤/伴侣偏好/再分析等 API 流程。

use super::*;

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
    let Some(provider) = crate::ai::provider::active_first_provider_chain(&store)? else {
        return Ok(AnalysisTriggerResult::NoProvider);
    };
    // 每次 worker tick 先重新清扫遗留的 running 任务：GUI 只在启动时恢复一次，
    // 若上一次队列运行中途退出，被 claim 的任务会孤悬到下次启动。
    crate::storage::with_busy_retry(|| {
        store
            .recover_interrupted_analysis_jobs()
            .context("analysis worker: recover interrupted jobs")
    })?;
    let processed = process_analysis_queue(&store, provider.as_ref())?;
    // 对话影子校验判定阶段（迁移 54）：预筛在聊天生成链已落 pending，
    // 这里用同一 provider 逐条判定适用性。失败不档其它队列（下轮重试）。
    let judged = process_chat_rule_check_queue(&store, provider.as_ref())?;
    Ok(AnalysisTriggerResult::Processed {
        count: processed + judged,
    })
}
