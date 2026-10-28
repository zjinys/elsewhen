use super::*;
use crate::sync::{
    crypto::ContentKey, pairing::InvitationRoute, runtime::tests::Fixture,
    transport_relay::RelayClient,
};

fn invitation() -> String {
    let identity = crate::sync::crypto::DeviceIdentity::generate().unwrap();
    let owner = PairingDevice {
        device_id: "owner".into(),
        stream_id: "owner-stream".into(),
        name: "trusted owner".into(),
        platform: "test".into(),
        signing_key: identity.public_key(),
    };
    PairingInvitation::issue_for_relay(
        "shared-dataset",
        owner,
        &identity,
        now(),
        InvitationRoute {
            relay_url: "http://localhost:9876".into(),
            issuer_mailbox: RelayClient::mailbox_key(&[3; 32]),
            request_sender: "a".repeat(64),
        },
    )
    .unwrap()
    .0
    .encode()
    .unwrap()
}
fn prepared(f: &Fixture) -> JoinView {
    let shown = preview(&f.config, &invitation()).unwrap();
    prepare_key(&f.config, &shown.token, "new device", "test", true).unwrap()
}

#[test]
fn joining_uses_an_isolated_staging_vault_and_never_activates_the_current_database() {
    let f = Fixture::new();
    f.store
        .submit_input_as_event("existing local fact", "capture", None)
        .unwrap();
    let shown = preview(&f.config, &invitation()).unwrap();
    assert_eq!(shown.phase, "preview");
    assert!(shown.entry_id.is_none());
    assert!(pending_entries(&f.config).unwrap().is_empty());
    assert!(prepare_key(&f.config, &shown.token, "new", "test", false).is_err());
    assert!(
        super::super::background_guard(&f.config, &f.store)
            .unwrap()
            .is_none()
    );
    let request = prepare_key(&f.config, &shown.token, "new device", "test", true).unwrap();
    let key = ContentKey::generate().unwrap().to_bytes();
    assert!(install_key(&f.config, &request.token, key.to_vec(), 0).is_err());
    let commit = install_key(&f.config, &request.token, key.to_vec(), 1).unwrap();
    assert_eq!(commit.phase, "key_confirmation");
    assert!(send_request(&f.config, &request.operation_id).is_err());
    assert!(super::super::confirm_key(&f.config, &commit.token, 1).is_err());
    let ready = confirm_key(&f.config, &commit.token, 1).unwrap();
    assert_eq!(ready.phase, "ready_to_request");
    assert!(ready.local_fingerprint.is_some());
    assert!(!super::super::unlocked(&f.config, 1));
    assert!(super::super::provider_master(&f.config.database_path, 1).is_none());
    assert!(f.store.sync_identity().unwrap().is_none());
    assert_eq!(f.store.list_events().unwrap().len(), 1);
    f.store
        .submit_input_as_event("capture during join", "capture", None)
        .unwrap();
    assert_eq!(f.store.list_events().unwrap().len(), 2);
    assert!(
        crate::sync::datasets::selected_database_path(&f.config.data_dir)
            .unwrap()
            .is_none()
    );
    let catalog = DatasetCatalog::open(&f.config.data_dir).unwrap();
    let path = catalog
        .path_for(request.entry_id.as_ref().unwrap())
        .unwrap();
    assert_ne!(path, f.config.database_path);
    let stage = Store::open(&path).unwrap();
    assert!(stage.sync_identity().unwrap().is_none());
    assert!(stage.list_events().unwrap().is_empty());
    assert_eq!(pending_entries(&f.config).unwrap().len(), 1);
}

#[test]
fn stale_tokens_wrong_scope_and_owner_release_cannot_advance_or_close_a_join() {
    let f = Fixture::new();
    let request = prepared(&f);
    let key = [7; 32];
    assert!(install_key(&f.config, "stale", key.to_vec(), 1).is_err());
    let wrong = AppConfig {
        data_dir: f.config.data_dir.clone(),
        database_path: f.config.data_dir.join("wrong.db"),
    };
    assert!(install_key(&wrong, &request.token, key.to_vec(), 1).is_err());
    assert!(!wrong.database_path.exists());
    assert!(!super::super::release_request(
        &f.config,
        &request.operation_id
    ));
    let commit = install_key(&f.config, &request.token, key.to_vec(), 1).unwrap();
    assert!(confirm_key(&f.config, &request.token, 1).is_err());
    assert!(confirm_key(&f.config, &commit.token, 0).is_err());
    confirm_key(&f.config, &commit.token, 1).unwrap();
    assert!(release(&f.config, &request.operation_id));
    let next = preview(&f.config, &invitation()).unwrap();
    assert!(!release(&f.config, &request.operation_id));
    assert_eq!(
        state(&f.config, &next.operation_id).unwrap().phase,
        "preview"
    );
}

#[test]
fn active_owner_or_background_lease_prevents_joining_without_releasing_the_owner() {
    let f = Fixture::new();
    let (_, key) = f.initialize();
    super::super::confirm_key(&f.config, &key.token, 1).unwrap();
    assert!(preview(&f.config, &invitation()).is_err());
    assert!(super::super::unlocked(&f.config, 1));
    super::super::lock(&f.config);
    let plain = Fixture::new();
    let guard = super::super::background_guard(&plain.config, &plain.store)
        .unwrap()
        .unwrap();
    assert!(preview(&plain.config, &invitation()).is_err());
    drop(guard);
    assert!(preview(&plain.config, &invitation()).is_ok());
}

#[test]
fn unapproved_restart_cannot_reconstruct_noise_or_replace_existing_native_keys() {
    let f = Fixture::new();
    let request = prepared(&f);
    let key = [7; 32];
    let commit = install_key(&f.config, &request.token, key.to_vec(), 1).unwrap();
    confirm_key(&f.config, &commit.token, 1).unwrap();
    release(&f.config, &request.operation_id);
    let resume = prepare_resume(&f.config, request.entry_id.as_ref().unwrap()).unwrap();
    assert!(!resume.creating);
    assert!(install_key(&f.config, &resume.token, key.to_vec(), 1).is_err());
    assert!(send_proof(&f.config, &resume.operation_id).is_err());
    let catalog = DatasetCatalog::open(&f.config.data_dir).unwrap();
    let stage = Store::open(
        &catalog
            .path_for(request.entry_id.as_ref().unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        stage
            .load_sync_vault(request.installation_id.as_ref().unwrap(), &key, 1)
            .unwrap()
            .revision,
        1
    );
    assert!(stage.sync_identity().unwrap().is_none());
}

#[test]
fn changed_catalog_entry_is_rejected_before_native_key_installation() {
    let f = Fixture::new();
    let request = prepared(&f);
    let catalog = rusqlite::Connection::open(f.config.data_dir.join("sync-workspace.db")).unwrap();
    catalog
        .execute(
            "UPDATE datasets SET dataset_id='other-dataset' WHERE id=?1",
            [request.entry_id.as_ref().unwrap()],
        )
        .unwrap();
    assert!(install_key(&f.config, &request.token, vec![7; 32], 1).is_err());
    assert!(f.store.sync_identity().unwrap().is_none());
}

/// 终态必须由**本地事实**推导：快照已应用 + 本机名册已激活。
///
/// 真机 2026-10-28 10:10 的现场是「两个事实早就成立，界面还在转圈」，因为那时
/// 枚举里没有终态、也没有任何地方去推导它。这条用例把推导的两半都钉住：
/// 少任何一个事实都**不许**判完成（否则会把「没被授权」显示成「已完成」）。
#[test]
fn a_join_is_complete_on_local_facts_not_on_a_confirmation_frame() {
    let f = Fixture::new();
    // ① 空库：两个事实都不成立。
    assert!(!joined_locally(&f.store).unwrap());
    assert_eq!(
        terminal_phase(Phase::Enrollment, joined_locally(&f.store).unwrap()),
        Phase::Enrollment
    );

    // ② 只有「快照已应用」还不够 —— 名册没装好就等于还没被授权。
    f.store
        .connection
        .execute(
            "INSERT INTO sync_snapshots(id,source_device_id,key_version,state,transport) VALUES('snap','owner',1,'applied','blob')",
            [],
        )
        .unwrap();
    assert!(!joined_locally(&f.store).unwrap());
    assert_eq!(
        terminal_phase(Phase::Enrollment, joined_locally(&f.store).unwrap()),
        Phase::Enrollment
    );

    // ③ 名册把本机装成 active ⇒ 两个事实成立 ⇒ 终态。
    f.store
        .connection
        .execute_batch(
            "INSERT INTO sync_replica(singleton,dataset_id,device_id,stream_id,processor_device_id)\
                 VALUES(1,'ds','me','mine','owner');\
             INSERT INTO sync_member_keys(device_id,stream_id,signing_key,active)\
                 VALUES('me','mine',zeroblob(32),1);\
             INSERT INTO sync_members(device_id,stream_id,active) VALUES('me','mine',1);",
        )
        .unwrap();
    assert!(joined_locally(&f.store).unwrap());
    assert_eq!(terminal_phase(Phase::Enrollment, true), Phase::Joined);
    assert_eq!(
        terminal_phase(Phase::EnrollmentConfirmation, true),
        Phase::Joined
    );
    assert_eq!(terminal_phase(Phase::AwaitingMembership, true), Phase::Joined);
    // 幂等：再推导一次不会把终态推回去。
    assert_eq!(terminal_phase(Phase::Joined, true), Phase::Joined);
    // 中间相位之外一律不动：如实现在 `Ready` 就宣布完成，就等于跳过了批准与安装。
    assert_eq!(terminal_phase(Phase::Ready, true), Phase::Ready);
    assert_eq!(terminal_phase(Phase::AwaitingApproval, true), Phase::AwaitingApproval);
    assert_eq!(
        terminal_phase(Phase::InstallationConfirmation, true),
        Phase::InstallationConfirmation
    );

    // ④ 终态不会被截止时间渲染成「邀请已过期」；未进入交接的相位照旧会过期。
    assert!(!deadline_passed(Phase::Joined, 1));
    assert!(!deadline_passed(Phase::Enrollment, 1));
    assert!(deadline_passed(Phase::Preview, 1));
    assert!(!deadline_passed(Phase::Preview, 0));

    // ⑤ 名册被撤（active=0）⇒ 立刻不再是终态。
    f.store
        .connection
        .execute("UPDATE sync_members SET active=0", [])
        .unwrap();
    assert!(!joined_locally(&f.store).unwrap());
}
