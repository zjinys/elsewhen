//! Owner-side bridge between durable pairing transitions and native revision
//! confirmation. No network I/O runs under the runtime mutex. Unapproved Noise
//! requests are still volatile and must be explicitly reviewed after restart.

use super::*;
use crate::{storage::sync::PairingCommit, sync::pairing::PendingApproval};

pub(crate) struct PairingKeyCommit {
    pub invitation_id: String,
    pub key: KeyCommit,
}

/// While a pairing mutation waits for the workspace, new background callers
/// are deferred so the mutation can run the moment the in-flight worker
/// finishes. Counted per workspace: a dropping reservation must not disarm a
/// concurrent pairing mutation's reservation.
static PAIRING_WAITING: OnceLock<Mutex<HashMap<PathBuf, u32>>> = OnceLock::new();
fn waiting() -> &'static Mutex<HashMap<PathBuf, u32>> {
    PAIRING_WAITING.get_or_init(|| Mutex::new(HashMap::new()))
}

struct BackgroundPause {
    root: PathBuf,
}
impl BackgroundPause {
    /// Must be called while holding the sessions mutex: a background caller
    /// cannot record itself busy (or start work) without that mutex first.
    fn acquire(root: &Path) -> Self {
        let mut map = waiting()
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        *map.entry(root.to_path_buf()).or_insert(0) += 1;
        Self { root: root.to_path_buf() }
    }
}
impl Drop for BackgroundPause {
    fn drop(&mut self) {
        let mut map = waiting()
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if let Some(count) = map.get_mut(&self.root) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                map.remove(&self.root);
            }
        }
    }
}

pub(super) fn background_pause_active(root: &Path) -> bool {
    waiting()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .contains_key(root)
}

/// How long a user-initiated pairing mutation may wait for an in-flight
/// background worker (LLM digest/analysis can span minutes) to finish.
pub(crate) const PAIRING_MUTATE_WAIT_MS: i64 = 120_000;

fn mutate_locked(
    config: &AppConfig,
    sessions: &mut std::collections::HashMap<PathBuf, Session>,
    operation: impl FnOnce(&Store, &mut Session) -> Result<PairingCommit>,
) -> Result<PairingKeyCommit> {
    let session = sessions
        .get_mut(&config.data_dir)
        .context("sync_runtime_request_missing")?;
    ensure!(
        session.joining.is_none()
            && !session.enrollment_busy
            && session.database == config.database_path
            && session.confirmed
            && session.master.is_some(),
        "sync_pairing_runtime_locked"
    );
    ensure!(
        !busy()
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .contains(&config.data_dir),
        "sync_runtime_in_use"
    );
    let store =
        Store::open(&session.database)?.with_provider_master(*session.master.as_ref().unwrap());
    let tx = store.begin_write_transaction()?;
    store.require_background_quiescent_for_sync()?;
    let result = operation(&store, session)?;
    if let Some(tx) = tx {
        tx.commit()?;
    }
    if session.revision != result.vault_revision {
        session.revision = result.vault_revision;
        session.confirmed = false;
        session.enrollment_confirmation = None;
        session.token = uuid::Uuid::new_v4().to_string();
        session.expires_at = now() + 10 * 60 * 1000;
    }
    Ok(PairingKeyCommit {
        invitation_id: result.invitation_id,
        key: KeyCommit {
            token: session.token.clone(),
            installation_id: session.installation.clone(),
            revision: session.revision,
        },
    })
}

pub(super) fn mutate(
    config: &AppConfig,
    operation: impl FnOnce(&Store, &mut Session) -> Result<PairingCommit>,
) -> Result<PairingKeyCommit> {
    mutate_with_wait(config, PAIRING_MUTATE_WAIT_MS, operation)
}

/// Wait-bound mutating entry point. With `wait_budget_ms <= 0` the historical
/// fail-fast semantics apply (`sync_runtime_in_use` while a background caller
/// holds the workspace). With a positive budget the mutation reserves the
/// workspace against *new* background callers and waits for the in-flight one
/// to finish; the busy check is always re-evaluated under the sessions mutex,
/// which background callers cannot bypass.
pub(super) fn mutate_with_wait(
    config: &AppConfig,
    wait_budget_ms: i64,
    operation: impl FnOnce(&Store, &mut Session) -> Result<PairingCommit>,
) -> Result<PairingKeyCommit> {
    let mut operation = Some(operation);
    let budget_end = now().saturating_add(wait_budget_ms.max(0));
    let mut _pause: Option<BackgroundPause> = None;
    loop {
        {
            let mut sessions = sessions().lock().unwrap_or_else(|p| p.into_inner());
            if !busy()
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .contains(&config.data_dir)
            {
                let result =
                    mutate_locked(config, &mut sessions, operation.take().expect("operation"));
                drop(sessions);
                return result;
            }
        }
        if wait_budget_ms <= 0 {
            anyhow::bail!("sync_runtime_in_use");
        }
        if _pause.is_none() {
            // Reserve under the sessions mutex so no background caller can
            // start behind our back while we sleep.
            let sessions = sessions().lock().unwrap_or_else(|p| p.into_inner());
            _pause = Some(BackgroundPause::acquire(&config.data_dir));
            drop(sessions);
        }
        if now() >= budget_end {
            anyhow::bail!("sync_runtime_in_use");
        }
        std::thread::sleep(std::time::Duration::from_millis(150));
    }
}

fn approve_operation(
    approval: PendingApproval,
) -> impl FnOnce(&Store, &mut Session) -> Result<PairingCommit> {
    move |store, session| {
        store.approve_sync_pairing(
            &session.installation,
            session.master.as_ref().unwrap(),
            session.revision,
            approval,
            now(),
        )
    }
}

pub(crate) fn approve_pairing(
    config: &AppConfig,
    approval: PendingApproval,
) -> Result<PairingKeyCommit> {
    mutate_with_wait(config, PAIRING_MUTATE_WAIT_MS, approve_operation(approval))
}

/// Test-facing fail-fast approval: same critical section, no waiting.
pub(crate) fn approve_pairing_now(
    config: &AppConfig,
    approval: PendingApproval,
) -> Result<PairingKeyCommit> {
    mutate_with_wait(config, 0, approve_operation(approval))
}

pub(crate) fn confirm_pairing_installation(
    config: &AppConfig,
    invitation: &str,
    proof: &[u8],
) -> Result<PairingKeyCommit> {
    mutate(config, |store, session| {
        store.confirm_sync_pairing_installation(
            &session.installation,
            session.master.as_ref().unwrap(),
            session.revision,
            invitation,
            proof,
            now(),
        )
    })
}

pub(crate) fn pairing_dispatch(
    config: &AppConfig,
    invitation: &str,
) -> Result<crate::storage::sync::PairingDispatch> {
    let sessions = sessions().lock().unwrap_or_else(|p| p.into_inner());
    let session = sessions
        .get(&config.data_dir)
        .context("sync_runtime_request_missing")?;
    ensure!(
        session.joining.is_none()
            && !session.enrollment_busy
            && session.database == config.database_path
            && session.confirmed
            && session.master.is_some(),
        "sync_pairing_runtime_locked"
    );
    Store::open(&session.database)?.sync_pairing_dispatch(
        &session.installation,
        session.master.as_ref().unwrap(),
        session.revision,
        invitation,
        now(),
    )
}

pub(crate) fn pairing_frame(config: &AppConfig, invitation: &str) -> Result<Vec<u8>> {
    let sessions = sessions().lock().unwrap_or_else(|p| p.into_inner());
    let session = sessions
        .get(&config.data_dir)
        .context("sync_runtime_request_missing")?;
    ensure!(
        session.joining.is_none()
            && !session.enrollment_busy
            && session.database == config.database_path
            && session.confirmed
            && session.master.is_some(),
        "sync_pairing_runtime_locked"
    );
    let store = Store::open(&session.database)?;
    store.sync_pairing_frame(
        &session.installation,
        session.master.as_ref().unwrap(),
        session.revision,
        invitation,
        now(),
    )
}

pub(crate) fn pairing_roster(config: &AppConfig, invitation: &str) -> Result<Vec<u8>> {
    let sessions = sessions().lock().unwrap_or_else(|p| p.into_inner());
    let session = sessions
        .get(&config.data_dir)
        .context("sync_runtime_request_missing")?;
    ensure!(
        session.joining.is_none()
            && !session.enrollment_busy
            && session.database == config.database_path
            && session.confirmed
            && session.master.is_some(),
        "sync_pairing_runtime_locked"
    );
    let store = Store::open(&session.database)?;
    store
        .sync_pairing_roster(
            &session.installation,
            session.master.as_ref().unwrap(),
            session.revision,
            invitation,
        )?
        .encode()
}

pub(crate) fn pairing_statuses(
    config: &AppConfig,
) -> Result<Vec<crate::sync::pairing::journal::PairingStatus>> {
    let sessions = sessions().lock().unwrap_or_else(|p| p.into_inner());
    let session = sessions
        .get(&config.data_dir)
        .context("sync_runtime_request_missing")?;
    ensure!(
        session.joining.is_none()
            && !session.enrollment_busy
            && session.database == config.database_path
            && session.confirmed
            && session.master.is_some(),
        "sync_pairing_runtime_locked"
    );
    Store::open(&session.database)?.sync_pairing_statuses(
        &session.installation,
        session.master.as_ref().unwrap(),
        session.revision,
        now(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::runtime::tests::Fixture;
    use crate::sync::{
        crypto::ContentKey,
        pairing::{InvitationRoute, PairingInvitation, PendingJoin},
        transport_relay::RelayClient,
    };

    fn approval(
        f: &Fixture,
        key: &[u8; 32],
        installation: &str,
    ) -> (PendingApproval, PendingJoin, Fixture, LocalJoin) {
        let owner = f.store.load_sync_vault(installation, key, 1).unwrap();
        let target = Fixture::new();
        let join_key = ContentKey::generate().unwrap().to_bytes();
        let join_vault = target
            .store
            .create_sync_vault("join-install", &join_key, "joiner", "test")
            .unwrap();
        let (invite, server) = PairingInvitation::issue_for_relay(
            &f.store.sync_identity().unwrap().unwrap().dataset_id,
            owner.device().clone(),
            owner.identity(),
            now(),
            InvitationRoute {
                relay_url: "http://localhost:9876".into(),
                issuer_mailbox: RelayClient::mailbox_key(owner.mailbox_owner()),
                request_sender: "f".repeat(64),
            },
        )
        .unwrap();
        let (join, request) = PendingJoin::begin_for_relay(
            invite,
            join_vault.device().clone(),
            join_vault.identity(),
            now(),
            RelayClient::mailbox_key(join_vault.mailbox_owner()),
        )
        .unwrap();
        (
            server.receive(&request, now()).unwrap(),
            join,
            target,
            LocalJoin { key: join_key },
        )
    }
    struct LocalJoin {
        key: [u8; 32],
    }

    #[test]
    fn every_pairing_mutation_pauses_runtime_until_native_revision_confirmation() {
        let f = Fixture::new();
        let (key, initial) = f.initialize();
        confirm_key(&f.config, &initial.token, 1).unwrap();
        let (request, join, b, join_key) = approval(&f, &key, &initial.installation_id);
        let committed = approve_pairing(&f.config, request).unwrap();
        assert_eq!(committed.key.revision, 2);
        assert!(!unlocked(&f.config, 2));
        assert!(provider_master(&f.config.database_path, 2).is_none());
        assert!(background_guard(&f.config, &f.store).unwrap().is_none());
        assert!(pairing_frame(&f.config, &committed.invitation_id).is_err());
        assert!(pairing_dispatch(&f.config, &committed.invitation_id).is_err());
        assert!(confirm_key(&f.config, &initial.token, 1).is_err());
        assert!(confirm_key(&f.config, &committed.key.token, 1).is_err());
        f.store
            .submit_input_as_event("capture while native confirmation waits", "capture", None)
            .unwrap();
        confirm_key(&f.config, &committed.key.token, 2).unwrap();
        let response = pairing_frame(&f.config, &committed.invitation_id).unwrap();
        let pair = join.finish(&response, 0, now()).unwrap();
        let installed = b
            .store
            .install_sync_pairing_join("join-install", &join_key.key, 1, pair, now())
            .unwrap();
        let proof = b
            .store
            .sync_pairing_frame(
                "join-install",
                &join_key.key,
                2,
                &installed.invitation_id,
                now(),
            )
            .unwrap();
        let completed =
            confirm_pairing_installation(&f.config, &committed.invitation_id, &proof).unwrap();
        assert_eq!(completed.key.revision, 3);
        assert!(pairing_roster(&f.config, &committed.invitation_id).is_err());
        confirm_key(&f.config, &completed.key.token, 3).unwrap();
        assert!(pairing_roster(&f.config, &committed.invitation_id).is_ok());
        let duplicate =
            confirm_pairing_installation(&f.config, &committed.invitation_id, &proof).unwrap();
        assert_eq!(duplicate.key.token, completed.key.token);
        assert!(unlocked(&f.config, 3));
    }

    #[test]
    fn restart_can_confirm_a_committed_revision_but_cannot_release_frames_early() {
        let f = Fixture::new();
        let (key, initial) = f.initialize();
        confirm_key(&f.config, &initial.token, 1).unwrap();
        let (request, _, _target, _) = approval(&f, &key, &initial.installation_id);
        let committed = approve_pairing(&f.config, request).unwrap();
        lock(&f.config);
        assert!(pairing_frame(&f.config, &committed.invitation_id).is_err());
        let request = prepare_key(&f.config, None, None).unwrap();
        assert_eq!(request.revision, 2);
        let reloaded = install_key(&f.config, &request.token, key.to_vec(), 1).unwrap();
        assert!(pairing_frame(&f.config, &committed.invitation_id).is_err());
        confirm_key(&f.config, &reloaded.token, 2).unwrap();
        let recovered = pairing_statuses(&f.config).unwrap();
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].invitation_id, committed.invitation_id);
        assert_eq!(recovered[0].state, "awaiting_installation");
        assert_eq!(recovered[0].peer_name, "joiner");
        assert!(!recovered[0].peer_device_id.is_empty());
        assert!(recovered[0].expires_at_ms > now());
        assert!(pairing_frame(&f.config, &recovered[0].invitation_id).is_ok());
    }

    #[test]
    fn live_background_lease_and_running_tasks_prevent_vault_mutation() {
        let f = Fixture::new();
        let (key, initial) = f.initialize();
        confirm_key(&f.config, &initial.token, 1).unwrap();
        let (request, _, _target, _) = approval(&f, &key, &initial.installation_id);
        let guard = background_guard(&f.config, &f.store).unwrap().unwrap();
        // Holding the background lease must fail the mutation closed without
        // waiting (still not write anything): production approve waits for the
        // worker to finish, this fail-fast path is what the guard enforces.
        assert!(approve_pairing_now(&f.config, request).is_err());
        assert!(unlocked(&f.config, 1));
        drop(guard);
        f.store
            .submit_input_as_event("running input", "capture", None)
            .unwrap();
        let job = f.store.claim_analysis_job().unwrap().unwrap();
        let (request, _, _target, _) = approval(&f, &key, &initial.installation_id);
        assert!(approve_pairing_now(&f.config, request).is_err());
        assert!(unlocked(&f.config, 1));
        assert_eq!(f.store.analysis_job_stats().unwrap().running, 1);
        f.store.fail_analysis(&job, "synthetic failure").unwrap();
    }

    #[test]
    fn approve_waits_for_in_flight_background_worker_then_proceeds() {
        use std::sync::mpsc;
        use std::time::Instant;

        let f = Fixture::new();
        let (key, initial) = f.initialize();
        confirm_key(&f.config, &initial.token, 1).unwrap();
        let (request, _, _target, _) = approval(&f, &key, &initial.installation_id);

        // Simulate a long in-flight background caller on another "worker thread".
        let config = f.config.clone();
        let (held_tx, held_rx) = mpsc::channel::<()>();
        let (dropped_tx, dropped_rx) = mpsc::channel::<()>();
        let holder = std::thread::spawn(move || {
            let store = crate::storage::Store::open(&config.database_path).unwrap();
            let guard = super::super::background_guard(&config, &store)
                .unwrap()
                .unwrap();
            held_tx.send(()).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(300));
            drop(guard);
            dropped_tx.send(()).unwrap();
        });
        held_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();

        // Production approve waits for the worker, then completes once the
        // lease is released instead of failing `sync_runtime_in_use`.
        let started = Instant::now();
        let committed = approve_pairing(&f.config, request).unwrap();
        assert!(
            started.elapsed() >= std::time::Duration::from_millis(250),
            "approve must wait for the in-flight background caller"
        );
        dropped_rx.recv_timeout(std::time::Duration::from_secs(5)).ok();
        holder.join().unwrap();
        assert_eq!(committed.key.revision, 2);
        // The reservation is gone once the mutation finished, so background
        // callers are no longer deferred.
        assert!(!super::background_pause_active(&f.config.data_dir));
        assert!(background_guard(&f.config, &f.store).unwrap().is_none());
    }
}
