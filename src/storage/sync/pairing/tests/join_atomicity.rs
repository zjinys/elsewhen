use super::super::*;
use super::installed;
use super::*;

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
