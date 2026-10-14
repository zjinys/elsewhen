use super::*;
use crate::sync::{
    crypto::{self, ContentKey, EncryptedEnvelope, EnvelopeHeader},
    engine,
    membership::ProcessingYieldProof,
    pairing::{PairingInvitation, PendingJoin},
};
use base64::{Engine, engine::general_purpose::STANDARD};

struct Db {
    store: Store,
    signer: DeviceIdentity,
    device: PairingDevice,
    dir: std::path::PathBuf,
}
impl Db {
    fn new(id: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("elsewhen-authority-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(&dir.join("test.db")).unwrap();
        let signer = DeviceIdentity::generate().unwrap();
        let device = PairingDevice {
            device_id: id.into(),
            stream_id: format!("stream-{id}"),
            name: id.into(),
            platform: "test".into(),
            signing_key: signer.public_key(),
        };
        store
            .initialize_sync_replica(&crate::storage::sync::ReplicaIdentity {
                dataset_id: "dataset".into(),
                device_id: id.into(),
                stream_id: device.stream_id.clone(),
                processor_device_id: "a".into(),
            })
            .unwrap();
        Self {
            store,
            signer,
            device,
            dir,
        }
    }
    fn owner() -> Self {
        let a = Self::new("a");
        a.store
            .initialize_sync_authority(&a.device, &a.signer)
            .unwrap();
        a
    }
    fn active(&self, id: &str) -> bool {
        self.store
            .connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sync_members WHERE device_id=?1 AND active=1)",
                [id],
                |r| r.get(0),
            )
            .unwrap()
    }
}
impl Drop for Db {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
fn pairing(a: &Db, b: &Db) -> (ApprovedPair, ApprovedPair) {
    let now = 1_800_000_000_000;
    let anchor = a.store.sync_roster_anchor().unwrap();
    let (invite, pending) =
        PairingInvitation::issue("dataset", a.device.clone(), &a.signer, now).unwrap();
    let (join, request) = PendingJoin::begin(invite, b.device.clone(), &b.signer, now).unwrap();
    let (completion, reply) = pending
        .receive(&request, now)
        .unwrap()
        .approve_for_dataset(&a.signer, 1, now, anchor.clone())
        .unwrap();
    let joining = join.finish(&reply, 0, now).unwrap();
    assert_eq!(joining.authority.as_ref(), Some(&anchor));
    b.store.pin_sync_authority_from_pair(&joining).unwrap();
    let confirmed = completion
        .confirm(&joining.completion_proof(&b.signer).unwrap(), now)
        .unwrap();
    (confirmed, joining)
}
fn enroll(a: &Db, b: &Db) -> (ApprovedPair, ApprovedPair, SignedRoster) {
    let (approved, joining) = pairing(a, b);
    let version = a.store.sync_roster_anchor().unwrap().revision;
    let roster = a
        .store
        .grant_sync_pair(version, &approved, &a.signer)
        .unwrap();
    b.store
        .apply_signed_roster(&roster.encode().unwrap())
        .unwrap();
    (approved, joining, roster)
}

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
fn noise_pins_owner_but_only_signed_grant_enrolls_the_joiner() {
    let a = Db::owner();
    let b = Db::new("b");
    let (approved, joining) = pairing(&a, &b);
    assert!(!b.active("b"));
    assert!(b.store.authorize_sync_member("a", "stream-a").is_err());
    assert!(b.store.require_local_sync_membership().is_err());
    let mut forged = a.store.current_signed_roster().unwrap().unwrap().roster;
    forged.owner = b.device.clone();
    forged.processor_device_id = "b".into();
    forged.members = vec![RosterMember {
        device: b.device.clone(),
        active: true,
    }];
    let forged = SignedRoster::sign(forged, &b.signer).unwrap();
    assert!(
        b.store
            .apply_signed_roster(&forged.encode().unwrap())
            .is_err()
    );
    assert!(a.store.grant_sync_pair(1, &joining, &b.signer).is_err());
    let grant = a.store.grant_sync_pair(1, &approved, &a.signer).unwrap();
    assert!(!b.active("b"));
    b.store
        .apply_signed_roster(&grant.encode().unwrap())
        .unwrap();
    assert!(b.active("b") && b.active("a"));
    assert!(a.active("b"));
    let reopened = Store::open(&b.store.path).unwrap();
    assert_eq!(reopened.sync_roster_anchor().unwrap().revision, 2);
    assert!(
        reopened
            .require_sync_transport_key("a", &a.signer.public_key(), false)
            .is_ok()
    );
    assert!(
        reopened
            .require_sync_transport_key("a", &b.signer.public_key(), false)
            .is_err()
    );
}

#[test]
fn stale_forked_or_self_renewed_rosters_cannot_restore_revoked_identity() {
    let a = Db::owner();
    let b = Db::new("b");
    let c = Db::new("c");
    let (_, _, grant_b) = enroll(&a, &b);
    let (_, _, grant_c) = enroll(&a, &c);
    let revoked = a
        .store
        .revoke_signed_sync_member(3, "b", &a.signer)
        .unwrap();
    // A complete newer roster may skip an intermediate revision.
    b.store
        .apply_signed_roster(&revoked.encode().unwrap())
        .unwrap();
    c.store
        .apply_signed_roster(&revoked.encode().unwrap())
        .unwrap();
    assert!(!b.active("b") && !c.active("b"));
    assert!(
        !b.store
            .apply_signed_roster(&grant_c.encode().unwrap())
            .unwrap()
    );
    assert!(
        !b.store
            .apply_signed_roster(&grant_b.encode().unwrap())
            .unwrap()
    );
    assert!(!b.active("b"));
    assert!(b.store.authorize_sync_member("b", "stream-b").is_err());
    assert!(
        b.store
            .revoke_signed_sync_member(4, "c", &b.signer)
            .is_err()
    );
    let mut fork = revoked.roster.clone();
    fork.members
        .iter_mut()
        .find(|m| m.device.device_id == "c")
        .unwrap()
        .device
        .name = "fork".into();
    assert!(
        b.store
            .apply_signed_roster(
                &SignedRoster::sign(fork, &a.signer)
                    .unwrap()
                    .encode()
                    .unwrap()
            )
            .is_err()
    );
    let mut reactivation = revoked.roster.clone();
    reactivation.revision = 5;
    reactivation
        .members
        .iter_mut()
        .find(|m| m.device.device_id == "b")
        .unwrap()
        .active = true;
    assert!(
        c.store
            .apply_signed_roster(
                &SignedRoster::sign(reactivation, &a.signer)
                    .unwrap()
                    .encode()
                    .unwrap()
            )
            .is_err()
    );
    let mut removed = revoked.roster.clone();
    removed.revision = 5;
    removed.members.retain(|m| m.device.device_id != "b");
    assert!(
        c.store
            .apply_signed_roster(
                &SignedRoster::sign(removed, &a.signer)
                    .unwrap()
                    .encode()
                    .unwrap()
            )
            .is_err()
    );
    let mut identity_change = revoked.roster.clone();
    identity_change.revision = 5;
    identity_change
        .members
        .iter_mut()
        .find(|m| m.device.device_id == "c")
        .unwrap()
        .device
        .signing_key = [42; 32];
    assert!(
        c.store
            .apply_signed_roster(
                &SignedRoster::sign(identity_change, &a.signer)
                    .unwrap()
                    .encode()
                    .unwrap()
            )
            .is_err()
    );
    assert!(
        a.store
            .revoke_signed_sync_member(4, "a", &a.signer)
            .is_err()
    );
    b.store
        .submit_input_as_event("kept after revocation", "capture", None)
        .unwrap();
    assert!(b.store.due_sync_wire_envelopes("a", i64::MAX, 10).is_err());
    assert_eq!(b.store.list_events().unwrap().len(), 1);
}

#[test]
fn membership_wire_is_atomic_with_receipt_and_rejects_wrong_sender_key() {
    let a = Db::owner();
    let b = Db::new("b");
    let (approved, joining) = pairing(&a, &b);
    a.store.grant_sync_pair(1, &approved, &a.signer).unwrap();
    engine::stage_membership_roster(&a.store, "b", 1, &approved.send_key, &a.signer).unwrap();
    let envelope = a
        .store
        .due_sync_wire_envelopes("b", 0, 10)
        .unwrap()
        .remove(0);
    let encoded = STANDARD.encode(&envelope.encrypted_envelope);
    b.store.connection.execute_batch("CREATE TRIGGER receipt_failure BEFORE INSERT ON sync_wire_receipts BEGIN SELECT RAISE(ABORT,'synthetic receipt failure'); END;").unwrap();
    assert!(
        engine::accept_relay_payload(
            &b.store,
            &encoded,
            "a",
            1,
            &joining.receive_key,
            &a.signer.public_key()
        )
        .is_err()
    );
    assert!(!b.active("b"));
    assert!(b.store.current_signed_roster().unwrap().is_none());
    b.store
        .connection
        .execute_batch("DROP TRIGGER receipt_failure")
        .unwrap();
    assert!(
        engine::accept_relay_payload(
            &b.store,
            &encoded,
            "a",
            1,
            &joining.receive_key,
            &a.signer.public_key()
        )
        .unwrap()
    );
    assert!(b.active("b"));
    engine::stage_receipts(&b.store, "a", 1, &joining.send_key, &b.signer).unwrap();
    let receipt = b
        .store
        .due_sync_wire_envelopes("a", 0, 10)
        .unwrap()
        .remove(0);
    assert!(
        engine::accept_relay_payload(
            &a.store,
            &STANDARD.encode(receipt.encrypted_envelope),
            "b",
            1,
            &approved.receive_key,
            &b.signer.public_key()
        )
        .unwrap()
    );
    assert!(
        a.store
            .due_sync_wire_envelopes("b", i64::MAX, 10)
            .unwrap()
            .is_empty()
    );
    assert!(
        engine::accept_relay_payload(
            &b.store,
            &encoded,
            "a",
            1,
            &joining.receive_key,
            &a.signer.public_key()
        )
        .unwrap()
    );
    a.store
        .submit_input_as_event("member fact", "capture", None)
        .unwrap();
    engine::stage_operations(&a.store, "b", 1, &approved.send_key, &a.signer).unwrap();
    let wire = a
        .store
        .due_sync_wire_envelopes("b", 0, 10)
        .unwrap()
        .remove(0);
    let trusted = EnvelopeHeader::new("dataset", "a", "b", 1, wire.message_id).unwrap();
    let plain = crypto::open(
        &EncryptedEnvelope::from_bytes(&wire.encrypted_envelope).unwrap(),
        &trusted,
        &joining.receive_key,
        &a.signer.public_key(),
    )
    .unwrap();
    let stranger = DeviceIdentity::generate().unwrap();
    let forged = crypto::seal(trusted, &joining.receive_key, &stranger, &plain)
        .unwrap()
        .to_bytes()
        .unwrap();
    assert!(
        engine::accept_relay_payload(
            &b.store,
            &STANDARD.encode(forged),
            "a",
            1,
            &joining.receive_key,
            &stranger.public_key()
        )
        .is_err()
    );
    assert_eq!(b.store.list_events().unwrap().len(), 0);
    assert!(
        engine::accept_relay_payload(
            &b.store,
            &STANDARD.encode(wire.encrypted_envelope),
            "a",
            1,
            &joining.receive_key,
            &a.signer.public_key()
        )
        .unwrap()
    );
}

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
fn roots_and_approval_do_not_overwrite_existing_independent_data() {
    let a = Db::owner();
    let b = Db::new("b");
    b.store
        .submit_input_as_event("independent original", "capture", None)
        .unwrap();
    let now = 1_800_000_000_000;
    let (invite, pending) =
        PairingInvitation::issue("dataset", a.device.clone(), &a.signer, now).unwrap();
    let (join, request) = PendingJoin::begin(invite, b.device.clone(), &b.signer, now).unwrap();
    let (_, reply) = pending
        .receive(&request, now)
        .unwrap()
        .approve_for_dataset(&a.signer, 1, now, a.store.sync_roster_anchor().unwrap())
        .unwrap();
    let pair = join.finish(&reply, 0, now).unwrap();
    assert!(b.store.pin_sync_authority_from_pair(&pair).is_err());
    assert!(!b.store.has_sync_authority().unwrap());
    assert_eq!(
        b.store.list_events().unwrap()[0].raw_text,
        "independent original"
    );
}

#[test]
fn roster_payloads_are_bounded_signed_and_not_pairing_identity_proofs() {
    let a = Db::owner();
    let signed = a.store.current_signed_roster().unwrap().unwrap();
    let anchor = signed.roster.anchor().unwrap();
    let bytes = signed.encode().unwrap();
    assert_eq!(
        bytes,
        SignedRoster::sign(signed.roster.clone(), &a.signer)
            .unwrap()
            .encode()
            .unwrap()
    );
    let mut changed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    changed["roster"]["revision"] = serde_json::json!(2);
    assert!(SignedRoster::decode(&serde_json::to_vec(&changed).unwrap(), &anchor).is_err());
    let pairing_signature = a
        .signer
        .sign_pairing("approve", &serde_json::to_vec(&signed.roster).unwrap())
        .unwrap();
    changed = serde_json::from_slice(&bytes).unwrap();
    changed["signature"] = serde_json::json!(pairing_signature);
    assert!(SignedRoster::decode(&serde_json::to_vec(&changed).unwrap(), &anchor).is_err());
    assert!(
        SignedRoster::decode(
            &vec![0; crate::sync::membership::MAX_ROSTER_BYTES + 1],
            &anchor
        )
        .is_err()
    );
    let mut too_many = signed.roster.clone();
    too_many.members = vec![too_many.members[0].clone(); crate::sync::membership::MAX_MEMBERS + 1];
    assert!(SignedRoster::sign(too_many, &a.signer).is_err());
    let mut wrong_floor = anchor.clone();
    wrong_floor.revision = 2;
    assert!(SignedRoster::decode(&bytes, &wrong_floor).is_err());
    let mut wrong_digest = anchor;
    wrong_digest.digest = "0".repeat(64);
    assert!(SignedRoster::decode(&bytes, &wrong_digest).is_err());
}

#[test]
fn paired_content_producer_cannot_add_unsigned_snapshot_members() {
    use crate::storage::sync::snapshot::SnapshotExporter;
    let a = Db::owner();
    let b = Db::new("b");
    let c = Db::new("c");
    enroll(&a, &b);
    enroll(&a, &c);
    // Model a valid content producer sending a snapshot with fabricated member
    // metadata. It holds only B's signing key, never the owner's signing key.
    let producer = Db::new("b");
    for peer in ["a", "c", "intruder"] {
        producer
            .store
            .authorize_sync_member(peer, &format!("stream-{peer}"))
            .unwrap();
    }
    producer
        .store
        .submit_input_as_event("must roll back", "capture", None)
        .unwrap();
    let key = ContentKey::generate().unwrap();
    let mut export = SnapshotExporter::new(&producer.store, "c", "unsigned-members", 1).unwrap();
    c.store
        .begin_snapshot_import("unsigned-members", "b", 1)
        .unwrap();
    let mut index = 0;
    while let Some(bytes) = export.next_encrypted_chunk(&key, &b.signer).unwrap() {
        c.store
            .stage_snapshot_chunk(
                "unsigned-members",
                index,
                "b",
                1,
                &bytes,
                &key,
                &b.signer.public_key(),
            )
            .unwrap();
        index += 1;
    }
    let manifest = export.finish(&key, &b.signer).unwrap();
    let error = c
        .store
        .publish_snapshot(
            "unsigned-members",
            "b",
            1,
            &manifest,
            &key,
            &b.signer.public_key(),
        )
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("sync_snapshot_member_needs_authority")
    );
    assert!(!c.active("intruder"));
    assert!(c.store.list_events().unwrap().is_empty());
}

#[test]
fn old_content_snapshot_cannot_reactivate_a_revoked_member_or_assign_processor() {
    use crate::storage::sync::snapshot::{SnapshotExporter, SnapshotManifest};
    let a = Db::owner();
    let b = Db::new("b");
    let c = Db::new("c");
    enroll(&a, &b);
    enroll(&a, &c);
    a.store
        .submit_input_as_event("historical fact", "capture", None)
        .unwrap();
    let key = ContentKey::generate().unwrap();
    let mut exporter = SnapshotExporter::new(&a.store, "c", "old-view", 1).unwrap();
    let mut chunks = vec![];
    while let Some(bytes) = exporter.next_encrypted_chunk(&key, &a.signer).unwrap() {
        chunks.push(bytes);
    }
    let manifest = exporter.finish(&key, &a.signer).unwrap();
    let revoked = a
        .store
        .revoke_signed_sync_member(3, "b", &a.signer)
        .unwrap();
    c.store
        .apply_signed_roster(&revoked.encode().unwrap())
        .unwrap();
    c.store.begin_snapshot_import("old-view", "a", 1).unwrap();
    for (i, bytes) in chunks.iter().enumerate() {
        c.store
            .stage_snapshot_chunk("old-view", i, "a", 1, bytes, &key, &a.signer.public_key())
            .unwrap();
    }
    let header = EnvelopeHeader::new("dataset", "a", "c", 1, "snapshot-end:old-view").unwrap();
    let mut bad: SnapshotManifest = serde_json::from_slice(
        &crypto::open(
            &EncryptedEnvelope::from_bytes(&manifest).unwrap(),
            &header,
            &key,
            &a.signer.public_key(),
        )
        .unwrap(),
    )
    .unwrap();
    bad.processor_device_id = "b".into();
    let bad = crypto::seal(header, &key, &a.signer, &serde_json::to_vec(&bad).unwrap())
        .unwrap()
        .to_bytes()
        .unwrap();
    assert!(
        c.store
            .publish_snapshot("old-view", "a", 1, &bad, &key, &a.signer.public_key())
            .is_err()
    );
    assert!(
        c.store
            .publish_snapshot("old-view", "a", 1, &manifest, &key, &a.signer.public_key())
            .unwrap()
    );
    assert!(!c.active("b"));
    assert_eq!(c.store.sync_roster_anchor().unwrap().revision, 4);
    assert_eq!(c.store.list_events().unwrap().len(), 1);
    assert!(
        c.store
            .require_sync_transport_key("b", &b.signer.public_key(), false)
            .is_err()
    );
}

// ---------- P4d processor handoff defense lines ----------

fn handoff_fixture() -> (Db, Db) {
    let a = Db::owner();
    let b = Db::new("b");
    enroll(&a, &b);
    (a, b)
}

fn signed_handoff_roster(
    a: &Db,
    proof: ProcessingYieldProof,
    target_revision: u64,
) -> SignedRoster {
    let mut roster = a.store.current_signed_roster().unwrap().unwrap().roster;
    roster.revision = target_revision;
    roster.processor_device_id = proof.to_device_id.clone();
    roster.handoff = Some(proof);
    SignedRoster::sign(roster, &a.signer).unwrap()
}

fn processor_of(store: &Store) -> String {
    store.sync_identity().unwrap().unwrap().processor_device_id
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
