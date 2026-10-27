// 公共测试辅助（原文件平移，pub(super) 供分组子文件复用）。
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
    /// 这一对设备的快照数据面（生产里是 relay `/blob/*`，测试里是内存实现）。
    ///
    /// 在 `new()` 里就给当前线程装上：`prepare_snapshot` 走的是生产的
    /// `snapshot_upload`，没有数据面它会诚实地报 `sync_protocol_unsupported`，
    /// 而这些用例本来也不该联网。
    plane: std::sync::Arc<crate::sync::mem_blob::MemBlob>,
    owner_key: [u8; 32],
    joiner_key: [u8; 32],
    invitation: String,
    entry: String,
    proof: Vec<u8>,
}

impl Pair {
    fn new() -> Self {
        let plane = crate::sync::runtime::use_mem_blob_plane();
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
            plane,
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
    /// 让 joiner（或 owner）真的从（测试用）blob 数据面拉一片窗口。
    ///
    /// 生产里这一拍由 `network.rs` / `flow.rs` 的 tick 触发；这些用例不跑 relay，
    /// 于是把 tick 里那一步单独拎出来调 —— 调的是同一段 `import_blob_snapshots`。
    fn import(
        &self,
        role: &str,
        plane: &std::sync::Arc<crate::sync::mem_blob::MemBlob>,
        rounds: usize,
    ) -> bool {
        let loan = self.loan(role);
        let store = loan.store().unwrap();
        let vault = store
            .load_sync_vault(&loan.installation, &loan.master, loan.revision)
            .unwrap();
        let record = loan.record.as_ref().unwrap();
        let (_, key, _) = vault
            .peer_keys(&record.dataset_id, &record.peer.device_id)
            .unwrap();
        crate::sync::engine::snapshot::import_blob_snapshots(
            &store,
            &crate::sync::mem_blob::mem_download(
                plane,
                &record.peer.device_id,
                record.key_version,
                &key,
                &record.peer.signing_key,
            ),
            rounds,
        )
        .unwrap()
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

mod proof_fence;

mod loan_flow;

mod scan_drops;
