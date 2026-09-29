//! 数据库 schema 的引导与版本化迁移。
//!
//! 每个版本一个 SQL 文件，放在仓库根的 [`migrations/`](../../migrations)：
//! `NNNN_描述.sql`，`NNNN` 即 [`MIGRATION_LIST`] 里的序号，加版本就是加文件。
//! 迁移正文不放 Rust 源码里——`goals` 那条一度写成内联 `const GOALS_V2: &str`，
//! 于是 SQL 正文、解释它为什么这么写的注释、以及触发器里那个上限字面量被拆在
//! 三个地方，改一处要同时看两个文件。
//!
//! **`0001_schema.sql` 是冻结的基线**，只作为版本 1 应用一次，之后不得再改：
//! `to_latest` 对任何 `user_version >= 1` 的库都会跳过它，所以改它对已有库
//! 完全无效——新库会带上改动，老库静默缺表缺列且不报任何错。
//! 后续一切 schema 变更都在 [`MIGRATION_LIST`] 末尾追加新文件。

use anyhow::Result;
use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};

/// 触发器拒绝写入时使用的标识，存储层据此把错误翻成人话。
pub(crate) const ACTIVE_GOAL_LIMIT_REACHED: &str = "active_goal_limit_reached";

/// v1：基线。**冻结，不得再改**——见模块文档。
const V1_BASELINE: &str = include_str!("../../migrations/0001_schema.sql");
/// v2：goals 表与活跃上限触发器。
const V2_GOALS: &str = include_str!("../../migrations/0002_goals.sql");

static MIGRATION_LIST: &[M<'static>] = &[
    M::up(V1_BASELINE).comment("v1: 基线，已冻结，不得修改"),
    M::up(V2_GOALS).comment("v2: goals 表与活跃上限触发器"),
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
            V2_GOALS.matches(&needle).count(),
            2,
            "两个触发器都应按 MAX_ACTIVE_GOALS 封顶；若你改了上限，\
             改 `goals.rs` 常量后忘了同步 \
             `migrations/0002_goals.sql` 里的触发器字面量，这里会失败"
        );
    }

    /// 迁移文件与 `MIGRATION_LIST` 必须一一对应、序号连续。
    ///
    /// 漏登记是最阴的那种错：`include_str!` 不会报错，多出来的那个文件只是永远
    /// 不被执行，`cargo test` 全绿、新库却少一张表。文件名序号跳号同理——改的人
    /// 会以为 `0003` 排在 `0002` 前面，而实际是字母序。
    #[test]
    fn every_migration_file_is_registered_in_order() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        let mut files: Vec<String> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("读不到 {}：{e}", dir.display()))
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".sql"))
            .collect();
        files.sort();

        assert_eq!(
            files.len(),
            MIGRATION_LIST.len(),
            "migrations/ 下有 {} 个 .sql（{files:?}），MIGRATION_LIST 只登记了 {} 条。\
             加了文件忘了登记，那个版本会静默不执行。",
            files.len(),
            MIGRATION_LIST.len()
        );
        for (index, file) in files.iter().enumerate() {
            assert!(
                file.starts_with(&format!("{:04}_", index + 1)),
                "第 {index} 条迁移的文件名应以 {:04}_ 开头，实际 {file}（按文件名排序即执行顺序）",
                index + 1
            );
        }
    }

    /// `MIGRATION_LIST` 里的两条常量确实来自那两个文件，而不是某人复制粘贴的副本。
    #[test]
    fn migration_consts_carry_their_files_content() {
        assert!(
            V1_BASELINE.contains("CREATE TABLE events"),
            "v1 应是基线（现 `migrations/0001_schema.sql` 的内容）"
        );
        assert!(
            V2_GOALS.contains("CREATE TABLE goals"),
            "v2 应是 goals 迁移"
        );
        assert!(
            !V1_BASELINE.contains("CREATE TABLE goals"),
            "goals 属于 v2，混进冻结的 v1 基线会让老库静默多出表"
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
