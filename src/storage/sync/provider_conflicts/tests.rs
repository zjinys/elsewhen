//! End-to-end P4b provider sync conflict review tests.
//!
//! These tests use synthetic keys and an injected content key (no LocalVault,
//! no native keystore); they cover masked review, explicit KeepLocal /
//! AcceptRemote resolution, revision binding, key rotation, profile deletion,
//! activation changes, and replay-after-resolution convergence. The sealed
//! envelope bytes must never contain plaintext keys.

use super::*;
use crate::storage::sync::ReplicaIdentity;
use crate::sync::crypto::{DeviceIdentity, seal};
use uuid::Uuid;

struct Db {
    store: Store,
    dir: std::path::PathBuf,
}

impl Db {
    fn new(device: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("elsewhen-sync-conflict-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(&dir.join("test.db")).unwrap();
        store
            .initialize_sync_replica(&ReplicaIdentity {
                dataset_id: "dataset".into(),
                device_id: device.into(),
                stream_id: format!("stream-{device}"),
                processor_device_id: "a".into(),
            })
            .unwrap();
        store.authorize_sync_member("a", "stream-a").unwrap();
        Self { store, dir }
    }
}

impl Drop for Db {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn encode(
    bundle: &ProviderBundle,
    signer: &DeviceIdentity,
    key: &ContentKey,
) -> (EnvelopeHeader, Vec<u8>) {
    let header = EnvelopeHeader::new(
        "dataset",
        "a",
        "b",
        1,
        format!("provider:{}", bundle.revision_id),
    )
    .unwrap();
    let bytes = seal(
        header.clone(),
        key,
        signer,
        &serde_json::to_vec(bundle).unwrap(),
    )
    .unwrap()
    .to_bytes()
    .unwrap();
    (header, bytes)
}

/// A "remote" store (device a) with two configured profiles, the second active.
fn remote_store() -> Db {
    let db = Db::new("a");
    db.store
        .save_ai_provider_config(
            None,
            "first",
            "openai-compatible",
            "http://127.0.0.1:9999",
            "model-1",
            "SYNTHETIC_REMOTE_KEY_FIRST",
            0.2,
            Some(2048),
        )
        .unwrap();
    let second = db
        .store
        .save_ai_provider_config(
            None,
            "second",
            "openai-compatible",
            "https://remote.invalid/v1",
            "model-2",
            "SYNTHETIC_REMOTE_KEY_SECOND",
            0.7,
            None,
        )
        .unwrap();
    db.store.set_active_ai_provider_config(&second).unwrap();
    db
}

/// Device b with its own independent local configuration → guaranteed conflict.
fn conflicted() -> (Db, Db, DeviceIdentity, ContentKey, ProviderBundle) {
    let a = remote_store();
    let b = Db::new("b");
    let signer = DeviceIdentity::generate().unwrap();
    let key = ContentKey::generate().unwrap();
    b.store
        .save_ai_provider_config(
            None,
            "local",
            "openai-compatible",
            "https://local.invalid",
            "local-model",
            "SYNTHETIC_LOCAL_KEY",
            0.5,
            None,
        )
        .unwrap();
    let bundle = a
        .store
        .export_provider_bundle("remote-rev-1", None)
        .unwrap();
    let (header, bytes) = encode(&bundle, &signer, &key);
    assert_eq!(
        b.store
            .apply_encrypted_provider_bundle(&bytes, &header, &key, &signer.public_key())
            .unwrap(),
        crate::storage::sync::providers::ProviderApplyResult::Conflict
    );
    (a, b, signer, key, bundle)
}

#[test]
fn review_masks_keys_and_binds_reviewed_revisions() {
    let (_a, b, signer, key, bundle) = conflicted();
    let reviews = b
        .store
        .list_sync_provider_conflicts_with(1, &key, &signer.public_key())
        .unwrap();
    assert_eq!(reviews.len(), 1);
    let review = &reviews[0];
    assert_eq!(review.revision_id, "remote-rev-1");
    assert_eq!(review.sender_device_id, "a");
    assert_eq!(
        review.local_revision,
        b.store.sync_provider_head_revision().unwrap()
    );

    // 远端两条、本地一条；Key 全部掩码，任何字段都不得含明文。
    assert_eq!(review.remote_profiles.len(), 2);
    assert_eq!(review.local_profiles.len(), 1);
    let serialized = serde_json::to_string(review).unwrap();
    for plaintext in [
        "SYNTHETIC_REMOTE_KEY_FIRST",
        "SYNTHETIC_REMOTE_KEY_SECOND",
        "SYNTHETIC_LOCAL_KEY",
    ] {
        assert!(
            !serialized.contains(plaintext),
            "review DTO must never contain a plaintext key"
        );
    }
    // 掩码保留前 3/后 4，中间遮蔽；<=8 字符整体遮蔽。
    let remote_first = review
        .remote_profiles
        .iter()
        .find(|p| p.name == "first")
        .unwrap();
    assert_eq!(remote_first.api_key_masked, "SYN****IRST");
    assert!(remote_first.api_key_present);
    let local = &review.local_profiles[0];
    assert_eq!(local.api_key_masked, "SYN****_KEY");

    // 激活项与故障切换顺序随审阅暴露。
    assert_eq!(review.remote_active_id, bundle.active_id);
    assert_eq!(review.remote_failover_order, bundle.failover_order);
    assert!(review.local_active_id.is_some());

    // 密封信封仍不含明文。
    let envelope: Vec<u8> = b
        .store
        .connection
        .query_row(
            "SELECT encrypted_envelope FROM sync_provider_versions WHERE revision_id='remote-rev-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!String::from_utf8_lossy(&envelope).contains("SYNTHETIC"));
}

#[test]
fn accept_remote_replaces_local_state_and_clears_conflict() {
    let (_a, b, signer, key, bundle) = conflicted();
    let local_revision = b.store.sync_provider_head_revision().unwrap();
    b.store
        .resolve_sync_provider_conflict_with(
            "remote-rev-1",
            &local_revision,
            "remote-rev-1",
            ProviderConflictResolution::AcceptRemote,
            1,
            &key,
            &signer.public_key(),
        )
        .unwrap();

    // 本地配置被远端整包替换（含真实合成 Key 与激活项）。
    let active = b.store.active_ai_provider_config().unwrap().unwrap();
    assert_eq!(active.api_key, "SYNTHETIC_REMOTE_KEY_SECOND");
    assert_eq!(b.store.list_ai_provider_configs().unwrap().len(), 2);
    assert_eq!(
        b.store.sync_provider_head_revision().unwrap(),
        "remote-rev-1"
    );
    let state: String = b
        .store
        .connection
        .query_row(
            "SELECT state FROM sync_provider_versions WHERE revision_id='remote-rev-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(state, "applied");

    // 冲突已解决：审阅列表为空；重投同 revision 是幂等 duplicate 而非新冲突。
    assert!(
        b.store
            .list_sync_provider_conflicts_with(1, &key, &signer.public_key())
            .unwrap()
            .is_empty()
    );
    let (header, bytes) = encode(&bundle, &signer, &key);
    assert_eq!(
        b.store
            .apply_encrypted_provider_bundle(&bytes, &header, &key, &signer.public_key())
            .unwrap(),
        crate::storage::sync::providers::ProviderApplyResult::Duplicate
    );
}

#[test]
fn keep_local_dismisses_and_readvertises_local_state() {
    let (_a, b, signer, key, bundle) = conflicted();
    let local_revision = b.store.sync_provider_head_revision().unwrap();
    b.store
        .resolve_sync_provider_conflict_with(
            "remote-rev-1",
            &local_revision,
            "remote-rev-1",
            ProviderConflictResolution::KeepLocal,
            1,
            &key,
            &signer.public_key(),
        )
        .unwrap();

    // 本地配置原样保留。
    let active = b.store.active_ai_provider_config().unwrap().unwrap();
    assert_eq!(active.api_key, "SYNTHETIC_LOCAL_KEY");
    assert_eq!(b.store.list_ai_provider_configs().unwrap().len(), 1);

    // 冲突被驳回（不可再审阅），并标记待同步以重新广播本地状态。
    let state: String = b
        .store
        .connection
        .query_row(
            "SELECT state FROM sync_provider_versions WHERE revision_id='remote-rev-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(state, "dismissed");
    let dirty: bool = b
        .store
        .connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_provider_dirty)",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(dirty);
    assert!(
        b.store
            .list_sync_provider_conflicts_with(1, &key, &signer.public_key())
            .unwrap()
            .is_empty()
    );

    // 重投已驳回的 revision：幂等，不复活为冲突。
    let (header, bytes) = encode(&bundle, &signer, &key);
    let result = b
        .store
        .apply_encrypted_provider_bundle(&bytes, &header, &key, &signer.public_key())
        .unwrap();
    assert_ne!(
        result,
        crate::storage::sync::providers::ProviderApplyResult::Conflict,
        "a dismissed revision must not resurrect as a new conflict"
    );
    assert_eq!(
        b.store
            .active_ai_provider_config()
            .unwrap()
            .unwrap()
            .api_key,
        "SYNTHETIC_LOCAL_KEY"
    );
}

#[test]
fn resolution_requires_the_exact_reviewed_revisions() {
    let (_a, b, signer, key, _bundle) = conflicted();
    let local_revision = b.store.sync_provider_head_revision().unwrap();

    // 错误的远端 revision（用户看到的与提交的不一致）。
    let error = b
        .store
        .resolve_sync_provider_conflict_with(
            "remote-rev-1",
            &local_revision,
            "some-other-revision",
            ProviderConflictResolution::AcceptRemote,
            1,
            &key,
            &signer.public_key(),
        )
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("provider_conflict_revision_mismatch")
    );

    // 错误的本地 revision（审阅期间本地又变了）。
    let error = b
        .store
        .resolve_sync_provider_conflict_with(
            "remote-rev-1",
            "stale-local-revision",
            "remote-rev-1",
            ProviderConflictResolution::AcceptRemote,
            1,
            &key,
            &signer.public_key(),
        )
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("provider_conflict_local_changed")
    );

    // 不存在的冲突行。
    let error = b
        .store
        .resolve_sync_provider_conflict_with(
            "never-seen",
            &local_revision,
            "never-seen",
            ProviderConflictResolution::KeepLocal,
            1,
            &key,
            &signer.public_key(),
        )
        .unwrap_err();
    assert!(error.to_string().contains("provider_conflict_not_pending"));

    // 已驳回（dismissed）的 revision 不可二次裁决。
    b.store
        .resolve_sync_provider_conflict_with(
            "remote-rev-1",
            &local_revision,
            "remote-rev-1",
            ProviderConflictResolution::KeepLocal,
            1,
            &key,
            &signer.public_key(),
        )
        .unwrap();
    let error = b
        .store
        .resolve_sync_provider_conflict_with(
            "remote-rev-1",
            &b.store.sync_provider_head_revision().unwrap(),
            "remote-rev-1",
            ProviderConflictResolution::AcceptRemote,
            1,
            &key,
            &signer.public_key(),
        )
        .unwrap_err();
    assert!(error.to_string().contains("provider_conflict_not_pending"));

    // 全部失败关闭：本地配置仍未被远端覆盖。
    assert_eq!(
        b.store
            .active_ai_provider_config()
            .unwrap()
            .unwrap()
            .api_key,
        "SYNTHETIC_LOCAL_KEY"
    );
    let state: String = b
        .store
        .connection
        .query_row(
            "SELECT state FROM sync_provider_versions WHERE revision_id='remote-rev-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(state, "dismissed");
}

#[test]
fn accept_remote_applies_rotation_deletion_and_activation_exactly() {
    let (_a, b, signer, key, mut bundle) = conflicted();
    let local_revision = b.store.sync_provider_head_revision().unwrap();

    // 远端第二次修订：轮换 Key、删除一条 profile、切换激活项。
    bundle.base_revision = Some("remote-rev-1".into());
    bundle.revision_id = "remote-rev-2".into();
    bundle.profiles.truncate(1);
    bundle.profiles[0].api_key = "SYNTHETIC_ROTATED".into();
    bundle.active_id = Some(bundle.profiles[0].id.clone());
    bundle.failover_order = vec![bundle.profiles[0].id.clone()];
    let (header, bytes) = encode(&bundle, &signer, &key);
    assert_eq!(
        b.store
            .apply_encrypted_provider_bundle(&bytes, &header, &key, &signer.public_key())
            .unwrap(),
        crate::storage::sync::providers::ProviderApplyResult::Conflict
    );

    // 接受第二次修订：本地精确落为「单 profile、轮换后 Key、激活项即它」。
    b.store
        .resolve_sync_provider_conflict_with(
            "remote-rev-2",
            &local_revision,
            "remote-rev-2",
            ProviderConflictResolution::AcceptRemote,
            1,
            &key,
            &signer.public_key(),
        )
        .unwrap();
    let configs = b.store.list_ai_provider_configs().unwrap();
    assert_eq!(configs.len(), 1);
    let active = b.store.active_ai_provider_config().unwrap().unwrap();
    assert_eq!(active.api_key, "SYNTHETIC_ROTATED");
    assert_eq!(active.id, configs[0].id);

    // 接受 remote-rev-2：remote-rev-1（用户明确不接受的较早候选）保持
    // conflict 终态，绝不自动静默应用；remote-rev-2 为 applied。
    let states: Vec<(String, String)> = {
        let mut statement = b
            .store
            .connection
            .prepare("SELECT revision_id,state FROM sync_provider_versions ORDER BY revision_id")
            .unwrap();
        statement
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
    };
    assert_eq!(
        states,
        vec![
            ("remote-rev-1".to_string(), "conflict".to_string()),
            ("remote-rev-2".to_string(), "applied".to_string()),
        ],
        "an earlier conflict candidate the user never accepted stays pending, never silently applied"
    );
    // 审阅列表仍显示 remote-rev-1：用户尚未对它做出明确选择；
    // 它绝不可被静默应用或消失，仍需用户显式保留本地或接受远端。
    let remaining = b
        .store
        .list_sync_provider_conflicts_with(1, &key, &signer.public_key())
        .unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].revision_id, "remote-rev-1");
}

#[test]
fn masking_is_utf8_safe_and_bounds_short_keys() {
    // <=8 char：整体遮蔽，不泄露任何字符。
    assert_eq!(
        crate::storage::provider_keys::mask_api_key("sk-short"),
        "********"
    );
    assert_eq!(crate::storage::provider_keys::mask_api_key("abc"), "***");
    // 长 key：保留前 3/后 4，中间遮蔽。
    assert_eq!(
        crate::storage::provider_keys::mask_api_key("sk-proj-abcdef1234567890"),
        "sk-****7890"
    );
    // 多字节字符绝不按字节切片（不会 panic 且边界正确）。
    let emoji = "🔑🔑🔑🔑🔑🔑🔑🔑🔑🔑";
    let masked = crate::storage::provider_keys::mask_api_key(emoji);
    assert!(masked.contains("****"));
    assert!(!masked.contains('\u{FFFD}'));
    assert_eq!(crate::storage::provider_keys::mask_api_key(""), "");
}

/// 链条分叉但内容与本地逐字节一致（重装后设备重发同一份快照的形态）：
/// 不制造人工审阅冲突，直接 applied 并把修订链接回本地 head——否则为
/// 「什么都没变」的修订反复打断用户，链条也永远无法自愈收敛。
#[test]
fn identical_content_on_forked_chain_applies_without_manual_review() {
    let a = remote_store();
    let b = Db::new("b");
    let signer = DeviceIdentity::generate().unwrap();
    let key = ContentKey::generate().unwrap();
    // b 先正常接受一份，再在本地做一次显式激活（推进 head，制造分叉 base），
    // 但激活项与远端一致 → 内容不变。
    let bundle = a.store.export_provider_bundle("remote-rev-1", None).unwrap();
    let (header, bytes) = encode(&bundle, &signer, &key);
    assert_eq!(
        b.store
            .apply_encrypted_provider_bundle(&bytes, &header, &key, &signer.public_key())
            .unwrap(),
        crate::storage::sync::providers::ProviderApplyResult::Applied
    );
    let head_before = b.store.sync_provider_head_revision().unwrap();
    let active = b.store.active_ai_provider_config().unwrap().unwrap();
    b.store
        .set_active_ai_provider_config_for_sync(&active.id, true)
        .unwrap();
    assert_ne!(
        b.store.sync_provider_head_revision().unwrap(),
        head_before,
        "precondition: local head advanced so the next remote base is stale"
    );

    // 远端重发同内容、base 指向旧 head：链条分叉但内容一致 → 免审 applied。
    let rebased = a
        .store
        .export_provider_bundle("remote-rev-2", Some(head_before.clone()))
        .unwrap();
    let mut local_relabeled = serde_json::to_value(
        b.store
            .export_provider_bundle("remote-rev-2", None)
            .unwrap(),
    )
    .unwrap();
    local_relabeled["base_revision"] = serde_json::Value::String(head_before.clone());
    // 显式激活会刷新 updated_at；内容同一性比较只关心结构字段。
    let strip_timestamps = |mut value: serde_json::Value| {
        if let Some(profiles) = value.get_mut("profiles").and_then(|p| p.as_array_mut()) {
            for profile in profiles {
                profile["updated_at"] = serde_json::Value::Null;
            }
        }
        value
    };
    assert_eq!(
        strip_timestamps(local_relabeled),
        strip_timestamps(serde_json::to_value(&rebased).unwrap()),
        "precondition: local state relabeled matches the incoming bundle"
    );
    let (header, bytes) = encode(&rebased, &signer, &key);
    assert_eq!(
        b.store
            .apply_encrypted_provider_bundle(&bytes, &header, &key, &signer.public_key())
            .unwrap(),
        crate::storage::sync::providers::ProviderApplyResult::Applied,
        "identical content on a forked chain must not demand manual review"
    );
    assert_eq!(
        b.store.sync_provider_head_revision().unwrap(),
        "remote-rev-2",
        "applying the identical bundle heals the revision chain"
    );
    assert!(
        b.store
            .list_sync_provider_conflicts_with(1, &key, &signer.public_key())
            .unwrap()
            .is_empty()
    );

    // 变异基线对照组：同一分叉形态但内容有实质差异 → 仍必须人工审阅。
    // （用新的 revision：重放 remote-rev-2 会命中同 revision 幂等防线。）
    let mut divergent = a
        .store
        .export_provider_bundle("remote-rev-3", Some(head_before))
        .unwrap();
    divergent.profiles[0].api_key = "SYNTHETIC_DIVERGED".into();
    let (header, bytes) = encode(&divergent, &signer, &key);
    assert_eq!(
        b.store
            .apply_encrypted_provider_bundle(&bytes, &header, &key, &signer.public_key())
            .unwrap(),
        crate::storage::sync::providers::ProviderApplyResult::Conflict,
        "content divergence on a forked chain still requires explicit review"
    );
    let head_after = b.store.sync_provider_head_revision().unwrap();
    assert_ne!(
        head_after, "remote-rev-3",
        "divergent revision must not advance the local head"
    );
}
