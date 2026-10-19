use super::processor_of;
use super::signed_handoff_roster;
use super::*;

#[test]
fn handoff_proof_validation_and_successor_verification() {
    let owner_signer = DeviceIdentity::generate().unwrap();
    let old_processor_signer = DeviceIdentity::generate().unwrap();
    let new_processor_signer = DeviceIdentity::generate().unwrap();

    fn make_test_device(id: &str, stream_id: &str, signer: &DeviceIdentity) -> PairingDevice {
        PairingDevice {
            device_id: id.into(),
            stream_id: stream_id.into(),
            name: id.into(),
            platform: "test".into(),
            signing_key: signer.public_key(),
        }
    }

    let owner = make_test_device("dev-owner", "stream-owner", &owner_signer);
    let old_p = make_test_device("dev-old", "stream-old", &old_processor_signer);
    let new_p = make_test_device("dev-new", "stream-new", &new_processor_signer);

    let mut roster_v1 = MembershipRoster {
        version: 1,
        dataset_id: "ds".into(),
        owner: owner.clone(),
        revision: 1,
        processor_device_id: old_p.device_id.clone(),
        members: vec![
            RosterMember {
                device: new_p.clone(),
                active: true,
            },
            RosterMember {
                device: old_p.clone(),
                active: true,
            },
            RosterMember {
                device: owner.clone(),
                active: true,
            },
        ],
        handoff: None,
    };
    roster_v1
        .members
        .sort_by(|a, b| a.device.device_id.cmp(&b.device.device_id));
    roster_v1.validate().unwrap();

    // 1. Trying to change processor without handoff fails
    let mut roster_v2_no_handoff = roster_v1.clone();
    roster_v2_no_handoff.revision = 2;
    roster_v2_no_handoff.processor_device_id = new_p.device_id.clone();
    assert_eq!(
        roster_v2_no_handoff
            .validate_successor(&roster_v1)
            .unwrap_err()
            .to_string(),
        "sync_processing_handoff_required"
    );

    // 2. Handoff signed by wrong signer (e.g. new processor instead of old)
    let bad_proof = ProcessingYieldProof::sign(
        "ds",
        &old_p.device_id,
        &new_p.device_id,
        2,
        42,
        &new_processor_signer, // signed with wrong key
    )
    .unwrap();
    let mut roster_v2_bad_proof = roster_v2_no_handoff.clone();
    roster_v2_bad_proof.handoff = Some(bad_proof);
    assert!(roster_v2_bad_proof.validate_successor(&roster_v1).is_err());

    // 3. Valid proof signed by old processor succeeds
    let valid_proof = ProcessingYieldProof::sign(
        "ds",
        &old_p.device_id,
        &new_p.device_id,
        2,
        42,
        &old_processor_signer,
    )
    .unwrap();
    let mut roster_v2_valid = roster_v2_no_handoff.clone();
    roster_v2_valid.handoff = Some(valid_proof);
    assert!(roster_v2_valid.validate_successor(&roster_v1).is_ok());

    // 4. Handoff attached when processor didn't change is rejected
    let mut roster_v2_unneeded = roster_v1.clone();
    roster_v2_unneeded.revision = 2;
    roster_v2_unneeded.handoff = Some(
        ProcessingYieldProof::sign(
            "ds",
            &new_p.device_id,
            &old_p.device_id,
            2,
            42,
            &new_processor_signer,
        )
        .unwrap(),
    );
    assert_eq!(
        roster_v2_unneeded
            .validate_successor(&roster_v1)
            .unwrap_err()
            .to_string(),
        "sync_handoff_unexpected_without_processor_change"
    );
}

#[test]
fn handoff_roster_moves_old_processor_and_holds_target_at_pending_takeover() {
    let (a, b) = handoff_fixture();
    let proof = a.store.yield_processor_role(3, "b", &a.signer).unwrap();

    // A processor change without the signed handoff proof is rejected.
    let mut no_handoff = a.store.current_signed_roster().unwrap().unwrap().roster;
    no_handoff.revision = 3;
    no_handoff.processor_device_id = "b".into();
    no_handoff.handoff = None;
    let forged = SignedRoster::sign(no_handoff, &a.signer).unwrap();
    assert!(
        a.store
            .apply_signed_roster(&forged.encode().unwrap())
            .is_err()
    );
    assert!(
        b.store
            .apply_signed_roster(&forged.encode().unwrap())
            .is_err()
    );

    let signed = signed_handoff_roster(&a, proof, 3);

    // The old processor adopts the successor immediately.
    a.store
        .apply_signed_roster(&signed.encode().unwrap())
        .unwrap();
    assert_eq!(processor_of(&a.store), "b");
    assert!(!a.store.background_processing_allowed().unwrap());
    let handoffs: i64 = a
        .store
        .connection
        .query_row("SELECT COUNT(*) FROM sync_processor_handoffs", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(handoffs, 1);

    // The takeover target holds at pending_takeover with no processing rights,
    // and the state survives a restart.
    b.store
        .apply_signed_roster(&signed.encode().unwrap())
        .unwrap();
    assert_eq!(processor_of(&b.store), "a");
    let state = b.store.get_processor_handoff_state().unwrap();
    assert_eq!(state.state, "pending_takeover");
    assert_eq!(state.from_device_id.as_deref(), Some("a"));
    assert_eq!(state.to_device_id.as_deref(), Some("b"));
    assert!(state.can_confirm);
    assert!(!b.store.background_processing_allowed().unwrap());
    let reopened = Store::open(&b.store.path).unwrap();
    assert_eq!(
        reopened.get_processor_handoff_state().unwrap().state,
        "pending_takeover"
    );
}

#[test]
fn takeover_requires_frontier_alignment_and_restores_processing() {
    let (a, b) = handoff_fixture();
    // Author one local op so the handoff boundary is non-trivial (final = 1).
    a.store
        .submit_input_as_event("frontier fact", "capture", None)
        .unwrap();
    let proof = a.store.yield_processor_role(3, "b", &a.signer).unwrap();
    assert_eq!(proof.final_sequence, 1);
    let signed = signed_handoff_roster(&a, proof, 3);
    a.store
        .apply_signed_roster(&signed.encode().unwrap())
        .unwrap();
    b.store
        .apply_signed_roster(&signed.encode().unwrap())
        .unwrap();

    // The old processor's stream frontier is still 0: takeover is blocked.
    assert_eq!(
        b.store
            .confirm_processor_takeover()
            .unwrap_err()
            .to_string(),
        "sync_frontier_lagging"
    );
    assert_eq!(processor_of(&b.store), "a");
    assert!(!b.store.background_processing_allowed().unwrap());

    // Catch the frontier up to the handoff boundary.
    let capture = a.store.pending_sync_operations("b", 10).unwrap().remove(0);
    b.store
        .receive_sync_operation("a", &serde_json::to_vec(&capture).unwrap())
        .unwrap();

    b.store.confirm_processor_takeover().unwrap();
    assert_eq!(processor_of(&b.store), "b");
    let state = b.store.get_processor_handoff_state().unwrap();
    assert_eq!(state.state, "active");
    assert!(!state.can_confirm);
    assert!(b.store.background_processing_allowed().unwrap());

    // Confirm is single-shot: the state machine never goes backwards implicitly.
    assert_eq!(
        b.store
            .confirm_processor_takeover()
            .unwrap_err()
            .to_string(),
        "sync_processor_not_in_pending_takeover"
    );
}

#[test]
fn delayed_analysis_from_former_processor_is_bounded_by_handoff() {
    let (a, b) = handoff_fixture();
    // The current processor completes two analyses (origin sequences 1 and 2).
    let mut results = Vec::new();
    for text in ["event one", "event two"] {
        b.store
            .submit_input_as_event(text, "capture", None)
            .unwrap();
        // Deliver every pending capture; re-delivery of an already-applied op
        // is deduplicated on the receiving side.
        for capture in b.store.pending_sync_operations("a", 10).unwrap() {
            a.store
                .receive_sync_operation("b", &serde_json::to_vec(&capture).unwrap())
                .unwrap();
        }
        let job = a.store.claim_analysis_job().unwrap().unwrap();
        a.store
            .complete_analysis(&job, "event-analysis", r#"{"recordable":true}"#)
            .unwrap();
    }
    results = a.store.pending_sync_operations("b", 10).unwrap();
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|op| op.origin_device_id == "a"));
    assert_eq!(results[0].origin_sequence, 1);
    assert_eq!(results[1].origin_sequence, 2);

    // Handoff with final_sequence = 2.
    let proof = a.store.yield_processor_role(3, "b", &a.signer).unwrap();
    assert_eq!(proof.final_sequence, 2);
    let signed = signed_handoff_roster(&a, proof, 3);
    a.store
        .apply_signed_roster(&signed.encode().unwrap())
        .unwrap();
    b.store
        .apply_signed_roster(&signed.encode().unwrap())
        .unwrap();

    // While still in pending_takeover the effective processor is unchanged, so
    // the delayed results apply normally.
    for result in &results {
        let report = b
            .store
            .receive_sync_operation("a", &serde_json::to_vec(result).unwrap())
            .unwrap();
        assert_eq!(report.rejected, 0);
        assert_eq!(report.applied, 1);
    }
    let count: i64 = b
        .store
        .connection
        .query_row("SELECT COUNT(*) FROM event_analyses", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 2);

    // Catch up and confirm: the former processor is no longer the current one.
    b.store.confirm_processor_takeover().unwrap();
    assert_eq!(processor_of(&b.store), "b");

    // Beyond the boundary: a follow-up analysis op from the former processor
    // past final_sequence is strictly rejected.
    let mut late = results[1].clone();
    late.origin_sequence = 3;
    late.op_id = "stream-a:3".into();
    let report = b
        .store
        .receive_sync_operation("a", &serde_json::to_vec(&late).unwrap())
        .unwrap();
    assert_eq!(report.rejected, 1);
    let count: i64 = b
        .store
        .connection
        .query_row("SELECT COUNT(*) FROM event_analyses", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 2, "no late analysis may be applied");

    // At or below the boundary a delayed analysis from the former processor is
    // still accepted after the takeover (idempotent replay is a legal path).
    let report = b
        .store
        .receive_sync_operation("a", &serde_json::to_vec(&results[0]).unwrap())
        .unwrap();
    assert_eq!(report.rejected, 0);
}

#[test]
fn owner_approves_handoff_and_enforces_proof_boundaries() {
    // --- Full owner loop: owner == old processor.
    let (a, b) = handoff_fixture();
    let proof = a.store.yield_processor_role(3, "b", &a.signer).unwrap();
    let signed = a
        .store
        .approve_processor_handoff(&proof, &a.signer)
        .unwrap();
    assert_eq!(signed.roster.revision, 3);
    assert_eq!(signed.roster.processor_device_id, "b");
    assert_eq!(processor_of(&a.store), "b");
    b.store
        .apply_signed_roster(&signed.encode().unwrap())
        .unwrap();
    assert_eq!(
        b.store.get_processor_handoff_state().unwrap().state,
        "pending_takeover"
    );

    // --- Owner != old processor: after the takeover, b yields back and only
    // the owner (a) can approve a successor roster. No ops were authored, so
    // b's frontier is already at the boundary (final_sequence = 0).
    b.store.confirm_processor_takeover().unwrap();
    let back = b.store.yield_processor_role(4, "a", &b.signer).unwrap();
    assert_eq!(back.from_device_id, "b");
    assert_eq!(back.to_device_id, "a");

    // A non-owner signer can never issue the successor roster.
    assert_eq!(
        b.store
            .approve_processor_handoff(&back, &b.signer)
            .unwrap_err()
            .to_string(),
        "sync_membership_owner_required"
    );
    // Proof revision must be exactly live + 1.
    let mut stale = back.clone();
    stale.roster_revision = 3;
    assert_eq!(
        a.store
            .approve_processor_handoff(&stale, &a.signer)
            .unwrap_err()
            .to_string(),
        "sync_handoff_revision_mismatch"
    );
    // Proof source must be the live processor.
    let mut wrong_source = back.clone();
    wrong_source.from_device_id = "a".into();
    wrong_source.to_device_id = "b".into();
    assert_eq!(
        a.store
            .approve_processor_handoff(&wrong_source, &a.signer)
            .unwrap_err()
            .to_string(),
        "sync_handoff_source_mismatch"
    );
    // Proof target must be an active roster member (correctly signed, so the
    // membership check is what rejects it).
    let stranger_target = ProcessingYieldProof::sign(
        "dataset",
        "b".to_string(),
        "zz".to_string(),
        4,
        back.final_sequence,
        &b.signer,
    )
    .unwrap();
    assert_eq!(
        a.store
            .approve_processor_handoff(&stranger_target, &a.signer)
            .unwrap_err()
            .to_string(),
        "sync_handoff_target_missing"
    );
    // A proof re-signed by anyone other than the old processor is rejected.
    let forged_proof = ProcessingYieldProof::sign(
        "dataset".to_string(),
        "b".to_string(),
        "a".to_string(),
        4,
        back.final_sequence,
        &a.signer,
    )
    .unwrap();
    assert_eq!(
        a.store
            .approve_processor_handoff(&forged_proof, &a.signer)
            .unwrap_err()
            .to_string(),
        "sync_handoff_signature_invalid"
    );

    // The legitimate proof issues the successor roster and returns the role.
    let signed = a.store.approve_processor_handoff(&back, &a.signer).unwrap();
    assert_eq!(signed.roster.revision, 4);
    assert_eq!(signed.roster.processor_device_id, "a");
    // a is now the takeover TARGET: the local processor pointer stays on the
    // old device until the explicit confirm, and the state holds there.
    assert_eq!(processor_of(&a.store), "b");
    let state = a.store.get_processor_handoff_state().unwrap();
    assert_eq!(state.state, "pending_takeover");
}
