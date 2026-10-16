mod boundaries;

use super::*;
use crate::sync::{
    crypto::ContentKey,
    datasets::DatasetCatalog,
    enrollment::SnapshotOffer,
    pairing::{PairingInvitation, PendingJoin},
    runtime::tests::Fixture,
};

struct Pair {
    owner: Fixture,
    joiner: Fixture,
    owner_key: [u8; 32],
    joiner_key: [u8; 32],
    invitation: String,
    entry: String,
    proof: Vec<u8>,
}

impl Pair {
    fn new() -> Self {
        let owner = Fixture::new();
        owner
            .store
            .upsert_ai_provider_config(
                "https://synthetic.invalid/v1",
                "shared-model",
                "SYNTHETIC_SHARED_KEY",
            )
            .unwrap();
        owner
            .store
            .submit_input_as_event("shared original", "capture", None)
            .unwrap();
        let (owner_key, initial) = owner.initialize();
        super::super::confirm_key(&owner.config, &initial.token, initial.revision).unwrap();
        let owner_vault = owner
            .store
            .load_sync_vault(&initial.installation_id, &owner_key, initial.revision)
            .unwrap();
        let (invite, pending) = PairingInvitation::issue_for_relay(
            &owner.store.sync_identity().unwrap().unwrap().dataset_id,
            owner_vault.device().clone(),
            owner_vault.identity(),
            now(),
            InvitationRoute {
                relay_url: "http://localhost:9876".into(),
                issuer_mailbox: RelayClient::mailbox_key(owner_vault.mailbox_owner()),
                request_sender: "a".repeat(64),
            },
        )
        .unwrap();
        let invitation = invite.invitation_id().to_string();
        let joiner = Fixture::new();
        joiner
            .store
            .upsert_ai_provider_config(
                "https://original.invalid/v1",
                "original-model",
                "SYNTHETIC_ORIGINAL_KEY",
            )
            .unwrap();
        joiner
            .store
            .submit_input_as_event("keep this independent original", "capture", None)
            .unwrap();
        let shown = joining::preview(&joiner.config, &invite.encode().unwrap()).unwrap();
        let request =
            joining::prepare_key(&joiner.config, &shown.token, "joining", "test", true).unwrap();
        let entry = request.entry_id.clone().unwrap();
        let joiner_key = ContentKey::generate().unwrap().to_bytes();
        let commit =
            joining::install_key(&joiner.config, &request.token, joiner_key.to_vec(), 1).unwrap();
        joining::confirm_key(&joiner.config, &commit.token, 1).unwrap();
        let catalog = DatasetCatalog::open(&joiner.config.data_dir).unwrap();
        let stage = Store::open(&catalog.path_for(&entry).unwrap())
            .unwrap()
            .with_provider_master(joiner_key);
        let joiner_vault = stage
            .load_sync_vault(request.installation_id.as_ref().unwrap(), &joiner_key, 1)
            .unwrap();
        let (join, bytes) = PendingJoin::begin_for_relay(
            invite,
            joiner_vault.device().clone(),
            joiner_vault.identity(),
            now(),
            RelayClient::mailbox_key(joiner_vault.mailbox_owner()),
        )
        .unwrap();
        let approval = pending.receive(&bytes, now()).unwrap();
        let approved = super::super::pairing::approve_pairing(&owner.config, approval).unwrap();
        super::super::confirm_key(&owner.config, &approved.key.token, approved.key.revision)
            .unwrap();
        let response = super::super::pairing::pairing_frame(&owner.config, &invitation).unwrap();
        let pair = join.finish(&response, 0, now()).unwrap();
        stage
            .install_sync_pairing_join(
                request.installation_id.as_ref().unwrap(),
                &joiner_key,
                1,
                pair,
                now(),
            )
            .unwrap();
        // Restore through the production staging/native handshake after commit.
        joining::release(&joiner.config, &request.operation_id);
        let resume = joining::prepare_resume(&joiner.config, &entry).unwrap();
        let restored =
            joining::install_key(&joiner.config, &resume.token, joiner_key.to_vec(), 1).unwrap();
        joining::confirm_key(&joiner.config, &restored.token, restored.revision).unwrap();
        let proof = stage
            .sync_pairing_frame(
                request.installation_id.as_ref().unwrap(),
                &joiner_key,
                restored.revision,
                &invitation,
                now(),
            )
            .unwrap();
        Self {
            owner,
            joiner,
            owner_key,
            joiner_key,
            invitation,
            entry,
            proof,
        }
    }

    fn config(&self, role: &str) -> &AppConfig {
        if role == "owner" {
            &self.owner.config
        } else {
            &self.joiner.config
        }
    }
    fn state(&self, role: &str) -> EnrollmentView {
        list(self.config(role))
            .unwrap()
            .into_iter()
            .find(|s| s.invitation_id == self.invitation)
            .unwrap()
    }
    fn loan(&self, role: &str) -> EnrollmentLoan {
        begin(
            self.config(role),
            role,
            &self.state(role).operation_id,
            &self.invitation,
        )
        .unwrap()
    }
    fn confirm(&self, state: EnrollmentView) -> EnrollmentView {
        if state.confirmation_token.is_none() {
            return state;
        }
        confirm(
            self.config(&state.role),
            &state.role,
            &state.operation_id,
            &state.invitation_id,
            state.confirmation_token.as_ref().unwrap(),
            state.revision,
        )
        .unwrap()
    }
    fn mutate(
        &self,
        role: &str,
        change: impl FnOnce(&EnrollmentLoan, &Store) -> Result<PairingCommit>,
    ) -> EnrollmentView {
        let loan = self.loan(role);
        loan.mutate(|store| change(&loan, store)).unwrap();
        let result = loan.result().unwrap();
        drop(loan);
        result
    }
    fn accept_proof(&self) -> EnrollmentView {
        self.mutate("owner", |loan, store| {
            store.confirm_sync_pairing_installation(
                &loan.installation,
                &loan.master,
                loan.revision,
                &loan.invitation,
                &self.proof,
                now(),
            )
        })
    }
    fn channels(&self) {
        self.confirm(self.accept_proof());
        self.confirm(self.mutate("owner", |loan, store| {
            store.confirm_enrollment_grant(
                &loan.installation,
                &loan.master,
                loan.revision,
                &loan.invitation,
                1,
            )
        }));
        self.confirm(self.mutate("owner", |loan, store| {
            store.confirm_enrollment_revoke(
                &loan.installation,
                &loan.master,
                loan.revision,
                &loan.invitation,
                2,
            )
        }));
        self.confirm(self.mutate("joiner", |loan, store| {
            store.confirm_enrollment_grant(
                &loan.installation,
                &loan.master,
                loan.revision,
                &loan.invitation,
                2,
            )
        }));
    }
    fn export(&self) -> SnapshotOffer {
        self.confirm(self.mutate("owner", |loan, store| {
            store.reserve_enrollment_snapshot(
                &loan.installation,
                &loan.master,
                loan.revision,
                &loan.invitation,
            )
        }));
        let loan = self.loan("owner");
        let store = loan.store().unwrap();
        let vault = store
            .load_sync_vault(&loan.installation, &loan.master, loan.revision)
            .unwrap();
        let record = loan.record.as_ref().unwrap();
        prepare_snapshot(&loan, &store, &vault, record).unwrap();
        let (key, _, _) = vault
            .peer_keys(&record.dataset_id, &record.peer.device_id)
            .unwrap();
        transfer::owner_offer(&store, record, &key, vault.identity()).unwrap()
    }
    fn wire(&self, id: &str) -> String {
        let loan = self.loan("owner");
        let record = loan.record.as_ref().unwrap();
        let bytes: Vec<u8> = loan.store().unwrap().connection.query_row(
            "SELECT encrypted_envelope FROM sync_wire_outbox WHERE recipient_device_id=?1 AND message_id=?2",
            rusqlite::params![record.peer.device_id, id], |r| r.get(0)).unwrap();
        STANDARD.encode(bytes)
    }
    fn receive(&self, role: &str, ciphertext: &str) -> EnrollmentView {
        let loan = self.loan(role);
        let store = loan.store().unwrap();
        let vault = store
            .load_sync_vault(&loan.installation, &loan.master, loan.revision)
            .unwrap();
        let record = loan.record.as_ref().unwrap();
        let (_, key, _) = vault
            .peer_keys(&record.dataset_id, &record.peer.device_id)
            .unwrap();
        match transfer::accept(
            &store,
            &loan.installation,
            &loan.master,
            loan.revision,
            record,
            ciphertext,
            &key,
        )
        .unwrap()
        {
            transfer::ReceiveOutcome::Waiting => panic!("expected a durable enrollment message"),
            transfer::ReceiveOutcome::Durable(Some(commit)) => {
                let mut all = sessions().lock().unwrap();
                let session = loan.current(&mut all).unwrap();
                EnrollmentLoan::pause(session, &loan.invitation, loan.role, commit);
            }
            transfer::ReceiveOutcome::Durable(None) => {}
        }
        let result = loan.result().unwrap();
        drop(loan);
        result
    }
    fn roster(&self) -> String {
        let loan = self.loan("owner");
        transfer::snapshot_roster_id(&loan.store().unwrap(), loan.record.as_ref().unwrap())
            .unwrap()
            .unwrap()
    }
    fn select(&self, offer: &SnapshotOffer) {
        self.confirm(self.receive("joiner", &self.wire(&self.roster())));
        let owner = self.loan("owner");
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
        self.confirm(self.receive(
            "joiner",
            &self.wire(&offer_id(&self.invitation, &offer.snapshot_id)),
        ));
    }
}

#[test]
fn proof_commit_is_native_fenced_and_old_role_or_token_cannot_confirm_it() {
    let f = Pair::new();
    let pending = f.accept_proof();
    assert!(pending.confirmation_token.is_some());
    assert!(
        begin(
            &f.owner.config,
            "owner",
            &pending.operation_id,
            &f.invitation
        )
        .is_err()
    );
    assert!(
        confirm(
            &f.owner.config,
            "joiner",
            &pending.operation_id,
            &f.invitation,
            pending.confirmation_token.as_ref().unwrap(),
            pending.revision
        )
        .is_err()
    );
    assert!(
        confirm(
            &f.owner.config,
            "owner",
            &pending.operation_id,
            "other-invitation",
            pending.confirmation_token.as_ref().unwrap(),
            pending.revision
        )
        .is_err()
    );
    assert!(
        confirm(
            &f.owner.config,
            "owner",
            &pending.operation_id,
            &f.invitation,
            "stale",
            pending.revision
        )
        .is_err()
    );
    f.owner
        .store
        .submit_input_as_event("capture during native confirmation", "capture", None)
        .unwrap();
    let token = pending.confirmation_token.clone().unwrap();
    let operation = pending.operation_id.clone();
    let revision = pending.revision;
    assert!(super::super::confirm_key(&f.owner.config, &token, revision).is_err());
    let ready = f.confirm(pending);
    assert_eq!(ready.phase, "preparing_channel");
    assert!(
        confirm(
            &f.owner.config,
            "owner",
            &operation,
            &f.invitation,
            &token,
            revision
        )
        .is_ok()
    );
    assert!(
        confirm(
            &f.owner.config,
            "owner",
            &operation,
            "other-invitation",
            &token,
            revision
        )
        .is_err()
    );
    assert_eq!(f.owner.store.list_events().unwrap().len(), 2);
    assert!(f.joiner.store.sync_identity().unwrap().is_none());
}

#[test]
fn grant_failure_rolls_back_and_reopen_preserves_the_exact_intent() {
    let f = Pair::new();
    f.confirm(f.accept_proof());
    let loan = f.loan("owner");
    let original = loan.record.as_ref().unwrap().receive.clone();
    assert_eq!(original.expected_revision, 0);
    assert!(
        loan.mutate(|store| store.confirm_enrollment_grant(
            &loan.installation,
            &loan.master,
            loan.revision,
            &loan.invitation,
            2
        ))
        .is_err()
    );
    let store = loan.store().unwrap();
    store.connection.execute_batch("CREATE TRIGGER fail_enrollment BEFORE UPDATE ON sync_local_vault BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;").unwrap();
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
    store
        .connection
        .execute_batch("DROP TRIGGER fail_enrollment")
        .unwrap();
    let revision = loan.revision;
    drop(loan);
    super::super::lock(&f.owner.config);
    let request = super::super::prepare_key(&f.owner.config, None, None).unwrap();
    let key = super::super::install_key(
        &f.owner.config,
        &request.token,
        f.owner_key.to_vec(),
        revision,
    )
    .unwrap();
    super::super::confirm_key(&f.owner.config, &key.token, key.revision).unwrap();
    let restored = f.loan("owner");
    assert_eq!(restored.record.as_ref().unwrap().receive, original);
    assert!(!restored.record.as_ref().unwrap().receive_ready);
    drop(restored);
    f.confirm(f.mutate("owner", |loan, store| {
        store.confirm_enrollment_grant(
            &loan.installation,
            &loan.master,
            loan.revision,
            &loan.invitation,
            1,
        )
    }));
}

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
