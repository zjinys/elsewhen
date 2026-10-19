use super::enroll;
use super::pairing;
use super::*;

#[test]
fn concurrent_membership_changes_invalidate_old_pairing_approval() {
    let a = Db::owner();
    let b = Db::new("b");
    let c = Db::new("c");
    let (old, _) = pairing(&a, &b);
    enroll(&a, &c);
    assert!(a.store.grant_sync_pair(1, &old, &a.signer).is_err());
    assert!(a.store.grant_sync_pair(2, &old, &a.signer).is_err());
    assert!(!a.active("b"));
    assert_eq!(a.store.sync_roster_anchor().unwrap().revision, 2);
}

#[test]
fn yield_requires_quiescence_and_disables_background_claims() {
    let a = Db::owner();

    // Only the active processor can yield.
    let other = Db::new("x");
    assert_eq!(
        other
            .store
            .yield_processor_role(2, "a", &other.signer)
            .unwrap_err()
            .to_string(),
        "sync_membership_not_processor"
    );

    // A running background job blocks the yield (fail-closed quiescence).
    a.store
        .submit_input_as_event("quiescence fact", "capture", None)
        .unwrap();
    let job = a.store.claim_analysis_job().unwrap().unwrap();
    assert_eq!(
        a.store
            .yield_processor_role(2, "b", &a.signer)
            .unwrap_err()
            .to_string(),
        "sync_background_processing_must_finish_before_enable"
    );
    assert_eq!(
        a.store.get_processor_handoff_state().unwrap().state,
        "idle",
        "failed yield must not leave a state row behind"
    );

    // Once in-flight work is finished, the yield succeeds and records the boundary.
    a.store
        .complete_analysis(&job, "event-analysis", r#"{"recordable":true}"#)
        .unwrap();
    let proof = a.store.yield_processor_role(2, "b", &a.signer).unwrap();
    proof.verify(&a.signer.public_key()).unwrap();
    assert_eq!(proof.from_device_id, "a");
    assert_eq!(proof.to_device_id, "b");
    assert_eq!(proof.roster_revision, 2);
    let next: i64 = a
        .store
        .connection
        .query_row("SELECT next_sequence FROM sync_replica", [], |r| r.get(0))
        .unwrap();
    assert_eq!(proof.final_sequence as i64, next - 1);

    let state = a.store.get_processor_handoff_state().unwrap();
    assert_eq!(state.state, "relinquished");
    assert_eq!(state.from_device_id.as_deref(), Some("a"));
    assert_eq!(state.to_device_id.as_deref(), Some("b"));
    assert_eq!(state.final_sequence, Some(next - 1));

    // The permission view hard-blocks new background claims after the yield.
    assert!(!a.store.background_processing_allowed().unwrap());
    assert!(a.store.claim_analysis_job().unwrap().is_none());
}

#[test]
fn owner_converges_stale_member_to_current_roster_revision() {
    let a = Db::owner();
    let b = Db::new("b");
    let c = Db::new("c");
    // b 配对并授权加入：两端先到同一修订。
    let (approved_b, joining_b) = pairing(&a, &b);
    let v1 = a.store.sync_roster_anchor().unwrap().revision;
    let roster_v = a.store.grant_sync_pair(v1, &approved_b, &a.signer).unwrap();
    b.store
        .apply_signed_roster(&roster_v.encode().unwrap())
        .unwrap();
    let base = a
        .store
        .current_signed_roster()
        .unwrap()
        .unwrap()
        .roster
        .revision;
    // c 加入并随后被 owner 撤销：名册修订两次推进，b 保持离线错过全部新修订。
    let (approved_c, _joining_c) = pairing(&a, &c);
    let v2 = a.store.sync_roster_anchor().unwrap().revision;
    a.store.grant_sync_pair(v2, &approved_c, &a.signer).unwrap();
    let at_c = a
        .store
        .current_signed_roster()
        .unwrap()
        .unwrap()
        .roster
        .revision;
    a.store
        .revoke_signed_sync_member(at_c, "c", &a.signer)
        .unwrap();
    let head = a
        .store
        .current_signed_roster()
        .unwrap()
        .unwrap()
        .roster
        .revision;
    assert!(head > base, "名册修订必须已推进");
    // owner 收敛：stage-if-stale 补发当前修订给 b 且幂等。
    assert!(
        engine::stage_membership_roster_if_stale(&a.store, "b", 1, &approved_b.send_key, &a.signer)
            .unwrap(),
        "离线成员落后时必须补发"
    );
    let envelope = a
        .store
        .due_sync_wire_envelopes("b", 0, 10)
        .unwrap()
        .remove(0);
    assert!(
        envelope
            .message_id
            .starts_with(&format!("membership:{head}:")),
        "补发的必须是当前修订，得到 {}",
        envelope.message_id
    );
    let encoded = STANDARD.encode(&envelope.encrypted_envelope);
    engine::accept_relay_payload(
        &b.store,
        &encoded,
        "a",
        1,
        &joining_b.receive_key,
        &a.signer.public_key(),
    )
    .unwrap();
    assert_eq!(
        b.store
            .current_signed_roster()
            .unwrap()
            .unwrap()
            .roster
            .revision,
        head,
        "长离线成员回连必须收敛到当前修订"
    );
    // 在途（未收到对端回执）不产生第二封（幂等收敛；余下的是既有重试语义）。
    assert!(
        !engine::stage_membership_roster_if_stale(
            &a.store,
            "b",
            1,
            &approved_b.send_key,
            &a.signer
        )
        .unwrap()
    );
    assert_eq!(
        a.store
            .due_sync_wire_envelopes("b", 0, 10)
            .unwrap()
            .iter()
            .filter(|e| e.message_id.starts_with("membership:"))
            .count(),
        1
    );
    // 成员侧没有分发义务。
    assert!(
        !engine::stage_membership_roster_if_stale(&b.store, "a", 1, &joining_b.send_key, &b.signer)
            .unwrap()
    );
    // 撤销成员（c 不活跃）不再获得名册补发。
    assert!(
        !engine::stage_membership_roster_if_stale(
            &a.store,
            "c",
            1,
            &approved_c.send_key,
            &a.signer
        )
        .unwrap()
    );
}
