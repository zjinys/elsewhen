//! 本机同步身份的故障出口（roadmap `2026-10-27-sync-lifecycle-boundaries` C3，缺陷 D1）。
//!
//! 只有 `requires_repair`（本机有 vault/replica 但 authority 空）才允许调用。
//! 其它任何状态都必须被拒 —— 这条能力不能被当成「重置本机」的通用按钮。

use crate::{config::AppConfig, storage::Store, sync::runtime};
use anyhow::{Context, Result, ensure};

use super::invite::{LocalSyncStateDto, local_sync_state_for};

/// C3 出口前判定：旧 vault master 是否还在本进程（未锁定）。
///
/// true ⇒ 出口会走重封迁移（C12），UI 确认弹窗应告诉用户「API Key 会自动
/// 保留」；false ⇒ 回退现状（密文删除），弹窗必须维持「需要重新填写」。
pub fn provider_keys_migratable() -> Result<bool> {
    let config = AppConfig::load().context("sync_workspace_unavailable")?;
    Ok(runtime::provider_keys_migratable(&config))
}

/// 清除本机同步身份，保留全部笔记/对话/知识库。
///
/// 副作用（UI 必须逐条告知用户）：
/// 1. 本机退出该数据集，需要重新配对/重新加入才能再同步；
/// 2. **已保存的 Provider API Key 将永久失效**（密封密文的 AAD 绑死旧身份），
///    需要重填；
/// 3. 尚未推送到其它设备的本地改动不会自动补发（业务数据仍在，只是不再参与同步）。
pub fn clear_local_sync_identity() -> Result<LocalSyncStateDto> {
    let config = AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let before = local_sync_state_for(&config, &store)?;
    ensure_reset_applicable(&before.state)?;
    // 后台分析任务跑着的时候清理会留下半清状态；这道检查由存储层在写事务内做
    // （`require_background_quiescent_for_sync` 自己要求已在领域事务中）。
    // 运行时会话里存着 master 与租约；不清掉会让随后的状态读取仍看到旧 revision。
    // C12：lock 会把 session 连同旧 master 一起删掉，所以**先**取 master 再 lock，
    // 让出口在密文还解得开时把它暂存进重封迁移槽；session 不在（已锁定/重启后）
    // 时传 None，出口回退到删密文。
    let migration_master = runtime::provider_keys_source_master(&config);
    runtime::lock(&config);
    store.clear_local_sync_identity_with_migration(migration_master)?;
    drop(store);
    local_sync_state_for(&config, &Store::open(&config.database_path)?)
}

/// 准入判定被抽成纯函数：它必须在**只有** `requires_repair` 时通过。
/// 这条闸门如果失效，一个手滑就能把正在正常同步的数据集踢下线。
pub(crate) fn ensure_reset_applicable(state: &str) -> Result<()> {
    ensure!(state == "requires_repair", "sync_reset_not_applicable");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ensure_reset_applicable;

    #[test]
    fn only_the_repair_state_may_clear_the_local_identity() {
        for rejected in [
            "not_configured",
            "membership_inactive",
            "local_ready",
            "locked",
            "",
        ] {
            let error = ensure_reset_applicable(rejected)
                .expect_err(&format!("{rejected:?} 状态必须被拒绝"));
            assert!(
                format!("{error:#}").contains("sync_reset_not_applicable"),
                "{rejected:?} 的拒绝理由必须是 sync_reset_not_applicable，实际：{error:#}"
            );
        }
        ensure_reset_applicable("requires_repair").expect("requires_repair 必须放行");
    }
}
