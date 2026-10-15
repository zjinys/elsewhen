//! Actual Worker enrollment: encrypted local vaults, Noise, owner authorization,
//! delayed content, application receipts and independent relay revocation.
use crate::{
    storage::{Store, sync::ReplicaIdentity},
    sync::{
        crypto::ContentKey,
        engine,
        pairing::{InvitationRoute, PairingInvitation, PendingJoin},
        transport_relay::RelayClient,
    },
};

pub(super) fn roundtrip(client: &RelayClient, directory: &std::path::Path) {
    use base64::{Engine, engine::general_purpose::STANDARD};
    let master_a = ContentKey::generate().unwrap().to_bytes();
    let master_b = ContentKey::generate().unwrap().to_bytes();
    let a = Store::open(&directory.join("authority-owner.db"))
        .unwrap()
        .with_provider_master(master_a);
    let b = Store::open(&directory.join("authority-joiner.db"))
        .unwrap()
        .with_provider_master(master_b);
    let vault_a = a
        .create_sync_vault("owner-installation", &master_a, "owner", "test")
        .unwrap();
    let vault_b = b
        .create_sync_vault("joiner-installation", &master_b, "joiner", "test")
        .unwrap();
    let owner = &vault_a.device().device_id;
    let joiner = &vault_b.device().device_id;
    a.initialize_sync_replica(&ReplicaIdentity {
        dataset_id: "authority-roundtrip".into(),
        device_id: owner.clone(),
        stream_id: vault_a.device().stream_id.clone(),
        processor_device_id: owner.clone(),
    })
    .unwrap();
    a.initialize_sync_authority(vault_a.device(), vault_a.identity())
        .unwrap();
    a.enable_provider_key_sealing().unwrap();
    let now = chrono::Utc::now().timestamp_millis();
    let a_mailbox = client.create_mailbox(vault_a.mailbox_owner()).unwrap();
    let b_mailbox = client.create_mailbox(vault_b.mailbox_owner()).unwrap();
    let temporary_sender = RelayClient::mailbox_key(&ContentKey::generate().unwrap().to_bytes());
    let (invite, pending) = PairingInvitation::issue_for_relay(
        "authority-roundtrip",
        vault_a.device().clone(),
        vault_a.identity(),
        now,
        InvitationRoute {
            relay_url: std::env::var("ELSEWHEN_RELAY_TEST_URL").unwrap(),
            issuer_mailbox: a_mailbox.clone(),
            request_sender: temporary_sender.clone(),
        },
    )
    .unwrap();
    let invitation_credential = invite.relay_credential().unwrap();
    let invitation_expiry = invite.expires_at_ms();
    let response_credential =
        RelayClient::derive_sender_credential(&invitation_credential, &a_mailbox, &b_mailbox)
            .unwrap();
    client
        .register_sender(
            &a_mailbox,
            vault_a.mailbox_owner(),
            &temporary_sender,
            &invitation_credential,
            invitation_expiry,
            0,
        )
        .unwrap();
    client
        .register_sender(
            &b_mailbox,
            vault_b.mailbox_owner(),
            &a_mailbox,
            &response_credential,
            invitation_expiry,
            0,
        )
        .unwrap();
    let (join, request) = PendingJoin::begin_for_relay(
        invite,
        vault_b.device().clone(),
        vault_b.identity(),
        now,
        b_mailbox.clone(),
    )
    .unwrap();
    // Noise frames cross the real Worker; no invite PSK or Provider key is sent.
    let request_id = client
        .send(
            &a_mailbox,
            &temporary_sender,
            &invitation_credential,
            0,
            &STANDARD.encode(request),
            invitation_expiry,
        )
        .unwrap();
    let received = client
        .fetch(&a_mailbox, vault_a.mailbox_owner(), "0", 10)
        .unwrap();
    assert_eq!(received.len(), 1);
    let approval = pending
        .receive(&STANDARD.decode(&received[0].ciphertext).unwrap(), now)
        .unwrap();
    let committed_a = a
        .approve_sync_pairing("owner-installation", &master_a, 1, approval, now)
        .unwrap();
    assert_eq!(committed_a.vault_revision, 2);
    client
        .ack(&a_mailbox, vault_a.mailbox_owner(), &[request_id])
        .unwrap();
    let reopened_a = Store::open(&a.path).unwrap().with_provider_master(master_a);
    assert!(
        reopened_a
            .sync_pairing_frame(
                "owner-installation",
                &master_a,
                1,
                &committed_a.invitation_id,
                now
            )
            .is_err()
    );
    let response = reopened_a
        .sync_pairing_dispatch(
            "owner-installation",
            &master_a,
            2,
            &committed_a.invitation_id,
            now,
        )
        .unwrap();
    assert_eq!(
        response.delivery.relay_url,
        std::env::var("ELSEWHEN_RELAY_TEST_URL").unwrap()
    );
    let response_id = client
        .send(
            &response.delivery.recipient_mailbox,
            &response.delivery.sender_mailbox,
            response.delivery.credential(),
            0,
            &STANDARD.encode(&response.frame),
            response.delivery.expires_at_ms,
        )
        .unwrap();
    let received = client
        .fetch(&b_mailbox, vault_b.mailbox_owner(), "0", 10)
        .unwrap();
    assert_eq!(received.len(), 1);
    let joining = join
        .finish(&STANDARD.decode(&received[0].ciphertext).unwrap(), 0, now)
        .unwrap();
    let committed_b = b
        .install_sync_pairing_join("joiner-installation", &master_b, 1, joining, now)
        .unwrap();
    client
        .ack(&b_mailbox, vault_b.mailbox_owner(), &[response_id])
        .unwrap();
    let reopened_b = Store::open(&b.path).unwrap().with_provider_master(master_b);
    let dispatch = reopened_b
        .sync_pairing_dispatch(
            "joiner-installation",
            &master_b,
            2,
            &committed_b.invitation_id,
            now,
        )
        .unwrap();
    let proof = dispatch.frame;
    assert_eq!(
        proof,
        reopened_b
            .sync_pairing_frame(
                "joiner-installation",
                &master_b,
                2,
                &committed_b.invitation_id,
                now
            )
            .unwrap()
    );
    let proof_id = client
        .send(
            &dispatch.delivery.recipient_mailbox,
            &dispatch.delivery.sender_mailbox,
            dispatch.delivery.credential(),
            0,
            &STANDARD.encode(&proof),
            dispatch.delivery.expires_at_ms,
        )
        .unwrap();
    let received = client
        .fetch(&a_mailbox, vault_a.mailbox_owner(), "0", 10)
        .unwrap();
    assert_eq!(received.len(), 1);
    let received_proof = STANDARD.decode(&received[0].ciphertext).unwrap();
    let completed = reopened_a
        .confirm_sync_pairing_installation(
            "owner-installation",
            &master_a,
            2,
            &committed_a.invitation_id,
            &received_proof,
            now,
        )
        .unwrap();
    assert_eq!(completed.vault_revision, 3);
    client
        .ack(&a_mailbox, vault_a.mailbox_owner(), &[proof_id])
        .unwrap();
    // Losing completion's response does not generate a second membership grant.
    assert_eq!(
        a.confirm_sync_pairing_installation(
            "owner-installation",
            &master_a,
            3,
            &committed_a.invitation_id,
            &received_proof,
            now
        )
        .unwrap()
        .vault_revision,
        3
    );
    let loaded_a = a
        .load_sync_vault("owner-installation", &master_a, 3)
        .unwrap();
    let loaded_b = b
        .load_sync_vault("joiner-installation", &master_b, 2)
        .unwrap();
    let (owner_send, owner_receive, _) = loaded_a.peer_keys("authority-roundtrip", joiner).unwrap();
    let (joiner_send, joiner_receive, _) =
        loaded_b.peer_keys("authority-roundtrip", owner).unwrap();
    assert_eq!(
        loaded_a
            .peer_route("authority-roundtrip", joiner)
            .unwrap()
            .peer_mailbox,
        b_mailbox
    );
    assert!(b.require_local_sync_membership().is_err());
    let send_credential =
        RelayClient::derive_sender_credential(&owner_send.to_bytes(), &a_mailbox, &b_mailbox)
            .unwrap();
    let return_credential =
        RelayClient::derive_sender_credential(&joiner_send.to_bytes(), &b_mailbox, &a_mailbox)
            .unwrap();
    let expires = now + 3_600_000;
    client
        .register_sender(
            &b_mailbox,
            loaded_b.mailbox_owner(),
            &a_mailbox,
            &send_credential,
            expires,
            1,
        )
        .unwrap();
    client
        .register_sender(
            &a_mailbox,
            vault_a.mailbox_owner(),
            &b_mailbox,
            &return_credential,
            expires,
            0,
        )
        .unwrap();
    client
        .revoke_sender(&a_mailbox, vault_a.mailbox_owner(), &temporary_sender, 1)
        .unwrap();
    assert!(
        client
            .send(
                &a_mailbox,
                &temporary_sender,
                &invitation_credential,
                0,
                &STANDARD.encode(&proof),
                invitation_expiry
            )
            .is_err()
    );
    let owner_public = vault_a.identity().public_key();
    let joiner_public = loaded_b.identity().public_key();
    let owner_peer = [engine::ReceivingPeer {
        device_id: owner,
        mailbox_id: &a_mailbox,
        key_version: 1,
        key: &joiner_receive,
        public_key: &owner_public,
    }];
    let joiner_peer = [engine::ReceivingPeer {
        device_id: joiner,
        mailbox_id: &b_mailbox,
        key_version: 1,
        key: &owner_receive,
        public_key: &joiner_public,
    }];
    a.submit_input_as_event("authorized owner fact", "capture", None)
        .unwrap();
    engine::stage_operations(&a, joiner, 1, &owner_send, vault_a.identity()).unwrap();
    assert_eq!(
        engine::push_due(
            &a,
            client,
            joiner,
            &b_mailbox,
            &a_mailbox,
            &send_credential,
            now
        )
        .unwrap()
        .uploaded,
        1
    );
    let premature = engine::pull_page(
        &b,
        client,
        &b_mailbox,
        loaded_b.mailbox_owner(),
        &owner_peer,
    )
    .unwrap();
    assert_eq!(premature.accepted, 0);
    assert_eq!(premature.rejected, 1);
    assert!(b.list_events().unwrap().is_empty());
    engine::stage_membership_roster(&a, joiner, 1, &owner_send, vault_a.identity()).unwrap();
    assert_eq!(
        engine::push_due(
            &a,
            client,
            joiner,
            &b_mailbox,
            &a_mailbox,
            &send_credential,
            now
        )
        .unwrap()
        .uploaded,
        1
    );
    let control = engine::pull_page(
        &b,
        client,
        &b_mailbox,
        loaded_b.mailbox_owner(),
        &owner_peer,
    )
    .unwrap();
    assert_eq!(control.accepted, 1);
    assert_eq!(b.sync_roster_anchor().unwrap().revision, 2);
    let roster = a
        .sync_pairing_roster(
            "owner-installation",
            &master_a,
            3,
            &committed_a.invitation_id,
        )
        .unwrap();
    let joined = b
        .finish_sync_pairing_join(
            "joiner-installation",
            &master_b,
            2,
            &committed_b.invitation_id,
            &roster.encode().unwrap(),
        )
        .unwrap();
    assert_eq!(joined.vault_revision, 3);
    assert!(b.provider_keys_sealed().unwrap());
    assert!(b.list_events().unwrap().is_empty());
    assert_eq!(
        engine::pull_page(
            &b,
            client,
            &b_mailbox,
            loaded_b.mailbox_owner(),
            &owner_peer
        )
        .unwrap()
        .accepted,
        1
    );
    assert_eq!(
        b.list_events().unwrap()[0].raw_text,
        "authorized owner fact"
    );
    assert!(b.claim_analysis_job().unwrap().is_none());
    engine::stage_receipts(&b, owner, 1, &joiner_send, loaded_b.identity()).unwrap();
    assert_eq!(
        engine::push_due(
            &b,
            client,
            owner,
            &a_mailbox,
            &b_mailbox,
            &return_credential,
            now
        )
        .unwrap()
        .uploaded,
        2
    );
    assert_eq!(
        engine::pull_page(
            &a,
            client,
            &a_mailbox,
            vault_a.mailbox_owner(),
            &joiner_peer
        )
        .unwrap()
        .accepted,
        2
    );
    assert!(a.pending_sync_operations(joiner, 10).unwrap().is_empty());
    assert!(
        a.due_sync_wire_envelopes(joiner, i64::MAX, 10)
            .unwrap()
            .is_empty()
    );
    let revoked = a
        .revoke_signed_sync_member(2, joiner, vault_a.identity())
        .unwrap();
    // Applying an authenticated control and revoking the relay capability are
    // distinct local/remote actions; this fixture checks both, not global atomicity.
    b.apply_signed_roster(&revoked.encode().unwrap()).unwrap();
    client
        .revoke_sender(&a_mailbox, vault_a.mailbox_owner(), &b_mailbox, 1)
        .unwrap();
    assert!(
        client
            .send(
                &a_mailbox,
                &b_mailbox,
                &return_credential,
                0,
                "c3ludGhldGlj",
                expires
            )
            .is_err()
    );
    b.submit_input_as_event("retained offline after revocation", "capture", None)
        .unwrap();
    assert!(b.due_sync_wire_envelopes(owner, i64::MAX, 10).is_err());
    assert_eq!(b.list_events().unwrap().len(), 2);
}
