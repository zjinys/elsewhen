use super::*;
use crate::sync::{
    runtime::tests::Fixture,
    pairing::PendingJoin,
    transport_relay::RelayEnvelope,
    vault::LocalVault,
};

fn owner() -> (Fixture, [u8;32], KeyCommit) {
    let fixture = Fixture::new();
    let (key, commit) = fixture.initialize();
    confirm_key(&fixture.config, &commit.token, 1).unwrap();
    (fixture, key, commit)
}
fn page(request: &NetworkLease, sender: &str, bytes: &[u8]) -> RelayPage {
    RelayPage {
        envelopes: vec![RelayEnvelope {
            id: "0000000000000001".into(), sender_mb: sender.into(), seq_hint: 0,
            ciphertext: STANDARD.encode(bytes), created_at: now(), expires_at: request.expires_at_ms,
        }], next_cursor: "0000000000000001".into(), has_more: false,
    }
}
fn presented(f: &Fixture) -> (NetworkLease, InvitationPreview, PendingJoin) {
    let request = prepare(&f.config, "http://localhost:9876", true).unwrap();
    let offered = registered(&f.config, &request, true).unwrap();
    let invitation = PairingInvitation::decode(&offered.code, now()).unwrap();
    let peer = LocalVault::create("joining", "review this device", "test").unwrap();
    let (join, bytes) = PendingJoin::begin_for_relay(invitation, peer.device().clone(), peer.identity(), now(),
        RelayClient::mailbox_key(peer.mailbox_owner())).unwrap();
    let shown = accept_page(&f.config, &request, Ok(page(&request, &request.sender, &bytes))).unwrap().unwrap();
    (request, shown, join)
}

#[test]
fn invitation_requires_unlocked_owner_consent_and_confirmed_relay_registration() {
    let f = Fixture::new();
    assert!(prepare(&f.config, "http://localhost:9876", true).is_err());
    let (_, commit) = f.initialize();
    assert!(prepare(&f.config, "http://localhost:9876", true).is_err());
    confirm_key(&f.config, &commit.token, 1).unwrap();
    assert!(prepare(&f.config, "http://localhost:9876", false).is_err());
    assert!(prepare(&f.config, "http://remote.invalid", true).is_err());
    let request = prepare(&f.config, "http://localhost:9876", true).unwrap();
    assert!(poll(&f.config, &request.invitation_id).is_err(), "must not fetch before registration");
    assert!(prepare(&f.config, "http://localhost:9876", true).is_err());
    assert!(registered(&f.config, &request, false).is_err());
    assert!(unlocked(&f.config, 1));
    assert!(sessions().lock().unwrap().get(&f.config.data_dir).unwrap().owner_invitation.is_none());
}

#[test]
fn preview_is_not_a_grant_and_only_its_displayed_token_can_approve() {
    let (f, _, _) = owner();
    let (request, shown, join) = presented(&f);
    assert_eq!(shown.invitation_id, request.invitation_id);
    assert_eq!(shown.device.name, "review this device");
    assert_eq!(poll(&f.config, &request.invitation_id).unwrap().unwrap().token, shown.token);
    assert!(approve(&f.config, &shown.invitation_id, &shown.token, false).is_err());
    assert!(approve(&f.config, &shown.invitation_id, "stale-preview", true).is_err());
    assert!(unlocked(&f.config, 1));
    let commit = approve(&f.config, &shown.invitation_id, &shown.token, true).unwrap();
    assert_eq!(commit.key.revision, 2);
    assert!(super::super::pairing::pairing_dispatch(&f.config, &shown.invitation_id).is_err());
    confirm_key(&f.config, &commit.key.token, 2).unwrap();
    let response = super::super::pairing::pairing_dispatch(&f.config, &shown.invitation_id).unwrap();
    assert_eq!(response.delivery.recipient_mailbox.len(), 64);
    assert!(join.finish(&response.frame, 0, now()).is_ok());
    assert!(approve(&f.config, &shown.invitation_id, &shown.token, true).is_err());
    assert_eq!(f.store.current_signed_roster().unwrap().unwrap().roster.members.len(), 1,
        "explicit approval still waits for installation proof");
}

#[test]
fn network_lease_survives_lock_and_late_registration_cannot_reopen_a_session() {
    let (f, key, initial) = owner();
    let request = prepare(&f.config, "http://localhost:9876", true).unwrap();
    lock(&f.config);
    assert!(registered(&f.config, &request, true).is_err());
    assert!(prepare_key(&f.config, None, None).is_err());
    drop(request);
    let next = prepare_key(&f.config, None, None).unwrap();
    let commit = install_key(&f.config, &next.token, key.to_vec(), 1).unwrap();
    confirm_key(&f.config, &commit.token, 1).unwrap();
    assert_eq!(f.store.load_sync_vault(&initial.installation_id, &key, 1).unwrap().pairing_statuses(now()).len(), 0);
    assert!(sessions().lock().unwrap().get(&f.config.data_dir).unwrap().owner_invitation.is_none());
}

#[test]
fn unrelated_mail_does_not_consume_the_invitation_and_forged_noise_never_creates_preview() {
    let (f, _, _) = owner();
    let request = prepare(&f.config, "http://localhost:9876", true).unwrap();
    registered(&f.config, &request, true).unwrap();
    assert!(accept_page(&f.config, &request, Ok(page(&request, &"b".repeat(64), b"unrelated"))).unwrap().is_none());
    assert!(accept_page(&f.config, &request, Ok(page(&request, &request.sender, b"not a Noise frame"))).is_err());
    assert!(sessions().lock().unwrap().get(&f.config.data_dir).unwrap().owner_invitation.is_none());
    assert_eq!(f.store.current_signed_roster().unwrap().unwrap().roster.members.len(), 1);
}

#[test]
fn failed_approval_commit_does_not_publish_keys_or_reuse_consumed_noise_state() {
    let (f, _, _) = owner();
    let (_request, shown, _join) = presented(&f);
    f.store.connection.execute_batch("CREATE TRIGGER fail_invite BEFORE UPDATE ON sync_local_vault BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;").unwrap();
    assert!(approve(&f.config, &shown.invitation_id, &shown.token, true).is_err());
    assert!(unlocked(&f.config, 1));
    assert!(sessions().lock().unwrap().get(&f.config.data_dir).unwrap().owner_invitation.is_none());
    assert!(super::super::pairing::pairing_dispatch(&f.config, &shown.invitation_id).is_err());
    assert_eq!(f.store.current_signed_roster().unwrap().unwrap().roster.members.len(), 1);
}
