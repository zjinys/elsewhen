use super::super::*;
use super::installed;
use super::*;

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
