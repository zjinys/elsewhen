//! 成员加入流程：预览、密钥准备/安装/确认、暂存与请求发送。

use super::*;

/// Read-only join progress. The operation and confirmation tokens have distinct
/// scopes; no arbitrary staging path, Noise frame or transport key crosses FRB.
#[derive(Clone, Debug)]
pub struct SyncJoinStateDto {
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

#[derive(Clone, Debug)]
pub struct SyncJoinEntryDto {
    pub entry_id: String,
    pub installation_id: String,
    pub dataset_id: String,
    pub name: String,
}

fn join_dto(state: runtime::joining::JoinView) -> SyncJoinStateDto {
    SyncJoinStateDto {
        operation_id: state.operation_id,
        token: state.token,
        entry_id: state.entry_id,
        installation_id: state.installation_id,
        dataset_id: state.dataset_id,
        owner_name: state.owner_name,
        owner_fingerprint: state.owner_fingerprint,
        local_fingerprint: state.local_fingerprint,
        relay_url: state.relay_url,
        expires_at_ms: state.expires_at_ms,
        phase: state.phase,
        revision: state.revision,
        creating: state.creating,
        approval_ack_pending: state.approval_ack_pending,
    }
}

/// Parses and binds an invitation preview without contacting its relay.
pub fn preview_sync_join(invitation_code: String) -> Result<SyncJoinStateDto> {
    Ok(join_dto(runtime::joining::preview(
        &AppConfig::load()?,
        &invitation_code,
    )?))
}

pub fn prepare_sync_join_key(
    preview_token: String,
    name: String,
    platform: String,
    acknowledge_provider_key_sharing: bool,
) -> Result<SyncJoinStateDto> {
    Ok(join_dto(runtime::joining::prepare_key(
        &AppConfig::load()?,
        &preview_token,
        &name,
        &platform,
        acknowledge_provider_key_sharing,
    )?))
}

pub fn list_pending_sync_joins() -> Result<Vec<SyncJoinEntryDto>> {
    Ok(runtime::joining::pending_entries(&AppConfig::load()?)?
        .into_iter()
        .map(|entry| SyncJoinEntryDto {
            entry_id: entry.id,
            installation_id: entry.installation_id,
            dataset_id: entry.dataset_id,
            name: entry.name,
        })
        .collect())
}

pub fn prepare_sync_join_resume(entry_id: String) -> Result<SyncJoinStateDto> {
    Ok(join_dto(runtime::joining::prepare_resume(
        &AppConfig::load()?,
        &entry_id,
    )?))
}

/// 删除一个未完成的加入暂存项（仅 staging；ready 数据集永不受理）。
/// 对应的本机密钥由 Dart 侧 SyncKeyStore 在同一动作里删除。
pub fn discard_sync_join_staging(entry_id: String) -> Result<()> {
    crate::sync::protocol::validate_id(&entry_id)?;
    crate::sync::datasets::DatasetCatalog::open(&AppConfig::load()?.data_dir)?
        .discard_staging(&entry_id)
}

pub fn install_sync_join_key(
    token: String,
    key: Vec<u8>,
    protected_revision: i64,
) -> Result<SyncJoinStateDto> {
    Ok(join_dto(runtime::joining::install_key(
        &AppConfig::load()?,
        &token,
        key,
        protected_revision,
    )?))
}

pub fn confirm_sync_join_key(token: String, protected_revision: i64) -> Result<SyncJoinStateDto> {
    Ok(join_dto(runtime::joining::confirm_key(
        &AppConfig::load()?,
        &token,
        protected_revision,
    )?))
}

pub fn get_sync_join_state(operation_id: String) -> Result<SyncJoinStateDto> {
    Ok(join_dto(runtime::joining::state(
        &AppConfig::load()?,
        &operation_id,
    )?))
}

pub fn send_sync_join_request(operation_id: String) -> Result<SyncJoinStateDto> {
    Ok(join_dto(runtime::joining::send_request(
        &AppConfig::load()?,
        &operation_id,
    )?))
}

pub fn inspect_sync_join_approval(operation_id: String) -> Result<SyncJoinStateDto> {
    Ok(join_dto(runtime::joining::poll(
        &AppConfig::load()?,
        &operation_id,
    )?))
}

pub fn send_sync_join_proof(operation_id: String) -> Result<SyncJoinStateDto> {
    Ok(join_dto(runtime::joining::send_proof(
        &AppConfig::load()?,
        &operation_id,
    )?))
}

/// Stops this local operation only; it neither recalls a delivered proof nor
/// deletes the staged database/native key, and it cannot release a newer session.
pub fn release_sync_join(operation_id: String) -> Result<bool> {
    Ok(runtime::joining::release(
        &AppConfig::load()?,
        &operation_id,
    ))
}

/// Enrollment progress never selects the staged database or claims ongoing sync.
#[derive(Clone, Debug)]
pub struct SyncEnrollmentStateDto {
    pub role: String,
    pub operation_id: String,
    pub invitation_id: String,
    pub installation_id: String,
    pub revision: i64,
    pub confirmation_token: Option<String>,
    pub phase: String,
    pub snapshot_id: Option<String>,
    pub received_chunks: i64,
    pub total_chunks: i64,
    pub local_applied: bool,
    pub remote_applied: bool,
    pub notification_pending: bool,
}

fn enrollment_dto(state: runtime::enrollment::EnrollmentView) -> SyncEnrollmentStateDto {
    SyncEnrollmentStateDto {
        role: state.role,
        operation_id: state.operation_id,
        invitation_id: state.invitation_id,
        installation_id: state.installation_id,
        revision: state.revision,
        confirmation_token: state.confirmation_token,
        phase: state.phase,
        snapshot_id: state.snapshot_id,
        received_chunks: state.received_chunks,
        total_chunks: state.total_chunks,
        local_applied: state.local_applied,
        remote_applied: state.remote_applied,
        notification_pending: state.notification_pending,
    }
}

pub fn list_sync_enrollments() -> Result<Vec<SyncEnrollmentStateDto>> {
    Ok(runtime::enrollment::list(&AppConfig::load()?)?
        .into_iter()
        .map(enrollment_dto)
        .collect())
}

pub fn advance_sync_enrollment(
    role: String,
    operation_id: String,
    invitation_id: String,
) -> Result<SyncEnrollmentStateDto> {
    Ok(enrollment_dto(runtime::enrollment::advance(
        &AppConfig::load()?,
        &role,
        &operation_id,
        &invitation_id,
    )?))
}

pub fn confirm_sync_enrollment(
    role: String,
    operation_id: String,
    invitation_id: String,
    token: String,
    protected_revision: i64,
) -> Result<SyncEnrollmentStateDto> {
    Ok(enrollment_dto(runtime::enrollment::confirm(
        &AppConfig::load()?,
        &role,
        &operation_id,
        &invitation_id,
        &token,
        protected_revision,
    )?))
}

/// What one bounded incremental sync tick actually did. All counters are
/// best-effort progress signals; receipt confirmation arrives in later ticks.
#[derive(Clone, Debug)]
pub struct SyncTickResultDto {
    /// false when a tick must not run now (locked, another worker busy, a
    /// snapshot import still staging, sync not initialized). Not an error.
    pub ran: bool,
    pub staged_operations: i64,
    pub uploaded: i64,
    pub pulled_accepted: i64,
    pub pulled_waiting: i64,
    pub pulled_rejected: i64,
    /// Rejected because the sender runs a protocol version we cannot decode.
    /// Deployment mismatch, not content corruption; upgrades heal it.
    pub pulled_version_mismatch: i64,
    /// True when this tick re-sent the current signed roster to a stale
    /// member (P4e convergence signal; diagnostic only).
    pub roster_converged: bool,
    pub acknowledgement_failed: bool,
    /// Devices waiting for a re-snapshot (fell too far behind); GC stays
    /// frozen for them until the transfer completes.
    pub resnapshot_pending: Vec<String>,
}

/// Runs one bounded push/pull cycle against every active peer. Callers are the
/// app background worker only; there is no manual "sync now" business trigger.
/// A failure here never affects local capture, which is purely local.
pub fn trigger_network_sync() -> Result<SyncTickResultDto> {
    let config = AppConfig::load()?;
    match runtime::network::tick(&config)? {
        Some(report) => Ok(SyncTickResultDto {
            ran: true,
            staged_operations: report.staged_operations as i64,
            uploaded: report.uploaded as i64,
            pulled_accepted: report.pulled_accepted as i64,
            pulled_waiting: report.pulled_waiting as i64,
            pulled_rejected: report.pulled_rejected as i64,
            pulled_version_mismatch: report.pulled_version_mismatch as i64,
            roster_converged: report.roster_converged,
            acknowledgement_failed: report.acknowledgement_failed,
            resnapshot_pending: report.resnapshot_pending.clone(),
        }),
        None => Ok(SyncTickResultDto {
            ran: false,
            staged_operations: 0,
            uploaded: 0,
            pulled_accepted: 0,
            pulled_waiting: 0,
            pulled_rejected: 0,
            pulled_version_mismatch: 0,
            roster_converged: false,
            acknowledgement_failed: false,
            resnapshot_pending: Vec::new(),
        }),
    }
}

/// One row of the installation-local dataset catalog.
#[derive(Clone, Debug)]
pub struct SyncDatasetEntryDto {
    pub entry_id: String,
    pub dataset_id: String,
    pub installation_id: String,
    pub name: String,
    pub ready: bool,
    pub selected: bool,
}
