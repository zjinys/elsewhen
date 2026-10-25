//! One process owns synchronization in a workspace. Native protected storage
//! confirms a vault revision before the runtime becomes usable; live background
//! callers retain the file lease, even if the UI locks the runtime meanwhile.

pub(crate) mod enrollment;
pub(crate) mod handoff;
pub(crate) mod invitations;
pub(crate) mod joining;
pub(crate) mod network;
pub(crate) mod pairing;

use crate::{
    config::AppConfig,
    storage::{Store, sync::ReplicaIdentity},
};
use anyhow::{Context, Result, ensure};
use std::{
    collections::{HashMap, HashSet},
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

/// 为一份快照导出准备 blob 上传上下文：接收方信箱 + 本机在该信箱里的 sender 凭据。
///
/// 只在 `SYNC_SNAPSHOT_TRANSPORT=blob` 时才去读路由/派生凭据（mail 传输不该多
/// 一次凭据派生）；拿不到任何东西就返回 `None`，让导出退回信件路径。
pub(crate) fn snapshot_upload(
    vault: &crate::sync::vault::LocalVault,
    dataset_id: &str,
    peer: &str,
) -> Result<Option<crate::sync::engine::snapshot::BlobUpload>> {
    use crate::storage::sync::SnapshotTransport;
    use crate::sync::transport_relay::RelayClient;
    if SnapshotTransport::from_env() != SnapshotTransport::Blob {
        return Ok(None);
    }
    let (send_key, _, key_version) = vault.peer_keys(dataset_id, peer)?;
    let route = vault.peer_route(dataset_id, peer)?.clone();
    let credential = RelayClient::derive_sender_credential(
        &send_key.to_bytes(),
        &route.local_mailbox,
        &route.peer_mailbox,
    )?;
    let client = RelayClient::new(&route.relay_url)?;
    // 能力协商（review 2026-10-25 M2）：环境变量只说「我们想要数据面」，
    // 没说「这台 relay 有」。没有 `/blob/*` 路由的旧 relay 上强行上传，就是每拍
    // 两次 404、还要退避；在配对面板看来叫「快照同步永远不完成」。
    //
    // 探测问的是**本机信箱**的 `/status`：同一台 relay 实例，能力等价，而发送方
    // 凭据本来无权读对端 `/status`。
    if !relay_supports_blob(&client, &route.local_mailbox, vault.mailbox_owner()) {
        return Ok(None);
    }
    Ok(Some(crate::sync::engine::snapshot::BlobUpload {
        client,
        mailbox: route.peer_mailbox.clone(),
        sender_mb: route.local_mailbox.clone(),
        credential,
        recipient: peer.to_string(),
        key_version,
        // 一拍最多为一个窗口等一等；等不到就本拍收手，下一拍续传。
        window_wait_ms: crate::sync::engine::snapshot::WINDOW_WAIT_MS,
    }))
}

/// 这台 relay 支不支持 blob 数据面（问一次 `/status`，结果按 `base_url` 缓存）。
///
/// 探测失败**不**缓存：一次网络抖动不该让本进程永久退回信件面。但「问到了
/// 且没有 `blob`」要缓存 —— 那是对这台 relay 的结论，不是一次事故。
fn relay_supports_blob(
    client: &crate::sync::transport_relay::RelayClient,
    local_mailbox: &str,
    owner_credential: &[u8; 32],
) -> bool {
    let url = client.base_url();
    if let Some(known) = crate::sync::transport_relay::blob_capability(url) {
        return known;
    }
    match client.status(local_mailbox, owner_credential) {
        Ok(status) => {
            let supported = status.capabilities.iter().any(|item| item == "blob");
            crate::sync::transport_relay::set_blob_capability(url, supported);
            supported
        }
        // 保守：信件路径永远能跑，数据面探测不到就先不用它。
        Err(_) => false,
    }
}

struct WorkspaceLease {
    _file: File,
}

struct Session {
    lease: Arc<WorkspaceLease>,
    database: PathBuf,
    token: String,
    operation: String,
    installation: String,
    revision: i64,
    expires_at: i64,
    master: Option<[u8; 32]>,
    confirmed: bool,
    creating: bool,
    name: String,
    platform: String,
    owner_invitation: Option<invitations::OwnerInvitation>,
    joining: Option<joining::JoinState>,
    enrollment_busy: bool,
    enrollment_confirmation: Option<(String, crate::sync::enrollment::EnrollmentRole)>,
    proof_hold_until: i64,
}
impl Drop for Session {
    fn drop(&mut self) {
        if let Some(key) = self.master.as_mut() {
            key.fill(0);
        }
    }
}
static SESSIONS: OnceLock<Mutex<HashMap<PathBuf, Session>>> = OnceLock::new();
fn sessions() -> &'static Mutex<HashMap<PathBuf, Session>> {
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Clone, Debug)]
pub(crate) struct KeyRequest {
    pub token: String,
    pub installation_id: String,
    pub revision: i64,
    pub creating: bool,
}
#[derive(Clone, Debug)]
pub(crate) struct KeyCommit {
    pub token: String,
    pub installation_id: String,
    pub revision: i64,
}

pub(crate) struct BackgroundGuard {
    _lease: Arc<WorkspaceLease>,
    busy_root: Option<PathBuf>,
}
static BACKGROUND_BUSY: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
fn busy() -> &'static Mutex<HashSet<PathBuf>> {
    BACKGROUND_BUSY.get_or_init(|| Mutex::new(HashSet::new()))
}
impl Drop for BackgroundGuard {
    fn drop(&mut self) {
        if let Some(root) = &self.busy_root {
            busy()
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .remove(root);
        }
    }
}

/// Recovery must not reuse a live worker's lease, even in the same process.
pub(crate) fn initialization_guard(config: &AppConfig) -> Result<Option<BackgroundGuard>> {
    match lease(&config.data_dir) {
        Ok(lease) => Ok(Some(BackgroundGuard {
            _lease: lease,
            busy_root: None,
        })),
        Err(error) if error.to_string() == "sync_runtime_in_use" => Ok(None),
        Err(error) => Err(error),
    }
}

fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
fn lease(root: &Path) -> Result<Arc<WorkspaceLease>> {
    let path = root.join("sync-runtime.lock");
    if let Ok(meta) = std::fs::symlink_metadata(&path) {
        ensure!(
            meta.is_file() && !meta.file_type().is_symlink(),
            "sync_runtime_lock_invalid"
        );
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    file.try_lock()
        .map_err(|_| anyhow::anyhow!("sync_runtime_in_use"))?;
    Ok(Arc::new(WorkspaceLease { _file: file }))
}

pub(crate) fn provider_master(database: &Path, revision: i64) -> Option<[u8; 32]> {
    sessions()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .values()
        .find(|s| {
            s.joining.is_none() && s.database == database && s.revision == revision && s.confirmed
        })
        .and_then(|s| s.master)
}

pub(crate) fn unlocked(config: &AppConfig, revision: i64) -> bool {
    sessions()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(&config.data_dir)
        .is_some_and(|s| {
            s.joining.is_none()
                && s.database == config.database_path
                && s.revision == revision
                && s.confirmed
                && s.master.is_some()
        })
}

/// P2 repair handoff: the master for a session's confirmed vault revision.
/// Callers must re-verify the target dataset themselves; this only proves the
/// process once held the native-confirmed key for that exact revision. Plain
/// sessions must be bound to the pinned database; a joining session qualifies
/// when its original scope matches (a completed restore is the same authority
/// as a plain unlock, and selection re-verifies entry/revision itself).
pub(crate) fn session_master(config: &AppConfig, revision: i64) -> Option<[u8; 32]> {
    sessions()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(&config.data_dir)
        .filter(|s| {
            s.revision == revision
                && s.confirmed
                && s.master.is_some()
                && match joining::join_scope(s) {
                    None => s.database == config.database_path,
                    // A resumed join binds to its catalog entry's database;
                    // the selection flow re-verifies entry/revision itself.
                    Some(scope) => scope == config.database_path,
                }
        })
        .and_then(|s| s.master)
}

pub(crate) fn lock(config: &AppConfig) {
    sessions()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(&config.data_dir);
}

pub(crate) fn release_request(config: &AppConfig, operation: &str) -> bool {
    let mut sessions = sessions().lock().unwrap_or_else(|p| p.into_inner());
    if sessions.get(&config.data_dir).is_some_and(|s| {
        s.joining.is_none() && s.database == config.database_path && s.operation == operation
    }) {
        sessions.remove(&config.data_dir);
        true
    } else {
        false
    }
}

pub(crate) fn background_guard(
    config: &AppConfig,
    store: &Store,
) -> Result<Option<BackgroundGuard>> {
    let authority = store.has_sync_authority()?;
    if !authority && store.sync_identity()?.is_some() {
        return Ok(None);
    }
    let sessions = sessions().lock().unwrap_or_else(|p| p.into_inner());
    // A pending pairing mutation has reserved the workspace: defer any new
    // background caller so the user-facing native write can proceed right
    // after the in-flight worker finishes (pairing::mutate_with_wait).
    if pairing::background_pause_active(&config.data_dir) {
        return Ok(None);
    }
    let mut busy = busy().lock().unwrap_or_else(|p| p.into_inner());
    if busy.contains(&config.data_dir) {
        return Ok(None);
    }
    let lease = if let Some(session) = sessions.get(&config.data_dir) {
        if session.joining.is_some()
            || session.proof_hold_until > now()
            || !authority
            || session.database != config.database_path
            || !session.confirmed
            || session.master.is_none()
        {
            return Ok(None);
        }
        let revision: i64 = store.connection.query_row(
            "SELECT revision FROM sync_local_vault WHERE singleton=1",
            [],
            |r| r.get(0),
        )?;
        if revision != session.revision {
            return Ok(None);
        }
        session.lease.clone()
    } else {
        if authority {
            return Ok(None);
        }
        match lease(&config.data_dir) {
            Ok(lease) => lease,
            Err(error) if error.to_string() == "sync_runtime_in_use" => return Ok(None),
            Err(error) => return Err(error),
        }
    };
    busy.insert(config.data_dir.clone());
    Ok(Some(BackgroundGuard {
        _lease: lease,
        busy_root: Some(config.data_dir.clone()),
    }))
}

pub(crate) fn prepare_key(
    config: &AppConfig,
    name: Option<&str>,
    platform: Option<&str>,
) -> Result<KeyRequest> {
    let store = Store::open(&config.database_path)?;
    use rusqlite::OptionalExtension;
    let existing: Option<(String, i64)> = store
        .connection
        .query_row(
            "SELECT installation_id,revision FROM sync_local_vault WHERE singleton=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let creating = existing.is_none();
    let (installation, revision) = if let Some(existing) = existing {
        ensure!(
            store.has_sync_authority()?,
            "sync_runtime_setup_requires_repair"
        );
        existing
    } else {
        ensure!(
            store.sync_identity()?.is_none(),
            "sync_runtime_setup_requires_repair"
        );
        let tx = store.begin_write_transaction()?;
        store.require_background_quiescent_for_sync()?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        let name = name.context("sync_device_name_required")?;
        let platform = platform.context("sync_device_platform_required")?;
        crate::sync::pairing::PairingDevice {
            device_id: "pending".into(),
            stream_id: "pending-stream".into(),
            name: name.into(),
            platform: platform.into(),
            signing_key: [0; 32],
        }
        .validate()?;
        (uuid::Uuid::new_v4().to_string(), 0)
    };
    let mut sessions = sessions().lock().unwrap_or_else(|p| p.into_inner());
    if sessions
        .get(&config.data_dir)
        .is_some_and(|s| s.joining.is_none() && !s.confirmed && s.expires_at <= now())
    {
        sessions.remove(&config.data_dir);
    }
    ensure!(
        !sessions.contains_key(&config.data_dir),
        "sync_runtime_request_already_open"
    );
    let lease = lease(&config.data_dir)?;
    let token = uuid::Uuid::new_v4().to_string();
    sessions.insert(
        config.data_dir.clone(),
        Session {
            lease,
            database: config.database_path.clone(),
            token: token.clone(),
            operation: token.clone(),
            installation: installation.clone(),
            revision,
            expires_at: now() + 10 * 60 * 1000,
            master: None,
            confirmed: false,
            creating,
            name: name.unwrap_or_default().into(),
            platform: platform.unwrap_or_default().into(),
            owner_invitation: None,
            joining: None,
            enrollment_busy: false,
            enrollment_confirmation: None,
            proof_hold_until: 0,
        },
    );
    Ok(KeyRequest {
        token,
        installation_id: installation,
        revision: if creating { 1 } else { revision },
        creating,
    })
}

pub(crate) fn install_key(
    config: &AppConfig,
    token: &str,
    mut key: Vec<u8>,
    protected_revision: i64,
) -> Result<KeyCommit> {
    let master: Result<[u8; 32]> = key
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("sync_master_key_invalid"));
    key.fill(0);
    let master = master?;
    let mut sessions = sessions().lock().unwrap_or_else(|p| p.into_inner());
    let session = sessions
        .get_mut(&config.data_dir)
        .context("sync_runtime_request_missing")?;
    ensure!(
        session.joining.is_none()
            && session.database == config.database_path
            && session.token == token
            && session.expires_at > now()
            && !session.confirmed,
        "sync_runtime_request_stale"
    );
    let store = Store::open(&config.database_path)?;
    if session.creating {
        ensure!(
            protected_revision == 1,
            "sync_new_installation_revision_not_reserved"
        );
        let journal: String = store
            .connection
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
        if !journal.eq_ignore_ascii_case("wal") {
            store
                .connection
                .pragma_update(None, "journal_mode", "WAL")?;
        }
        let tx = store.begin_write_transaction()?;
        let vault = store.create_sync_vault(
            &session.installation,
            &master,
            &session.name,
            &session.platform,
        )?;
        store.initialize_sync_replica(&ReplicaIdentity {
            dataset_id: uuid::Uuid::new_v4().to_string(),
            device_id: vault.device().device_id.clone(),
            stream_id: vault.device().stream_id.clone(),
            processor_device_id: vault.device().device_id.clone(),
        })?;
        store.initialize_sync_authority(vault.device(), vault.identity())?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        session.revision = vault.revision;
        session.creating = false;
    } else {
        ensure!(protected_revision > 0, "sync_protected_revision_missing");
        let vault = store.load_sync_vault(&session.installation, &master, protected_revision)?;
        ensure!(
            vault.revision == session.revision,
            "sync_runtime_vault_changed"
        );
        store.require_sync_signer(vault.identity())?;
    }
    session.master = Some(master);
    session.token = uuid::Uuid::new_v4().to_string();
    session.expires_at = now() + 10 * 60 * 1000;
    Ok(KeyCommit {
        token: session.token.clone(),
        installation_id: session.installation.clone(),
        revision: session.revision,
    })
}

pub(crate) fn confirm_key(config: &AppConfig, token: &str, protected_revision: i64) -> Result<()> {
    confirm_key_inner(config, token, protected_revision, false)
}

pub(super) fn confirm_enrollment_key(
    config: &AppConfig,
    token: &str,
    protected_revision: i64,
) -> Result<()> {
    confirm_key_inner(config, token, protected_revision, true)
}

fn confirm_key_inner(
    config: &AppConfig,
    token: &str,
    protected_revision: i64,
    enrollment: bool,
) -> Result<()> {
    let mut sessions = sessions().lock().unwrap_or_else(|p| p.into_inner());
    let session = sessions
        .get_mut(&config.data_dir)
        .context("sync_runtime_request_missing")?;
    ensure!(
        session.joining.is_none()
            && session.database == config.database_path
            && session.token == token
            && session.expires_at > now()
            && protected_revision == session.revision
            && session.master.is_some()
            && !session.enrollment_busy
            && enrollment == session.enrollment_confirmation.is_some(),
        "sync_runtime_confirmation_stale"
    );
    if session.confirmed {
        return Ok(());
    }
    let store =
        Store::open(&config.database_path)?.with_provider_master(*session.master.as_ref().unwrap());
    let vault = store.load_sync_vault(
        &session.installation,
        session.master.as_ref().unwrap(),
        protected_revision,
    )?;
    ensure!(
        vault.revision == protected_revision,
        "sync_runtime_vault_changed"
    );
    store.require_sync_signer(vault.identity())?;
    if enrollment {
        // Enrollment mutations already required quiescence. Never turn a later
        // confirmation into recovery of unrelated live background work.
        ensure!(
            !busy()
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .contains(&config.data_dir),
            "sync_runtime_in_use"
        );
        let tx = store.begin_write_transaction()?;
        store.require_background_quiescent_for_sync()?;
        ensure!(store.provider_keys_sealed()?, "provider_keys_locked");
        if let Some(tx) = tx {
            tx.commit()?;
        }
    } else {
        store.enable_provider_key_sealing()?;
        store.recover_interrupted_analysis_jobs()?;
        store.recover_interrupted_digest_jobs()?;
    }
    session.proof_hold_until = vault
        .pairing_statuses(now())
        .into_iter()
        .filter(|p| p.state == "awaiting_installation")
        .map(|p| p.expires_at_ms)
        .max()
        .unwrap_or(0);
    session.confirmed = true;
    Ok(())
}

#[cfg(test)]
mod capability_tests;
#[cfg(test)]
mod tests;
