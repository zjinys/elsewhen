//! 同步状态 DTO 的积压投影（缺陷 D4）。
//!
//! 这里测的是**口径**：`pending_operations` 说的是「本机还有多少操作没被回执」，
//! 而 `pending_wire_envelopes` 说的是「对端还没确认收到几条」。两者混起来，
//! 就会出现「显示 64 条待确认，但其实一条都没发出去」这种误导。

use super::local_sync_state_for;
use crate::config::AppConfig;
use crate::storage::Store;

fn scratch(tag: &str) -> (std::path::PathBuf, AppConfig, Store) {
    let dir = std::env::temp_dir().join(format!("elsewhen-backlog-{tag}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("test.db");
    let store = Store::open(&path).unwrap();
    let config = AppConfig {
        data_dir: dir.clone(),
        database_path: path,
    };
    (dir, config, store)
}

#[test]
fn pending_backlog_counts_only_unacknowledged_envelopes() {
    let (dir, config, store) = scratch("count");
    for (id, state, bytes) in [
        ("m1", "pending", 10usize),
        ("m2", "pending", 20),
        ("m3", "acknowledged", 999),
    ] {
        store
            .connection
            .execute(
                "INSERT INTO sync_wire_outbox(recipient_device_id,message_id,payload_hash,key_version,encrypted_envelope,state)\
                 VALUES('peer',?1,'hash',1,?2,?3)",
                rusqlite::params![id, vec![0u8; bytes], state],
            )
            .unwrap();
    }

    let (count, bytes) = store.pending_wire_backlog().unwrap();
    assert_eq!((count, bytes), (2, 30), "只算 pending，已回执的不算积压");

    let state = local_sync_state_for(&config, &store).unwrap();
    assert_eq!(state.pending_wire_envelopes, 2, "DTO 必须带出条数");
    assert_eq!(state.pending_bytes, 30, "DTO 必须带出字节数");
    assert_eq!(
        state.pending_operations, 0,
        "本机操作计数与对端未确认是两个口径，不能互相顶替"
    );
    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_empty_outbox_reports_zero_rather_than_failing() {
    // SUM() 在空集上是 NULL；不 COALESCE 就会把「没有积压」变成读状态报错。
    let (dir, config, store) = scratch("empty");
    assert_eq!(store.pending_wire_backlog().unwrap(), (0, 0));
    let state = local_sync_state_for(&config, &store).unwrap();
    assert_eq!((state.pending_wire_envelopes, state.pending_bytes), (0, 0));
    assert_eq!(state.state, "not_configured");
    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
}
