use super::*;
use crate::sync::{
    crypto::ContentKey,
    pairing::{InvitationRoute, PendingInvitation},
    runtime::tests::Fixture,
    transport_relay::RelayEnvelope,
};

struct Flow {
    owner: Fixture,
    joiner: Fixture,
    owner_key: [u8; 32],
    join_key: [u8; 32],
    owner_installation: String,
    ready: JoinView,
    pending: Option<PendingInvitation>,
}
impl Flow {
    fn new() -> Self {
        let owner = Fixture::new();
        let (owner_key, commit) = owner.initialize();
        crate::sync::runtime::confirm_key(&owner.config, &commit.token, 1).unwrap();
        let vault = owner
            .store
            .load_sync_vault(&commit.installation_id, &owner_key, 1)
            .unwrap();
        let (invite, pending) = PairingInvitation::issue_for_relay(
            &owner.store.sync_identity().unwrap().unwrap().dataset_id,
            vault.device().clone(),
            vault.identity(),
            now(),
            InvitationRoute {
                relay_url: "http://localhost:9876".into(),
                issuer_mailbox: RelayClient::mailbox_key(vault.mailbox_owner()),
                request_sender: "a".repeat(64),
            },
        )
        .unwrap();
        let joiner = Fixture::new();
        let shown = preview(&joiner.config, &invite.encode().unwrap()).unwrap();
        let request = prepare_key(&joiner.config, &shown.token, "joining", "test", true).unwrap();
        let join_key = ContentKey::generate().unwrap().to_bytes();
        let installed = install_key(&joiner.config, &request.token, join_key.to_vec(), 1).unwrap();
        let ready = confirm_key(&joiner.config, &installed.token, 1).unwrap();
        Self {
            owner,
            joiner,
            owner_key,
            join_key,
            owner_installation: commit.installation_id,
            ready,
            pending: Some(pending),
        }
    }
    fn request(&self) -> NetworkLease {
        let net = begin(
            &self.joiner.config,
            &self.ready.operation_id,
            Action::Request,
        )
        .unwrap();
        let mut all = sessions().lock().unwrap();
        current(&mut all, &self.joiner.config, &net)
            .unwrap()
            .joining
            .as_mut()
            .unwrap()
            .network_busy = false;
        net
    }
    fn approve(&mut self) -> Vec<u8> {
        let request = self.request();
        let approval = self
            .pending
            .take()
            .unwrap()
            .receive(&request.frame, now())
            .unwrap();
        let committed = self
            .owner
            .store
            .approve_sync_pairing(
                &self.owner_installation,
                &self.owner_key,
                1,
                approval,
                now(),
            )
            .unwrap();
        self.owner
            .store
            .sync_pairing_frame(
                &self.owner_installation,
                &self.owner_key,
                committed.vault_revision,
                &committed.invitation_id,
                now(),
            )
            .unwrap()
    }
    fn install(&mut self) -> JoinView {
        let response = self.approve();
        let net = begin(&self.joiner.config, &self.ready.operation_id, Action::Poll).unwrap();
        accept_page(
            &self.joiner.config,
            &net,
            Ok(page(&net.recipient, &response)),
        )
        .unwrap()
    }
    fn stage(&self) -> Store {
        let catalog = DatasetCatalog::open(&self.joiner.config.data_dir).unwrap();
        Store::open(
            &catalog
                .path_for(self.ready.entry_id.as_ref().unwrap())
                .unwrap(),
        )
        .unwrap()
        .with_provider_master(self.join_key)
    }
}
fn page(sender: &str, bytes: &[u8]) -> RelayPage {
    RelayPage {
        envelopes: vec![RelayEnvelope {
            id: "0000000000000001".into(),
            sender_mb: sender.into(),
            seq_hint: 0,
            ciphertext: STANDARD.encode(bytes),
            created_at: now(),
            expires_at: now() + 30_000,
        }],
        next_cursor: "0000000000000001".into(),
        has_more: false,
    }
}

#[test]
fn approval_installation_requires_native_revision_before_any_proof_and_keeps_membership_inactive() {
    let mut f = Flow::new();
    let installed = f.install();
    assert_eq!(installed.phase, "installation_confirmation");
    assert_eq!(installed.revision, 2);
    assert!(begin(&f.joiner.config, &installed.operation_id, Action::Proof).is_err());
    assert!(confirm_key(&f.joiner.config, &installed.token, 1).is_err());
    assert!(confirm_key(&f.joiner.config, &f.ready.token, 2).is_err());
    let confirmed = confirm_key(&f.joiner.config, &installed.token, 2).unwrap();
    assert_eq!(confirmed.phase, "awaiting_membership");
    let dispatch = begin(&f.joiner.config, &confirmed.operation_id, Action::Proof).unwrap();
    assert!(!dispatch.frame.is_empty());
    assert!(confirmed.approval_ack_pending);
    assert!(!f.stage().local_sync_membership_active().unwrap());
    assert!(f.joiner.store.sync_identity().unwrap().is_none());
    assert!(
        crate::sync::datasets::selected_database_path(&f.joiner.config.data_dir)
            .unwrap()
            .is_none()
    );
}

#[test]
fn committed_join_resumes_with_the_exact_proof_and_native_floor_not_an_old_noise_state() {
    let mut f = Flow::new();
    let installed = f.install();
    let stage = f.stage();
    let invitation = stage
        .sync_pairing_statuses(
            installed.installation_id.as_ref().unwrap(),
            &f.join_key,
            2,
            now(),
        )
        .unwrap()
        .remove(0)
        .invitation_id;
    let original = stage
        .sync_pairing_frame(
            installed.installation_id.as_ref().unwrap(),
            &f.join_key,
            2,
            &invitation,
            now(),
        )
        .unwrap();
    release(&f.joiner.config, &installed.operation_id);
    let request = prepare_resume(&f.joiner.config, installed.entry_id.as_ref().unwrap()).unwrap();
    assert!(install_key(&f.joiner.config, &request.token, f.join_key.to_vec(), 3).is_err());
    assert!(install_key(&f.joiner.config, &request.token, vec![0; 32], 1).is_err());
    let reloaded = install_key(&f.joiner.config, &request.token, f.join_key.to_vec(), 1).unwrap();
    assert_eq!(reloaded.revision, 2);
    assert!(begin(&f.joiner.config, &request.operation_id, Action::Proof).is_err());
    confirm_key(&f.joiner.config, &reloaded.token, 2).unwrap();
    let dispatch = begin(&f.joiner.config, &request.operation_id, Action::Proof).unwrap();
    assert_eq!(dispatch.frame, original);
    assert!(!f.stage().local_sync_membership_active().unwrap());
}

#[test]
fn request_retry_reuses_noise_bytes_and_original_temporary_grant_parameters() {
    let f = Flow::new();
    let first = f.request();
    let again = f.request();
    assert_eq!(first.frame, again.frame);
    assert_eq!(first.response_credential, again.response_credential);
    assert_eq!(first.expires_at, again.expires_at);
    assert_eq!(first.sender, again.sender);
    assert_eq!(first.mailbox, again.mailbox);
}

#[test]
fn cancelled_network_request_retains_lease_but_cannot_complete_into_a_new_session() {
    let f = Flow::new();
    let request = begin(&f.joiner.config, &f.ready.operation_id, Action::Request).unwrap();
    assert!(release(&f.joiner.config, &f.ready.operation_id));
    assert!(
        crate::sync::runtime::prepare_key(&f.joiner.config, Some("owner"), Some("test")).is_err()
    );
    assert!(
        accept_page(
            &f.joiner.config,
            &request,
            Ok(page(&request.recipient, b"late"))
        )
        .is_err()
    );
    drop(request);
    assert!(
        crate::sync::runtime::prepare_key(&f.joiner.config, Some("owner"), Some("test")).is_ok()
    );
}

#[test]
fn unrelated_or_forged_approval_does_not_install_or_create_a_proof() {
    let f = Flow::new();
    f.request();
    let net = begin(&f.joiner.config, &f.ready.operation_id, Action::Poll).unwrap();
    let unchanged = accept_page(
        &f.joiner.config,
        &net,
        Ok(page(&"b".repeat(64), b"unrelated")),
    )
    .unwrap();
    assert_eq!(unchanged.phase, "awaiting_approval");
    let net = begin(&f.joiner.config, &f.ready.operation_id, Action::Poll).unwrap();
    assert!(
        accept_page(
            &f.joiner.config,
            &net,
            Ok(page(&net.recipient, b"forged approval"))
        )
        .is_err()
    );
    assert_eq!(
        state(&f.joiner.config, &f.ready.operation_id)
            .unwrap()
            .phase,
        "requires_new_invitation"
    );
    assert!(f.stage().sync_identity().unwrap().is_none());
    assert!(begin(&f.joiner.config, &f.ready.operation_id, Action::Proof).is_err());
}

#[test]
fn failed_installation_transaction_does_not_pin_authority_or_publish_a_proof() {
    let mut f = Flow::new();
    let response = f.approve();
    f.stage().connection.execute_batch("CREATE TRIGGER fail_join BEFORE UPDATE ON sync_local_vault BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;").unwrap();
    let net = begin(&f.joiner.config, &f.ready.operation_id, Action::Poll).unwrap();
    assert!(accept_page(&f.joiner.config, &net, Ok(page(&net.recipient, &response))).is_err());
    assert!(f.stage().sync_identity().unwrap().is_none());
    assert!(!f.stage().has_sync_authority().unwrap());
    assert_eq!(
        f.stage()
            .load_sync_vault(f.ready.installation_id.as_ref().unwrap(), &f.join_key, 1)
            .unwrap()
            .revision,
        1
    );
    assert!(begin(&f.joiner.config, &f.ready.operation_id, Action::Proof).is_err());
}

#[test]
fn failed_fetch_is_retryable_and_expired_join_cannot_start_more_network_work() {
    let f = Flow::new();
    f.request();
    let net = begin(&f.joiner.config, &f.ready.operation_id, Action::Poll).unwrap();
    assert!(
        accept_page(
            &f.joiner.config,
            &net,
            Err(anyhow::anyhow!("SYNTHETIC_SECRET"))
        )
        .is_err()
    );
    let next = begin(&f.joiner.config, &f.ready.operation_id, Action::Poll).unwrap();
    let mut all = sessions().lock().unwrap();
    let session = current(&mut all, &f.joiner.config, &next).unwrap();
    session.joining.as_mut().unwrap().network_busy = false;
    session.joining.as_mut().unwrap().deadline = now() - 1;
    drop(all);
    assert!(begin(&f.joiner.config, &f.ready.operation_id, Action::Request).is_err());
    assert_eq!(
        state(&f.joiner.config, &f.ready.operation_id)
            .unwrap()
            .phase,
        "expired"
    );
}
