//! P1 through the production owner/joiner runtimes against the isolated Worker.
//! Native writes are simulated here; no signed member or snapshot is installed.
use crate::{
    config::AppConfig,
    storage::Store,
    sync::{
        crypto::ContentKey,
        datasets::DatasetCatalog,
        pairing::PairingInvitation,
        runtime::{self, joining},
        transport_relay::RelayClient,
    },
};
use base64::{Engine, engine::general_purpose::STANDARD};

pub(super) fn roundtrip(client: &RelayClient, directory: &std::path::Path) {
    fn config(directory: &std::path::Path, name: &str) -> AppConfig {
        let data_dir = directory.join(name);
        std::fs::create_dir_all(&data_dir).unwrap();
        AppConfig {
            database_path: data_dir.join("elsewhen.db"),
            data_dir,
        }
    }
    struct Close<'a>(&'a AppConfig);
    impl Drop for Close<'_> {
        fn drop(&mut self) {
            runtime::lock(self.0);
        }
    }
    let owner = config(directory, "p1-owner");
    let joiner = config(directory, "p1-joiner");
    let _owner_lease = Close(&owner);
    let _joiner_lease = Close(&joiner);
    let original = Store::open(&joiner.database_path).unwrap();
    original
        .submit_input_as_event("independent original", "capture", None)
        .unwrap();
    original
        .upsert_ai_provider_config(
            "https://synthetic.invalid/v1",
            "local-only",
            "ORIGINAL_SYNTHETIC_KEY",
        )
        .unwrap();
    let original_provider = original.active_ai_provider_config().unwrap().unwrap();
    let owner_key = ContentKey::generate().unwrap().to_bytes();
    let owner_request = runtime::prepare_key(&owner, Some("owner"), Some("test")).unwrap();
    let owner_commit =
        runtime::install_key(&owner, &owner_request.token, owner_key.to_vec(), 1).unwrap();
    runtime::confirm_key(&owner, &owner_commit.token, 1).unwrap();
    let url = std::env::var("ELSEWHEN_RELAY_TEST_URL").unwrap();
    let offer = runtime::invitations::issue(&owner, &url, true).unwrap();
    let invitation =
        PairingInvitation::decode(&offer.code, chrono::Utc::now().timestamp_millis()).unwrap();
    let route = invitation.route().unwrap().clone();
    let shown = joining::preview(&joiner, &offer.code).unwrap();
    assert_eq!(shown.phase, "preview");
    assert!(joining::prepare_key(&joiner, &shown.token, "joining", "test", false).is_err());
    let request = joining::prepare_key(&joiner, &shown.token, "joining", "test", true).unwrap();
    let join_key = ContentKey::generate().unwrap().to_bytes();
    let commit = joining::install_key(&joiner, &request.token, join_key.to_vec(), 1).unwrap();
    assert!(joining::send_request(&joiner, &request.operation_id).is_err());
    let ready = joining::confirm_key(&joiner, &commit.token, 1).unwrap();
    joining::send_request(&joiner, &ready.operation_id).unwrap();
    // A same-request retry reuses Noise bytes and the exact temporary grant.
    joining::send_request(&joiner, &ready.operation_id).unwrap();
    let preview = runtime::invitations::poll(&owner, &offer.invitation_id)
        .unwrap()
        .unwrap();
    assert_eq!(Some(preview.device.fingerprint()), ready.local_fingerprint);
    assert_eq!(
        joining::poll(&joiner, &ready.operation_id).unwrap().phase,
        "awaiting_approval"
    );
    let approval =
        runtime::invitations::approve(&owner, &offer.invitation_id, &preview.token, true).unwrap();
    assert!(runtime::invitations::send_approval(&owner, &offer.invitation_id).is_err());
    runtime::confirm_key(&owner, &approval.key.token, approval.key.revision).unwrap();
    runtime::invitations::send_approval(&owner, &offer.invitation_id).unwrap();
    let installed = joining::poll(&joiner, &ready.operation_id).unwrap();
    assert_eq!(installed.phase, "installation_confirmation");
    assert_eq!(installed.revision, 2);
    assert!(joining::send_proof(&joiner, &ready.operation_id).is_err());
    // Crash after SQLite commit but before the external revision write.
    joining::release(&joiner, &ready.operation_id);
    let entries = joining::pending_entries(&joiner).unwrap();
    assert_eq!(entries.len(), 1);
    let restored = joining::prepare_resume(&joiner, &entries[0].id).unwrap();
    let restored_key =
        joining::install_key(&joiner, &restored.token, join_key.to_vec(), 1).unwrap();
    assert!(joining::send_proof(&joiner, &restored.operation_id).is_err());
    joining::confirm_key(&joiner, &restored_key.token, 2).unwrap();
    assert_eq!(
        joining::send_proof(&joiner, &restored.operation_id)
            .unwrap()
            .phase,
        "proof_sent"
    );
    joining::send_proof(&joiner, &restored.operation_id).unwrap();
    let catalog = DatasetCatalog::open(&joiner.data_dir).unwrap();
    let stage = Store::open(&catalog.path_for(&entries[0].id).unwrap())
        .unwrap()
        .with_provider_master(join_key);
    let dispatch = stage
        .sync_pairing_dispatch(
            &entries[0].installation_id,
            &join_key,
            2,
            &offer.invitation_id,
            chrono::Utc::now().timestamp_millis(),
        )
        .unwrap();
    let owner_store = Store::open(&owner.database_path)
        .unwrap()
        .with_provider_master(owner_key);
    let owner_vault = owner_store
        .load_sync_vault(
            &owner_request.installation_id,
            &owner_key,
            approval.key.revision,
        )
        .unwrap();
    let incoming = client
        .fetch(&route.issuer_mailbox, owner_vault.mailbox_owner(), "0", 100)
        .unwrap();
    assert_eq!(
        incoming
            .iter()
            .filter(|mail| mail.ciphertext == STANDARD.encode(&dispatch.frame))
            .count(),
        2
    );
    assert_eq!(
        owner_store
            .current_signed_roster()
            .unwrap()
            .unwrap()
            .roster
            .members
            .len(),
        1,
        "P1 must not automatically install a membership grant"
    );
    assert!(!stage.local_sync_membership_active().unwrap());
    assert!(catalog.prepare_selection(&entries[0].id).is_err());
    assert!(
        crate::sync::datasets::selected_database_path(&joiner.data_dir)
            .unwrap()
            .is_none()
    );
    assert_eq!(original.list_events().unwrap().len(), 1);
    let still_local = original.active_ai_provider_config().unwrap().unwrap();
    assert_eq!(still_local.id, original_provider.id);
    assert_eq!(still_local.api_key, original_provider.api_key);
    assert!(original.sync_identity().unwrap().is_none());
    client
        .revoke_sender(
            &route.issuer_mailbox,
            owner_vault.mailbox_owner(),
            &route.request_sender,
            1,
        )
        .unwrap();
    assert!(joining::send_proof(&joiner, &restored.operation_id).is_err());
    assert!(!stage.local_sync_membership_active().unwrap());
}
