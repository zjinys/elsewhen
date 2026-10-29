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
        (
            "clear_local_sync_identity",
            store
                .clear_local_sync_identity()
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
         「清除本机同步身份」出口（C3）必须能把它推到 not_configured。"
    );
}

fn empty_store(tag: &str) -> (std::path::PathBuf, Store) {
    let dir = std::env::temp_dir().join(format!("elsewhen-sync-reset-{tag}-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let store = Store::open(&dir.join("test.db")).unwrap();
    (dir, store)
}

fn half_configured_store(tag: &str) -> (std::path::PathBuf, Store) {
    let (dir, store) = empty_store(tag);
    let master = crate::sync::crypto::ContentKey::generate()
        .unwrap()
        .to_bytes();
    let vault = store
        .create_sync_vault("installation", &master, "device", "test")
        .unwrap();
    store
        .initialize_sync_replica(&replica(&vault))
        .unwrap();
    (dir, store)
}

#[test]
fn clearing_a_half_configured_identity_returns_to_not_configured() {
    let (dir, store) = half_configured_store("clear");
    assert_eq!(half_configured(&store), (true, false));

    store.clear_local_sync_identity().unwrap();

    assert_eq!(
        half_configured(&store),
        (false, false),
        "清完之后必须回到 not_configured 的形状（无身份、无 authority）"
    );
    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn clearing_is_refused_while_the_local_identity_is_healthy() {
    // 反向守卫：这条能力不是「重置本机」的通用按钮。有 authority 时必须拒绝，
    // 否则一个手滑就能把正常配对的数据集踢下线。
    let (dir, store) = empty_store("refuse");
    let master = crate::sync::crypto::ContentKey::generate()
        .unwrap()
        .to_bytes();
    let vault = store
        .create_sync_vault("installation", &master, "device", "test")
        .unwrap();
    store
        .initialize_sync_replica(&replica(&vault))
        .unwrap();
    store
        .connection
        .execute(
            "INSERT INTO sync_authority(singleton,anchor,local_device,signed_roster)\
             VALUES(1,'{}','device',x'00')",
            [],
        )
        .unwrap();

    let error = store.clear_local_sync_identity().unwrap_err();
    assert!(
        format!("{error:#}").contains("sync_reset_requires_broken_identity"),
        "有 authority 时必须失败关闭，实际：{error:#}"
    );
    assert_eq!(
        half_configured(&store),
        (true, true),
        "拒绝之后身份必须原封不动"
    );
    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn clearing_keeps_business_data_and_drops_undecryptable_provider_keys() {
    let (dir, store) = half_configured_store("keep");
    store
        .connection
        .execute(
            "INSERT INTO conversations(id,title,tag,created_at,updated_at,assistant_mode,archived)\
             VALUES('c1','我的笔记','general','2026-10-27T00:00:00Z','2026-10-27T00:00:00Z','personal_secretary',0)",
            [],
        )
        .unwrap();
    store
        .connection
        .execute(
            "INSERT INTO messages(id,conversation_id,role,content,created_at,kind)\
             VALUES('m1','c1','user','不能丢的内容','2026-10-27T00:00:00Z','chat')",
            [],
        )
        .unwrap();
    store
        .connection
        .execute(
            "INSERT INTO ai_provider_configs(id,name,provider_type,base_url,model,api_key_source,enabled,created_at,updated_at,is_active)\
             VALUES('p1','OpenAI','openai','https://api.openai.com','gpt','inline',1,'2026-10-27T00:00:00Z','2026-10-27T00:00:00Z',1)",
            [],
        )
        .unwrap();
    store
        .connection
        .execute(
            "INSERT INTO provider_key_secrets(config_id,ciphertext) VALUES('p1',x'00')",
            [],
        )
        .unwrap();

    store.clear_local_sync_identity().unwrap();

    let count = |sql: &str| -> i64 { store.connection.query_row(sql, [], |r| r.get(0)).unwrap() };
    assert_eq!(count("SELECT COUNT(*) FROM conversations"), 1, "对话不能丢");
    assert_eq!(count("SELECT COUNT(*) FROM messages"), 1, "消息不能丢");
    assert_eq!(
        count("SELECT COUNT(*) FROM ai_provider_configs"),
        1,
        "provider 配置结构要留着（用户手填的 base_url/模型）"
    );
    assert_eq!(
        count("SELECT COUNT(*) FROM provider_key_secrets"),
        0,
        "解不开的密文必须清掉，否则设置页显示『有 key 但用不了』"
    );
    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
}

/// C12 重封迁移：出口瞬间旧 master 还在（session 未锁定，API 层在 lock 前取出
/// 传入）时，密文被解成明文暂存进进程内迁移槽；新 vault 建好后用新 master +
/// 新 AAD 重封回库，读回的明文与原来一致。回退路径（旧 master 不可用）由
/// `clearing_keeps_business_data_and_drops_undecryptable_provider_keys` 钉住。
#[test]
fn provider_keys_survive_an_identity_reset_when_the_old_master_is_available() {
    let (dir, store) = half_configured_store("reseal");
    let old_master = crate::sync::crypto::ContentKey::generate()
        .unwrap()
        .to_bytes();
    // 密封前提：provider 配置 + 「必须密封」策略，用旧身份 master 密封一把 key。
    {
        let writer = store.clone().with_provider_master(old_master);
        let transaction = writer.begin_write_transaction().unwrap();
        writer
            .connection
            .execute(
                "INSERT INTO ai_provider_configs(id,name,provider_type,base_url,model,api_key_source,enabled,created_at,updated_at,is_active)\
                 VALUES('p1','OpenAI','openai','https://api.openai.com','gpt','inline',1,'2026-10-27T00:00:00Z','2026-10-27T00:00:00Z',1)",
                [],
            )
            .unwrap();
        writer
            .connection
            .execute("INSERT INTO provider_key_policy(singleton) VALUES(1)", [])
            .unwrap();
        writer.write_provider_key_on("p1", Some("sk-live-abc123")).unwrap();
        if let Some(transaction) = transaction {
            transaction.commit().unwrap();
        }
    }
    let sealed_count: i64 = store
        .connection
        .query_row("SELECT COUNT(*) FROM provider_key_secrets", [], |r| r.get(0))
        .unwrap();
    assert_eq!(sealed_count, 1, "前提：密文应已密封落库");

    // 出口：旧 master 可用 ⇒ 密文解出、暂存迁移槽，库里密文照常清掉。
    store
        .clear_local_sync_identity_with_migration(Some(old_master))
        .unwrap();
    let after_exit: i64 = store
        .connection
        .query_row("SELECT COUNT(*) FROM provider_key_secrets", [], |r| r.get(0))
        .unwrap();
    assert_eq!(after_exit, 0, "出口后库里不留旧密文");

    // 新 vault（新 installation = 新 master），随后重封迁移槽。
    let new_master = crate::sync::crypto::ContentKey::generate()
        .unwrap()
        .to_bytes();
    let vault = store
        .create_sync_vault("installation-2", &new_master, "device", "test")
        .unwrap();
    store.initialize_sync_replica(&replica(&vault)).unwrap();
    let resealed = store.reseal_staged_provider_keys(&new_master).unwrap();
    assert_eq!(resealed, 1, "迁移槽里的 key 应被重封");

    // 用新 master 读回，明文必须与密封前一致。
    let restored = store
        .clone()
        .with_provider_master(new_master)
        .read_provider_key("p1")
        .unwrap();
    assert_eq!(restored.as_deref(), Some("sk-live-abc123"));
    // 再取一次槽：已消费，不该重复重封。
    assert_eq!(store.reseal_staged_provider_keys(&new_master).unwrap(), 0);
    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
}

/// 回退路径：旧 master 不可用（None）时出口维持现状 —— 密文删除、迁移槽
/// 不留条目（新 vault 建好后重封是空操作）。
#[test]
fn provider_keys_stay_dropped_when_the_old_master_is_unavailable() {
    let (dir, store) = half_configured_store("fallback");
    {
        let writer = store.clone();
        let transaction = writer.begin_write_transaction().unwrap();
        writer
            .connection
            .execute(
                "INSERT INTO ai_provider_configs(id,name,provider_type,base_url,model,api_key_source,enabled,created_at,updated_at,is_active)\
                 VALUES('p1','OpenAI','openai','https://api.openai.com','gpt','inline',1,'2026-10-27T00:00:00Z','2026-10-27T00:00:00Z',1)",
                [],
            )
            .unwrap();
        writer
            .connection
            .execute("INSERT INTO provider_key_policy(singleton) VALUES(1)", [])
            .unwrap();
        writer
            .connection
            .execute(
                "INSERT INTO provider_key_secrets(config_id,ciphertext) VALUES('p1',x'00')",
                [],
            )
            .unwrap();
        if let Some(transaction) = transaction {
            transaction.commit().unwrap();
        }
    }
    store.clear_local_sync_identity().unwrap();
    let fallback_master = crate::sync::crypto::ContentKey::generate()
        .unwrap()
        .to_bytes();
    let vault = store
        .create_sync_vault("installation-2", &fallback_master, "device", "test")
        .unwrap();
    store.initialize_sync_replica(&replica(&vault)).unwrap();
    assert_eq!(
        store.reseal_staged_provider_keys(&fallback_master).unwrap(),
        0,
        "旧 master 不可用时不能有东西被重封"
    );
    let count: i64 = store
        .connection
        .query_row("SELECT COUNT(*) FROM provider_key_secrets", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0, "解不开的密文必须清掉");
    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn every_replication_table_is_covered_by_the_reset_list() {
    // 迁移新增一张复制表却忘了加进 REPLICATION_TABLES，会让「清除身份」后留下
    // 孤儿行（例如旧 members/wire 残留），下次配对时被当成当前状态读走。
    // 这条用例把「忘了列」变成红灯，而不是靠人记得。
    let (dir, store) = empty_store("coverage");
    let actual: Vec<String> = {
        let mut statement = store
            .connection
            .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name LIKE 'sync\\_%' ESCAPE '\\' ORDER BY name")
            .unwrap();
        statement
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<String>>>()
            .unwrap()
    };
    let listed: Vec<String> = crate::storage::sync::reset::REPLICATION_TABLES
        .iter()
        .map(|name| name.to_string())
        .collect();
    let missing: Vec<&String> = actual.iter().filter(|n| !listed.contains(n)).collect();
    let stale: Vec<&String> = listed.iter().filter(|n| !actual.contains(n)).collect();
    assert!(
        missing.is_empty(),
        "这些 sync_ 表没被「清除本机同步身份」覆盖：{missing:?} —— 要么加进 REPLICATION_TABLES，要么改名前缀"
    );
    assert!(
        stale.is_empty(),
        "REPLICATION_TABLES 里有已不存在的表：{stale:?}"
    );
    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
}
