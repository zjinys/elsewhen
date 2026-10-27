//! 「清除本机同步身份」出口（roadmap `2026-10-27-sync-lifecycle-boundaries` C3，缺陷 D1）。
//!
//! 背景：本机有 vault/replica 但 authority 空时，`invite.rs` 恒判 `requires_repair`，
//! 而此前全仓没有任何 API 能把它推回 `not_configured` —— 用户只能删整个库（连笔记一起没了）。
//! 这个形状不是运维臆想，三条正常路径就会产生：掉电/杀进程（`vault.rs:79`）、
//! 交接写到一半（`enrollment.rs:99`）、join 中断（`sync/enrollment.rs:116`）。
//!
//! 本模块只做一件事：清**同步身份**连同它派生的复制中间态，业务数据一格不碰。
//!
//! 两道闸门让它不会被当成通用「重置本机」：
//! 1. 必须在写事务内（避免半清）；
//! 2. **authority 必须为空** —— 有权威的本机是健康身份，要重来该走「切换数据集」，
//!    不该走这条故障出口。

use super::*;

/// 复制运行时拥有的全部表。
///
/// 显式列出而不是按 `sync_%` 前缀动态发现：将来若有人新增一张**不是**复制运行时的
/// `sync_` 前缀表（比如同步优先级之类的 UI 偏好），动态删除会静默吞掉它。
/// 代价是这张表需要跟着迁移走 —— 由
/// `storage::sync::tests::local_identity::every_replication_table_is_covered_by_the_reset_list`
/// 兜底：漏列会让用例变红，逼着做决定而不是靠记性。
pub(crate) const REPLICATION_TABLES: [&str; 38] = [
    "sync_authority",
    "sync_authority_versions",
    "sync_checkpoint_operations",
    "sync_checkpoint_providers",
    "sync_conflicts",
    "sync_frontiers",
    "sync_gc_state",
    "sync_known_revisions",
    "sync_local_vault",
    "sync_member_keys",
    "sync_member_state",
    "sync_members",
    "sync_message_order",
    "sync_object_heads",
    "sync_operation_fragment_outbox",
    "sync_operation_fragment_parts",
    "sync_operation_fragments",
    "sync_operations",
    "sync_outbox",
    "sync_processor_handoffs",
    "sync_processor_state",
    "sync_provider_dirty",
    "sync_provider_state",
    "sync_provider_versions",
    "sync_receipts",
    "sync_relay_cursors",
    "sync_replica",
    "sync_resnapshot_requests",
    "sync_snapshot_chunks",
    "sync_snapshot_export_wires",
    "sync_snapshot_exports",
    "sync_snapshot_target",
    "sync_snapshots",
    "sync_wiki_slug_seen",
    "sync_wire_outbox",
    "sync_wire_receipts",
    "sync_wire_rejections",
    "sync_wire_seq",
];

impl Store {
    /// 事务内版本。调用方负责提交。
    pub(crate) fn clear_local_sync_identity_on(&self) -> Result<()> {
        ensure!(
            !self.connection.is_autocommit(),
            "sync_reset_requires_transaction"
        );
        ensure!(
            !self.has_sync_authority()?,
            "sync_reset_requires_broken_identity"
        );
        for table in REPLICATION_TABLES {
            self.connection
                .execute(&format!("DELETE FROM \"{table}\""), [])?;
        }
        // Provider 密钥的密文 AAD 绑死 `dataset_id` + `installation_id`。身份一清，
        // 这些密文**永久**打不开 —— 留着只会让设置页显示一串「有配置但用不了」的
        // provider。删掉密文后读路径返回「未填写」，用户重填即可。
        // 刻意不动：`provider_key_policy`（保留「密钥必须密封」的策略，别退回明文）、
        // `ai_provider_configs` 的 base_url/模型等结构（用户手工填过的部分留着）。
        self.connection
            .execute("DELETE FROM provider_key_secrets", [])?;
        Ok(())
    }

    pub(crate) fn clear_local_sync_identity(&self) -> Result<()> {
        let transaction = self.begin_write_transaction()?;
        self.clear_local_sync_identity_on()?;
        if let Some(transaction) = transaction {
            transaction.commit()?;
        }
        Ok(())
    }
}
