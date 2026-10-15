//! 回顾数据面（R1）：周/月拼装统计的窗口查询。
//!
//! 只读、无持久化（roadmap 裁决：第一刀实时聚合，`deviation_reports`
//! 推迟到偏差检测切片）。日界口径与 `events::local_day_bounds` 一致。

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
