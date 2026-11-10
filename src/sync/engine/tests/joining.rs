use super::*;
use crate::sync::{
    membership::SignedRoster,
    pairing::{InvitationRoute, PairingInvitation, PendingJoin},
};

struct Joining {
    owner_id: String,
    joiner_id: String,
    a: Database,
    b: Database,
    signer: DeviceIdentity,
    key: ContentKey,
    roster: SignedRoster,
    enrollment: crate::sync::enrollment::EnrollmentRecord,
}
impl Joining {
    fn new() -> Self {
        fn fresh() -> Database {
            let directory =
                std::env::temp_dir().join(format!("elsewhen-joining-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&directory).unwrap();
            Database {
                store: Store::open(&directory.join("test.db")).unwrap(),
                directory,
            }
        }
        let a = fresh();
        let mut b = fresh();
        let va = a
            .store
            .create_sync_vault("owner-install", &[41; 32], "owner", "test")
            .unwrap();
        let vb = b
            .store
            .create_sync_vault("join-install", &[42; 32], "joiner", "test")
            .unwrap();
        let owner = va.device().clone();
        let joiner = vb.device().clone();
        let owner_id = owner.device_id.clone();
        let joiner_id = joiner.device_id.clone();
        let signer = DeviceIdentity::from_pkcs8(&va.identity().to_pkcs8()).unwrap();
        let join_signer = DeviceIdentity::from_pkcs8(&vb.identity().to_pkcs8()).unwrap();
        for (store, device) in [(&a.store, &owner), (&b.store, &joiner)] {
            store
                .initialize_sync_replica(&ReplicaIdentity {
                    dataset_id: "dataset".into(),
                    device_id: device.device_id.clone(),
                    stream_id: device.stream_id.clone(),
                    processor_device_id: owner_id.clone(),
                })
                .unwrap();
        }
        b.store = Store::open(&b.store.path)
            .unwrap()
            .with_provider_master([42; 32]);
        a.store.initialize_sync_authority(&owner, &signer).unwrap();
        let now = 1_800_000_000_000;
        let (invite, pending) = PairingInvitation::issue_for_relay(
            "dataset",
            owner,
            &signer,
            now,
            InvitationRoute {
                relay_url: "http://localhost:9876".into(),
                issuer_mailbox: RelayClient::mailbox_key(va.mailbox_owner()),
                request_sender: "ef".repeat(32),
            },
        )
        .unwrap();
        let (join, request) = PendingJoin::begin_for_relay(
            invite,
            joiner,
            &join_signer,
            now,
            RelayClient::mailbox_key(vb.mailbox_owner()),
        )
        .unwrap();
        let (completion, response) = pending
            .receive(&request, now)
            .unwrap()
            .approve_for_dataset(&signer, 1, now, a.store.sync_roster_anchor().unwrap())
            .unwrap();
        let approved = join.finish(&response, 0, now).unwrap();
        b.store
            .install_sync_vault_pair("join-install", &[42; 32], 1, &approved)
            .unwrap();
        b.store.pin_sync_authority_from_pair(&approved).unwrap();
        let enrollment = crate::sync::enrollment::EnrollmentRecord::new(
            &approved,
            crate::sync::enrollment::EnrollmentRole::Joiner,
            now,
        )
        .unwrap();
        let confirmed = completion
            .confirm(&approved.completion_proof(&join_signer).unwrap(), now)
            .unwrap();
        let roster = a.store.grant_sync_pair(1, &confirmed, &signer).unwrap();
        Self {
            owner_id,
            joiner_id,
            a,
            b,
            signer,
            key: approved.receive_key,
            roster,
            enrollment,
        }
    }
    fn wires(&self) -> Vec<(String, Vec<u8>)> {
        self.a
            .store
            .submit_input_as_event("recorded during join", "capture", None)
            .unwrap();
        stage_operations(&self.a.store, &self.joiner_id, 1, &self.key, &self.signer).unwrap();
        self.a
            .store
            .save_ai_provider_config(
                None,
                "provider",
                "openai-compatible",
                "http://localhost:9876/v1",
                "model",
                "SYNTHETIC_KEY",
                0.5,
                Some(1024),
            )
            .unwrap();
        stage_provider_updates(
            &self.a.store,
            &[EncryptionPeer {
                device_id: &self.joiner_id,
                key_version: 1,
                key: &self.key,
            }],
            &self.signer,
        )
        .unwrap();
        self.a.wires(&self.joiner_id)
    }
}

#[test]
fn joining_business_mail_waits_without_rejections_ack_or_drop_then_applies_once() {
    let j = Joining::new();
    let wires = j.wires();
    assert!(!wires.is_empty());
    // A bounded loopback relay records actual HTTP verbs: all nine scans must
    // fetch only, even after crossing the ordinary three-rejection threshold.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let sender_mb = "ab".repeat(32);
    let envelopes:Vec<_> = wires.iter().take(3).enumerate().map(|(index,(_,bytes))|serde_json::json!({"id":format!("{:016}",index+1),"sender_mb":sender_mb,"seq_hint":index+1,"ciphertext":STANDARD.encode(bytes),"created_at":1,"expires_at":9_000_000_000_000_i64})).collect();
    let waiting = envelopes.len();
    let body = serde_json::json!({"envelopes":envelopes,"next_cursor":format!("{waiting:016}"),"has_more":false}).to_string();
    let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let requests = calls.clone();
    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let active = running.clone();
    let server = std::thread::spawn(move || {
        while active.load(std::sync::atomic::Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream
                        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                        .unwrap();
                    let mut buffer = [0; 8192];
                    let n = std::io::Read::read(&mut stream, &mut buffer).unwrap();
                    let first = String::from_utf8_lossy(&buffer[..n])
                        .lines()
                        .next()
                        .unwrap()
                        .to_owned();
                    let response = if first.starts_with("GET /fetch?") {
                        body.clone()
                    } else {
                        "{\"deleted\":1}".into()
                    };
                    requests.lock().unwrap().push(first);
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{response}",
                        response.len()
                    );
                    std::io::Write::write_all(&mut stream, response.as_bytes()).unwrap();
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(1))
                }
                Err(e) => panic!("{e}"),
            }
        }
    });
    let client = RelayClient::new(&url).unwrap();
    for _ in 0..9 {
        let report = pull_page(
            &j.b.store,
            &client,
            &"cd".repeat(32),
            &[1; 32],
            &[ReceivingPeer {
                device_id: &j.owner_id,
                mailbox_id: &sender_mb,
                key_version: 1,
                key: &j.key,
                public_key: &j.signer.public_key(),
            }],
        )
        .unwrap();
        assert_eq!(
            (
                report.waiting,
                report.rejected,
                report.dropped,
                report.accepted
            ),
            (waiting, 0, 0, 0)
        );
    }
    running.store(false, std::sync::atomic::Ordering::Relaxed);
    server.join().unwrap();
    assert_eq!(calls.lock().unwrap().len(), 9);
    assert!(
        calls
            .lock()
            .unwrap()
            .iter()
            .all(|s| s.starts_with("GET /fetch?"))
    );
    assert_eq!(j.b.count("sync_wire_rejections"), 0);
    assert_eq!(j.b.count("sync_receipts"), 0);
    assert_eq!(j.b.count("events"), 0);
    let reopened = Store::open(&j.b.store.path)
        .unwrap()
        .with_provider_master([42; 32]);
    assert!(!accept(
        &reopened,
        &wires[0].1,
        &j.owner_id,
        &j.key,
        &j.signer
    ));
    let membership = crypto::seal(
        EnvelopeHeader::new(
            "dataset",
            &j.owner_id,
            &j.joiner_id,
            1,
            membership_message_id(&j.roster).unwrap(),
        )
        .unwrap(),
        &j.key,
        &j.signer,
        &j.roster.encode().unwrap(),
    )
    .unwrap()
    .to_bytes()
    .unwrap();
    assert!(
        accept(&reopened, &membership, &j.owner_id, &j.key, &j.signer),
        "roster controls must progress during the wait"
    );
    reopened.enable_provider_key_sealing().unwrap();
    for _ in 0..2 {
        for (_, bytes) in &wires {
            assert!(accept(&reopened, bytes, &j.owner_id, &j.key, &j.signer));
        }
    }
    assert_eq!(j.b.count("events"), 1);
    assert_eq!(
        reopened.list_ai_provider_configs_for_runtime().unwrap()[0].api_key,
        "SYNTHETIC_KEY"
    );
    assert_eq!(
        j.b.store.list_events().unwrap()[0].raw_text,
        "recorded during join"
    );
}

#[test]
fn joining_wait_requires_pinned_owner_valid_signature_domain_and_never_revoked_membership() {
    let j = Joining::new();
    for id in [
        "op-early",
        "op-part-early",
        "provider:early",
        "snapshot-start:early",
    ] {
        let bytes = sealed_control(id, &j.a, &j.b, &j.key, &j.signer);
        assert!(!accept(&j.b.store, &bytes, &j.owner_id, &j.key, &j.signer));
        let attacker = DeviceIdentity::generate().unwrap();
        let forged = sealed_control(id, &j.a, &j.b, &j.key, &attacker);
        assert!(
            accept_result(&j.b.store, &forged, &j.owner_id, &j.key, &j.signer).is_err(),
            "forged signature cannot masquerade as a dependency wait"
        );
        assert!(
            accept_result(&j.b.store, &forged, &j.owner_id, &j.key, &attacker).is_err(),
            "a replacement owner key is untrusted"
        );
        for header in [
            EnvelopeHeader::new("another-dataset", &j.owner_id, &j.joiner_id, 1, id).unwrap(),
            EnvelopeHeader::new("dataset", &j.owner_id, "another-device", 1, id).unwrap(),
            EnvelopeHeader::new("dataset", &j.owner_id, &j.joiner_id, 2, id).unwrap(),
        ] {
            let bad = crypto::seal(header, &j.key, &j.signer, b"{}")
                .unwrap()
                .to_bytes()
                .unwrap();
            assert!(accept_result(&j.b.store, &bad, &j.owner_id, &j.key, &j.signer).is_err());
        }
        let stranger = Database::new("stranger");
        let bad = sealed_control(id, &stranger, &j.b, &j.key, &attacker);
        assert!(accept_result(&j.b.store, &bad, "stranger", &j.key, &attacker).is_err());
    }
    j.b.store
        .apply_signed_roster(&j.roster.encode().unwrap())
        .unwrap();
    let revoked =
        j.a.store
            .revoke_signed_sync_member(2, &j.joiner_id, &j.signer)
            .unwrap();
    j.b.store
        .apply_signed_roster(&revoked.encode().unwrap())
        .unwrap();
    let bytes = sealed_control("op-after-revoke", &j.a, &j.b, &j.key, &j.signer);
    assert!(accept_result(&j.b.store, &bytes, &j.owner_id, &j.key, &j.signer).is_err());
    assert_eq!(j.b.count("events"), 0);
}

#[test]
fn joining_enrollment_path_defers_verified_business_and_snapshot_start_only_after_authentication() {
    let j = Joining::new();
    for id in ["op-early", "provider:early", "snapshot-start:early"] {
        let bytes = sealed_control(id, &j.a, &j.b, &j.key, &j.signer);
        let outcome = enrollment::accept(
            &j.b.store,
            "test-install",
            &[0; 32],
            1,
            &j.enrollment,
            &STANDARD.encode(&bytes),
            &j.key,
        )
        .unwrap();
        assert!(matches!(outcome, enrollment::ReceiveOutcome::Waiting));
        let attacker = DeviceIdentity::generate().unwrap();
        let forged = sealed_control(id, &j.a, &j.b, &j.key, &attacker);
        assert!(
            enrollment::accept(
                &j.b.store,
                "test-install",
                &[0; 32],
                1,
                &j.enrollment,
                &STANDARD.encode(&forged),
                &j.key
            )
            .is_err()
        );
    }
    assert_eq!(j.b.count("sync_wire_receipts"), 0);
    assert_eq!(j.b.count("events"), 0);
}
