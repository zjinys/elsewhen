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
    run_scaled(fault_proxy, Bulk::Single(60_000), false)
}

/// C0 复现用的快照体形状。事故库是 241 条事件共 ~44 MB 明文（≈174 个 256 KiB 分片），
/// 不是「一条超大记录」——`MAX_RECORD_BYTES = 20 MiB` 会先把后者拦下，
/// 那是另一条错误路径（`sync_snapshot_record_too_large`），不是现场症状。
pub(crate) enum Bulk {
    /// 一条 `"large shared original ".repeat(n)` 事件（≈ n/21 MB）。
    Single(usize),
    /// `count` 条 `bytes` 字节的独立事件 —— 事故规模的形状。
    Spread { count: usize, bytes: usize },
}

impl Bulk {
    fn records(&self) -> usize {
        match self {
            Bulk::Single(_) => 1,
            Bulk::Spread { count, .. } => *count,
        }
    }

    fn seed(&self, store: &Store) {
        match self {
            Bulk::Single(repeat) => {
                store
                    .submit_input_as_event(
                        &"large shared original ".repeat(*repeat),
                        "capture",
                        None,
                    )
                    .unwrap();
            }
            Bulk::Spread { count, bytes } => {
                for index in 0..*count {
                    let payload = format!("{index:06}-{}", "y".repeat(bytes.saturating_sub(7)));
                    store
                        .submit_input_as_event(&payload, "capture", None)
                        .unwrap();
                }
            }
        }
    }
}

/// C0 复现入口：同一套生产链路，只把快照体放大到事故规模（≈174 片）。
///
/// `trace=true` 时不再对 `advance` 的错误 panic，而是每拍打印双方相位、
/// joiner 的 `received_chunks/total_chunks` 与错误串 —— 真机上这些全在
/// logcat 之外的 Dart 侧，拿不到，只能在本地复现里第一次看到。
#[allow(clippy::too_many_lines)]
pub(crate) fn run_scaled(fault_proxy: bool, bulk: Bulk, trace: bool) -> CompletedEnrollment {
    let bulk_events = bulk.records();
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
    bulk.seed(&source);
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
    let deadline = Duration::from_secs(if trace { 900 } else { 100 });
    loop {
        assert!(started.elapsed() < deadline, "P2 enrollment did not settle");
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
                    Err(error) if trace => {
                        eprintln!(
                            "[c0] {role} advance error at +{:?}: {error:#}",
                            started.elapsed()
                        );
                        std::thread::sleep(Duration::from_millis(400));
                        continue;
                    }
                    Err(error) => panic!("P2 runtime failed: {error}"),
                }
            };
            if trace {
                eprintln!(
                    "[c0] +{:?} {role} phase={} chunks={}/{} local_applied={} remote_applied={}",
                    started.elapsed(),
                    state.phase,
                    state.received_chunks,
                    state.total_chunks,
                    state.local_applied,
                    state.remote_applied
                );
            }
            // 「快照冻结之后再写一笔」的抓取点：blob 数据面下导出要跨多拍才 `ready`，
            // 所以 owner 根本不会再停在 `awaiting_roster`（那个相位只在导出已 ready、
            // 名单还没 ack 时成立）。这里的语义只要求两个事实：导出视图已钉住、
            // 接收方还没拿到这一笔。`awaiting_snapshot_selection` 与 `sending_snapshot`
            // 都以「导出 ready」为前提（manifest 已落盘 = 视图早钉住了），满足前一条；
            // 后一条无论写得多晚都成立——快照是一份时间点视图，晚写本来就不该进去。
            if role == "owner"
                && matches!(
                    state.phase.as_str(),
                    "awaiting_snapshot_selection" | "sending_snapshot" | "awaiting_receipt"
                )
                && !captured_late
            {
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
    assert_eq!(target.list_events().unwrap().len(), bulk_events);
    assert_eq!(source.list_events().unwrap().len(), bulk_events + 1);
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

/// C0 复现：事故规模的 mail 快照（≈174 片 / 63 MB 明文）走同一套生产链路，
/// 打印 joiner 每拍相位与错误串。**只用于定性，不是 CI 守卫**。
#[test]
#[ignore = "C0 repro; requires an explicitly launched isolated relay"]
fn c0_large_mail_snapshot_repro() {
    let completed = run_scaled(
        false,
        Bulk::Spread {
            count: 1_500,
            bytes: 30_000,
        },
        true,
    );
    crate::sync::runtime::lock(&completed.owner);
    crate::sync::runtime::lock(&completed.joiner);
}
