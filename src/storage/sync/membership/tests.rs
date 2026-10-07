use super::*;
use crate::sync::{
    crypto::{self, ContentKey, EncryptedEnvelope, EnvelopeHeader},
    engine,
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
