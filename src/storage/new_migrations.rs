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
use include_dir::{Dir, include_dir};
use rusqlite::Connection;
use rusqlite_migration::{Migrations, SchemaVersion};

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

/// 代码期望的最新 schema 版本（迁移目录的最大序号）。
/// 后台路径用它判断库是否待升级：版本落后就跳过本轮工作，
/// 绝不在后台补跑迁移——启动写入只属于前台初始化路径。
pub(crate) fn latest_version() -> Result<i64> {
    let mut connection = Connection::open_in_memory()?;
    debug_assert!(matches!(
        migrations()?.current_version(&connection)?,
        SchemaVersion::NoneSet
    ));
    initialize(&mut connection)?;
    let version: i64 = connection.query_row("PRAGMA main.user_version", [], |row| row.get(0))?;
    Ok(version)
}

#[cfg(test)]
pub(crate) fn initialize_test_version(connection: &mut Connection, version: usize) -> Result<()> {
    migrations()?.to_version(connection, version)?;
    Ok(())
}

#[cfg(test)]
#[cfg(test)]
#[path = "new_migrations_tests.rs"]
mod tests;
