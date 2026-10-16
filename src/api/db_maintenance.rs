//! 数据库维护：体量统计与 VACUUM 整理（数据主权的一部分）。
//!
//! 背景：`sync_wire_outbox` 历史上曾大规模堆积后清空，DELETE 释放的页仍挂在
//! freelist 上，库文件只增不缩（曾实测 588MB 里 449MB 是空闲页）。VACUUM 需要独占写，
//! 建议在无对话进行时触发；后台任务并发写会导致 busy，由前端提示重试。

use super::*;

pub struct DatabaseStatsDto {
    pub file_bytes: i64,
    pub allocated_bytes: i64,
    pub free_bytes: i64,
    pub page_size: i64,
}

pub struct VacuumResultDto {
    pub before_bytes: i64,
    pub after_bytes: i64,
}

fn database_stats_value(store: &Store) -> Result<(i64, i64, i64)> {
    let page_size: i64 = store
        .connection
        .query_row("PRAGMA page_size", [], |r| r.get(0))
        .context("读 page_size")?;
    let page_count: i64 = store
        .connection
        .query_row("PRAGMA page_count", [], |r| r.get(0))
        .context("读 page_count")?;
    let freelist_count: i64 = store
        .connection
        .query_row("PRAGMA freelist_count", [], |r| r.get(0))
        .context("读 freelist_count")?;
    Ok((page_size, page_count, freelist_count))
}

pub fn database_stats() -> Result<DatabaseStatsDto> {
    let store = Store::open(&crate::config::AppConfig::load()?.database_path)?;
    let (page_size, page_count, freelist_count) = database_stats_value(&store)?;
    Ok(DatabaseStatsDto {
        file_bytes: page_count * page_size,
        allocated_bytes: (page_count - freelist_count) * page_size,
        free_bytes: freelist_count * page_size,
        page_size,
    })
}

pub fn vacuum_database() -> Result<VacuumResultDto> {
    let store = Store::open(&crate::config::AppConfig::load()?.database_path)?;
    let (page_size, page_count, _) = database_stats_value(&store)?;
    let before_bytes = page_count * page_size;
    // VACUUM 不能在事务里跑；并发写 busy 时 busy_timeout 兜底 5s，仍失败由前端提示。
    store
        .connection
        .execute_batch("VACUUM;")
        .context("VACUUM 被并发写入阻塞（请稍后重试）")?;
    store
        .connection
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .ok();
    let (page_size, page_count, _) = database_stats_value(&store)?;
    Ok(VacuumResultDto {
        before_bytes,
        after_bytes: page_count * page_size,
    })
}
