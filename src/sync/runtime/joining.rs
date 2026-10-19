//! Joining is a workspace-locked staging operation, never an owner activation.
//! Its tokens cannot use the current-database key APIs or resume background work.

#[path = "joining_install.rs"]
mod joining_install;

pub(in crate::sync::runtime) use joining_install::{confirm_enrollment_key, join_scope};
pub use joining_install::{confirm_key, install_key};
pub(in crate::sync::runtime) use joining_install::{
    enrollment_available, enrollment_scope, pause_for_enrollment,
};

mod network;
pub(crate) use network::{poll, send_proof, send_request};

use super::*;
use crate::sync::{
    datasets::{DatasetCatalog, DatasetEntry},
    pairing::{PairingDevice, PairingInvitation, PendingJoin},
    vault::LocalVault,
};
use rusqlite::OptionalExtension;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Preview,
    KeyRequired,
    KeyConfirmation,
    Ready,
    AwaitingApproval,
    InstallationConfirmation,
    AwaitingMembership,
    ProofSent,
    Enrollment,
    EnrollmentConfirmation,
    Failed,
}
impl Phase {
    fn name(self) -> &'static str {
        match self {
            Self::Preview => "preview",
            Self::KeyRequired => "key_required",
            Self::KeyConfirmation => "key_confirmation",
            Self::Ready => "ready_to_request",
            Self::AwaitingApproval => "awaiting_approval",
            Self::InstallationConfirmation => "installation_confirmation",
            Self::AwaitingMembership => "awaiting_membership",
            Self::ProofSent => "proof_sent",
            Self::Enrollment => "enrollment",
            Self::EnrollmentConfirmation => "enrollment_confirmation",
            Self::Failed => "requires_new_invitation",
        }
    }
}

pub(super) struct JoinState {
    scope: PathBuf,
    entry: Option<DatasetEntry>,
    invitation: Option<PairingInvitation>,
    peer: Option<PairingDevice>,
    dataset: String,
    invitation_id: String,
    relay_url: String,
    deadline: i64,
    phase: Phase,
    restoring: bool,
    fingerprint: Option<String>,
    pending: Option<PendingJoin>,
    request: Vec<u8>,
    cursor: String,
    network_busy: bool,
    approval_mail: Option<String>,
}

/// Metadata only; neither a transport frame nor an installation capability.
#[derive(Clone, Debug)]
pub(crate) struct JoinView {
    pub operation_id: String,
    pub token: String,
    pub entry_id: Option<String>,
    pub installation_id: Option<String>,
    pub dataset_id: String,
    pub owner_name: String,
    pub owner_fingerprint: String,
    pub local_fingerprint: Option<String>,
    pub relay_url: String,
    pub expires_at_ms: i64,
    pub phase: String,
    pub revision: i64,
    pub creating: bool,
    pub approval_ack_pending: bool,
}

fn view(session: &Session) -> JoinView {
    let join = session.joining.as_ref().expect("joining session");
    let expired = join.deadline > 0
        && now() >= join.deadline
        && !matches!(
            join.phase,
            Phase::Enrollment | Phase::EnrollmentConfirmation
        );
    JoinView {
        operation_id: session.operation.clone(),
        token: session.token.clone(),
        entry_id: join.entry.as_ref().map(|e| e.id.clone()),
        installation_id: join.entry.as_ref().map(|e| e.installation_id.clone()),
        dataset_id: join.dataset.clone(),
        owner_name: join
            .peer
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_default(),
        owner_fingerprint: join
            .peer
            .as_ref()
            .map(PairingDevice::fingerprint)
            .unwrap_or_default(),
        local_fingerprint: join.fingerprint.clone(),
        relay_url: join.relay_url.clone(),
        expires_at_ms: join.deadline,
        phase: if expired {
            "expired"
        } else {
            join.phase.name()
        }
        .into(),
        revision: session.revision,
        creating: session.creating,
        approval_ack_pending: join.approval_mail.is_some(),
    }
}

fn session<'a>(
    all: &'a mut HashMap<PathBuf, Session>,
    config: &AppConfig,
) -> Result<&'a mut Session> {
    let session = all
        .get_mut(&config.data_dir)
        .context("sync_join_session_missing")?;
    let join = session
        .joining
        .as_ref()
        .context("sync_join_wrong_session_kind")?;
    ensure!(
        join.scope == config.database_path,
        "sync_join_scope_changed"
    );
    Ok(session)
}

fn insert_session(config: &AppConfig, join: JoinState) -> Result<JoinView> {
    let mut all = sessions().lock().unwrap_or_else(|p| p.into_inner());
    ensure!(
        !all.contains_key(&config.data_dir),
        "sync_runtime_request_already_open"
    );
    let lease = lease(&config.data_dir)?;
    let store = Store::open(&config.database_path)?;
    let tx = store.begin_write_transaction()?;
    store.require_background_quiescent_for_sync()?;
    if let Some(tx) = tx {
        tx.commit()?;
    }
    let token = uuid::Uuid::new_v4().to_string();
    let session = Session {
        lease,
        database: config.database_path.clone(),
        token: token.clone(),
        operation: token,
        installation: String::new(),
        revision: 0,
        expires_at: now() + 10 * 60 * 1000,
        master: None,
        confirmed: false,
        creating: true,
        name: String::new(),
        platform: String::new(),
        owner_invitation: None,
        joining: Some(join),
        enrollment_busy: false,
        enrollment_confirmation: None,
        proof_hold_until: 0,
    };
    let result = view(&session);
    all.insert(config.data_dir.clone(), session);
    Ok(result)
}

pub fn preview(config: &AppConfig, code: &str) -> Result<JoinView> {
    let invitation = PairingInvitation::decode(code.trim(), now())?;
    let route = invitation.route().context("pairing_relay_route_missing")?;
    insert_session(
        config,
        JoinState {
            scope: config.database_path.clone(),
            entry: None,
            peer: Some(invitation.issuer().clone()),
            dataset: invitation.dataset_id().into(),
            invitation_id: invitation.invitation_id().into(),
            relay_url: route.relay_url.clone(),
            deadline: invitation.expires_at_ms(),
            invitation: Some(invitation),
            phase: Phase::Preview,
            restoring: false,
            fingerprint: None,
            pending: None,
            request: Vec::new(),
            cursor: "0".into(),
            network_busy: false,
            approval_mail: None,
        },
    )
}

pub fn prepare_key(
    config: &AppConfig,
    token: &str,
    name: &str,
    platform: &str,
    consent: bool,
) -> Result<JoinView> {
    ensure!(consent, "sync_provider_key_consent_required");
    PairingDevice {
        device_id: "pending".into(),
        stream_id: "pending-stream".into(),
        name: name.into(),
        platform: platform.into(),
        signing_key: [0; 32],
    }
    .validate()?;
    let mut all = sessions().lock().unwrap_or_else(|p| p.into_inner());
    let session = session(&mut all, config)?;
    let join = session.joining.as_mut().unwrap();
    ensure!(
        session.token == token && join.phase == Phase::Preview && now() < join.deadline,
        "sync_join_preview_stale"
    );
    let catalog = DatasetCatalog::open(&config.data_dir)?;
    let installation = uuid::Uuid::new_v4().to_string();
    let entry = catalog.create_staging(&join.dataset, &installation, name)?;
    session.database = catalog.path_for(&entry.id)?;
    session.installation = installation;
    session.name = name.into();
    session.platform = platform.into();
    session.revision = 1;
    session.token = uuid::Uuid::new_v4().to_string();
    join.entry = Some(entry);
    join.phase = Phase::KeyRequired;
    Ok(view(session))
}

fn staging_store(config: &AppConfig, session: &Session) -> Result<Store> {
    let join = session
        .joining
        .as_ref()
        .context("sync_join_wrong_session_kind")?;
    let expected = join.entry.as_ref().context("sync_join_staging_missing")?;
    let catalog = DatasetCatalog::open(&config.data_dir)?;
    let current = catalog
        .list()?
        .into_iter()
        .find(|e| e.id == expected.id)
        .context("sync_join_staging_missing")?;
    ensure!(
        current == *expected
            && !current.ready
            && current.installation_id == session.installation
            && current.dataset_id == join.dataset,
        "sync_join_staging_changed"
    );
    let path = catalog.path_for(&current.id)?;
    ensure!(
        path == session.database && path != config.database_path,
        "sync_join_scope_changed"
    );
    if !session.creating {
        ensure!(path.is_file(), "sync_join_staging_missing");
    }
    let store = Store::open(&path)?;
    Ok(match session.master {
        Some(master) => store.with_provider_master(master),
        None => store,
    })
}

/// Inventory is bounded by the catalog and never reads native keys or secrets.
pub fn pending_entries(config: &AppConfig) -> Result<Vec<DatasetEntry>> {
    if !config.data_dir.join("sync-workspace.db").try_exists()? {
        return Ok(Vec::new());
    }
    Ok(DatasetCatalog::open(&config.data_dir)?
        .list()?
        .into_iter()
        .filter(|e| !e.ready)
        .collect())
}

pub fn prepare_resume(config: &AppConfig, entry_id: &str) -> Result<JoinView> {
    // Acquire the shared lease before examining a target or reserving a session.
    let initial = insert_session(
        config,
        JoinState {
            scope: config.database_path.clone(),
            entry: None,
            invitation: None,
            peer: None,
            dataset: String::new(),
            invitation_id: String::new(),
            relay_url: String::new(),
            deadline: 0,
            phase: Phase::KeyRequired,
            restoring: true,
            fingerprint: None,
            pending: None,
            request: Vec::new(),
            cursor: "0".into(),
            network_busy: false,
            approval_mail: None,
        },
    )?;
    let result = (|| {
        let mut all = sessions().lock().unwrap_or_else(|p| p.into_inner());
        let session = session(&mut all, config)?;
        ensure!(
            session.operation == initial.operation_id,
            "sync_join_request_stale"
        );
        let catalog = DatasetCatalog::open(&config.data_dir)?;
        let entry = catalog
            .list()?
            .into_iter()
            .find(|e| e.id == entry_id && !e.ready)
            .context("sync_join_staging_missing")?;
        let path = catalog.path_for(&entry.id)?;
        ensure!(
            path.is_file() && path != config.database_path,
            "sync_join_staging_missing"
        );
        let store = Store::open(&path)?;
        let record: Option<(String, i64)> = store
            .connection
            .query_row(
                "SELECT installation_id,revision FROM sync_local_vault WHERE singleton=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let (installation, revision) = record.context("sync_join_requires_new_invitation")?;
        ensure!(
            installation == entry.installation_id && revision > 0,
            "sync_join_staging_changed"
        );
        session.database = path;
        session.installation = installation;
        session.revision = revision;
        session.creating = false;
        let join = session.joining.as_mut().unwrap();
        join.dataset = entry.dataset_id.clone();
        join.entry = Some(entry);
        Ok(view(session))
    })();
    if result.is_err() {
        release(config, &initial.operation_id);
    }
    result
}

pub fn state(config: &AppConfig, operation: &str) -> Result<JoinView> {
    let mut all = sessions().lock().unwrap_or_else(|p| p.into_inner());
    let session = session(&mut all, config)?;
    ensure!(session.operation == operation, "sync_join_request_stale");
    Ok(view(session))
}

/// Local cancellation only. Temporary relay grants expire; a sent request or
/// installation proof cannot be recalled by closing this local staging session.
pub fn release(config: &AppConfig, operation: &str) -> bool {
    let mut all = sessions().lock().unwrap_or_else(|p| p.into_inner());
    if all.get(&config.data_dir).is_some_and(|s| {
        s.operation == operation
            && s.joining
                .as_ref()
                .is_some_and(|j| j.scope == config.database_path)
    }) {
        all.remove(&config.data_dir);
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests;
