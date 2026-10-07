use super::*;
use crate::sync::{
    membership::{MembershipRoster, RosterMember},
    transport_relay::RelayClient,
    vault::LocalVault,
};

const NOW: i64 = 1_800_000_000_000;

fn devices() -> (LocalVault, LocalVault) {
    (
        LocalVault::create("install-a", "device-a", "test").unwrap(),
        LocalVault::create("install-b", "device-b", "test").unwrap(),
    )
}

fn invitation(a: &LocalVault) -> (PairingInvitation, PendingInvitation) {
    PairingInvitation::issue_for_relay(
        "dataset",
        a.device().clone(),
        a.identity(),
        NOW,
        InvitationRoute {
            relay_url: "https://relay.invalid".into(),
            issuer_mailbox: RelayClient::mailbox_key(a.mailbox_owner()),
            request_sender: "c".repeat(64),
        },
    )
    .unwrap()
}

fn approve(pending: PendingApproval, a: &LocalVault) -> (PendingCompletion, Vec<u8>) {
    let anchor = MembershipRoster {
        version: 1,
        dataset_id: "dataset".into(),
        owner: a.device().clone(),
        revision: 1,
        processor_device_id: a.device().device_id.clone(),
        members: vec![RosterMember {
            device: a.device().clone(),
            active: true,
        }],
    }
    .anchor()
    .unwrap();
    pending
        .approve_for_dataset(a.identity(), 1, NOW, anchor)
        .unwrap()
}

fn pair(a: &LocalVault, b: &LocalVault) -> (ApprovedPair, ApprovedPair) {
    let (invitation, pending) = invitation(a);
    let encoded = invitation.encode().unwrap();
    let decoded = PairingInvitation::decode(&encoded, NOW).unwrap();
    assert_eq!(
        invitation.relay_credential().unwrap(),
        decoded.relay_credential().unwrap()
    );
    let (join, request) = PendingJoin::begin_for_relay(
        decoded,
        b.device().clone(),
        b.identity(),
        NOW,
        RelayClient::mailbox_key(b.mailbox_owner()),
    )
    .unwrap();
    let (completion, response) = approve(pending.receive(&request, NOW).unwrap(), a);
    let joined = join.finish(&response, 0, NOW).unwrap();
    let issuer = completion
        .confirm(&joined.completion_proof(b.identity()).unwrap(), NOW)
        .unwrap();
    (issuer, joined)
}

#[test]
fn relay_routes_and_direction_keys_survive_local_vault_restart() {
    let (mut a, mut b) = devices();
    let (a_pair, b_pair) = pair(&a, &b);
    let ar = a_pair.route.as_ref().unwrap();
    let br = b_pair.route.as_ref().unwrap();
    assert_eq!(ar.local_mailbox, br.peer_mailbox);
    assert_eq!(ar.peer_mailbox, br.local_mailbox);
    assert_ne!(ar.local_mailbox, ar.peer_mailbox);
    assert_eq!(ar.relay_url, br.relay_url);
    a.install_pair(&a_pair).unwrap();
    b.install_pair(&b_pair).unwrap();
    assert!(!a.install_pair(&a_pair).unwrap());
    let master = ContentKey::generate().unwrap().to_bytes();
    let bytes = a.seal(&master).unwrap();
    let loaded = LocalVault::open("install-a", 1, &bytes, &master).unwrap();
    let route = loaded.peer_route("dataset", &b.device().device_id).unwrap();
    assert_eq!(route, ar);
    let (send, receive, _) = loaded.peer_keys("dataset", &b.device().device_id).unwrap();
    let (b_send, b_receive, _) = b.peer_keys("dataset", &a.device().device_id).unwrap();
    assert_eq!(send.to_bytes(), b_receive.to_bytes());
    assert_eq!(receive.to_bytes(), b_send.to_bytes());
    let outgoing = RelayClient::derive_sender_credential(
        &send.to_bytes(),
        &ar.local_mailbox,
        &ar.peer_mailbox,
    )
    .unwrap();
    let accepted = RelayClient::derive_sender_credential(
        &b_receive.to_bytes(),
        &br.peer_mailbox,
        &br.local_mailbox,
    )
    .unwrap();
    assert_eq!(outgoing, accepted);
    assert_ne!(
        outgoing,
        RelayClient::derive_sender_credential(
            &receive.to_bytes(),
            &ar.peer_mailbox,
            &ar.local_mailbox
        )
        .unwrap()
    );
    assert!(!String::from_utf8_lossy(&bytes).contains(&ar.relay_url));
}

#[test]
fn tampering_with_any_invitation_route_field_breaks_the_noise_prologue() {
    let (a, b) = devices();
    for field in ["relay", "mailbox", "sender"] {
        let (mut invitation, pending) = invitation(&a);
        let original_credential = invitation.relay_credential().unwrap();
        let route = invitation.context.route.as_mut().unwrap();
        match field {
            "relay" => route.relay_url = "https://other.invalid".into(),
            "mailbox" => route.issuer_mailbox = "d".repeat(64),
            _ => route.request_sender = "e".repeat(64),
        }
        assert_ne!(original_credential, invitation.relay_credential().unwrap());
        let (_, request) = PendingJoin::begin_for_relay(
            invitation,
            b.device().clone(),
            b.identity(),
            NOW,
            RelayClient::mailbox_key(b.mailbox_owner()),
        )
        .unwrap();
        assert!(pending.receive(&request, NOW).is_err());
    }
}

#[test]
fn relay_pairing_rejects_downgrade_colliding_mailboxes_and_missing_authority() {
    let (a, b) = devices();
    let (invitation, pending) = invitation(&a);
    assert!(PendingJoin::begin(invitation.clone(), b.device().clone(), b.identity(), NOW).is_err());
    for mailbox in [
        invitation.route().unwrap().issuer_mailbox.clone(),
        "c".repeat(64),
        "not-a-mailbox".into(),
    ] {
        assert!(
            PendingJoin::begin_for_relay(
                invitation.clone(),
                b.device().clone(),
                b.identity(),
                NOW,
                mailbox
            )
            .is_err()
        );
    }
    let (_, request) = PendingJoin::begin_for_relay(
        invitation,
        b.device().clone(),
        b.identity(),
        NOW,
        RelayClient::mailbox_key(b.mailbox_owner()),
    )
    .unwrap();
    assert!(
        pending
            .receive(&request, NOW)
            .unwrap()
            .approve(a.identity(), 1, NOW)
            .is_err()
    );
    let (legacy, _) =
        PairingInvitation::issue("dataset", a.device().clone(), a.identity(), NOW).unwrap();
    assert!(legacy.relay_credential().is_err());
    assert!(
        PendingJoin::begin_for_relay(
            legacy,
            b.device().clone(),
            b.identity(),
            NOW,
            RelayClient::mailbox_key(b.mailbox_owner())
        )
        .is_err()
    );
}

#[test]
fn validly_signed_approval_cannot_replace_the_joiners_requested_mailbox() {
    let (a, b) = devices();
    let (invitation, pending) = invitation(&a);
    let (join, request) = PendingJoin::begin_for_relay(
        invitation,
        b.device().clone(),
        b.identity(),
        NOW,
        RelayClient::mailbox_key(b.mailbox_owner()),
    )
    .unwrap();
    let mut pending = pending.receive(&request, NOW).unwrap();
    pending.mailbox = Some("f".repeat(64));
    let (_, response) = approve(pending, &a);
    assert!(join.finish(&response, 0, NOW).is_err());
}

#[test]
fn vault_rejects_foreign_local_mailbox_same_epoch_route_edits_and_revoked_routes() {
    let (mut a, b) = devices();
    let (mut pair, _) = pair(&a, &b);
    a.install_pair(&pair).unwrap();
    pair.route.as_mut().unwrap().peer_mailbox = "e".repeat(64);
    assert!(a.install_pair(&pair).is_err());
    pair.key_version += 1;
    pair.route.as_mut().unwrap().local_mailbox = "f".repeat(64);
    assert!(a.install_pair(&pair).is_err());
    a.revoke_peer("dataset", &b.device().device_id).unwrap();
    assert!(a.peer_route("dataset", &b.device().device_id).is_err());
}

#[test]
fn relay_urls_are_bounded_and_cannot_embed_credentials_or_unencrypted_remote_hosts() {
    for url in [
        "https://relay.invalid",
        "http://127.0.0.1:8787",
        "http://[::1]:8787",
        "http://localhost:8787",
    ] {
        route::validate_relay(url).unwrap();
    }
    for url in [
        "http://relay.invalid",
        "ftp://relay.invalid",
        "https://user:secret@relay.invalid",
        "https://relay.invalid?key=secret",
        "https://relay.invalid#secret",
        "file:///tmp/relay",
        "http://127.0.0.1.evil.invalid",
    ] {
        assert!(route::validate_relay(url).is_err(), "{url}");
    }
    assert!(route::validate_relay(&format!("https://relay.invalid/{}", "x".repeat(512))).is_err());
}
