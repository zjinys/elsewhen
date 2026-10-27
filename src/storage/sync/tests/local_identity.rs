//! 「本机同步身份处于半配置状态时无出口」——D1 的定性用例（roadmap
//! `2026-10-27-sync-lifecycle-boundaries` C0）。
//!
//! 这条用例要钉住的不是某段实现的形状，而是**状态机缺一条边**：
//! 本机有身份、但 authority 空时，`src/api/sync/invite.rs:144-160` 恒判
//! `requires_repair`，而全仓没有任何 API 能把它推回 `not_configured`。
//!
//! 这个形状在真机上由三条正常路径产生（掉电、杀进程、恢复备份），不是运维臆想：
//! - `src/storage/sync/vault.rs:79`（revision 不匹配）
//! - `src/storage/sync/enrollment.rs:99`（交接写到一半）
//! - `src/sync/enrollment.rs:116`（join 中断后 bootstrap 缺失）
//!
//! 用例的契约是「**存在**一个可达出口」：把现有恢复入口逐个试一遍，最后断言其中
//! 至少有一个把身份清干净（`not_configured`）或补出授权（`authority` 非空）。C3 落地
//! 「清除本机同步身份」后，只需把该调用加进 `attempts`，本用例即转绿 —— 断言本身
//! 不需要改。

use super::*;

fn replica(vault: &crate::sync::vault::LocalVault) -> ReplicaIdentity {
    let device = vault.device();
    ReplicaIdentity {
        dataset_id: "test-dataset".into(),
        device_id: device.device_id.clone(),
        stream_id: device.stream_id.clone(),
        processor_device_id: device.device_id.clone(),
    }
}

fn half_configured(store: &Store) -> (bool, bool) {
    (
        store.sync_identity().unwrap().is_some(),
        store.has_sync_authority().unwrap(),
    )
}

#[test]
fn a_half_configured_local_identity_has_no_exit_back_to_not_configured() {
    let dir = std::env::temp_dir().join(format!("elsewhen-sync-exit-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let store = Store::open(&dir.join("test.db")).unwrap();

    let master = crate::sync::crypto::ContentKey::generate()
        .unwrap()
        .to_bytes();
    let vault = store
        .create_sync_vault("installation", &master, "device", "test")
        .unwrap();
    store
        .initialize_sync_replica(&replica(&vault))
        .unwrap();

    // 前提：正是 `invite.rs:144-160` 判成 `requires_repair` 的形状。
    assert_eq!(
        half_configured(&store),
        (true, false),
        "前提不成立：本机身份应在、authority 应为空"
    );

    // UI 上可达的全部「恢复」动作。每项都记下结果，失败原因要留在断言消息里。
    let attempts = [
        (
            "create_sync_vault",
            store
                .create_sync_vault("installation", &master, "device", "test")
                .err()
                .map(|error| format!("{error:#}")),
        ),
        (
            "create_sync_join_vault",
            store
                .create_sync_join_vault("installation", &master, "device", "test")
                .err()
                .map(|error| format!("{error:#}")),
        ),
        (
            "initialize_sync_replica",
            store
                .initialize_sync_replica(&replica(&vault))
                .err()
                .map(|error| format!("{error:#}")),
        ),
    ];

    let (identity_present, authority_present) = half_configured(&store);
    drop(store);
    let _ = std::fs::remove_dir_all(&dir);

    assert!(
        !identity_present || authority_present,
        "requires_repair 是无出口状态：本机身份在、authority 空，试遍现有恢复入口后形状未变。\n\
         各入口结果：{attempts:#?}\n\
         C3 必须提供「清除本机同步身份」出口（清 vault/replica/authority，保留业务数据），\
         把这里推到 not_configured 或补出 authority。"
    );
}
