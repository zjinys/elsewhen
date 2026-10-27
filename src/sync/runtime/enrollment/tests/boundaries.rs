use super::*;
use crate::sync::{
    crypto::{self, DeviceIdentity, EnvelopeHeader},
    transport_relay::{RelayEnvelope, RelayPage},
};

fn inject_checkpoint(store: &Store, vault: &mut LocalVault, master: &[u8; 32]) {
    // Test-only old-format/corrupt-state injection, never a production transition.
    vault.revision += 1;
    store
        .connection
        .execute(
            "UPDATE sync_local_vault SET revision=?1,encrypted_secrets=?2 WHERE installation_id=?3",
            rusqlite::params![
                vault.revision,
                vault.seal(master).unwrap(),
                vault.installation_id
            ],
        )
        .unwrap();
}

#[test]
fn relay_permission_and_revision_rejections_are_not_transient_retries() {
    use crate::sync::transport_relay::RelayHttpError;
    for status in [401, 403, 409] {
        let error = RelayHttpError(reqwest::StatusCode::from_u16(status).unwrap());
        assert_eq!(
            grant_failure(error.into(), "retry").to_string(),
            "sync_enrollment_authorization_changed_requires_repair"
        );
    }
    let error = RelayHttpError(reqwest::StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        grant_failure(error.into(), "retry-original").to_string(),
        "retry-original"
    );
}

fn page(sender: &str, bodies: &[Vec<u8>]) -> RelayPage {
    let envelopes: Vec<_> = bodies
        .iter()
        .enumerate()
        .map(|(i, body)| RelayEnvelope {
            id: format!("{:016}", i + 1),
            sender_mb: sender.into(),
            seq_hint: 0,
            ciphertext: STANDARD.encode(body),
            created_at: now(),
            expires_at: now() + 30_000,
        })
        .collect();
    RelayPage {
        next_cursor: envelopes.last().unwrap().id.clone(),
        envelopes,
        has_more: false,
    }
}

#[test]
fn invalid_proof_does_not_pin_the_cursor_or_ack_unrelated_mail() {
    let f = Pair::new();
    let loan = f.loan("owner");
    let identity = loan.store().unwrap().sync_identity().unwrap().unwrap();
    let vault = loan
        .store()
        .unwrap()
        .load_sync_vault(&loan.installation, &loan.master, loan.revision)
        .unwrap();
    let pairing = vault.pairing(&f.invitation).unwrap();
    let header = EnvelopeHeader::new(
        &identity.dataset_id,
        &pairing.peer.device_id,
        &identity.device_id,
        pairing.key_version,
        format!("pair-complete:{}", f.invitation),
    )
    .unwrap();
    let bad = crypto::seal(
        header,
        &ContentKey::generate().unwrap(),
        &DeviceIdentity::generate().unwrap(),
        b"untrusted proof",
    )
    .unwrap()
    .to_bytes()
    .unwrap();
    accept_proof_page(
        &loan,
        page(&loan.bootstrap.request_sender, &[bad, f.proof.clone()]),
    )
    .unwrap();
    assert_eq!(
        loan.store()
            .unwrap()
            // enrollment 扫描有独立游标键：普通增量扫描会把共用行写成 "0" 回卷它。
            .relay_scan_cursor(&super::super::flow::scan_key(
                &loan.bootstrap.issuer_mailbox
            ))
            .unwrap(),
        "0000000000000002"
    );
    let commit = loan.result().unwrap();
    assert!(commit.confirmation_token.is_some());
    assert!(loan.check().is_err());
    drop(loan);
    f.confirm(commit);
}

#[test]
fn proof_storage_failure_is_reported_without_advancing_the_cursor() {
    let f = Pair::new();
    let loan = f.loan("owner");
    let store = loan.store().unwrap();
    store.connection.execute_batch("CREATE TRIGGER fail_proof BEFORE UPDATE ON sync_local_vault BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;").unwrap();
    let error = accept_proof_page(
        &loan,
        page(&loan.bootstrap.request_sender, &[f.proof.clone()]),
    )
    .unwrap_err();
    assert!(error.is::<rusqlite::Error>());
    assert_eq!(
        store
            .relay_scan_cursor(&loan.bootstrap.issuer_mailbox)
            .unwrap(),
        "0"
    );
    assert_eq!(f.state("owner").revision, loan.revision);
    store
        .connection
        .execute_batch("DROP TRIGGER fail_proof")
        .unwrap();
    accept_proof_page(
        &loan,
        page(&loan.bootstrap.request_sender, &[f.proof.clone()]),
    )
    .unwrap();
}

#[test]
fn enrollment_native_confirmation_never_recovers_a_live_background_job() {
    let f = Pair::new();
    let pending = f.accept_proof();
    let job = f.owner.store.claim_analysis_job().unwrap().unwrap();
    assert!(
        confirm(
            &f.owner.config,
            "owner",
            &pending.operation_id,
            &f.invitation,
            pending.confirmation_token.as_ref().unwrap(),
            pending.revision
        )
        .is_err()
    );
    assert_eq!(f.owner.store.analysis_job_stats().unwrap().running, 1);
    assert!(!super::super::super::unlocked(
        &f.owner.config,
        pending.revision
    ));
    f.owner
        .store
        .fail_analysis(&job, "synthetic finished attempt")
        .unwrap();
    f.confirm(pending);
}

#[test]
fn original_deadline_gates_first_proof_but_not_an_authorized_handoff() {
    let f = Pair::new();
    let loan = f.loan("owner");
    let mut vault = loan
        .store()
        .unwrap()
        .load_sync_vault(&loan.installation, &loan.master, loan.revision)
        .unwrap();
    let pending = vault.pairing(&f.invitation).unwrap();
    assert!(
        pending
            .confirmed_pair(&f.proof, pending.expires_at_ms)
            .is_err()
    );
    drop(loan);
    f.confirm(f.accept_proof());
    let state = f.state("owner");
    let store = Store::open(&f.owner.config.database_path)
        .unwrap()
        .with_provider_master(f.owner_key);
    // Change only the already-consumed invitation deadline; the frozen formal
    // grant remains unchanged. No network or real wall-clock wait is needed.
    vault = store
        .load_sync_vault(&state.installation_id, &f.owner_key, state.revision)
        .unwrap();
    let grant = vault.enrollment(&f.invitation).unwrap().receive.clone();
    vault.pairing_mut(&f.invitation).unwrap().expires_at_ms = now() - 1;
    inject_checkpoint(&store, &mut vault, &f.owner_key);
    super::super::super::lock(&f.owner.config);
    let request = super::super::super::prepare_key(&f.owner.config, None, None).unwrap();
    let restored = super::super::super::install_key(
        &f.owner.config,
        &request.token,
        f.owner_key.to_vec(),
        state.revision,
    )
    .unwrap();
    super::super::super::confirm_key(&f.owner.config, &restored.token, restored.revision).unwrap();
    let loan = f.loan("owner");
    assert_eq!(loan.record.as_ref().unwrap().receive, grant);
    assert!(loan.check().is_ok());
    assert!(
        loan.record
            .as_ref()
            .unwrap()
            .require_live(grant.expires_at_ms)
            .is_err()
    );
}

#[test]
fn legacy_missing_bootstrap_is_visible_but_cannot_invent_revocation_metadata() {
    let f = Pair::new();
    let state = f.state("owner");
    let mut vault = f
        .owner
        .store
        .load_sync_vault(&state.installation_id, &f.owner_key, state.revision)
        .unwrap();
    vault.pairing_mut(&f.invitation).unwrap().bootstrap = None;
    inject_checkpoint(&f.owner.store, &mut vault, &f.owner_key);
    super::super::super::lock(&f.owner.config);
    let request = super::super::super::prepare_key(&f.owner.config, None, None).unwrap();
    let restored = super::super::super::install_key(
        &f.owner.config,
        &request.token,
        f.owner_key.to_vec(),
        state.revision,
    )
    .unwrap();
    super::super::super::confirm_key(&f.owner.config, &restored.token, restored.revision).unwrap();
    let state = f.state("owner");
    assert_eq!(state.phase, "requires_repair");
    assert!(begin(&f.owner.config, "owner", &state.operation_id, &f.invitation).is_err());
}

#[test]
fn original_revoke_intent_cannot_be_retargeted_to_a_peer_mailbox() {
    let f = Pair::new();
    f.confirm(f.accept_proof());
    let loan = f.loan("owner");
    let mut record = loan.record.clone().unwrap();
    let revoke = record.revoke.as_ref().unwrap();
    assert_eq!(revoke.sender, record.bootstrap.request_sender);
    assert_eq!(revoke.expected_revision, 1);
    assert_ne!(revoke.intent_id, record.receive.intent_id);
    record.revoke.as_mut().unwrap().sender = record.route.peer_mailbox.clone();
    assert!(record.validate().is_err());
    record = loan.record.clone().unwrap();
    record.receive.expected_revision += 1;
    assert!(record.validate().is_err());
    record = loan.record.clone().unwrap();
    record.key_version += 1;
    let store = loan.store().unwrap();
    let mut vault = store
        .load_sync_vault(&loan.installation, &loan.master, loan.revision)
        .unwrap();
    *vault.enrollment_mut(&f.invitation).unwrap() = record;
    vault.revision += 1;
    if let Ok(ciphertext) = vault.seal(&loan.master) {
        store
            .connection
            .execute(
                "UPDATE sync_local_vault SET revision=?1,encrypted_secrets=?2",
                rusqlite::params![vault.revision, ciphertext],
            )
            .unwrap();
        assert!(
            store
                .checked_enrollment(
                    &loan.installation,
                    &loan.master,
                    vault.revision,
                    &f.invitation
                )
                .is_err()
        );
    }
}

#[test]
fn production_send_scope_requires_both_receipts_before_snapshot_chunks() {
    let f = Pair::new();
    f.channels();
    let offer = f.export();
    let loan = f.loan("owner");
    let store = loan.store().unwrap();
    let vault = store
        .load_sync_vault(&loan.installation, &loan.master, loan.revision)
        .unwrap();
    let record = loan.record.as_ref().unwrap();
    let roster = transfer::snapshot_roster_id(&store, record)
        .unwrap()
        .unwrap();
    let (send, _, _) = vault
        .peer_keys(&record.dataset_id, &record.peer.device_id)
        .unwrap();
    transfer::stage_offer(&store, record, &send, vault.identity(), false).unwrap();
    let ids = transfer::outgoing_scope(&store, record).unwrap();
    assert_eq!(ids, vec![roster.clone()]);
    let ack = |message: &str| {
        let hash: String = store.connection.query_row("SELECT payload_hash FROM sync_wire_outbox WHERE recipient_device_id=?1 AND message_id=?2",
            rusqlite::params![record.peer.device_id, message], |r| r.get(0)).unwrap();
        store
            .acknowledge_sync_wire(&record.peer.device_id, message, &hash)
            .unwrap();
    };
    ack(&roster);
    let ids = transfer::outgoing_scope(&store, record).unwrap();
    assert!(ids.contains(&offer_id(&f.invitation, &offer.snapshot_id)));
    ack(&offer_id(&f.invitation, &offer.snapshot_id));
    let ids = transfer::outgoing_scope(&store, record).unwrap();
    assert!(ids.contains(&format!("snapshot-end:{}", offer.snapshot_id)));
    let due = store
        .due_enrollment_wires(&record.peer.device_id, now(), &ids)
        .unwrap();
    assert!(!due.is_empty());
    // 快照线只有两条信令：`snapshot-start`（blob 在哪）+ `snapshot-end`（manifest）。
    assert!(due.iter().all(|w| {
        w.message_id.starts_with("snapshot-start:") || w.message_id.starts_with("snapshot-end:")
    }));
}

#[test]
fn abandoned_export_does_not_block_the_same_roster_in_a_new_ready_attempt() {
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
    let loan = f.loan("owner");
    let store = loan.store().unwrap();
    let vault = store
        .load_sync_vault(&loan.installation, &loan.master, loan.revision)
        .unwrap();
    let record = loan.record.as_ref().unwrap();
    let old = record.snapshot.as_ref().unwrap().id.clone();
    store
        .begin_snapshot_export(&old, &record.peer.device_id, record.key_version)
        .unwrap();
    let signed = store.current_signed_roster().unwrap().unwrap();
    let id = engine::membership_message_id(&signed).unwrap();
    let header = EnvelopeHeader::new(
        &record.dataset_id,
        &record.local_device_id,
        &record.peer.device_id,
        record.key_version,
        &id,
    )
    .unwrap();
    let (key, _, _) = vault
        .peer_keys(&record.dataset_id, &record.peer.device_id)
        .unwrap();
    store
        .stage_snapshot_authority(
            &old,
            &header,
            &key,
            vault.identity(),
            &signed.encode().unwrap(),
        )
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
    let offer = f.export();
    assert_ne!(offer.snapshot_id, old);
    let loan = f.loan("owner");
    let store = loan.store().unwrap();
    let record = loan.record.as_ref().unwrap();
    let ids = transfer::outgoing_scope(&store, record).unwrap();
    let due = store
        .due_enrollment_wires(&record.peer.device_id, now(), &ids)
        .unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].message_id, id);
}
