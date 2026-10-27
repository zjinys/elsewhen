//! 本机同步身份的故障出口（roadmap `2026-10-27-sync-lifecycle-boundaries` C3，缺陷 D1）。
//!
//! 只有 `requires_repair`（本机有 vault/replica 但 authority 空）才允许调用。
//! 其它任何状态都必须被拒 —— 这条能力不能被当成「重置本机」的通用按钮。

use crate::{config::AppConfig, storage::Store, sync::runtime};
use anyhow::{Result, ensure};

use super::invite::{LocalSyncStateDto, local_sync_state_for};

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
    // 后台/前台 worker 正在写同步表时清理会留下半清状态。
    store.require_background_quiescent_for_sync()?;
    // 运行时会话里存着 master 与租约；不清掉会让随后的状态读取仍看到旧 revision。
    runtime::lock(&config);
    store.clear_local_sync_identity()?;
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
