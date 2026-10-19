//! 事件读面：列表/按日/每日条目、日记 review、情绪与规则命中查询。

use super::*;

impl Store {
    pub fn save_daily_review(
        &self,
        date: chrono::NaiveDate,
        prompt_version: &str,
        result_json: &str,
        source_event_ids: &[String],
    ) -> Result<String> {
        let prompt_version = prompt_version.trim();
        if prompt_version.is_empty() {
            anyhow::bail!("daily review prompt_version 不能为空");
        }
        if source_event_ids.is_empty() {
            anyhow::bail!("daily review 必须引用至少一条来源事件");
        }
        let daily_event_ids = self
            .daily_entries(date)?
            .into_iter()
            .map(|entry| entry.event_id)
            .collect::<Vec<_>>();
        for event_id in source_event_ids {
            if !daily_event_ids
                .iter()
                .any(|candidate| candidate == event_id)
            {
                anyhow::bail!("daily review 来源不属于目标日期: {event_id}");
            }
        }

        let transaction = self.connection.unchecked_transaction()?;
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        transaction.execute(
            "INSERT INTO daily_reviews
         (id,review_date,prompt_version,result_json,created_at)
         VALUES (?1,?2,?3,?4,?5)",
            params![
                id,
                date.format("%Y-%m-%d").to_string(),
                prompt_version,
                result_json,
                now
            ],
        )?;
        for event_id in source_event_ids {
            transaction.execute(
                "INSERT OR IGNORE INTO daily_review_sources (review_id,event_id)
             VALUES (?1,?2)",
                params![id, event_id],
            )?;
        }
        transaction.commit()?;
        Ok(id)
    }

    pub fn latest_daily_review(
        &self,
        date: chrono::NaiveDate,
    ) -> Result<Option<DailyReviewRecord>> {
        let review = self
            .connection
            .query_row(
                "SELECT id,review_date,prompt_version,result_json,created_at
             FROM daily_reviews WHERE review_date=?1
             ORDER BY created_at DESC,rowid DESC LIMIT 1",
                [date.format("%Y-%m-%d").to_string()],
                |row| {
                    Ok(DailyReviewRecord {
                        id: row.get(0)?,
                        date: row.get(1)?,
                        prompt_version: row.get(2)?,
                        result_json: row.get(3)?,
                        source_event_ids: Vec::new(),
                        created_at: row.get(4)?,
                    })
                },
            )
            .optional()?;
        let Some(mut review) = review else {
            return Ok(None);
        };
        let mut statement = self.connection.prepare(
            "SELECT event_id FROM daily_review_sources
         WHERE review_id=?1 ORDER BY rowid",
        )?;
        review.source_event_ids = statement
            .query_map([&review.id], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(Some(review))
    }

    pub fn list_events(&self) -> Result<Vec<EventSummary>> {
        let mut statement = self.connection.prepare(
            "SELECT id, recorded_at, raw_text, source, status
         FROM events ORDER BY recorded_at DESC, id DESC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(EventSummary {
                id: row.get(0)?,
                recorded_at: row.get(1)?,
                raw_text: row.get(2)?,
                source: row.get(3)?,
                status: row.get(4)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// 查某个「本地日历日」记录的事件（按 recorded_at，本地时区日界 → UTC 区间，倒序）。
    /// 用户说「6月20日有哪些事件」→ 用本地日界解释，跨时区也正确。
    pub fn events_on_date(&self, date: chrono::NaiveDate) -> Result<Vec<EventSummary>> {
        use chrono::{Local, TimeZone};
        let day_edges = |d: chrono::NaiveDate| {
            let naive_local = d.and_hms_opt(0, 0, 0).expect("midnight is valid");
            match Local
                .from_local_datetime(&naive_local)
                .single()
                .or_else(|| Local.from_local_datetime(&naive_local).earliest())
            {
                Some(dt) => dt.with_timezone(&chrono::Utc).to_rfc3339(),
                // DST 空洞等罕见情形：按 UTC 同名时刻兜底，避免 panic
                None => naive_local.and_utc().to_rfc3339(),
            }
        };
        let start_utc = day_edges(date);
        let end_utc = day_edges(date + chrono::Duration::days(1));
        let mut statement = self.connection.prepare(
            "SELECT id, recorded_at, raw_text, source, status FROM events
         WHERE recorded_at >= ?1 AND recorded_at < ?2
         ORDER BY recorded_at DESC, id DESC",
        )?;
        let rows = statement.query_map(params![start_utc, end_utc], |row| {
            Ok(EventSummary {
                id: row.get(0)?,
                recorded_at: row.get(1)?,
                raw_text: row.get(2)?,
                source: row.get(3)?,
                status: row.get(4)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// 统一日流：events 是权威全集；input_records 仅补充新流程的关联信息。
    /// 因此历史事件与新 Capture/对话输入都会出现，且每个 event 只返回一次。
    pub fn daily_entries(&self, date: chrono::NaiveDate) -> Result<Vec<DailyEntry>> {
        let (start, end) = local_day_bounds(date);

        let mut statement = self.connection.prepare(
            "SELECT e.id,i.id,i.message_id,e.raw_text,e.source,e.status,e.recorded_at
         FROM events e LEFT JOIN input_records i ON i.event_id=e.id
         WHERE e.recorded_at>=?1 AND e.recorded_at<?2
         ORDER BY e.recorded_at DESC,e.id DESC",
        )?;
        let rows = statement.query_map(params![start, end], |row| {
            Ok(DailyEntry {
                event_id: row.get(0)?,
                input_id: row.get(1)?,
                message_id: row.get(2)?,
                raw_text: row.get(3)?,
                source: row.get(4)?,
                event_status: row.get(5)?,
                recorded_at: row.get(6)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// 共同记忆回引（D6 每日回顾勾连）：回看一周（7 天前那天），空则看
    /// 8/9 天前；原文快照不评价。最多 max（3）条。
    pub fn memory_echo_for(
        &self,
        date: chrono::NaiveDate,
        max: usize,
    ) -> Result<Vec<(String, String, String)>> {
        for lookback in 7..=9 {
            let entries = self.daily_entries(date - chrono::Duration::days(lookback))?;
            if !entries.is_empty() {
                return Ok(entries
                    .into_iter()
                    .take(max)
                    .map(|entry| (entry.event_id, entry.recorded_at, entry.raw_text))
                    .collect());
            }
        }
        Ok(Vec::new())
    }

    /// 当日规则命中复用同款本地日界（D6 每日回顾勾连）。
    pub fn rule_hits_on(
        &self,
        date: chrono::NaiveDate,
    ) -> Result<Vec<crate::storage::rule_triggers::RuleHitView>> {
        let (start, end) = local_day_bounds(date);
        self.rule_hits_on_day(&start, &end)
    }

    /// 最近 N 天内的最多 limit 条事件（按记录时间倒序）
    /// 知识库页聚合视图（D4）：哪些事件的分析 guidance 引用了此页。
    /// 从 event_analyses.result_json 派生（fail-soft：坏 JSON 行跳过），
    /// 只读不写——事件与页面真源都不动。
    /// 情绪标注列表（D5 派生层）：扫最近分析的 `$.emotion`。任何单行坏 JSON
    /// 静默跳过；读面故障降级为空列表（面板不因展示瑕疵报错）。
    pub fn list_event_emotions(&self, limit: i64) -> Result<Vec<EventEmotionItem>> {
        let mut statement = self.connection.prepare(
            "SELECT a.id, a.event_id, e.recorded_at,
                    json_extract(je.value, '$.target'),
                    COALESCE(json_extract(je.value, '$.feeling'), '')
             FROM event_analyses a
             JOIN events e ON e.id = a.event_id
             JOIN json_each(a.result_json, '$.emotion') je
             ORDER BY e.recorded_at DESC, a.created_at DESC
             LIMIT ?1",
        )?;
        let mut items = Vec::new();
        let rows = statement.query_map(params![limit], |row| {
            let target: Option<String> = row.get(3)?;
            let target = target
                .map(|t| t.trim().to_owned())
                .filter(|t| !t.is_empty());
            Ok(EventEmotionItem {
                analysis_id: row.get(0)?,
                event_id: row.get(1)?,
                recorded_at: row.get(2)?,
                target,
                feeling: row.get::<_, String>(4)?.trim().to_owned(),
            })
        });
        match rows {
            Ok(mapped) => {
                // json_each 遇坏 JSON 行会在迭代中报错：丢掉整段（fail-soft）。
                for item in mapped.flatten() {
                    items.push(item);
                }
            }
            Err(_) => return Ok(Vec::new()),
        }
        items.retain(|item| !item.feeling.is_empty());
        Ok(items)
    }

    /// 删除一条情绪标注：只有 result_json 的 emotion 数组被动（事件真源不动）。
    /// 返回是否找到并移除；数组清空后整字段移除。找不到/解析失败返回 Ok(false)。
    pub fn delete_event_emotion(
        &self,
        analysis_id: &str,
        target: Option<&str>,
        feeling: &str,
    ) -> Result<bool> {
        let raw: Option<String> = self
            .connection
            .query_row(
                "SELECT result_json FROM event_analyses WHERE id=?1",
                params![analysis_id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(raw) = raw else {
            return Ok(false);
        };
        let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&raw) else {
            return Ok(false);
        };
        let Some(emotions) = value.pointer_mut("/emotion").and_then(|v| v.as_array_mut()) else {
            return Ok(false);
        };
        let wanted = target
            .map(|t| t.trim().to_owned())
            .filter(|t| !t.is_empty());
        let feeling = feeling.trim();
        let position = emotions.iter().position(|tag| {
            tag.get("feeling").and_then(|f| f.as_str()) == Some(feeling)
                && tag
                    .get("target")
                    .and_then(|t| t.as_str())
                    .map(|t| t.trim().to_owned())
                    .filter(|t| !t.is_empty())
                    == wanted
        });
        let Some(position) = position else {
            return Ok(false);
        };
        emotions.remove(position);
        if emotions.is_empty() {
            value
                .as_object_mut()
                .map(|obj| obj.remove("emotion"))
                .context("移除空 emotion 字段")?;
        }
        let updated = serde_json::to_string(&value)?;
        let changed = self.connection.execute(
            "UPDATE event_analyses SET result_json=?1 WHERE id=?2",
            params![updated, analysis_id],
        )?;
        Ok(changed > 0)
    }
}
