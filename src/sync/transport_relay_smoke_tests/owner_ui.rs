//! Production owner invitation operations exercised against the isolated Worker.
//! Native revision writes are simulated explicitly, not claimed as OS acceptance.

use crate::{
    config::AppConfig,
    storage::Store,
    sync::{
        crypto::ContentKey,
        pairing::{PairingInvitation, PendingJoin},
        runtime,
        transport_relay::RelayClient,
    },
};
use base64::{Engine, engine::general_purpose::STANDARD};

pub(super) fn roundtrip(client: &RelayClient, directory: &std::path::Path) {
    let root = directory.join("owner-app-flow");
    std::fs::create_dir_all(&root).unwrap();
    let config = AppConfig {
        database_path: root.join("elsewhen.db"),
        data_dir: root,
    };
    struct Close<'a>(&'a AppConfig);
    impl Drop for Close<'_> {
        fn drop(&mut self) {
            runtime::lock(self.0);
        }
    }
    let _close = Close(&config);
    let master_a = ContentKey::generate().unwrap().to_bytes();
    let request = runtime::prepare_key(&config, Some("owner app"), Some("test")).unwrap();
    let commit = runtime::install_key(&config, &request.token, master_a.to_vec(), 1).unwrap();
    runtime::confirm_key(&config, &commit.token, 1).unwrap();
    let a = Store::open(&config.database_path)
        .unwrap()
        .with_provider_master(master_a);
    let va = a
        .load_sync_vault(&commit.installation_id, &master_a, 1)
        .unwrap();
    let url = std::env::var("ELSEWHEN_RELAY_TEST_URL").unwrap();
    assert!(runtime::invitations::issue(&config, &url, false).is_err());
    let offer = runtime::invitations::issue(&config, &url, true).unwrap();
    assert!(
        runtime::invitations::poll(&config, &offer.invitation_id)
            .unwrap()
            .is_none()
    );
    let invitation =
        PairingInvitation::decode(&offer.code, chrono::Utc::now().timestamp_millis()).unwrap();
    let route = invitation.route().unwrap().clone();
    let credential = invitation.relay_credential().unwrap();
    let expires = invitation.expires_at_ms();
    let master_b = ContentKey::generate().unwrap().to_bytes();
    let b = Store::open(&config.data_dir.join("joiner.db"))
        .unwrap()
        .with_provider_master(master_b);
    let vb = b
        .create_sync_vault("joiner-app", &master_b, "joining app", "test")
        .unwrap();
    let mailbox_b = client.create_mailbox(vb.mailbox_owner()).unwrap();
    let response_credential =
        RelayClient::derive_sender_credential(&credential, &route.issuer_mailbox, &mailbox_b)
            .unwrap();
    client
        .register_sender(
            &mailbox_b,
            vb.mailbox_owner(),
            &route.issuer_mailbox,
            &response_credential,
            expires,
            0,
        )
        .unwrap();
    let (join, frame) = PendingJoin::begin_for_relay(
        invitation,
        vb.device().clone(),
        vb.identity(),
        chrono::Utc::now().timestamp_millis(),
        mailbox_b.clone(),
    )
    .unwrap();
    let request_mail = client
        .send(
            &route.issuer_mailbox,
            &route.request_sender,
            &credential,
            0,
            &STANDARD.encode(frame),
            expires,
        )
        .unwrap();
    let preview = runtime::invitations::poll(&config, &offer.invitation_id)
        .unwrap()
        .unwrap();
    assert_eq!(preview.device, *vb.device());
    assert_eq!(
        a.current_signed_roster()
            .unwrap()
            .unwrap()
            .roster
            .members
            .len(),
        1
    );
    assert!(
        runtime::invitations::approve(&config, &offer.invitation_id, "wrong-token", true).is_err()
    );
    let approved =
        runtime::invitations::approve(&config, &offer.invitation_id, &preview.token, true).unwrap();
    assert!(runtime::invitations::send_approval(&config, &offer.invitation_id).is_err());
    runtime::confirm_key(&config, &approved.key.token, approved.key.revision).unwrap();
    runtime::invitations::send_approval(&config, &offer.invitation_id).unwrap();
    let responses = client
        .fetch(&mailbox_b, vb.mailbox_owner(), "0", 10)
        .unwrap();
    assert_eq!(responses.len(), 1);
    let pair = join
        .finish(
            &STANDARD.decode(&responses[0].ciphertext).unwrap(),
            0,
            chrono::Utc::now().timestamp_millis(),
        )
        .unwrap();
    b.install_sync_pairing_join(
        "joiner-app",
        &master_b,
        1,
        pair,
        chrono::Utc::now().timestamp_millis(),
    )
    .unwrap();
    let proof = b
        .sync_pairing_dispatch(
            "joiner-app",
            &master_b,
            2,
            &offer.invitation_id,
            chrono::Utc::now().timestamp_millis(),
        )
        .unwrap();
    let proof_mail = client
        .send(
            &proof.delivery.recipient_mailbox,
            &proof.delivery.sender_mailbox,
            proof.delivery.credential(),
            0,
            &STANDARD.encode(proof.frame),
            proof.delivery.expires_at_ms,
        )
        .unwrap();
    let received = client
        .fetch(&route.issuer_mailbox, va.mailbox_owner(), "0", 10)
        .unwrap();
    let proof = received.iter().find(|m| m.id == proof_mail).unwrap();
    let completed = runtime::pairing::confirm_pairing_installation(
        &config,
        &offer.invitation_id,
        &STANDARD.decode(&proof.ciphertext).unwrap(),
    )
    .unwrap();
    runtime::confirm_key(&config, &completed.key.token, completed.key.revision).unwrap();
    let roster = runtime::pairing::pairing_roster(&config, &offer.invitation_id).unwrap();
    b.finish_sync_pairing_join("joiner-app", &master_b, 2, &offer.invitation_id, &roster)
        .unwrap();
    assert!(b.local_sync_membership_active().unwrap());
    assert!(b.provider_keys_sealed().unwrap());
    client
        .ack(
            &route.issuer_mailbox,
            va.mailbox_owner(),
            &[request_mail, proof_mail],
        )
        .unwrap();
    assert_eq!(
        a.current_signed_roster()
            .unwrap()
            .unwrap()
            .roster
            .members
            .len(),
        2
    );
}
