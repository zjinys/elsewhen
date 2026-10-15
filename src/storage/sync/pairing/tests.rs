use super::*;
use crate::sync::{
    crypto::ContentKey,
    pairing::{InvitationRoute, PairingInvitation, PendingJoin},
    transport_relay::RelayClient,
};

const NOW: i64 = 1_800_000_000_000;
struct Device {
    root: std::path::PathBuf,
    store: Store,
    master: [u8; 32],
    vault: LocalVault,
}
impl Device {
    fn new(owner: bool) -> Self {
        let root =
            std::env::temp_dir().join(format!("elsewhen-pair-journal-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let master = ContentKey::generate().unwrap().to_bytes();
        let store = Store::open(&root.join("test.db"))
            .unwrap()
            .with_provider_master(master);
        let vault = store
            .create_sync_vault("install", &master, "synthetic device", "test")
            .unwrap();
        if owner {
            store
                .initialize_sync_replica(&ReplicaIdentity {
                    dataset_id: "dataset".into(),
                    device_id: vault.device().device_id.clone(),
                    stream_id: vault.device().stream_id.clone(),
                    processor_device_id: vault.device().device_id.clone(),
                })
                .unwrap();
            store
                .initialize_sync_authority(vault.device(), vault.identity())
                .unwrap();
            store.enable_provider_key_sealing().unwrap();
        }
        Self {
            root,
            store,
            master,
            vault,
        }
    }
    fn reopen(&self) -> Store {
        Store::open(&self.store.path)
            .unwrap()
            .with_provider_master(self.master)
    }
    fn revision(&self) -> i64 {
        self.store
            .connection
            .query_row("SELECT revision FROM sync_local_vault", [], |r| r.get(0))
            .unwrap()
    }
    fn active(&self, peer: &Device) -> bool {
        self.store
            .connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sync_members WHERE device_id=?1 AND active=1)",
                [&peer.vault.device().device_id],
                |r| r.get(0),
            )
            .unwrap()
    }
    fn request(&self, joiner: &Device, now: i64) -> (PendingApproval, PendingJoin) {
        let (invite, pending) = PairingInvitation::issue_for_relay(
            "dataset",
            self.vault.device().clone(),
            self.vault.identity(),
            now,
            InvitationRoute {
                relay_url: "http://127.0.0.1:9876".into(),
                issuer_mailbox: RelayClient::mailbox_key(self.vault.mailbox_owner()),
                request_sender: "f".repeat(64),
            },
        )
        .unwrap();
        let (join, request) = PendingJoin::begin_for_relay(
            invite,
            joiner.vault.device().clone(),
            joiner.vault.identity(),
            now,
            RelayClient::mailbox_key(joiner.vault.mailbox_owner()),
        )
        .unwrap();
        (pending.receive(&request, now).unwrap(), join)
    }
    fn approve(&self, joiner: &Device, now: i64) -> (PairingCommit, PendingJoin) {
        let (approval, join) = self.request(joiner, now);
        (
            self.store
                .approve_sync_pairing("install", &self.master, self.revision(), approval, now)
                .unwrap(),
            join,
        )
    }
    fn frame(&self, commit: &PairingCommit, now: i64) -> Vec<u8> {
        self.reopen()
            .sync_pairing_frame(
                "install",
                &self.master,
                commit.vault_revision,
                &commit.invitation_id,
                now,
            )
            .unwrap()
    }
}
impl Drop for Device {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
fn installed(a: &Device, b: &Device) -> (PairingCommit, PairingCommit, Vec<u8>) {
    let (approved, join) = a.approve(b, NOW);
    let pair = join.finish(&a.frame(&approved, NOW), 0, NOW).unwrap();
    let installed = b
        .store
        .install_sync_pairing_join("install", &b.master, 1, pair, NOW)
        .unwrap();
    let proof = b.frame(&installed, NOW);
    (approved, installed, proof)
}

#[test]
fn committed_approval_and_installation_resume_with_exact_frames_after_reopen() {
    let a = Device::new(true);
    let b = Device::new(false);
    let (approved, join) = a.approve(&b, NOW);
    assert_eq!(approved.vault_revision, 2);
    assert!(!a.active(&b));
    assert!(
        a.store
            .sync_pairing_frame("install", &a.master, 1, &approved.invitation_id, NOW)
            .is_err()
    );
    let response = a.frame(&approved, NOW);
    assert_eq!(response, a.frame(&approved, NOW + 1));
    let pair = join.finish(&response, 0, NOW).unwrap();
    let installed = b
        .store
        .install_sync_pairing_join("install", &b.master, 1, pair, NOW)
        .unwrap();
    assert_eq!(installed.vault_revision, 2);
    assert!(!b.active(&b));
    assert!(!b.store.background_processing_allowed().unwrap());
    assert!(
        b.store
            .sync_pairing_frame("install", &b.master, 1, &installed.invitation_id, NOW)
            .is_err()
    );
    let proof = b.frame(&installed, NOW);
    assert_eq!(proof, b.frame(&installed, NOW + 1));
    let completed = a
        .reopen()
        .confirm_sync_pairing_installation(
            "install",
            &a.master,
            2,
            &approved.invitation_id,
            &proof,
            NOW,
        )
        .unwrap();
    assert_eq!(completed.vault_revision, 3);
    assert!(a.active(&b));
    assert!(
        a.store
            .sync_pairing_roster("install", &a.master, 2, &approved.invitation_id)
            .is_err()
    );
    let roster = a
        .reopen()
        .sync_pairing_roster("install", &a.master, 3, &approved.invitation_id)
        .unwrap();
    let joined = b
        .reopen()
        .finish_sync_pairing_join(
            "install",
            &b.master,
            2,
            &installed.invitation_id,
            &roster.encode().unwrap(),
        )
        .unwrap();
    assert_eq!(joined.vault_revision, 3);
    assert!(b.active(&b));
    assert!(b.store.provider_keys_sealed().unwrap());
    assert!(!b.store.background_processing_allowed().unwrap());
    // The installation receipt remains usable after expiry, without re-approval.
    let replay = a
        .reopen()
        .confirm_sync_pairing_installation(
            "install",
            &a.master,
            3,
            &approved.invitation_id,
            &proof,
            NOW + 90_000,
        )
        .unwrap();
    assert_eq!(replay.vault_revision, 3);
    assert_eq!(a.store.sync_roster_anchor().unwrap().revision, 2);
    let replay = b
        .reopen()
        .finish_sync_pairing_join(
            "install",
            &b.master,
            3,
            &installed.invitation_id,
            &roster.encode().unwrap(),
        )
        .unwrap();
    assert_eq!(replay.vault_revision, 3);
}

#[test]
fn failed_owner_completion_rolls_back_grant_keys_outbox_and_receipt_together() {
    let a = Device::new(true);
    let b = Device::new(false);
    let (approved, _, proof) = installed(&a, &b);
    a.store
        .submit_input_as_event("local event", "capture", None)
        .unwrap();
    a.store.connection.execute_batch("CREATE TRIGGER fail_pair_commit BEFORE UPDATE ON sync_local_vault BEGIN SELECT RAISE(ABORT,'synthetic commit failure'); END;").unwrap();
    assert!(
        a.store
            .confirm_sync_pairing_installation(
                "install",
                &a.master,
                2,
                &approved.invitation_id,
                &proof,
                NOW
            )
            .is_err()
    );
    assert!(!a.active(&b));
    assert_eq!(a.revision(), 2);
    assert_eq!(a.store.sync_roster_anchor().unwrap().revision, 1);
    assert_eq!(
        a.store
            .connection
            .query_row("SELECT COUNT(*) FROM sync_outbox", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    let vault = a.store.load_sync_vault("install", &a.master, 2).unwrap();
    assert!(
        vault
            .peer_keys("dataset", &b.vault.device().device_id)
            .is_err()
    );
    assert_eq!(a.frame(&approved, NOW), a.frame(&approved, NOW + 1));
    a.store
        .connection
        .execute_batch("DROP TRIGGER fail_pair_commit")
        .unwrap();
    a.store
        .confirm_sync_pairing_installation(
            "install",
            &a.master,
            2,
            &approved.invitation_id,
            &proof,
            NOW,
        )
        .unwrap();
    assert!(a.active(&b));
    assert_eq!(
        a.store
            .connection
            .query_row("SELECT COUNT(*) FROM sync_outbox", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn expired_or_tampered_proof_does_not_consume_pending_approval() {
    let a = Device::new(true);
    let b = Device::new(false);
    let (approved, _, proof) = installed(&a, &b);
    let mut bad = proof.clone();
    let last = bad.len() - 1;
    bad[last] ^= 1;
    assert!(
        a.store
            .confirm_sync_pairing_installation(
                "install",
                &a.master,
                2,
                &approved.invitation_id,
                &bad,
                NOW
            )
            .is_err()
    );
    assert!(
        a.store
            .confirm_sync_pairing_installation(
                "install",
                &a.master,
                2,
                &approved.invitation_id,
                &proof,
                NOW + 24 * 60 * 60 * 1000
            )
            .is_err()
    );
    assert!(
        a.store
            .sync_pairing_frame(
                "install",
                &a.master,
                2,
                &approved.invitation_id,
                NOW + 24 * 60 * 60 * 1000
            )
            .is_err()
    );
    assert_eq!(a.revision(), 2);
    assert!(!a.active(&b));
    a.store
        .confirm_sync_pairing_installation(
            "install",
            &a.master,
            2,
            &approved.invitation_id,
            &proof,
            NOW,
        )
        .unwrap();
    assert!(
        a.store
            .confirm_sync_pairing_installation(
                "install",
                &a.master,
                3,
                &approved.invitation_id,
                &bad,
                NOW
            )
            .is_err()
    );
    assert_eq!(a.revision(), 3);
}

#[test]
fn repeated_completion_never_reauthorizes_a_revoked_installation() {
    let a = Device::new(true);
    let b = Device::new(false);
    let (approved, _, proof) = installed(&a, &b);
    a.store
        .confirm_sync_pairing_installation(
            "install",
            &a.master,
            2,
            &approved.invitation_id,
            &proof,
            NOW,
        )
        .unwrap();
    a.store
        .revoke_signed_sync_member(2, &b.vault.device().device_id, a.vault.identity())
        .unwrap();
    assert!(
        a.store
            .confirm_sync_pairing_installation(
                "install",
                &a.master,
                3,
                &approved.invitation_id,
                &proof,
                NOW
            )
            .is_err()
    );
    assert!(
        a.store
            .sync_pairing_roster("install", &a.master, 3, &approved.invitation_id)
            .is_err()
    );
    let (approval, _) = a.request(&b, NOW);
    assert!(
        a.store
            .approve_sync_pairing("install", &a.master, 3, approval, NOW)
            .is_err()
    );
    assert!(!a.active(&b));
}

#[test]
fn revoked_joiner_cannot_publish_an_old_installation_proof_or_finish_with_old_roster() {
    let a = Device::new(true);
    let b = Device::new(false);
    let (approved, installed, proof) = installed(&a, &b);
    a.store
        .confirm_sync_pairing_installation(
            "install",
            &a.master,
            2,
            &approved.invitation_id,
            &proof,
            NOW,
        )
        .unwrap();
    let grant = a
        .store
        .current_signed_roster()
        .unwrap()
        .unwrap()
        .encode()
        .unwrap();
    let revoked = a
        .store
        .revoke_signed_sync_member(2, &b.vault.device().device_id, a.vault.identity())
        .unwrap();
    b.store
        .apply_signed_roster(&revoked.encode().unwrap())
        .unwrap();
    let status = b
        .store
        .sync_pairing_statuses("install", &b.master, 2, NOW)
        .unwrap();
    assert_eq!(status[0].state, "revoked");
    assert!(
        b.store
            .sync_pairing_frame("install", &b.master, 2, &installed.invitation_id, NOW)
            .is_err()
    );
    assert!(
        b.store
            .finish_sync_pairing_join("install", &b.master, 2, &installed.invitation_id, &grant)
            .is_err()
    );
    assert!(!b.active(&b));
    assert_eq!(b.revision(), 2);
    assert!(!b.store.provider_keys_sealed().unwrap());
}

#[test]
fn temporary_direction_capabilities_survive_reopen_without_the_invitation_psk() {
    let a = Device::new(true);
    let b = Device::new(false);
    let (approved, join) = a.approve(&b, NOW);
    assert!(
        a.store
            .sync_pairing_dispatch("install", &a.master, 1, &approved.invitation_id, NOW)
            .is_err()
    );
    let response = a
        .reopen()
        .sync_pairing_dispatch("install", &a.master, 2, &approved.invitation_id, NOW)
        .unwrap();
    assert_eq!(
        response.delivery.sender_mailbox,
        RelayClient::mailbox_key(a.vault.mailbox_owner())
    );
    assert_eq!(
        response.delivery.recipient_mailbox,
        RelayClient::mailbox_key(b.vault.mailbox_owner())
    );
    assert_eq!(response.delivery.expires_at_ms, NOW + 24 * 60 * 60 * 1000);
    assert_eq!(
        format!("{:?}", response.delivery),
        "PairingDelivery([REDACTED])"
    );
    let pair = join.finish(&response.frame, 0, NOW).unwrap();
    assert_ne!(*response.delivery.credential(), pair.receive_key.to_bytes());
    let committed = b
        .store
        .install_sync_pairing_join("install", &b.master, 1, pair, NOW)
        .unwrap();
    let proof = b
        .reopen()
        .sync_pairing_dispatch("install", &b.master, 2, &committed.invitation_id, NOW)
        .unwrap();
    assert_eq!(proof.delivery.sender_mailbox, "f".repeat(64));
    assert_eq!(
        proof.delivery.recipient_mailbox,
        RelayClient::mailbox_key(a.vault.mailbox_owner())
    );
    assert_ne!(proof.delivery.credential(), response.delivery.credential());
    assert!(
        b.store
            .sync_pairing_dispatch(
                "install",
                &b.master,
                2,
                &committed.invitation_id,
                NOW + 24 * 60 * 60 * 1000
            )
            .is_err()
    );
    let retry = b
        .reopen()
        .sync_pairing_dispatch("install", &b.master, 2, &committed.invitation_id, NOW)
        .unwrap();
    assert_eq!(proof.frame, retry.frame);
    assert_eq!(proof.delivery.credential(), retry.delivery.credential());
    a.store
        .confirm_sync_pairing_installation(
            "install",
            &a.master,
            2,
            &approved.invitation_id,
            &proof.frame,
            NOW,
        )
        .unwrap();
    assert!(
        a.store
            .sync_pairing_dispatch("install", &a.master, 3, &approved.invitation_id, NOW)
            .is_err()
    );
}

#[test]
fn joining_nonempty_database_is_rejected_without_touching_local_data() {
    for provider in [false, true] {
        let a = Device::new(true);
        let b = Device::new(false);
        if provider {
            b.store
                .save_ai_provider_config(
                    None,
                    "keep",
                    "openai-compatible",
                    "https://invalid.test",
                    "synthetic",
                    "SYNTHETIC_KEEP_KEY",
                    0.7,
                    None,
                )
                .unwrap();
        } else {
            b.store
                .submit_input_as_event("keep original", "capture", None)
                .unwrap();
        }
        let (approved, join) = a.approve(&b, NOW);
        let pair = join.finish(&a.frame(&approved, NOW), 0, NOW).unwrap();
        assert!(
            b.store
                .install_sync_pairing_join("install", &b.master, 1, pair, NOW)
                .is_err()
        );
        assert!(b.store.sync_identity().unwrap().is_none());
        assert!(!b.store.has_sync_authority().unwrap());
        assert_eq!(b.revision(), 1);
        if provider {
            assert_eq!(
                b.store
                    .active_ai_provider_config()
                    .unwrap()
                    .unwrap()
                    .api_key,
                "SYNTHETIC_KEEP_KEY"
            );
        } else {
            assert_eq!(b.store.list_events().unwrap()[0].raw_text, "keep original");
        }
    }
}

#[test]
fn join_commit_and_membership_finalization_are_atomic_and_do_not_execute_work() {
    let a = Device::new(true);
    let b = Device::new(false);
    let (approved, join) = a.approve(&b, NOW);
    let pair = join.finish(&a.frame(&approved, NOW), 0, NOW).unwrap();
    b.store.connection.execute_batch("CREATE TRIGGER fail_pair_commit BEFORE UPDATE ON sync_local_vault BEGIN SELECT RAISE(ABORT,'synthetic commit failure'); END;").unwrap();
    assert!(
        b.store
            .install_sync_pairing_join("install", &b.master, 1, pair, NOW)
            .is_err()
    );
    assert!(b.store.sync_identity().unwrap().is_none());
    assert!(!b.store.has_sync_authority().unwrap());
    assert_eq!(b.revision(), 1);
    b.store
        .connection
        .execute_batch("DROP TRIGGER fail_pair_commit")
        .unwrap();
    // A new invitation is necessary after consuming an in-memory Noise result.
    let (approved, join) = a.approve(&b, NOW + 24 * 60 * 60 * 1000 + 1);
    let pair = join
        .finish(
            &a.frame(&approved, NOW + 24 * 60 * 60 * 1000 + 1),
            0,
            NOW + 24 * 60 * 60 * 1000 + 1,
        )
        .unwrap();
    assert_eq!(pair.key_version, 2);
    let committed = b
        .store
        .install_sync_pairing_join("install", &b.master, 1, pair, NOW + 24 * 60 * 60 * 1000 + 1)
        .unwrap();
    let proof = b.frame(&committed, NOW + 24 * 60 * 60 * 1000 + 1);
    let committed_a = a
        .store
        .confirm_sync_pairing_installation(
            "install",
            &a.master,
            approved.vault_revision,
            &approved.invitation_id,
            &proof,
            NOW + 24 * 60 * 60 * 1000 + 1,
        )
        .unwrap();
    let roster = a
        .store
        .sync_pairing_roster(
            "install",
            &a.master,
            committed_a.vault_revision,
            &approved.invitation_id,
        )
        .unwrap()
        .encode()
        .unwrap();
    b.store.connection.execute_batch("CREATE TRIGGER fail_pair_commit BEFORE UPDATE ON sync_local_vault BEGIN SELECT RAISE(ABORT,'synthetic commit failure'); END;").unwrap();
    assert!(
        b.store
            .finish_sync_pairing_join("install", &b.master, 2, &committed.invitation_id, &roster)
            .is_err()
    );
    assert!(!b.active(&b));
    assert!(!b.store.provider_keys_sealed().unwrap());
    assert_eq!(b.frame(&committed, NOW + 24 * 60 * 60 * 1000 + 1), proof);
    b.store
        .connection
        .execute_batch("DROP TRIGGER fail_pair_commit")
        .unwrap();
    b.store
        .finish_sync_pairing_join("install", &b.master, 2, &committed.invitation_id, &roster)
        .unwrap();
    assert!(b.active(&b));
    assert_eq!(b.store.analysis_job_stats().unwrap().running, 0);
}

#[test]
fn authority_change_requires_a_new_review_instead_of_refreshing_the_approval_basis() {
    let a = Device::new(true);
    let b = Device::new(false);
    let c = Device::new(false);
    let (pending_b, _, proof_b) = installed(&a, &b);
    let (pending_c, _, proof_c) = installed(&a, &c);
    let committed = a
        .store
        .confirm_sync_pairing_installation(
            "install",
            &a.master,
            pending_c.vault_revision,
            &pending_c.invitation_id,
            &proof_c,
            NOW,
        )
        .unwrap();
    assert!(
        a.store
            .confirm_sync_pairing_installation(
                "install",
                &a.master,
                committed.vault_revision,
                &pending_b.invitation_id,
                &proof_b,
                NOW
            )
            .is_err()
    );
    assert!(!a.active(&b));
    assert!(a.active(&c));
}

#[test]
fn pending_journal_is_encrypted_bounded_and_cannot_use_a_rolled_back_floor() {
    let a = Device::new(true);
    let mut peers = Vec::new();
    for _ in 0..8 {
        let b = Device::new(false);
        let (commit, _) = a.approve(&b, NOW);
        assert!(
            a.store
                .sync_pairing_frame(
                    "install",
                    &[0; 32],
                    commit.vault_revision,
                    &commit.invitation_id,
                    NOW
                )
                .is_err()
        );
        assert!(
            a.store
                .sync_pairing_frame(
                    "another-install",
                    &a.master,
                    commit.vault_revision,
                    &commit.invitation_id,
                    NOW
                )
                .is_err()
        );
        let frame = a.frame(&commit, NOW);
        let encrypted: Vec<u8> = a
            .store
            .connection
            .query_row("SELECT encrypted_secrets FROM sync_local_vault", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(!encrypted.windows(frame.len()).any(|w| w == frame));
        assert!(
            !encrypted
                .windows(b.vault.device().device_id.len())
                .any(|w| w == b.vault.device().device_id.as_bytes())
        );
        peers.push(b);
    }
    let ninth = Device::new(false);
    let (approval, _) = a.request(&ninth, NOW);
    assert!(
        a.store
            .approve_sync_pairing("install", &a.master, 9, approval, NOW)
            .unwrap_err()
            .to_string()
            .contains("pending_limit")
    );
    assert_eq!(a.revision(), 9);
    let (commit, _) = a.approve(&ninth, NOW + 24 * 60 * 60 * 1000 + 1);
    assert_eq!(commit.vault_revision, 10);
    assert!(a.store.load_sync_vault("install", &a.master, 11).is_err());
    assert!(
        a.store
            .sync_pairing_frame(
                "install",
                &a.master,
                9,
                &commit.invitation_id,
                NOW + 24 * 60 * 60 * 1000 + 1
            )
            .is_err()
    );
}
