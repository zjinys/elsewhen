//! 单例读取接缝的语义（roadmap `2026-10-27-sync-lifecycle-boundaries` C1）。
//!
//! 两条不变式，各自对应一个真实故障：
//! 1. **未配置 ⇒ 领域错误**。缺行不是空值、不是临时故障，是「本机还没准备同步身份」
//!    这个终态。漏成 rusqlite 文本时，后台 worker 会把它当瞬时失败每 tick 重试
//!    （`AnyhowException(Query returned no rows)`）。
//! 2. **已配置 ⇒ 读到真值**。把缺行一律当 0/空串会静默造出「序号从 0 开始」「时钟
//!    回退」这类更贵的错，所以正确性也要钉。

use super::*;

fn assert_not_configured<T: std::fmt::Debug>(result: anyhow::Result<T>, what: &str) {
    let error = result.expect_err(&format!("{what}：未配置时必须报错，不能返回默认值"));
    let text = format!("{error:#}");
    assert!(
        text.contains("sync_not_configured"),
        "{what}：未配置时必须是领域错误 sync_not_configured，实际：{text}"
    );
}

#[test]
fn every_singleton_accessor_reports_sync_not_configured_on_an_empty_store() {
    let dir = std::env::temp_dir().join(format!("elsewhen-singleton-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let store = Store::open(&dir.join("test.db")).unwrap();

    assert_not_configured(store.replica_next_sequence(), "replica_next_sequence");
    assert_not_configured(store.replica_logical_clock(), "replica_logical_clock");
    assert_not_configured(store.local_vault_basis(), "local_vault_basis");
    assert_not_configured(store.local_vault_installation(), "local_vault_installation");
    assert_not_configured(store.local_vault_revision(), "local_vault_revision");

    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn every_singleton_accessor_reads_the_real_row_once_configured() {
    let dir = std::env::temp_dir().join(format!("elsewhen-singleton-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let store = Store::open(&dir.join("test.db")).unwrap();
    let master = crate::sync::crypto::ContentKey::generate()
        .unwrap()
        .to_bytes();
    let vault = store
        .create_sync_vault("install-xyz", &master, "device", "test")
        .unwrap();
    let device = vault.device();
    store
        .initialize_sync_replica(&ReplicaIdentity {
            dataset_id: "test-dataset".into(),
            device_id: device.device_id.clone(),
            stream_id: device.stream_id.clone(),
            processor_device_id: device.device_id.clone(),
        })
        .unwrap();

    assert_eq!(store.local_vault_installation().unwrap(), "install-xyz");
    assert_eq!(
        store.local_vault_revision().unwrap(),
        vault.revision,
        "revision 必须与 vault 自身一致（AAD 与解锁都绑它）"
    );
    assert_eq!(
        store.local_vault_basis().unwrap(),
        ("install-xyz".to_string(), vault.revision)
    );
    let next_sequence = store.replica_next_sequence().unwrap();
    let logical_clock = store.replica_logical_clock().unwrap();
    assert!(next_sequence >= 1, "next_sequence 从 1 起，实际 {next_sequence}");
    assert!(logical_clock >= 0, "logical_clock 不得为负，实际 {logical_clock}");

    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
}
