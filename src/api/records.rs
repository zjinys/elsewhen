//! 事件录入（submit/url/会话输入）与事件/分析清单查询、分析队列处理。

use super::*;

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

pub(crate) fn entry_is_recordable(store: &Store, event_id: &str) -> bool {
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

pub(crate) fn effective_event_recordability(
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
