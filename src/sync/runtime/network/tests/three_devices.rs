use super::*;
use crate::storage::sync::ReplicaIdentity;
use crate::sync::{
    crypto::ContentKey,
    pairing::{InvitationRoute, PairingInvitation, PendingJoin},
    transport_relay::RelayClient,
};

/// Build actual Noise pair keys and signed rosters. Each leaf deliberately has
/// no key/route to the other leaf; all HTTP delivery runs against a temp relay.
#[test]
#[ignore = "requires an explicitly launched isolated relay; run npm test in relay/"]
fn owner_routes_three_devices_and_their_application_receipts() {
    let url = std::env::var("ELSEWHEN_RELAY_TEST_URL").unwrap();
    let parsed = reqwest::Url::parse(&url).unwrap();
    assert!(parsed.host_str().is_some_and(|h| {
        h == "localhost"
            || h.trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    }));
    let client = RelayClient::new(&url).unwrap();
    let a = runtime::tests::Fixture::new();
    let (ak, commit) = a.initialize();
    runtime::confirm_key(&a.config, &commit.token, 1).unwrap();
    runtime::lock(&a.config);
    let installation: String = a
        .store
        .connection
        .query_row(
            "SELECT installation_id FROM sync_local_vault WHERE singleton=1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let mut av = a.store.load_sync_vault(&installation, &ak, 1).unwrap();
    client.create_mailbox(av.mailbox_owner()).unwrap();
    let dataset = a.store.sync_identity().unwrap().unwrap().dataset_id;
    let b = runtime::tests::Fixture::new();
    let c = runtime::tests::Fixture::new();
    let mut leaves = Vec::new();
    for (index, f) in [&b, &c].iter().enumerate() {
        let master = ContentKey::generate().unwrap().to_bytes();
        let install = format!("three-device-{index}");
        let vault = f
            .store
            .create_sync_vault(&install, &master, "leaf", "test")
            .unwrap();
        f.store
            .initialize_sync_replica(&ReplicaIdentity {
                dataset_id: dataset.clone(),
                device_id: vault.device().device_id.clone(),
                stream_id: vault.device().stream_id.clone(),
                processor_device_id: av.device().device_id.clone(),
            })
            .unwrap();
        let mailbox = client.create_mailbox(vault.mailbox_owner()).unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        let (invite, pending) = PairingInvitation::issue_for_relay(
            &dataset,
            av.device().clone(),
            av.identity(),
            now,
            InvitationRoute {
                relay_url: url.clone(),
                issuer_mailbox: RelayClient::mailbox_key(av.mailbox_owner()),
                request_sender: RelayClient::mailbox_key(
                    &ContentKey::generate().unwrap().to_bytes(),
                ),
            },
        )
        .unwrap();
        let (join, request) = PendingJoin::begin_for_relay(
            invite,
            vault.device().clone(),
            vault.identity(),
            now,
            mailbox.clone(),
        )
        .unwrap();
        let anchor = a.store.sync_roster_anchor().unwrap();
        let (completion, response) = pending
            .receive(&request, now)
            .unwrap()
            .approve_for_dataset(av.identity(), 1, now, anchor.clone())
            .unwrap();
        let joined = join.finish(&response, 0, now).unwrap();
        let approved = completion
            .confirm(&joined.completion_proof(vault.identity()).unwrap(), now)
            .unwrap();
        let signed = a
            .store
            .grant_sync_pair(anchor.revision, &approved, av.identity())
            .unwrap();
        f.store.pin_sync_authority_from_pair(&joined).unwrap();
        let revision = f
            .store
            .install_sync_vault_pair(&install, &master, 1, &joined)
            .unwrap();
        f.store
            .apply_signed_roster(&signed.encode().unwrap())
            .unwrap();
        let ar = a
            .store
            .install_sync_vault_pair(&installation, &ak, av.revision, &approved)
            .unwrap();
        av = a.store.load_sync_vault(&installation, &ak, ar).unwrap();
        let bv = f
            .store
            .load_sync_vault(&install, &master, revision)
            .unwrap();
        let (send, receive, _) = bv.peer_keys(&dataset, &av.device().device_id).unwrap();
        let owner_mb = RelayClient::mailbox_key(av.mailbox_owner());
        let expiry = now + 7 * 24 * 60 * 60 * 1000;
        client
            .register_sender(
                &owner_mb,
                av.mailbox_owner(),
                &mailbox,
                &RelayClient::derive_sender_credential(&send.to_bytes(), &mailbox, &owner_mb)
                    .unwrap(),
                expiry,
                0,
            )
            .unwrap();
        client
            .register_sender(
                &mailbox,
                bv.mailbox_owner(),
                &owner_mb,
                &RelayClient::derive_sender_credential(&receive.to_bytes(), &owner_mb, &mailbox)
                    .unwrap(),
                expiry,
                0,
            )
            .unwrap();
        leaves.push((master, revision, bv.device().device_id.clone()));
    }
    // The phone still has the old roster. Deliver the third member's grant
    // through the actual background network path before forwarding its ops.
    assert!(!b.store.sync_member_is_active(&leaves[1].2).unwrap());
    unlock_plain(&a.config, "owner", ak, av.revision);
    unlock_plain(&b.config, "phone", leaves[0].0, leaves[0].1);
    unlock_plain(&c.config, "tablet", leaves[1].0, leaves[1].1);
    let large = "phone original ".repeat(40_000);
    b.store
        .submit_input_as_event(&large, "capture", None)
        .unwrap();
    c.store
        .submit_input_as_event("tablet original", "capture", None)
        .unwrap();
    a.store
        .submit_input_as_event("owner original", "capture", None)
        .unwrap();
    let owner = av.device().device_id.clone();
    let drained = || {
        [&a, &b, &c]
            .iter()
            .all(|f| f.store.list_events().unwrap().len() == 3)
            && b.store
                .pending_sync_operations(&owner, 100)
                .unwrap()
                .is_empty()
            && c.store
                .pending_sync_operations(&owner, 100)
                .unwrap()
                .is_empty()
            && leaves.iter().all(|(_, _, leaf)| {
                a.store
                    .pending_sync_operations(leaf, 100)
                    .unwrap()
                    .is_empty()
            })
    };
    for _ in 0..80 {
        for f in [&b, &c, &a] {
            runtime::network::tick(&f.config).unwrap();
        }
        if drained() {
            break;
        }
    }
    assert!(
        drained(),
        "three devices must converge and retire every hop's operations"
    );
    for f in [&a, &b, &c] {
        let events = f.store.list_events().unwrap();
        assert!(events.iter().any(|e| e.raw_text == large.trim()));
        assert!(events.iter().any(|e| e.raw_text == "tablet original"));
        assert!(events.iter().any(|e| e.raw_text == "owner original"));
    }
}
