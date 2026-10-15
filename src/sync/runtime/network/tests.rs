//! Steady-state tick against the real Worker: after a completed enrollment,
//! ordinary captures travel without any new pairing or snapshot staging.
//! Requires ELSEWHEN_RELAY_TEST_URL (loopback only); run via relay npm test.

use crate::{config::AppConfig, sync::runtime};

fn enrollment_run() -> crate::sync::transport_relay_smoke_tests::enrollment_ui::CompletedEnrollment
{
    crate::sync::transport_relay_smoke_tests::enrollment_ui::run_with_fault_proxy(false)
}

/// A fresh process restart for a plain (non-joining) device: a new session on
/// the pinned database with the same protected revision floor.
fn unlock_plain(config: &AppConfig, name: &str, master: [u8; 32], revision: i64) {
    let request = runtime::prepare_key(config, Some(name), Some("test")).unwrap();
    assert!(!request.creating);
    assert_eq!(request.revision, revision);
    let commit = runtime::install_key(config, &request.token, master.to_vec(), revision).unwrap();
    runtime::confirm_key(config, &commit.token, revision).unwrap();
}

/// Commits the catalog entry with the joiner's live confirmed session and
/// returns the pinned-database config the app would see after a restart.
/// Selection is a foreground action on the live session, exactly as after a
/// completed P2 restore; it is not a restart or a new key request.
fn select_joiner_dataset(config: &AppConfig, entry: &str, revision: i64) -> AppConfig {
    let master = runtime::session_master(config, revision).unwrap();
    let catalog = crate::sync::datasets::DatasetCatalog::open(&config.data_dir).unwrap();
    let selection = catalog.prepare_selection(entry).unwrap();
    catalog
        .select_import(&selection, &master, revision)
        .unwrap();
    let selected = crate::sync::datasets::selected_database_path(&config.data_dir)
        .unwrap()
        .expect("selection must commit the staged entry");
    assert_eq!(selected, catalog.path_for(entry).unwrap());
    AppConfig {
        database_path: selected,
        data_dir: config.data_dir.clone(),
    }
}

fn cleanup(configs: &[&AppConfig]) {
    for config in configs {
        runtime::lock(config);
        let _ = std::fs::remove_dir_all(&config.data_dir);
    }
}

#[test]
#[ignore = "requires an explicitly launched isolated relay; run npm test in relay/"]
fn tick_moves_deltas_between_joined_devices_without_new_pairing() {
    let url = std::env::var("ELSEWHEN_RELAY_TEST_URL")
        .expect("test harness must provide ELSEWHEN_RELAY_TEST_URL");
    let parsed = reqwest::Url::parse(&url).unwrap();
    assert!(
        parsed.host_str().is_some_and(|host| host == "localhost"
            || host
                .trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())),
        "network tick tests may only access loopback"
    );
    let client = crate::sync::transport_relay::RelayClient::new(&url).unwrap();
    assert!(client.health().unwrap());
    let completed = enrollment_run();
    // A restart ends the enrollment-era sessions; the owner unlocks fresh.
    // The joiner keeps its resumed enrollment session: catalog selection is a
    // foreground action on that live session, exactly as after a P2 restore.
    runtime::lock(&completed.owner);
    let config_a = &completed.owner;
    unlock_plain(
        config_a,
        "owner",
        completed.owner_key,
        completed.owner_revision,
    );
    let config_b_selected = select_joiner_dataset(
        &completed.joiner,
        &completed.staged_entry,
        completed.joiner_revision,
    );

    // Locked runtime pauses sync work without affecting local capture.
    runtime::lock(&config_b_selected);
    let selected_store = crate::storage::Store::open(&config_b_selected.database_path).unwrap();
    selected_store
        .submit_input_as_event("offline joiner fact", "capture", None)
        .unwrap();
    assert!(
        runtime::network::tick(&config_b_selected)
            .unwrap()
            .is_none()
    );
    let owner_store = crate::storage::Store::open(&config_a.database_path).unwrap();
    assert!(
        owner_store
            .list_events()
            .unwrap()
            .iter()
            .all(|e| e.raw_text != "offline joiner fact")
    );

    // Re-unlocking after the simulated restart: a fresh plain session on the
    // selected database resumes sync work; no resume or re-pairing is needed.
    unlock_plain(
        &config_b_selected,
        "joiner",
        completed.joiner_key,
        completed.joiner_revision,
    );
    let pushed = runtime::network::tick(&config_b_selected).unwrap().unwrap();
    // One operation plus the deferred receipt for the owner's late capture.
    assert!(pushed.uploaded >= 1);
    let pulled = runtime::network::tick(config_a).unwrap().unwrap();
    assert!(pulled.pulled_accepted >= 1);
    assert!(
        owner_store
            .list_events()
            .unwrap()
            .iter()
            .any(|e| e.raw_text == "offline joiner fact")
    );

    // The original default DB stays isolated from the selected dataset.
    let original = crate::storage::Store::open(&completed.joiner.database_path).unwrap();
    assert!(
        original
            .list_events()
            .unwrap()
            .iter()
            .all(|e| e.raw_text != "offline joiner fact")
    );

    // Receipts travel back over later ticks until both outboxes drain.
    let owner_device = owner_store.sync_identity().unwrap().unwrap().device_id;
    for _ in 0..8 {
        runtime::network::tick(config_a).unwrap();
        runtime::network::tick(&config_b_selected).unwrap();
        if selected_store
            .pending_sync_operations(&owner_device, 10)
            .unwrap()
            .is_empty()
            && owner_store
                .pending_sync_operations(
                    &selected_store.sync_identity().unwrap().unwrap().device_id,
                    10,
                )
                .unwrap()
                .is_empty()
        {
            break;
        }
    }
    let stuck: Vec<(String, i64)> = selected_store
        .connection
        .prepare("SELECT op_id,acknowledged FROM sync_outbox WHERE recipient_device_id=?1")
        .unwrap()
        .query_map([&owner_device], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert!(
        selected_store
            .pending_sync_operations(&owner_device, 10)
            .unwrap()
            .is_empty(),
        "joiner outbox for owner must drain; rows: {stuck:?}"
    );

    // Owner writes reach the joiner through the same steady-state tick.
    owner_store
        .submit_input_as_event("post-enrollment owner fact", "capture", None)
        .unwrap();
    runtime::network::tick(config_a).unwrap();
    let report = runtime::network::tick(&config_b_selected).unwrap().unwrap();
    assert!(report.pulled_accepted >= 1);
    assert!(
        selected_store
            .list_events()
            .unwrap()
            .iter()
            .any(|e| e.raw_text == "post-enrollment owner fact")
    );

    cleanup(&[config_a, &config_b_selected]);
}
