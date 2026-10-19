//! 回顾页周/月统计与 daily overview 聚合。

use super::*;

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
pub(crate) const PERIOD_TOP_N: usize = 12;

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

pub(crate) fn generate_daily_review_with_provider(
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
