use super::super::*;
use super::*;
use crate::storage::sync::SnapshotTransport;

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
        .due_enrollment_wires(&record.peer.device_id, now(), &[roster_id], None)
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
                &["provider:not-allowed".into()],
                None
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
        .begin_snapshot_export(&original, &record.peer.device_id, record.key_version, SnapshotTransport::Mail)
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
    for (_, wire) in &wires {
        f.receive("joiner", &STANDARD.encode(wire));
    }
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
        let receipts: Vec<(String, Vec<u8>)> = stage
            .connection
            .prepare(
                "SELECT message_id,encrypted_envelope FROM sync_wire_outbox WHERE recipient_device_id=?1 AND message_id LIKE 'envelope-receipt-%'",
            )
            .unwrap()
            .query_map([&owner_device], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
            })
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert!(!receipts.is_empty(), "joiner 应已为 roster/offer 诗回执");
        drop(stage);
        drop(joiner);
        receipts
            .into_iter()
            .map(|(_, bytes)| STANDARD.encode(bytes))
            .collect::<Vec<String>>()
    };
    for wire in &receipts {
        f.receive("owner", wire);
    }
    assert_eq!(f.state("owner").phase, "sending_snapshot");

    // Owner 端：分片全部入 outbox 但都未回执 → 进度 0/total。
    assert_eq!(f.state("owner").total_chunks, offer.chunks);
    assert_eq!(f.state("owner").received_chunks, 0);
    assert!(offer.chunks >= 2, "测试至少需要两个分片");

    // 收集 chunk 信封（message_id 形如 snapshot:{id}:{index}，不含 snapshot-end）。
    let (peer, chunk_ids): (String, Vec<String>) = {
        let owner = f.loan("owner");
        let store = owner.store().unwrap();
        let peer: String = store
            .connection
            .query_row(
                "SELECT recipient_device_id FROM sync_snapshot_exports WHERE id=?1",
                [&offer.snapshot_id],
                |r| r.get(0),
            )
            .unwrap();
        let ids: Vec<String> = store
            .connection
            .prepare(
                "SELECT message_id FROM sync_snapshot_export_wires WHERE snapshot_id=?1 AND message_id LIKE 'snapshot:%' ORDER BY message_id",
            )
            .unwrap()
            .query_map([&offer.snapshot_id], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        (peer, ids)
    };
    assert_eq!(chunk_ids.len() as i64, offer.chunks);
    let half = chunk_ids.len() / 2;
    {
        let owner = f.loan("owner");
        let store = owner.store().unwrap();
        for id in &chunk_ids[..half] {
            store
                .connection
                .execute(
                    "UPDATE sync_wire_outbox SET state='acknowledged' WHERE recipient_device_id=?1 AND message_id=?2 AND state='pending'",
                    rusqlite::params![peer, id],
                )
                .unwrap();
        }
    }

    // Owner 视图：已回执分片数即发送进度，仍小于总数。
    let mid = f.state("owner");
    assert_eq!(mid.total_chunks, offer.chunks);
    assert_eq!(mid.received_chunks as usize, half);
    assert!(mid.received_chunks < mid.total_chunks);

    // Joiner 端：实际接收一半分片 → received_chunks 等量增长。
    for id in &chunk_ids[..half] {
        f.receive("joiner", &f.wire(id));
    }
    let joiner_view = f.state("joiner");
    assert_eq!(joiner_view.phase, "receiving_snapshot");
    assert_eq!(joiner_view.total_chunks, offer.chunks);
    assert_eq!(joiner_view.received_chunks as usize, half);
    assert!(!joiner_view.local_applied);
}

/// 乱序到达的真实死锁：manifest 先到并被消费，最后一片 chunk 到齐才触发发布，
/// 于是回执的 subject 是那片 chunk。旧实现里这封回执既不在发送 scope 里
/// (`receipt_subjects` 写死四项)，owner 又拿 manifest 的哈希去比它——回执永远
/// 发不出去/对不上，导出停在 ready，两端都停在「等待成员授权与初始同步」。
#[test]
fn chunk_triggered_publication_still_delivers_a_receipt_and_completes() {
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
        let owner_device = record.peer.device_id.clone();
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
        drop(stage);
        drop(joiner);
        receipts
            .into_iter()
            .map(|bytes| STANDARD.encode(bytes))
            .collect()
    };
    for wire in &receipts {
        f.receive("owner", wire);
    }
    assert_eq!(f.state("owner").phase, "sending_snapshot");
    assert!(offer.chunks >= 2, "测试至少需要两个分片");

    let (peer, chunk_ids): (String, Vec<String>) = {
        let owner = f.loan("owner");
        let store = owner.store().unwrap();
        let peer: String = store
            .connection
            .query_row(
                "SELECT recipient_device_id FROM sync_snapshot_exports WHERE id=?1",
                [&offer.snapshot_id],
                |r| r.get(0),
            )
            .unwrap();
        let ids: Vec<String> = store
            .connection
            .prepare(
                "SELECT message_id FROM sync_snapshot_export_wires WHERE snapshot_id=?1 AND message_id LIKE 'snapshot:%' ORDER BY message_id",
            )
            .unwrap()
            .query_map([&offer.snapshot_id], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        (peer, ids)
    };

    // 1) manifest 先到：只被持久化，不发布、不回执。
    f.receive(
        "joiner",
        &f.wire(&format!("snapshot-end:{}", offer.snapshot_id)),
    );
    assert!(!f.state("joiner").local_applied);
    // 2) 分片走完最后一片才发布 → 回执的 subject 是那片 chunk。
    for id in &chunk_ids {
        f.receive("joiner", &f.wire(id));
    }
    let applied = f.state("joiner");
    assert!(applied.local_applied, "全部分片到齐即发布");
    assert_eq!(applied.phase, "awaiting_receipt");

    // 3) 回执必须真的进 outbox、并且落在发送 scope 内（旧实现两条都不满足）。
    let chunk_receipts: Vec<String> = chunk_ids
        .iter()
        .map(|id| {
            format!(
                "envelope-receipt-{}",
                crate::storage::sync::hash(id.as_bytes())
            )
        })
        .collect();
    // 3) 回执必须真的进 outbox、并且落在发送 scope 内。触发方是分片，但
    //    publish_and_receipt 用的是 manifest 的 header → subject 是 snapshot-end。
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
        let (scope, body) = transfer::outgoing_scope(&store, record).unwrap();
        assert!(body.is_none(), "joiner 不投递快照体，只发回执");
        let manifest_receipt = format!(
            "envelope-receipt-{}",
            crate::storage::sync::hash(
                format!("snapshot-end:{}", offer.snapshot_id).as_bytes()
            )
        );
        let pending: Vec<String> = store
            .connection
            .prepare("SELECT message_id FROM sync_wire_receipts WHERE target_device_id=?1 AND pending=1")
            .unwrap()
            .query_map([&record.peer.device_id], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert!(
            pending.contains(&format!("snapshot-end:{}", offer.snapshot_id)),
            "发布必须为 manifest 排队回执: {:?}",
            pending
        );
        assert!(
            !pending.iter().any(|id| chunk_ids.contains(id)),
            "分片不该各自排队回执（回执是整份快照级别的）: {:?}",
            pending
        );
        assert!(
            scope.contains(&manifest_receipt),
            "这封回执必须可发送: scope={:?}",
            scope
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

    // 4) owner 收下回执 → 整条导出线（含全部分片）一起退休，不再重发。
    f.receive("owner", &receipt);
    assert!(f.state("owner").remote_applied, "导出应已被确认");
    let remaining: i64 = {
        let owner = f.loan("owner");
        let store = owner.store().unwrap();
        store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM sync_wire_outbox WHERE recipient_device_id=?1 AND message_id LIKE 'snapshot%' AND state!='acknowledged'",
                [&peer],
                |r| r.get(0),
            )
            .unwrap()
    };
    assert_eq!(remaining, 0, "确认后不再重发任何分片");
    assert!(!chunk_receipts.is_empty());
}

