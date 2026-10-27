//! 单例行的**单一读取接缝**（roadmap `2026-10-27-sync-lifecycle-boundaries` C1）。
//!
//! `sync_replica` 与 `sync_local_vault` 都是 `singleton=1` 的单行表。它们的「缺行」
//! 有一个确定含义：**本机还没准备同步身份**（终态），不是空值、也不是临时故障。
//! 裸 `query_row` 会把这个终态渲染成 rusqlite 的 `Query returned no rows`，调用方
//! 因此无法区分「未配置，别重试」和「抢锁失败，可重试」—— 后台 AI worker 曾据此每
//! tick 抛一次 `AnyhowException(Query returned no rows)` 并无限重试。
//!
//! 规则：**凡读这两张表的单例行，一律经这里**。缺行一律 `sync_not_configured`。
//! 同表里原本就有两种写法并存（`vault.rs` 用 `.optional()? + context`，`provider_keys.rs`
//! 裸读），说明这不是设计选择而是漏改；本文件把它收成一处。

use super::Store;
use anyhow::{Result, anyhow};
use rusqlite::types::FromSql;

/// 单例行缺失时的领域错误码。调用方（含 Dart 侧 worker）按它判定「终态、不要重试」。
pub(crate) const SYNC_NOT_CONFIGURED: &str = "sync_not_configured";

impl Store {
    /// 读单例行的一个标量列。缺行 => `sync_not_configured`。
    ///
    /// 取 `sql` 而不是内置常量，是为了让同一张表的单例行（各列被不同模块按需读）
    /// 共用一个语义出口；这是本仓库里 `sync_replica` 唯一的单例读取路径。
    fn singleton_value<T: FromSql>(&self, sql: &str) -> Result<T> {
        use rusqlite::OptionalExtension;
        self.connection
            .query_row(sql, [], |r| r.get(0))
            .optional()?
            .ok_or_else(|| anyhow!(SYNC_NOT_CONFIGURED))
    }

    /// `sync_replica.next_sequence`：下一个待分配的本地序号。
    pub(crate) fn replica_next_sequence(&self) -> Result<i64> {
        self.singleton_value("SELECT next_sequence FROM sync_replica WHERE singleton=1")
    }

    /// `sync_replica.logical_clock`：本机逻辑时钟。
    pub(crate) fn replica_logical_clock(&self) -> Result<i64> {
        self.singleton_value("SELECT logical_clock FROM sync_replica WHERE singleton=1")
    }

    /// `sync_local_vault` 的 `(installation_id, revision)`：解锁与 AAD 绑定的依据。
    pub(crate) fn local_vault_basis(&self) -> Result<(String, i64)> {
        use rusqlite::OptionalExtension;
        self.connection
            .query_row(
                "SELECT installation_id,revision FROM sync_local_vault WHERE singleton=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| anyhow!(SYNC_NOT_CONFIGURED))
    }

    /// `sync_local_vault.installation_id`。
    pub(crate) fn local_vault_installation(&self) -> Result<String> {
        self.singleton_value("SELECT installation_id FROM sync_local_vault WHERE singleton=1")
    }

    /// `sync_local_vault.revision`。
    pub(crate) fn local_vault_revision(&self) -> Result<i64> {
        self.singleton_value("SELECT revision FROM sync_local_vault WHERE singleton=1")
    }
}
