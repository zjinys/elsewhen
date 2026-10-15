//! 回顾数据面（R1）：周/月拼装统计的窗口查询。

//! R3 追加 `deviation_reports` 的存取（管道产物，不进快照）。
//!
//! 日界口径与 `events::local_day_bounds` 一致。

use super::Store;
use crate::event::EventSummary;
use anyhow::Result;
use rusqlite::params;

impl Store {
    /// 窗口内（含两端）事件全集，按记录时间倒序。
    /// 与 `daily_entries` 同款本地日界；recordability 由调用方判定
    /// （manual 决策覆盖与 V1 解析都在 api 层，分层职责不挪）。
    pub fn events_between(
        &self,
        from: chrono::NaiveDate,
        to_inclusive: chrono::NaiveDate,
    ) -> Result<Vec<EventSummary>> {
        anyhow::ensure!(
            from <= to_inclusive,
            "period bounds reversed: {from} > {to_inclusive}"
        );
        let (start, _) = super::events::local_day_bounds(from);
        let (_, end) = super::events::local_day_bounds(to_inclusive);
        let mut statement = self.connection.prepare(
            "SELECT id, recorded_at, raw_text, source, status FROM events
             WHERE recorded_at >= ?1 AND recorded_at < ?2
             ORDER BY recorded_at DESC, id DESC",
        )?;
        let rows = statement.query_map(params![start, end], |row| {
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

    /// 窗口内完成的待办数。「完成日」以 `updated_at` 为准（todos 无独立
    /// completed_at 列；先 done 后编辑会刷新 updated_at，第一刀如实接受
    /// 该近似——不做推断性修正），本地日界与事件窗口同款。
    pub fn todos_done_between(
        &self,
        from: chrono::NaiveDate,
        to_inclusive: chrono::NaiveDate,
    ) -> Result<i64> {
        anyhow::ensure!(
            from <= to_inclusive,
            "period bounds reversed: {from} > {to_inclusive}"
        );
        let (start, _) = super::events::local_day_bounds(from);
        let (_, end) = super::events::local_day_bounds(to_inclusive);
        Ok(self.connection.query_row(
            "SELECT COUNT(*) FROM todos
             WHERE status='done' AND updated_at >= ?1 AND updated_at < ?2",
            params![start, end],
            |row| row.get(0),
        )?)
    }

    // ── deviation_reports（R3 管道产物）────────────────────────────

    /// 幂等写入周期报告：同 (period, period_type) 已存在时不覆盖
    /// （表层 UNIQUE 兜底），返回是否新建。
    pub fn insert_deviation_report(
        &self,
        id: &str,
        period: chrono::NaiveDate,
        period_type: &str,
        report_json: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<bool> {
        anyhow::ensure!(
            period_type == "week" || period_type == "month",
            "未知 period_type：{period_type}"
        );
        let inserted = self.connection.execute(
            "INSERT OR IGNORE INTO deviation_reports
                 (id, period, period_type, generated_at, report_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?4)",
            params![
                id,
                period.format("%F").to_string(),
                period_type,
                now.to_rfc3339(),
                report_json,
            ],
        )?;
        Ok(inserted == 1)
    }

    /// 待检测的报告（最旧优先）：deviations_json 为 NULL = 尚未检测，
    /// 是合法状态；检测成功前会一直出现在这个列表里。
    pub fn pending_deviation_report_ids(&self) -> Result<Vec<(String, String)>> {
        let mut statement = self.connection.prepare(
            "SELECT id, period_type FROM deviation_reports
             WHERE deviations_json IS NULL
             ORDER BY created_at ASC, id ASC",
        )?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// 读取报告 JSON（含 goal_content 去重所需的历史偏差）。
    pub fn deviation_report_row(
        &self,
        id: &str,
    ) -> Result<Option<(String, String, Option<String>)>> {
        let mut statement = self.connection.prepare(
            "SELECT period_type, report_json, deviations_json
             FROM deviation_reports WHERE id = ?1",
        )?;
        let mut rows = statement.query_map(params![id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    /// 检测结果入档（仅当仍为 NULL：不覆盖已有检测结论）。
    pub fn record_detected_deviations(
        &self,
        id: &str,
        deviations_json: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        let changed = self.connection.execute(
            "UPDATE deviation_reports
             SET deviations_json = ?2, detected_at = ?3
             WHERE id = ?1 AND deviations_json IS NULL",
            params![id, deviations_json, now.to_rfc3339()],
        )?;
        anyhow::ensure!(changed == 1, "报告不存在或已有检测结论: {id}");
        Ok(())
    }

    /// 历史（已检测过）偏差的类型集合，供确定性去重：
    /// 同 (goal_content, deviation_type) 不重报——情况未变时不重复输出，
    /// 不用 AI 判断重复（不变式 3）。
    pub fn reported_deviation_keys(&self, period_type: &str) -> Result<Vec<(String, String)>> {
        let mut statement = self.connection.prepare(
            "SELECT deviations_json FROM deviation_reports
             WHERE period_type = ?1 AND deviations_json IS NOT NULL
             ORDER BY created_at DESC, id DESC",
        )?;
        let rows = statement.query_map(params![period_type], |row| row.get::<_, String>(0))?;
        let mut keys = Vec::new();
        for row in rows {
            let raw = row?;
            if let Ok(list) = serde_json::from_str::<Vec<serde_json::Value>>(&raw) {
                for item in list {
                    if let (Some(goal), Some(kind)) = (
                        item.get("goal_content").and_then(|v| v.as_str()),
                        item.get("deviation_type").and_then(|v| v.as_str()),
                    ) {
                        keys.push((goal.trim().to_string(), kind.trim().to_string()));
                    }
                }
            }
        }
        Ok(keys)
    }
}

#[cfg(test)]
mod review_tests {
    use super::*;
    use crate::storage::Store;

    fn store() -> Store {
        let dir = std::env::temp_dir().join(format!(
            "elsewhen-review-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Store::open(&dir.join("test.db")).unwrap()
    }

    fn insert(store: &Store, id: &str, recorded_at: &str) {
        store
            .connection
            .execute(
                "INSERT INTO events (id, occurred_at, recorded_at, raw_text, source, status, created_at, updated_at)
                 VALUES (?1, ?2, ?2, 'x', 'cli', 'ready', ?2, ?2)",
                params![id, recorded_at],
            )
            .unwrap();
    }

    #[test]
    fn window_bounds_follow_local_day_edges_inclusive() {
        let store = store();
        let from = chrono::NaiveDate::from_ymd_opt(2026, 10, 12).unwrap();
        let to = chrono::NaiveDate::from_ymd_opt(2026, 10, 14).unwrap();
        // 边界时刻从 local_day_bounds 派生，不假定机器时区（库内 recorded_at
        // 统一 UTC rfc3339）。首日边界后 1 分钟与末日边界前 1 分钟都归属
        // 窗口；末日之后 1 分钟不进（本地日界，非 UTC 日界）。
        let rfc = |dt: chrono::DateTime<chrono::Utc>| dt.to_rfc3339();
        let parse = |s: &str| {
            chrono::DateTime::parse_from_rfc3339(s)
                .unwrap()
                .with_timezone(&chrono::Utc)
        };
        let (start, _) = super::super::events::local_day_bounds(from);
        // 窗口上界 = 末日次日的 00:00（events_between 左闭右开）。
        let (window_end, _) =
            super::super::events::local_day_bounds(to + chrono::Duration::days(1));
        insert(
            &store,
            "a",
            &rfc(parse(&start) + chrono::Duration::minutes(1)),
        );
        // 末日 23:59 本地（= 上界前 1 分钟）。
        insert(
            &store,
            "b",
            &rfc(parse(&window_end) - chrono::Duration::minutes(1)),
        );
        // 次日 00:01 本地（= 上界后 1 分钟）——同点尾仍不进。
        insert(
            &store,
            "c",
            &rfc(parse(&window_end) + chrono::Duration::minutes(1)),
        );
        let ids: Vec<String> = store
            .events_between(from, to)
            .unwrap()
            .into_iter()
            .map(|e| e.id)
            .collect();
        assert_eq!(ids, vec!["b".to_string(), "a".to_string()]);
    }

    #[test]
    fn reversed_bounds_fail_closed() {
        let store = store();
        let a = chrono::NaiveDate::from_ymd_opt(2026, 10, 14).unwrap();
        assert!(
            store
                .events_between(a, a - chrono::Duration::days(1))
                .is_err()
        );
        assert!(
            store
                .todos_done_between(a, a - chrono::Duration::days(1))
                .is_err()
        );
    }

    #[test]
    fn todo_done_counts_refreshed_completions_only() {
        let store = store();
        let from = chrono::NaiveDate::from_ymd_opt(2026, 10, 12).unwrap();
        let to = chrono::NaiveDate::from_ymd_opt(2026, 10, 14).unwrap();
        let insert_todo = |id: &str, status: &str, updated: &str| {
            store
                .connection
                .execute(
                    "INSERT INTO todos (id, title, status, updated_at, created_at)
                     VALUES (?1, 't', ?2, ?3, ?3)",
                    params![id, status, updated],
                )
                .unwrap();
        };
        insert_todo("t1", "done", "2026-10-13T10:00:00+08:00");
        insert_todo("t2", "done", "2026-10-15T10:00:00+08:00");
        insert_todo("t3", "open", "2026-10-13T10:00:00+08:00");
        assert_eq!(store.todos_done_between(from, to).unwrap(), 1);
    }
}
