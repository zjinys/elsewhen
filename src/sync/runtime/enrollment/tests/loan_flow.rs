use super::super::*;
use super::*;

#[test]
fn native_roster_and_selected_offer_do_not_activate_or_overwrite_the_original_dataset() {
    let f = Pair::new();
    f.channels();
    let offer = f.export();
    let pending = f.receive("joiner", &f.wire(&f.roster()));
    assert!(pending.confirmation_token.is_some());
    assert!(
        begin(
            &f.joiner.config,
            "joiner",
            &pending.operation_id,
            &f.invitation
        )
        .is_err()
    );
    f.confirm(pending);
    let owner = f.loan("owner");
    let store = owner.store().unwrap();
    let vault = store
        .load_sync_vault(&owner.installation, &owner.master, owner.revision)
        .unwrap();
    let record = owner.record.as_ref().unwrap();
    let (key, _, _) = vault
        .peer_keys(&record.dataset_id, &record.peer.device_id)
        .unwrap();
    transfer::stage_offer(&store, record, &key, vault.identity(), false).unwrap();
    let offered_id = offer_id(&f.invitation, &offer.snapshot_id);
    let roster_id = transfer::snapshot_roster_id(&store, record)
        .unwrap()
        .unwrap();
    let only_roster = store
        .due_enrollment_wires(&record.peer.device_id, now(), &[roster_id])
        .unwrap();
    assert!(
        only_roster
            .iter()
            .all(|wire| wire.message_id.starts_with("membership:"))
    );
    assert!(
        store
            .due_enrollment_wires(
                &record.peer.device_id,
                now(),
                &["provider:not-allowed".into()]
            )
            .is_err()
    );
    drop(owner);
    let selected = f.receive("joiner", &f.wire(&offered_id));
    assert!(selected.confirmation_token.is_some());
    f.confirm(selected);
    let duplicate = f.receive("joiner", &f.wire(&offered_id));
    assert!(duplicate.confirmation_token.is_none());
    let joiner = f.loan("joiner");
    let mut changed = offer.clone();
    changed.snapshot_id = uuid::Uuid::new_v4().to_string();
    assert!(
        joiner
            .store()
            .unwrap()
            .select_enrollment_snapshot(
                &joiner.installation,
                &joiner.master,
                joiner.revision,
                &joiner.invitation,
                &changed,
                &serde_json::to_vec(&changed).unwrap()
            )
            .is_err()
    );
    assert_eq!(
        joiner
            .record
            .as_ref()
            .unwrap()
            .snapshot
            .as_ref()
            .unwrap()
            .id,
        offer.snapshot_id
    );
    assert!(joiner.store().unwrap().provider_keys_sealed().unwrap());
    assert!(f.joiner.store.sync_identity().unwrap().is_none());
    assert_eq!(
        f.joiner
            .store
            .active_ai_provider_config()
            .unwrap()
            .unwrap()
            .model,
        "original-model"
    );
    assert!(
        crate::sync::datasets::selected_database_path(&f.joiner.config.data_dir)
            .unwrap()
            .is_none()
    );
}

#[test]
fn incomplete_export_gets_a_new_id_but_a_ready_export_is_not_regenerated() {
    let f = Pair::new();
    f.channels();
    f.confirm(f.mutate("owner", |loan, store| {
        store.reserve_enrollment_snapshot(
            &loan.installation,
            &loan.master,
            loan.revision,
            &loan.invitation,
        )
    }));
    let original = f.state("owner").snapshot_id.unwrap();
    let loan = f.loan("owner");
    let record = loan.record.as_ref().unwrap();
    loan.store()
        .unwrap()
        .begin_snapshot_export(&original, &record.peer.device_id, record.key_version)
        .unwrap();
    drop(loan);
    f.confirm(f.mutate("owner", |loan, store| {
        store.reserve_enrollment_snapshot(
            &loan.installation,
            &loan.master,
            loan.revision,
            &loan.invitation,
        )
    }));
    let next = f.state("owner").snapshot_id.unwrap();
    assert_ne!(next, original);
    let offer = f.export();
    assert_eq!(offer.snapshot_id, next);
    let loan = f.loan("owner");
    let no_change = loan
        .store()
        .unwrap()
        .reserve_enrollment_snapshot(
            &loan.installation,
            &loan.master,
            loan.revision,
            &loan.invitation,
        )
        .unwrap();
    assert_eq!(no_change.vault_revision, loan.revision);
}

#[test]
fn cancelled_loan_cannot_mutate_a_new_session_and_keeps_its_file_lease() {
    let f = Pair::new();
    f.confirm(f.accept_proof());
    let loan = f.loan("owner");
    super::super::lock(&f.owner.config);
    assert!(loan.check().is_err());
    assert!(super::super::prepare_key(&f.owner.config, None, None).is_err());
    assert!(
        loan.mutate(|store| store.confirm_enrollment_grant(
            &loan.installation,
            &loan.master,
            loan.revision,
            &loan.invitation,
            1
        ))
        .is_err()
    );
    drop(loan);
    let request = super::super::prepare_key(&f.owner.config, None, None).unwrap();
    let key = super::super::install_key(&f.owner.config, &request.token, f.owner_key.to_vec(), 3)
        .unwrap();
    super::super::confirm_key(&f.owner.config, &key.token, key.revision).unwrap();
    assert!(!f.loan("owner").record.as_ref().unwrap().receive_ready);
}

#[test]
fn selected_snapshot_joiner_restores_with_the_original_key_and_a_new_operation() {
    let f = Pair::new();
    f.channels();
    let offer = f.export();
    f.select(&offer);
    let prior = f.state("joiner");
    joining::release(&f.joiner.config, &prior.operation_id);
    let request = joining::prepare_resume(&f.joiner.config, &f.entry).unwrap();
    assert!(!request.creating);
    assert!(
        joining::install_key(
            &f.joiner.config,
            &request.token,
            vec![0; 32],
            prior.revision
        )
        .is_err()
    );
    let key = joining::install_key(
        &f.joiner.config,
        &request.token,
        f.joiner_key.to_vec(),
        prior.revision,
    )
    .unwrap();
    joining::confirm_key(&f.joiner.config, &key.token, key.revision).unwrap();
    let resumed = f.state("joiner");
    assert_ne!(resumed.operation_id, prior.operation_id);
    assert_eq!(resumed.snapshot_id, prior.snapshot_id);
    assert_eq!(resumed.phase, "receiving_snapshot");
    assert!(
        begin(
            &f.joiner.config,
            "joiner",
            &prior.operation_id,
            &f.invitation
        )
        .is_err()
    );
}

#[test]
fn snapshot_preserves_keys_and_waits_for_the_authenticated_final_receipt() {
    let f = Pair::new();
    f.channels();
    let offer = f.export();
    f.select(&offer);
    let owner = f.loan("owner");
    let store = owner.store().unwrap();
    let mut query = store.connection.prepare("SELECT w.message_id,w.encrypted_envelope FROM sync_wire_outbox w JOIN sync_snapshot_export_wires x ON x.recipient_device_id=w.recipient_device_id AND x.message_id=w.message_id WHERE x.snapshot_id=?1 AND (w.message_id LIKE 'snapshot:%' OR w.message_id LIKE 'snapshot-end:%') ORDER BY w.message_id").unwrap();
    let wires: Vec<(String, Vec<u8>)> = query
        .query_map([&offer.snapshot_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    drop(query);
    drop(owner);
    // 快照线只有一封：manifest。分片走 blob 数据面，由接收端主动拉 —— 只收
    // 信令不会发布（否则等于「拿半份数据启动」）。
    f.receive(
        "joiner",
        &f.wire(&format!("snapshot-start:{}", offer.snapshot_id)),
    );
    for (_, wire) in &wires {
        f.receive("joiner", &STANDARD.encode(wire));
    }
    assert!(!f.state("joiner").local_applied, "manifest 单独到不能发布");
    assert!(f.import("joiner", &f.plane, 8), "分片拉齐即发布");
    assert!(f.state("joiner").local_applied);
    assert!(!f.state("joiner").remote_applied);
    assert!(!f.state("owner").remote_applied);
    let joiner = f.loan("joiner");
    let stage = joiner.store().unwrap();
    assert_eq!(stage.list_events().unwrap().len(), 1);
    assert_eq!(
        stage.active_ai_provider_config().unwrap().unwrap().api_key,
        "SYNTHETIC_SHARED_KEY"
    );
    assert!(stage.claim_analysis_job().unwrap().is_none());
    let vault = stage
        .load_sync_vault(&joiner.installation, &joiner.master, joiner.revision)
        .unwrap();
    let record = joiner.record.as_ref().unwrap();
    let (send, _, _) = vault
        .peer_keys(&record.dataset_id, &record.peer.device_id)
        .unwrap();
    transfer::stage_receipts(&stage, record, &send, vault.identity()).unwrap();
    let subject = format!("snapshot-end:{}", offer.snapshot_id);
    let receipt_id = format!(
        "envelope-receipt-{}",
        crate::storage::sync::hash(subject.as_bytes())
    );
    let receipt: Vec<u8> = stage.connection.query_row("SELECT encrypted_envelope FROM sync_wire_outbox WHERE recipient_device_id=?1 AND message_id=?2",
        rusqlite::params![record.peer.device_id, receipt_id], |r| r.get(0)).unwrap();
    drop(joiner);
    f.receive("owner", &STANDARD.encode(receipt));
    assert!(f.state("owner").remote_applied);
    let pending = f.mutate("owner", |loan, store| {
        store.complete_enrollment(
            &loan.installation,
            &loan.master,
            loan.revision,
            &loan.invitation,
            None,
        )
    });
    f.confirm(pending);
    let owner = f.loan("owner");
    let vault = owner
        .store()
        .unwrap()
        .load_sync_vault(&owner.installation, &owner.master, owner.revision)
        .unwrap();
    let record = owner.record.as_ref().unwrap();
    let (send, _, _) = vault
        .peer_keys(&record.dataset_id, &record.peer.device_id)
        .unwrap();
    transfer::stage_offer(
        &owner.store().unwrap(),
        record,
        &send,
        vault.identity(),
        true,
    )
    .unwrap();
    let completion = completion_id(&f.invitation, &offer.snapshot_id);
    owner
        .store()
        .unwrap()
        .record_sync_wire_attempt(
            &record.peer.device_id,
            &completion,
            now(),
            Some("synthetic-relay-id"),
            None,
        )
        .unwrap();
    drop(owner);
    assert!(
        f.state("owner").notification_pending,
        "relay delivery is not the final application receipt"
    );
    let done = f.receive("joiner", &f.wire(&completion));
    f.confirm(done);
    assert!(f.state("joiner").remote_applied);
    assert_eq!(f.state("joiner").phase, "complete");
    let catalog = DatasetCatalog::open(&f.joiner.config.data_dir).unwrap();
    assert!(
        !catalog
            .list()
            .unwrap()
            .iter()
            .find(|e| e.id == f.entry)
            .unwrap()
            .ready
    );
    assert_eq!(f.joiner.store.list_events().unwrap().len(), 1);
    assert_eq!(
        f.joiner
            .store
            .active_ai_provider_config()
            .unwrap()
            .unwrap()
            .api_key,
        "SYNTHETIC_ORIGINAL_KEY"
    );
}

#[test]
fn snapshot_transfer_surfaces_progress_to_both_sides() {
    let f = Pair::new();
    f.channels();
    // 只有本测试需要跨多个 256KB 分片的快照体；其余用例依赖单事件 fixture。
    for i in 0..700 {
        let payload = format!("bulk-{}-{}", i, "x".repeat(1024));
        f.owner
            .store
            .submit_input_as_event(&payload, "capture", None)
            .unwrap();
    }
    let offer = f.export();
    f.select(&offer);
    assert!(offer.chunks >= 2, "测试至少需要两个分片");

    // 真实流程：joiner 收到 roster/offer 后产生回执，owner 收到回执后
    // roster/offer 置为 acknowledged，phase 才推进到 sending_snapshot。
    let receipts: Vec<String> = {
        let joiner = f.loan("joiner");
        let stage = joiner.store().unwrap();
        let vault = stage
            .load_sync_vault(&joiner.installation, &joiner.master, joiner.revision)
            .unwrap();
        let record = joiner.record.as_ref().unwrap();
        let (send, _, _) = vault
            .peer_keys(&record.dataset_id, &record.peer.device_id)
            .unwrap();
        transfer::stage_receipts(&stage, record, &send, vault.identity()).unwrap();
        let owner_device: String = record.peer.device_id.clone();
        let receipts: Vec<Vec<u8>> = stage
            .connection
            .prepare(
                "SELECT encrypted_envelope FROM sync_wire_outbox WHERE recipient_device_id=?1 AND message_id LIKE 'envelope-receipt-%'",
            )
            .unwrap()
            .query_map([&owner_device], |r| r.get::<_, Vec<u8>>(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert!(!receipts.is_empty(), "joiner 应已为 roster/offer 诗回执");
        drop(stage);
        drop(joiner);
        receipts
            .into_iter()
            .map(|value| STANDARD.encode(value))
            .collect::<Vec<String>>()
    };
    for wire in &receipts {
        f.receive("owner", wire);
    }
    assert_eq!(f.state("owner").phase, "sending_snapshot");

    // Owner 端：分片已经全部在 relay 上（上传是同步做完的），进度就是 total。
    // 换句话说，「还差几片」这个数字在 owner 侧恒等于 0 —— 剩下等的是回执。
    let seen = f.state("owner");
    assert_eq!(seen.total_chunks, offer.chunks);
    assert_eq!(seen.received_chunks, seen.total_chunks);

    // Joiner 端：眼下 relay 上只有一半（模拟窗口没拉完），进度必须如实反映。
    let blob_id: String = {
        let owner = f.loan("owner");
        let store = owner.store().unwrap();
        store
            .connection
            .query_row(
                "SELECT blob_id FROM sync_snapshot_exports WHERE id=?1",
                [&offer.snapshot_id],
                |r| r.get(0),
            )
            .unwrap()
    };
    let half = (offer.chunks as usize) / 2;
    assert!(half > 0);
    f.plane.truncate(&blob_id, half);
    f.receive(
        "joiner",
        &f.wire(&format!("snapshot-start:{}", offer.snapshot_id)),
    );
    assert!(!f.import("joiner", &f.plane, 1));
    let joiner_view = f.state("joiner");
    assert_eq!(joiner_view.phase, "receiving_snapshot");
    assert_eq!(joiner_view.total_chunks, offer.chunks);
    assert_eq!(joiner_view.received_chunks as usize, half);
    assert!(!joiner_view.local_applied);
}

/// 端到端（不联网）：manifest 先到 → 分片从 blob 拉齐才发布 → 回执到达才收口。
///
/// 这条链路就是 2026-10-24 那次真机死锁的全部关键节点：
/// ① manifest 早于分片到达时只能被暂存，不能发布（否则接收方会带着半份数据启动）；
/// ② 只有 manifest 能退休一份导出（分片不再有逐片回执）；
/// ③ 回执必须真的进 outbox、落在发送 scope 内，否则导出永远停在 `ready`。
#[test]
fn blob_snapshot_publishes_only_when_complete_and_completes_on_the_manifest_receipt() {
    let f = Pair::new();
    f.channels();
    for i in 0..700 {
        let payload = format!("bulk-{}-{}", i, "x".repeat(1024));
        f.owner
            .store
            .submit_input_as_event(&payload, "capture", None)
            .unwrap();
    }
    let offer = f.export();
    f.select(&offer);
    assert!(offer.chunks >= 2, "测试至少需要两个分片");

    let peer: String = {
        let owner = f.loan("owner");
        owner
            .store()
            .unwrap()
            .connection
            .query_row(
                "SELECT recipient_device_id FROM sync_snapshot_exports WHERE id=?1",
                [&offer.snapshot_id],
                |r| r.get(0),
            )
            .unwrap()
    };
    let blob_id: String = {
        let owner = f.loan("owner");
        owner
            .store()
            .unwrap()
            .connection
            .query_row(
                "SELECT blob_id FROM sync_snapshot_exports WHERE id=?1",
                [&offer.snapshot_id],
                |r| r.get(0),
            )
            .unwrap()
    };

    // 1) manifest 先到：只被持久化，不发布。
    f.receive(
        "joiner",
        &f.wire(&format!("snapshot-start:{}", offer.snapshot_id)),
    );
    f.receive(
        "joiner",
        &f.wire(&format!("snapshot-end:{}", offer.snapshot_id)),
    );
    assert!(!f.state("joiner").local_applied, "manifest 单独到不能发布");

    // 2) relay 上只有一半：拉一窗也发布不了。
    let half = (offer.chunks as usize) / 2;
    f.plane.truncate(&blob_id, half);
    assert_eq!(
        f.import("joiner", &f.plane, 8),
        false,
        "缺片时 import 必须返回 false"
    );
    assert!(!f.state("joiner").local_applied);

    // 3) 剩下的片补上去 → 全齐就发布。
    f.plane.restore(&blob_id);
    assert!(
        f.import("joiner", &f.plane, 8),
        "片齐了必须发布（返回 true）"
    );
    let applied = f.state("joiner");
    assert!(applied.local_applied, "全部分片到齐即发布");

    // 4) 只有一条回执：manifest 那条。分片不产生回执（那是删除的 mail 路径的
    //    造物，也是每拍重传整份快照的根源）。
    let pending_receipts: Vec<String> = {
        let joiner = f.loan("joiner");
        let store = joiner.store().unwrap();
        store
            .connection
            .prepare("SELECT message_id FROM sync_wire_receipts WHERE pending=1")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    };
    assert!(
        pending_receipts.contains(&format!("snapshot-end:{}", offer.snapshot_id)),
        "发布必须为 manifest 排队回执: {pending_receipts:?}"
    );
    assert!(
        pending_receipts
            .iter()
            .all(|id| !id.starts_with("snapshot:")),
        "分片不许再有逐片回执: {pending_receipts:?}"
    );

    // 5) 回执回到 owner → 整条导出线一起退休。
    let receipt = {
        let joiner = f.loan("joiner");
        let store = joiner.store().unwrap();
        let vault = store
            .load_sync_vault(&joiner.installation, &joiner.master, joiner.revision)
            .unwrap();
        let record = joiner.record.as_ref().unwrap();
        let (send, _, _) = vault
            .peer_keys(&record.dataset_id, &record.peer.device_id)
            .unwrap();
        transfer::stage_receipts(&store, record, &send, vault.identity()).unwrap();
        let scope = transfer::outgoing_scope(&store, record).unwrap();
        let manifest_receipt = format!(
            "envelope-receipt-{}",
            crate::storage::sync::hash(format!("snapshot-end:{}", offer.snapshot_id).as_bytes())
        );
        assert!(
            scope.contains(&manifest_receipt),
            "这封回执必须可发送: scope={scope:?}"
        );
        let bytes: Vec<u8> = store
            .connection
            .query_row(
                "SELECT encrypted_envelope FROM sync_wire_outbox WHERE recipient_device_id=?1 AND message_id=?2",
                rusqlite::params![record.peer.device_id, manifest_receipt],
                |r| r.get(0),
            )
            .unwrap();
        drop(joiner);
        STANDARD.encode(bytes)
    };
    f.receive("owner", &receipt);
    assert!(f.state("owner").remote_applied, "导出应已被确认");
    let remaining: i64 = {
        let owner = f.loan("owner");
        let store = owner.store().unwrap();
        store
            .connection
            .query_row(
                // `snapshot-start:` 有自己的回执（通知类信号，由它自己的回执退役）；
                // 这里守的是「导出线一封都不再重发」。
                "SELECT COUNT(*) FROM sync_wire_outbox WHERE recipient_device_id=?1 AND message_id LIKE 'snapshot%' AND message_id NOT LIKE 'snapshot-start:%' AND state!='acknowledged'",
                [&peer],
                |r| r.get(0),
            )
            .unwrap()
    };
    assert_eq!(remaining, 0, "确认后不再重发任何快照信令");
}

/// blob 数据面下 `snapshot-start` 会**先于** offer 到达。这不是乱序，而是唯一
/// 可行的顺序：offer 带的是 manifest 摘要（最后一片传完才有），而最后一片传完
/// 要接收端 ack，接收端要先拿到开场信令才敢去拉 —— 压着信令等 offer 就是互等，
/// 真机现场就停在「等待成员授权与初始同步」。
#[test]
fn snapshot_start_before_the_offer_stages_chunks_without_publishing() {
    let f = Pair::new();
    f.channels();
    let offer = f.export();
    // 名单照旧先到（信令面）；此刻 joiner 手里还没有 offer。
    let pending = f.receive("joiner", &f.wire(&f.roster()));
    f.confirm(pending);
    let started = f.receive(
        "joiner",
        &f.wire(&format!("snapshot-start:{}", offer.snapshot_id)),
    );
    assert!(
        started.snapshot_id.is_none(),
        "开场信令不能替 offer 选中快照"
    );
    assert!(!started.local_applied);
    {
        let joiner = f.loan("joiner");
        let store = joiner.store().unwrap();
        assert!(
            store.snapshot_import_pending().unwrap(),
            "开场信令必须留下一个待拉取的 blob"
        );
    }
    // 分片能拉：这正是发送端把导出跑成 `ready` 的前提。
    f.import("joiner", &f.plane, 8);
    assert!(!f.state("joiner").local_applied, "没有 offer 谁也不许发布");
    // offer 迟到也收敛到同一份：先选中，manifest 到了才发布。
    let owner = f.loan("owner");
    let store = owner.store().unwrap();
    let vault = store
        .load_sync_vault(&owner.installation, &owner.master, owner.revision)
        .unwrap();
    let record = owner.record.as_ref().unwrap();
    let (key, _, _) = vault
        .peer_keys(&record.dataset_id, &record.peer.device_id)
        .unwrap();
    transfer::stage_offer(&store, record, &key, vault.identity(), false).unwrap();
    drop(owner);
    let selected = f.receive(
        "joiner",
        &f.wire(&offer_id(&f.invitation, &offer.snapshot_id)),
    );
    assert_eq!(
        selected.snapshot_id.as_deref(),
        Some(offer.snapshot_id.as_str())
    );
    f.confirm(selected);
    let applied = f.receive(
        "joiner",
        &f.wire(&format!("snapshot-end:{}", offer.snapshot_id)),
    );
    assert!(applied.local_applied, "manifest 到了才发布");
    let joiner = f.loan("joiner");
    let store = joiner.store().unwrap();
    assert_eq!(store.list_events().unwrap().len(), 1);
}

/// 真机 2026-10-27 的现场形状：接收端把分片**全部拉完并且 ack 干净**之后，
/// manifest（`snapshot-end:`）才落库。
///
/// 这时 relay 上已经拉不到任何窗口（`get_window` 返回 `from=None`），而天真的
/// 实现只在「ack 完一窗之后」才问一次「齐了没」—— 于是 `break` 把发布那一段
/// 整段跳过，此后每一拍都重复同一个 `break`，再没有任何地方会调
/// `publish_and_receipt`：手机停在 174/174、桌面 `snapshot-end:` 永远 pending。
///
/// 判据：「能不能发布」必须每拍从**状态**重新算出来，而不是只挂在某次事件
/// （`ack` 成功）的回调里。
#[test]
fn chunks_that_all_arrive_before_the_manifest_still_publish() {
    let f = Pair::new();
    f.channels();
    for i in 0..700 {
        let payload = format!("bulk-{}-{}", i, "x".repeat(1024));
        f.owner
            .store
            .submit_input_as_event(&payload, "capture", None)
            .unwrap();
    }
    let offer = f.export();
    f.select(&offer);
    assert!(offer.chunks >= 2, "测试至少需要两个分片");
    let blob_id: String = {
        let owner = f.loan("owner");
        owner
            .store()
            .unwrap()
            .connection
            .query_row(
                "SELECT blob_id FROM sync_snapshot_exports WHERE id=?1",
                [&offer.snapshot_id],
                |r| r.get(0),
            )
            .unwrap()
    };

    // 1) 只有开场信令：接收端知道 blob 在哪，但还没有 manifest。
    f.receive(
        "joiner",
        &f.wire(&format!("snapshot-start:{}", offer.snapshot_id)),
    );

    // 2) 分片分窗拉完并逐窗 ack —— relay 上下一步就什么都没有了。
    for _ in 0..16 {
        if f.plane.live_chunks(&blob_id) == 0 {
            break;
        }
        assert!(
            !f.import("joiner", &f.plane, 8),
            "manifest 还没到，不许发布"
        );
    }
    assert_eq!(f.plane.live_chunks(&blob_id), 0, "分片应已被逐窗 ack 释放");
    assert!(!f.state("joiner").local_applied, "没有 manifest 不能发布");

    // 3) manifest 迟到（真机上比最后一片晚约 20 s）：接受这一封信的那一拍，
    //    片已经齐了，就必须当场发布 —— 不能指望「下一拍再拉一片顺便检查」，
    //    因为 relay 上已经无片可拉，`import_blob_snapshots` 会直接 `break`。
    let applied = f.receive(
        "joiner",
        &f.wire(&format!("snapshot-end:{}", offer.snapshot_id)),
    );
    assert!(
        applied.local_applied,
        "片已齐时，manifest 到达那一拍就要发布"
    );
    assert!(f.state("joiner").local_applied, "全部分片到齐即发布");
    // 4) 再跑一拍是幂等的：不重复发布，也不会把已发布的快照弄坏。
    assert!(!f.import("joiner", &f.plane, 2), "已发布就不再算待导入");
    assert!(f.state("joiner").local_applied);
    let pending: Vec<String> = {
        let joiner = f.loan("joiner");
        joiner
            .store()
            .unwrap()
            .connection
            .prepare("SELECT message_id FROM sync_wire_receipts WHERE pending=1")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    };
    assert!(
        pending.contains(&format!("snapshot-end:{}", offer.snapshot_id)),
        "发布必须为 manifest 排队回执: {pending:?}"
    );
}
