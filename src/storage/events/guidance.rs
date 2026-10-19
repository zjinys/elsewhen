//! 页面指引反馈与洞见（insights）：近事件、写入/列举、本地日界工具。

use super::*;

impl Store {
    pub fn page_guidance_items(&self, slug: &str) -> Result<Vec<PageGuidanceItem>> {
        let mut statement = self.connection.prepare(
            "SELECT a.id, a.event_id, e.recorded_at,
                    json_extract(je.value, '$.note'),
                    COALESCE(json_extract(a.result_json, '$.guidance.lessons'), '[]'),
                    COALESCE(json_extract(a.result_json, '$.guidance.follow_ups_refined'), '[]'),
                    gf.feedback
             FROM event_analyses a
             JOIN json_each(a.result_json, '$.guidance.related') je ON json_extract(je.value, '$.slug') = ?1
             JOIN events e ON e.id = a.event_id
             LEFT JOIN guidance_feedback gf ON gf.analysis_id = a.id
                 AND gf.slug = ?1
                 AND gf.note = COALESCE(json_extract(je.value, '$.note'), '')
             ORDER BY e.recorded_at DESC, a.created_at DESC
             LIMIT 50",
        )?;
        let mapped = match statement.query_map(params![slug], |row| {
            Ok(PageGuidanceItem {
                analysis_id: row.get(0)?,
                event_id: row.get(1)?,
                recorded_at: row.get(2)?,
                note: row.get(3)?,
                lessons: parse_json_string_array(row.get::<_, String>(4)?.as_str()),
                follow_ups_refined: parse_json_string_array(row.get::<_, String>(5)?.as_str()),
                feedback: row.get(6)?,
            })
        }) {
            Ok(mapped) => mapped,
            // prepare/绑定层面就失败（如 JSON1 不可用）：降级逐行解析。
            Err(_) => return self.page_guidance_items_fallback(slug),
        };
        // json_each 遇坏 JSON 行时在迭代中报错——同样的降级。
        match mapped.collect::<rusqlite::Result<Vec<_>>>() {
            Ok(list) => Ok(list),
            Err(_) => self.page_guidance_items_fallback(slug),
        }
    }

    fn page_guidance_items_fallback(&self, slug: &str) -> Result<Vec<PageGuidanceItem>> {
        let mut statement = self.connection.prepare(
            "SELECT a.id, a.event_id, e.recorded_at, a.result_json
             FROM event_analyses a JOIN events e ON e.id = a.event_id
             ORDER BY e.recorded_at DESC, a.created_at DESC LIMIT 200",
        )?;
        let mut items = Vec::new();
        let rows = statement
            .query_map(params![], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (analysis_id, event_id, recorded_at, raw) in rows {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
                continue;
            };
            let Some(guidance) = value.get("guidance") else {
                continue;
            };
            let Some(related) = guidance.get("related").and_then(|v| v.as_array()) else {
                continue;
            };
            for r in related {
                if r.get("slug").and_then(|v| v.as_str()) == Some(slug) {
                    items.push(PageGuidanceItem {
                        analysis_id: analysis_id.clone(),
                        event_id: event_id.clone(),
                        recorded_at: recorded_at.clone(),
                        note: r.get("note").and_then(|v| v.as_str()).map(str::to_owned),
                        lessons: guidance
                            .get("lessons")
                            .and_then(|v| v.as_array())
                            .map(|a| {
                                a.iter()
                                    .filter_map(|l| l.as_str().map(str::to_owned))
                                    .collect()
                            })
                            .unwrap_or_default(),
                        follow_ups_refined: guidance
                            .get("follow_ups_refined")
                            .and_then(|v| v.as_array())
                            .map(|a| {
                                a.iter()
                                    .filter_map(|l| l.as_str().map(str::to_owned))
                                    .collect()
                            })
                            .unwrap_or_default(),
                        feedback: None,
                    });
                    break;
                }
            }
        }
        // 降级路径逐行解析得知 note 后，批量补 feedback（一次查询）。
        if !items.is_empty() {
            let mut statement = self.connection.prepare(
                "SELECT analysis_id, note, feedback FROM guidance_feedback WHERE slug=?1",
            )?;
            let feedbacks: std::collections::HashMap<(String, String), String> = statement
                .query_map(params![slug], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<(String, String, String)>>>()?
                .into_iter()
                .map(|(a, n, f)| ((a, n), f))
                .collect();
            for item in items.iter_mut() {
                let key = (
                    item.analysis_id.clone(),
                    item.note.clone().unwrap_or_default(),
                );
                if let Some(feedback) = feedbacks.get(&key) {
                    item.feedback = Some(feedback.clone());
                }
            }
        }
        Ok(items)
    }

    /// 设置单条 guidance 条目的采纳反馈（遗留收口切片 B，只记录不推断）。
    /// 定位键（analysis_id, slug, note）与读面 JOIN 键一致；None = 清除
    /// （toggle off，删行）。analysis 不存在 → false（与 rule_hit 同语义）。
    pub fn set_guidance_feedback(
        &self,
        analysis_id: &str,
        slug: &str,
        note: Option<&str>,
        feedback: Option<&str>,
    ) -> Result<bool> {
        if let Some(feedback) = feedback {
            anyhow::ensure!(
                matches!(feedback, "helpful" | "not_relevant"),
                "feedback 只能是 helpful 或 not_relevant"
            );
        }
        let exists: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM event_analyses WHERE id=?1)",
            [analysis_id],
            |r| r.get(0),
        )?;
        if !exists {
            return Ok(false);
        }
        let note = note.unwrap_or_default();
        let changed =
            match feedback {
                Some(value) => {
                    self.connection.execute(
                        "INSERT INTO guidance_feedback(analysis_id,slug,note,feedback,created_at) \
                     VALUES(?1,?2,?3,?4,strftime('%Y-%m-%dT%H:%M:%fZ','now')) \
                     ON CONFLICT(analysis_id,slug,note) DO UPDATE SET feedback=excluded.feedback",
                        params![analysis_id, slug, note, value],
                    )? > 0
                }
                None => self.connection.execute(
                    "DELETE FROM guidance_feedback WHERE analysis_id=?1 AND slug=?2 AND note=?3",
                    params![analysis_id, slug, note],
                )? > 0,
            };
        Ok(changed)
    }

    pub fn recent_events(&self, days: i64, limit: usize) -> Result<Vec<EventSummary>> {
        let since = (chrono::Utc::now() - chrono::Duration::days(days)).to_rfc3339();
        let mut statement = self.connection.prepare(
            "SELECT id, recorded_at, raw_text, source, status FROM events
         WHERE recorded_at >= ?1 ORDER BY recorded_at DESC, id DESC LIMIT ?2",
        )?;
        let rows = statement.query_map(params![since, limit as i64], |row| {
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

    /// 保存一条 AI 生成的认知洞察（derived data，不影响原始事件）
    pub fn insert_insight(
        &self,
        window_days: i64,
        prompt_version: &str,
        lens: &str,
        title: &str,
        observation: &str,
        related_events: &[String],
        action: Option<&str>,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let related_raw = serde_json::to_string(related_events)?;
        self.connection.execute(
        "INSERT INTO insights
         (id, created_at, window_days, prompt_version, lens, title, observation, related_raw, action, status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'new')",
        params![
            id,
            now,
            window_days,
            prompt_version,
            lens,
            title,
            observation,
            related_raw,
            action,
        ],
    )?;
        Ok(id)
    }

    /// 列出全部已存洞察（新的在前）
    pub fn list_insights(&self) -> Result<Vec<InsightSummary>> {
        let mut statement = self.connection.prepare(
            "SELECT id, created_at, window_days, prompt_version, lens, title, observation,
                related_raw, action, status
         FROM insights ORDER BY created_at DESC",
        )?;
        let rows = statement.query_map([], |row| {
            let related_raw: String = row.get(7)?;
            let related_events: Vec<String> =
                serde_json::from_str(&related_raw).unwrap_or_default();
            Ok(InsightSummary {
                id: row.get(0)?,
                created_at: row.get(1)?,
                window_days: row.get(2)?,
                prompt_version: row.get(3)?,
                lens: row.get(4)?,
                title: row.get(5)?,
                observation: row.get(6)?,
                related_events,
                action: row.get(8)?,
                status: row.get(9)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }
}

/// 本地日历日 → [当日 00:00, 次日 00:00) 的 UTC rfc3339 边界。
/// 与 daily_entries 原内联 edge 逻辑一致（DST 歧义回退最早时刻）。
pub(crate) fn local_day_bounds(date: chrono::NaiveDate) -> (String, String) {
    use chrono::{Local, TimeZone};
    let edge = |day: chrono::NaiveDate| {
        let local_midnight = day.and_hms_opt(0, 0, 0).expect("midnight is valid");
        Local
            .from_local_datetime(&local_midnight)
            .single()
            .or_else(|| Local.from_local_datetime(&local_midnight).earliest())
            .map(|value| value.with_timezone(&chrono::Utc).to_rfc3339())
            .unwrap_or_else(|| local_midnight.and_utc().to_rfc3339())
    };
    (edge(date), edge(date + chrono::Duration::days(1)))
}
