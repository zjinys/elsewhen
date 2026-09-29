//! 数据库 schema 的引导与版本化迁移。
//!
//! 迁移正文按 `rusqlite_migration` 官方约定放在仓库根的 [`migrations/`](../../migrations)：
//! 每个版本一个子目录 `{序号}-{名字}/up.sql`。序号是**从 1 开始的连续整数**
//! （crate 内部用 `id - 1` 去索引数组，跳号或重号会直接报错），执行顺序由序号
//! 决定而非文件名排序，所以不要求补零对齐。需要回滚时同目录下再加 `down.sql`，
//! 本仓库不需要。
//!
//! 整个目录用 `include_dir!` 在编译期嵌进二进制，再交给
//! `Migrations::from_directory` 扫描——运行时不依赖磁盘上的 `migrations/`，
//! 六种安装包不用多带一份资源文件。迁移正文也不放 Rust 源码里：`goals` 那条
//! 一度写成内联 `const GOALS_V2: &str`，于是 SQL、解释它为什么这么写的注释、
//! 以及触发器里那个上限字面量被拆在三个地方，改一处要同时看两个文件。
//!
//! **`01-baseline/up.sql` 是冻结的基线**，只作为版本 1 应用一次，之后不得再改：
//! `to_latest` 对任何 `user_version >= 1` 的库都会跳过它，所以改它对已有库
//! 完全无效——新库会带上改动，老库静默缺表缺列且不报任何错。
//! 后续一切 schema 变更都是新建 `migrations/{下一个序号}-{名字}/up.sql`。
//!
//! 基线里有一行 `CREATE TABLE schema_migrations`，那是旧迁移链的版本记录，现在
//! 已无人读写，由 `03-drop-legacy-migrations` 删掉。留着不动是因为基线冻结；
//! 这也正是「只追加、不改历史」的用处——新库走完 v1 建、v3 删，与老库终态一致。
//!
//! 版本的唯一真源是 `PRAGMA user_version`，也就是文件头第 60 字节起的 4 个字节。
//! 库里没有第二张表在记版本。

use anyhow::Result;
use include_dir::{include_dir, Dir};
use rusqlite::Connection;
use rusqlite_migration::Migrations;

/// `migrations/` 整个目录，编译期嵌入二进制。
///
/// 用 `$CARGO_MANIFEST_DIR` 而非 `../../`：`include_dir!` 里相对路径的基准是
/// **调用处所在文件**，而 `env!` 的基准是 crate 根，不会因源文件挪位置而失效。
static MIGRATION_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/migrations");

/// 触发器拒绝写入时使用的标识，存储层据此把错误翻成人话。
pub(crate) const ACTIVE_GOAL_LIMIT_REACHED: &str = "active_goal_limit_reached";

/// 按官方约定从 `migrations/` 组装迁移集。
///
/// 返回 `Result` 而不是常量：`from_directory` 会校验目录结构（缺 `up.sql`、
/// 序号跳号或重号都在这里报错）。这是 crate 给的结构性防线，绕开它自己维护
/// 一张登记表，就等于把「新增文件忘了登记 → 版本静默不执行」请回来。
fn migrations() -> Result<Migrations<'static>> {
    Ok(Migrations::from_directory(&MIGRATION_DIR)?)
}

pub(crate) fn initialize(connection: &mut Connection) -> Result<()> {
    migrations()?.to_latest(connection)?;
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

    /// 官方内置自检：把全部 up 迁移在临时内存库上从头跑到尾。
    ///
    /// 这是 crate 推荐的迁移测试入口，覆盖「SQL 本身跑不跑得通」；下面那些测试
    /// 覆盖的是目标表的语义，两者互补。
    #[test]
    fn migrations_validate() {
        migrations().unwrap().validate().unwrap();
    }

    /// 目录发现的结构性保证：五个迁移、按序号取名。
    ///
    /// 「新增文件却忘了登记」这类错误在这里不可能发生——`from_directory` 直接
    /// 扫目录，不经过任何手工登记表。这条测试盯的是别一种漂移：有人把子目录
    /// 改名成不满足 `{序号}-{名字}` 的形式，或把序号写成不连续。
    #[test]
    fn migrations_are_discovered_from_the_directory() {
        let mut names: Vec<String> = MIGRATION_DIR
            .dirs()
            .map(|d| d.path().file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                "01-baseline",
                "02-goals",
                "03-drop-legacy-migrations",
                "04-knowledge",
                "05-source-compilation",
            ],
            "迁移子目录应形如 {{序号}}-{{名字}}，且序号从 1 起连续"
        );
    }

    /// 冻结约定与内容归属：基线里不该出现 `goals`。
    ///
    /// `goals` 若混进冻结的基线，新库正常而老库（`user_version >= 1`，基线被
    /// 跳过）静默缺表——这是本仓库最阴的失败模式，没有报错。
    #[test]
    fn baseline_stays_frozen_and_goals_live_in_the_second_migration() {
        let baseline = include_str!("../../migrations/01-baseline/up.sql");
        assert!(
            baseline.contains("CREATE TABLE events"),
            "01-baseline 应是当前 schema 基线"
        );
        assert!(
            !baseline.contains("CREATE TABLE goals"),
            "goals 属于 02-goals，混进冻结的基线会让老库静默少一张表"
        );
    }

    /// 全部迁移跑完后，版本号就是 `PRAGMA user_version`。
    ///
    /// 这条断言同时是「版本真源只有文件头」的证据：库里没有第二张表在记版本。
    #[test]
    fn migration_applies_goals_table() {
        let connection = memory_db();
        let version: i64 = connection
            .query_row("PRAGMA main.user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 5, "五条迁移跑完，user_version 应为 5");
    }

    #[test]
    fn existing_v4_proposals_keep_manual_ownership_after_upgrade() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrations()
            .unwrap()
            .to_version(&mut connection, 4)
            .unwrap();
        connection.execute("INSERT INTO knowledge_proposals(id,dedupe_key,target_slug,kind,title,content_md,reason,created_at)
            VALUES ('old','old','method/old','method','旧建议','旧正文','manual','2026-09-29')", []).unwrap();
        initialize(&mut connection).unwrap();
        let origin: String = connection
            .query_row(
                "SELECT origin FROM knowledge_proposals WHERE id='old'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(origin, "manual");
    }

    /// 旧迁移链留下的 `schema_migrations` 表必须已被 v3 删掉。
    ///
    /// 它声称「1–31」而迁移已重新编号，留着会误导所有后来打开库的人；基线里
    /// 仍建着它是因为基线冻结、不得改，所以只能在 v3 里删——这也正是「只追加、
    /// 不改历史」的意义：新库走完 v1 建、v3 删，与老库终态一致。
    #[test]
    fn legacy_version_table_is_gone() {
        let connection = memory_db();
        let count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type='table' AND name='schema_migrations'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 0,
            "版本状态只该记在 PRAGMA user_version 上，不该再有 schema_migrations 表"
        );
    }

    /// 触发器 SQL 里的上限是硬编码字面量（SQL 无法引用 Rust 常量），而报错文案
    /// 用的是 [`crate::storage::MAX_ACTIVE_GOALS`]。两处若漂移，用户会看到
    /// 「已达 4 条上限」却连第 4 条都加不进去。这条测试把漂移变成编译期就红。
    #[test]
    fn trigger_cap_literal_matches_the_error_message_constant() {
        let goals_sql = include_str!("../../migrations/02-goals/up.sql");
        let needle = format!(">= {}", crate::storage::MAX_ACTIVE_GOALS);
        assert_eq!(
            goals_sql.matches(&needle).count(),
            2,
            "两个触发器都应按 MAX_ACTIVE_GOALS 封顶；若你改了上限，\
             改 `goals.rs` 常量后忘了同步 \
             `migrations/02-goals/up.sql` 里的触发器字面量，这里会失败"
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
