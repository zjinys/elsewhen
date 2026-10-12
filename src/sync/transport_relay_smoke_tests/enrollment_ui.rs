//! Full P1 -> P2 through production runtimes and an explicitly launched Worker.
//! Only native key-store persistence is simulated; transitions are not SQL-driven.
use crate::{
    config::AppConfig,
    storage::Store,
    sync::{
        crypto::ContentKey,
        datasets::DatasetCatalog,
        runtime::{self, enrollment, joining},
    },
};
use std::{
    io::{Read, Write},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

fn config(root: &std::path::Path, name: &str) -> AppConfig {
    let data_dir = root.join(name);
    std::fs::create_dir_all(&data_dir).unwrap();
    AppConfig {
        database_path: data_dir.join("elsewhen.db"),
        data_dir,
    }
}

struct Cleanup {
    root: std::path::PathBuf,
    owner: AppConfig,
    joiner: AppConfig,
}
impl Drop for Cleanup {
    fn drop(&mut self) {
        runtime::lock(&self.owner);
        runtime::lock(&self.joiner);
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn current_vault_revision(database: &std::path::Path) -> i64 {
    Store::open(database)
        .unwrap()
        .connection
        .query_row(
            "SELECT revision FROM sync_local_vault WHERE singleton=1",
            [],
            |r| r.get(0),
        )
        .unwrap()
}

fn provider() -> (String, Arc<AtomicUsize>, std::thread::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let thread = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut buffer = [0; 4096];
            let n = socket.read(&mut buffer).unwrap();
            assert!(n > 0 && bytes.len() + n < 65536);
            bytes.extend_from_slice(&buffer[..n]);
            if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase();
                let length: usize = headers
                    .lines()
                    .find_map(|s| s.strip_prefix("content-length:"))
                    .unwrap()
                    .trim()
                    .parse()
                    .unwrap();
                if bytes.len() >= end + 4 + length {
                    assert!(headers.contains("authorization: bearer p2_synthetic_key"));
                    break;
                }
            }
        }
        counter.fetch_add(1, Ordering::SeqCst);
        let body =
            r#"{"choices":[{"message":{"role":"assistant","content":"P2 recovered provider"}}]}"#;
        write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
    });
    (url, calls, thread)
}

/// Final vault revisions and key material of a completed production enrollment,
/// so follow-up tests (steady-state network ticks) resume without re-pairing.
pub(crate) struct CompletedEnrollment {
    pub(crate) owner: AppConfig,
    pub(crate) joiner: AppConfig,
    pub(crate) owner_key: [u8; 32],
    pub(crate) joiner_key: [u8; 32],
    /// Final durable vault revisions; resume unlocks must use these, not the
    /// intermediate confirmation floors.
    pub(crate) owner_revision: i64,
    pub(crate) joiner_revision: i64,
    pub(crate) staged_entry: String,
}

pub(crate) fn run() -> CompletedEnrollment {
    run_with_fault_proxy(true)
}

/// The steady-state P3 test uses a plain relay; lost-response recovery itself
/// is only asserted when the fault proxy is in front.
pub(crate) fn run_with_fault_proxy(fault_proxy: bool) -> CompletedEnrollment {
    let url =
        std::env::var("ELSEWHEN_RELAY_TEST_URL").expect("isolated test harness supplies relay URL");
    let parsed = reqwest::Url::parse(&url).unwrap();
    assert!(
        parsed
            .host_str()
            .unwrap()
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
    );
    let root = std::env::temp_dir().join(format!("elsewhen-p2-worker-{}", uuid::Uuid::new_v4()));
    let fixture = Cleanup {
        owner: config(&root, "owner"),
        joiner: config(&root, "joiner"),
        root,
    };
    let owner = &fixture.owner;
    let joiner = &fixture.joiner;
    let original = Store::open(&joiner.database_path).unwrap();
    original
        .submit_input_as_event("independent original", "capture", None)
        .unwrap();
    original
        .upsert_ai_provider_config(
            "https://original.synthetic.invalid/v1",
            "local-model",
            "P2_ORIGINAL_KEY",
        )
        .unwrap();
    let source = Store::open(&owner.database_path).unwrap();
    source
        .submit_input_as_event(&"large shared original ".repeat(60_000), "capture", None)
        .unwrap();
    let (provider_url, calls, provider_thread) = provider();
    let primary = source
        .save_ai_provider_config(
            None,
            "primary",
            "openai-compatible",
            &provider_url,
            "synthetic",
            "P2_SYNTHETIC_KEY",
            0.2,
            Some(2048),
        )
        .unwrap();
    source
        .save_ai_provider_config(
            None,
            "fallback",
            "openai-compatible",
            "https://fallback.synthetic.invalid/v1",
            "backup",
            "P2_FALLBACK_KEY",
            0.7,
            None,
        )
        .unwrap();
    source
        .connection
        .execute(
            "UPDATE ai_provider_configs SET context_window=65536 WHERE id=?1",
            [&primary],
        )
        .unwrap();
    let owner_key = ContentKey::generate().unwrap().to_bytes();
    let request = runtime::prepare_key(owner, Some("owner"), Some("test")).unwrap();
    let commit = runtime::install_key(owner, &request.token, owner_key.to_vec(), 1).unwrap();
    runtime::confirm_key(owner, &commit.token, commit.revision).unwrap();
    let source = source.with_provider_master(owner_key);
    let offer = runtime::invitations::issue(owner, &url, true).unwrap();
    let preview = joining::preview(joiner, &offer.code).unwrap();
    let request = joining::prepare_key(joiner, &preview.token, "joiner", "test", true).unwrap();
    let entry = request.entry_id.clone().unwrap();
    let joiner_key = ContentKey::generate().unwrap().to_bytes();
    let commit = joining::install_key(joiner, &request.token, joiner_key.to_vec(), 1).unwrap();
    joining::confirm_key(joiner, &commit.token, commit.revision).unwrap();
    joining::send_request(joiner, &request.operation_id).unwrap();
    let preview = runtime::invitations::poll(owner, &offer.invitation_id)
        .unwrap()
        .unwrap();
    let approval =
        runtime::invitations::approve(owner, &offer.invitation_id, &preview.token, true).unwrap();
    runtime::confirm_key(owner, &approval.key.token, approval.key.revision).unwrap();
    runtime::invitations::send_approval(owner, &offer.invitation_id).unwrap();
    let installed = joining::poll(joiner, &request.operation_id).unwrap();
    assert!(joining::send_proof(joiner, &request.operation_id).is_err());
    joining::confirm_key(joiner, &installed.token, installed.revision).unwrap();
    // Simulate an interrupted P1 proof send; P2 must retry the persisted proof.
    let mut floors = [approval.key.revision, installed.revision];
    let mut owner_reopened = false;
    let mut joiner_reopened = false;
    let mut captured_late = false;
    let mut grant_failures = 0;
    let started = Instant::now();
    loop {
        assert!(
            started.elapsed() < Duration::from_secs(100),
            "P2 enrollment did not settle"
        );
        let mut complete = 0;
        for (index, (config, role)) in [(owner, "owner"), (joiner, "joiner")]
            .into_iter()
            .enumerate()
        {
            let state = enrollment::list(config)
                .unwrap()
                .into_iter()
                .find(|s| s.invitation_id == offer.invitation_id)
                .unwrap();
            if state.phase == "complete" && !state.notification_pending {
                complete += 1;
                continue;
            }
            let state = if state.confirmation_token.is_some() {
                state
            } else {
                match enrollment::advance(config, role, &state.operation_id, &offer.invitation_id) {
                    Ok(next) => next,
                    Err(error)
                        if error.to_string().contains("grant_unconfirmed")
                            || error.to_string().contains("revocation_unconfirmed") =>
                    {
                        grant_failures += 1;
                        continue;
                    }
                    Err(error) => panic!("P2 runtime failed: {error}"),
                }
            };
            if role == "owner" && state.phase == "awaiting_roster" && !captured_late {
                source
                    .submit_input_as_event("captured after frozen snapshot", "capture", None)
                    .unwrap();
                captured_late = true;
            }
            if let Some(token) = &state.confirmation_token {
                assert!(
                    enrollment::advance(config, role, &state.operation_id, &offer.invitation_id)
                        .is_err()
                );
                assert!(
                    enrollment::confirm(
                        config,
                        role,
                        &state.operation_id,
                        &offer.invitation_id,
                        token,
                        floors[index]
                    )
                    .is_err()
                );
                if role == "owner" && !owner_reopened {
                    // SQLite committed, fake native save did not. Re-open the same
                    // checkpoint with the original key and protected revision floor.
                    runtime::lock(owner);
                    let request = runtime::prepare_key(owner, None, None).unwrap();
                    let commit =
                        runtime::install_key(owner, &request.token, owner_key.to_vec(), floors[0])
                            .unwrap();
                    floors[0] = commit.revision;
                    runtime::confirm_key(owner, &commit.token, floors[0]).unwrap();
                    owner_reopened = true;
                    continue;
                }
                floors[index] = state.revision;
                enrollment::confirm(
                    config,
                    role,
                    &state.operation_id,
                    &offer.invitation_id,
                    token,
                    floors[index],
                )
                .unwrap();
                if role == "joiner" && state.snapshot_id.is_some() && !joiner_reopened {
                    joining::release(joiner, &state.operation_id);
                    let request = joining::prepare_resume(joiner, &entry).unwrap();
                    let commit = joining::install_key(
                        joiner,
                        &request.token,
                        joiner_key.to_vec(),
                        floors[1],
                    )
                    .unwrap();
                    joining::confirm_key(joiner, &commit.token, commit.revision).unwrap();
                    joiner_reopened = true;
                }
            }
        }
        if complete == 2 {
            break;
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    assert!(owner_reopened && joiner_reopened && captured_late);
    if fault_proxy {
        assert!(
            grant_failures >= 3,
            "fault proxy must lose both formal grants and the revoke response"
        );
    }
    let catalog = DatasetCatalog::open(&joiner.data_dir).unwrap();
    let target_path = catalog.path_for(&entry).unwrap();
    let target = Store::open(&target_path)
        .unwrap()
        .with_provider_master(joiner_key);
    assert_eq!(target.list_events().unwrap().len(), 1);
    assert_eq!(source.list_events().unwrap().len(), 2);
    assert_eq!(original.list_events().unwrap().len(), 1);
    assert_eq!(
        original
            .active_ai_provider_config()
            .unwrap()
            .unwrap()
            .api_key,
        "P2_ORIGINAL_KEY"
    );
    assert!(
        crate::sync::datasets::selected_database_path(&joiner.data_dir)
            .unwrap()
            .is_none()
    );
    assert!(
        !catalog
            .list()
            .unwrap()
            .iter()
            .find(|e| e.id == entry)
            .unwrap()
            .ready
    );
    assert!(target.provider_keys_sealed().unwrap());
    assert!(target.claim_analysis_job().unwrap().is_none());
    let peer = target.sync_identity().unwrap().unwrap().device_id;
    assert!(
        !source
            .pending_sync_operations(&peer, 100)
            .unwrap()
            .is_empty(),
        "snapshot receipt cannot acknowledge later captures"
    );
    let ordinary_uploads: i64 = source.connection.query_row("SELECT COUNT(*) FROM sync_wire_outbox WHERE relay_id IS NOT NULL AND (message_id LIKE 'provider:%' OR message_id LIKE 'op-%')", [], |r| r.get(0)).unwrap();
    assert_eq!(
        ordinary_uploads, 0,
        "P2 cannot upload ordinary deltas alongside the snapshot"
    );
    assert_eq!(
        serde_json::to_value(source.export_provider_bundle("comparison", None).unwrap()).unwrap(),
        serde_json::to_value(target.export_provider_bundle("comparison", None).unwrap()).unwrap()
    );
    let cleartext: i64 = target
        .connection
        .query_row(
            "SELECT COUNT(*) FROM ai_provider_configs WHERE api_key IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(cleartext, 0);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "configuration import must not probe a model"
    );
    let restored = target.active_ai_provider_config().unwrap().unwrap();
    assert_eq!(restored.base_url, provider_url);
    assert_eq!(restored.context_window, Some(65536));
    use crate::ai::provider::{AiProvider, OpenAiCompatibleConfig, OpenAiCompatibleProvider};
    let provider = OpenAiCompatibleProvider::new(OpenAiCompatibleConfig {
        base_url: restored.base_url,
        api_key: restored.api_key,
        model: restored.model,
        temperature: restored.temperature as f32,
        max_tokens: restored.max_tokens.map(|n| n as u32),
        context_window: restored.context_window.map(|n| n as u32),
    })
    .unwrap();
    let reply = provider
        .generate_reply(vec![crate::ai::memory::ContextMessage::new(
            "user",
            "explicit synthetic request",
        )])
        .unwrap();
    assert_eq!(reply.content, "P2 recovered provider");
    provider_thread.join().unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let completed = CompletedEnrollment {
        owner: owner.clone(),
        joiner: joiner.clone(),
        owner_key,
        joiner_key,
        owner_revision: current_vault_revision(&fixture.owner.database_path),
        // The joiner's synced identity lives in the staged dataset database,
        // never the installation's original default DB.
        joiner_revision: current_vault_revision(&target_path),
        staged_entry: entry.clone(),
    };
    // Keep the workspace intact for follow-up steady-state tests.
    std::mem::forget(fixture);
    completed
}

#[test]
#[ignore = "requires an explicitly launched isolated Worker and fault proxy; run npm test in relay/"]
fn production_enrollment_recovers_grants_native_writes_and_receipts() {
    let completed = run();
    crate::sync::runtime::lock(&completed.owner);
    crate::sync::runtime::lock(&completed.joiner);
    let _ = std::fs::remove_dir_all(completed.owner.data_dir.parent().unwrap());
}
