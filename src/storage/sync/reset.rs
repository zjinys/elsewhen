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
    ///
    /// `migration_master` 是出口 API 在 `runtime::lock` **之前**从 session 取出的
    /// 旧 vault master（roadmap C12）：Some 时先把密文解成明文、暂存进进程内
    /// 迁移槽，等同一进程内建好新 vault 后重封；None（已锁定/进程重启后/解密
    /// 失败）时回退现状 —— 直接删密文。出口本身绝不因迁移失败而失败。
    pub(crate) fn clear_local_sync_identity_on(
        &self,
        migration_master: Option<[u8; 32]>,
    ) -> Result<()> {
        ensure!(
            !self.connection.is_autocommit(),
            "sync_reset_requires_transaction"
        );
        ensure!(
            !self.has_sync_authority()?,
            "sync_reset_requires_broken_identity"
        );
        // C12 迁移必须在删复制表**之前**做：AAD 要从 `sync_identity` /
        // `sync_local_vault` 组装，而下面那一刀会把它们一起清掉。旧 master 由
        // 出口 API 在 `runtime::lock` 之前取出传入；不可用（已锁定/重启后）或
        // 解密失败时回退现状 —— 密文照删，出口绝不因迁移失败而失败。
        // 先清槽，避免上一次出口留下的陈旧明文被误重封。
        crate::sync::runtime::stage_provider_keys_for_resealing(&self.path, Vec::new());
        if let Some(master) = migration_master
            && let Ok(keys) = self.open_provider_keys_with(&master)
        {
            crate::sync::runtime::stage_provider_keys_for_resealing(&self.path, keys);
        }
        // 复制表之间互相有外键（replica → members → keys、exports → chunks…），
        // 逐表删除必然在中途撞上「父行已没了子行还在」以外的情况：这里是反过来，
        // 先删父表。与其维护一份脆弱的删除顺序，不如把外键检查推迟到 COMMIT ——
        // 那一刻所有复制表都已清空，一致性自然成立。该 pragma 在 COMMIT 后自动失效。
        self.connection
            .execute_batch("PRAGMA defer_foreign_keys = ON")?;
        for table in REPLICATION_TABLES {
            self.connection
                .execute(&format!("DELETE FROM \"{table}\""), [])?;
        }
        // Provider 密钥的密文 AAD 绑死 `dataset_id` + `installation_id`。身份一清，
        // 这些密文**永久**打不开 —— 留着只会让设置页显示一串「有配置但用不了」的
        // provider。删掉密文后读路径返回「未填写」，用户重填即可。
        self.connection
            .execute("DELETE FROM provider_key_secrets", [])?;
        Ok(())
    }

    /// 事务由本函数开：`require_background_quiescent_for_sync` 要求调用方已在
    /// 领域写事务内，否则它自己就会拒绝（`sync_requires_domain_transaction`）。
    pub(crate) fn clear_local_sync_identity(&self) -> Result<()> {
        self.clear_local_sync_identity_with_migration(None)
    }

    /// C12：带重封迁移的出口。`migration_master` 由 API 层在 `runtime::lock`
    /// 之前从 session 取出传入（None = 回退现状）。
    pub(crate) fn clear_local_sync_identity_with_migration(
        &self,
        migration_master: Option<[u8; 32]>,
    ) -> Result<()> {
        let transaction = self.begin_write_transaction()?;
        self.require_background_quiescent_for_sync()?;
        self.clear_local_sync_identity_on(migration_master)?;
        if let Some(transaction) = transaction {
            transaction.commit()?;
        }
        Ok(())
    }
}
