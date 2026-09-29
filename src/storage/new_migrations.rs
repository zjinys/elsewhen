//! 数据库 schema 的引导与版本化迁移。
//!
//! `schema.sql` 是**冻结的 v1 基线**，只作为版本 1 应用一次，之后不得再改：
//! `to_latest` 对任何 `user_version >= 1` 的库都会跳过它，所以改它对已有库
//! 完全无效——新库会带上改动，老库静默缺表缺列且不报任何错。
//! 后续一切 schema 变更都在 [`MIGRATION_LIST`] 末尾追加新版本。

use anyhow::Result;
use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};

/// v2：目标表与活跃目标上限（FR-PES-005-01）。
///
/// `phase` 是标签不是槽位，故刻意不加 `UNIQUE(phase)`：同一阶段可以有多条活跃
/// 目标，只要活跃总数不超过 3，且不要求三个阶段都有。
const GOALS_V2: &str = r#"
CREATE TABLE goals (
  id TEXT PRIMARY KEY,
  content TEXT NOT NULL CHECK(length(trim(content)) > 0),
  phase TEXT NOT NULL CHECK(phase IN ('near','mid','long')),
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','superseded')),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  superseded_at TEXT
);
CREATE INDEX idx_goals_status ON goals(status);

-- 活跃集上限 3 条，在数据库层强制而非 Rust 侧数一遍：应用层的 count-then-insert
-- 是 check-then-insert 竞态，本仓库已为此修过两处。触发器写法沿用 schema.sql 里
-- 既有的 prevent_raw_event_mutation（RAISE(ABORT, ...)）。
--
-- 需要两个触发器：新增走 INSERT，但复活一条已归档目标是对既有行改 status，只挡
-- INSERT 会漏掉这条路径。UPDATE 那个的 WHEN 必须排除「状态没变」的情形
-- （OLD.status='active'，即编辑一条活跃目标的正文），否则正常编辑会被误计入上限。
CREATE TRIGGER goals_cap_active_insert
BEFORE INSERT ON goals WHEN NEW.status = 'active'
BEGIN
  SELECT CASE WHEN (SELECT COUNT(*) FROM goals WHERE status='active') >= 3
    THEN RAISE(ABORT, 'active_goal_limit_reached') END;
END;

CREATE TRIGGER goals_cap_active_update
BEFORE UPDATE OF status ON goals
WHEN NEW.status='active' AND OLD.status <> 'active'
BEGIN
  SELECT CASE WHEN (SELECT COUNT(*) FROM goals WHERE status='active') >= 3
    THEN RAISE(ABORT, 'active_goal_limit_reached') END;
END;
"#;

/// 触发器拒绝写入时使用的标识，存储层据此把错误翻成人话。
pub(crate) const ACTIVE_GOAL_LIMIT_REACHED: &str = "active_goal_limit_reached";

static MIGRATION_LIST: &[M<'static>] = &[
    M::up(include_str!("../../schema.sql")).comment("v1: 当前 schema 基线，已冻结，不得修改"),
    M::up(GOALS_V2).comment("v2: goals 表与活跃上限触发器"),
];

pub(crate) fn initialize(connection: &mut Connection) -> Result<()> {
    Migrations::from_slice(MIGRATION_LIST).to_latest(connection)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory_db() -> Connection {
        let mut connection = Connection::open_in_memory().unwrap();
        initialize(&mut connection).unwrap();
        connection
    }

    fn insert(connection: &Connection, id: &str, phase: &str, status: &str) {
        connection
            .execute(
                "INSERT INTO goals(id, content, phase, status, created_at, updated_at, superseded_at)
                 VALUES (?1, 'x', ?2, ?3, 't', 't', ?4)",
                rusqlite::params![id, phase, status, if status == "active" { None } else { Some("t0") }],
            )
            .unwrap();
    }

    #[test]
    fn migration_applies_goals_table() {
        let connection = memory_db();
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 2);
    }

    /// 触发器 SQL 里的上限是硬编码字面量（SQL 无法引用 Rust 常量），而报错文案
    /// 用的是 [`crate::storage::MAX_ACTIVE_GOALS`]。两处若漂移，用户会看到
    /// 「已达 4 条上限」却连第 4 条都加不进去。这条测试把漂移变成编译期就红。
    #[test]
    fn trigger_cap_literal_matches_the_error_message_constant() {
        let needle = format!(">= {}", crate::storage::MAX_ACTIVE_GOALS);
        assert_eq!(
            GOALS_V2.matches(&needle).count(),
            2,
            "两个触发器都应按 MAX_ACTIVE_GOALS 封顶；若你改了上限，\
             改 `goals.rs` 常量后忘了同步触发器字面量，这里会失败"
        );
    }

    #[test]
    fn cap_rejects_fourth_active_goal() {
        let connection = memory_db();
        for (id, phase) in [("a", "near"), ("b", "near"), ("c", "long")] {
            insert(&connection, id, phase, "active");
        }
        let err = connection
            .execute(
                "INSERT INTO goals(id, content, phase, status, created_at, updated_at)
                 VALUES ('d', 'x', 'mid', 'active', 't', 't')",
                [],
            )
            .unwrap_err();
        assert!(
            err.to_string().contains(ACTIVE_GOAL_LIMIT_REACHED),
            "应因上限被拒，实际: {err}"
        );
    }

    /// 复活已归档目标走 UPDATE 路径，只挡 INSERT 的实现会漏掉这里。
    #[test]
    fn cap_also_guards_the_reactivate_update_path() {
        let connection = memory_db();
        for (id, phase) in [("a", "near"), ("b", "near"), ("c", "long")] {
            insert(&connection, id, phase, "active");
        }
        insert(&connection, "d", "mid", "superseded");

        let err = connection
            .execute("UPDATE goals SET status='active' WHERE id='d'", [])
            .unwrap_err();
        assert!(
            err.to_string().contains(ACTIVE_GOAL_LIMIT_REACHED),
            "复活路径也应受上限约束，实际: {err}"
        );
        assert_eq!(active_count(&connection), 3);
    }

    /// 上限满时编辑活跃目标正文不得被误判为第四条。
    #[test]
    fn editing_a_live_goal_does_not_trip_the_cap() {
        let connection = memory_db();
        for (id, phase) in [("a", "near"), ("b", "near"), ("c", "long")] {
            insert(&connection, id, phase, "active");
        }
        connection
            .execute("UPDATE goals SET content='改了' WHERE id='a'", [])
            .unwrap();
        let content: String = connection
            .query_row("SELECT content FROM goals WHERE id='a'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(content, "改了");
    }

    #[test]
    fn archiving_frees_a_slot_but_keeps_history() {
        let connection = memory_db();
        for (id, phase) in [("a", "near"), ("b", "near"), ("c", "long")] {
            insert(&connection, id, phase, "active");
        }
        connection
            .execute(
                "UPDATE goals SET status='superseded', superseded_at='t1' WHERE id='a'",
                [],
            )
            .unwrap();
        insert(&connection, "d", "mid", "active");
        assert_eq!(active_count(&connection), 3);
        assert_eq!(superseded_count(&connection), 1);
    }

    /// phase 是标签：同一阶段可以有多条活跃目标。
    #[test]
    fn phase_is_a_label_not_a_slot() {
        let connection = memory_db();
        insert(&connection, "a", "near", "active");
        insert(&connection, "b", "near", "active");
        let n: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM goals WHERE status='active' AND phase='near'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(n, 2);
    }

    fn active_count(connection: &Connection) -> i64 {
        connection
            .query_row(
                "SELECT COUNT(*) FROM goals WHERE status='active'",
                [],
                |row| row.get(0),
            )
            .unwrap()
    }

    fn superseded_count(connection: &Connection) -> i64 {
        connection
            .query_row(
                "SELECT COUNT(*) FROM goals WHERE status='superseded'",
                [],
                |row| row.get(0),
            )
            .unwrap()
    }
}
