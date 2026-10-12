//! Local synchronization setup, protected-key handshake and owner invitations.
//! Invitations require an unlocked owner and explicit Provider-key disclosure;
//! ordinary synchronization is not started by these pairing-only operations.

use crate::{config::AppConfig, storage::Store, sync::runtime};
use anyhow::{Context, Result, ensure};
use rusqlite::OptionalExtension;

#[derive(Clone, Debug)]
pub struct LocalSyncStateDto {
    pub state: String,
    pub installation_id: Option<String>,
    pub vault_revision: i64,
    pub dataset_id: Option<String>,
    pub device_id: Option<String>,
    pub device_name: Option<String>,
    pub is_owner: bool,
    pub is_processor: bool,
    pub active_peers: i64,
    pub pending_operations: i64,
}
#[derive(Clone, Debug)]
pub struct SyncKeyRequestDto {
    pub token: String,
    pub installation_id: String,
    pub revision: i64,
    pub creating: bool,
}
#[derive(Clone, Debug)]
pub struct SyncKeyCommitDto {
    pub token: String,
    pub installation_id: String,
    pub revision: i64,
}

/// Secret-bearing result for the explicit invitation copy UI, not a settings DTO.
#[derive(Clone)]
pub struct SyncInvitationOfferDto {
    pub invitation_id: String,
    pub code: String,
    pub expires_at_ms: i64,
}

#[derive(Clone, Debug)]
pub struct SyncInvitationPreviewDto {
    pub invitation_id: String,
    pub token: String,
    pub device_name: String,
    pub device_platform: String,
    pub fingerprint: String,
    pub expires_at_ms: i64,
}

pub fn create_owner_sync_invitation(
    relay_url: String,
    acknowledge_provider_key_sharing: bool,
) -> Result<SyncInvitationOfferDto> {
    let offer = runtime::invitations::issue(
        &AppConfig::load()?,
        &relay_url,
        acknowledge_provider_key_sharing,
    )?;
    Ok(SyncInvitationOfferDto {
        invitation_id: offer.invitation_id,
        code: offer.code,
        expires_at_ms: offer.expires_at_ms,
    })
}

pub fn inspect_owner_sync_invitation(
    invitation_id: String,
) -> Result<Option<SyncInvitationPreviewDto>> {
    Ok(
        runtime::invitations::poll(&AppConfig::load()?, &invitation_id)?.map(|preview| {
            SyncInvitationPreviewDto {
                invitation_id: preview.invitation_id,
                token: preview.token,
                fingerprint: preview.device.fingerprint(),
                device_name: preview.device.name,
                device_platform: preview.device.platform,
                expires_at_ms: preview.expires_at_ms,
            }
        }),
    )
}

/// Bind approval to the original displayed request. Saving the returned revision
/// in native storage and confirm_sync_key must precede send_owner_sync_approval.
pub fn approve_owner_sync_invitation(
    invitation_id: String,
    preview_token: String,
    acknowledge_provider_key_sharing: bool,
) -> Result<SyncKeyCommitDto> {
    let approved = runtime::invitations::approve(
        &AppConfig::load()?,
        &invitation_id,
        &preview_token,
        acknowledge_provider_key_sharing,
    )?;
    Ok(SyncKeyCommitDto {
        token: approved.key.token,
        installation_id: approved.key.installation_id,
        revision: approved.key.revision,
    })
}

pub fn cancel_owner_sync_invitation(invitation_id: String) -> Result<()> {
    runtime::invitations::cancel(&AppConfig::load()?, &invitation_id)
}

/// Relay acceptance is not an installed member or completed synchronization.
pub fn send_owner_sync_approval(invitation_id: String) -> Result<()> {
    runtime::invitations::send_approval(&AppConfig::load()?, &invitation_id)
}

pub fn get_local_sync_state() -> Result<LocalSyncStateDto> {
    let config = AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let vault: Option<(String, i64)> = store
        .connection
        .query_row(
            "SELECT installation_id,revision FROM sync_local_vault WHERE singleton=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let identity = store.sync_identity()?;
    let authority = store.has_sync_authority()?;
    let roster = if authority {
        store.current_signed_roster()?
    } else {
        None
    };
    let active = authority && store.local_sync_membership_active()?;
    let state = if !authority {
        if vault.is_some() || identity.is_some() {
            "requires_repair"
        } else {
            "not_configured"
        }
    } else if !active {
        "membership_inactive"
    } else if runtime::unlocked(&config, vault.as_ref().map(|v| v.1).unwrap_or(0)) {
        "local_ready"
    } else {
        "locked"
    };
    let device_name = identity.as_ref().and_then(|id| {
        roster.as_ref().and_then(|r| {
            r.roster
                .members
                .iter()
                .find(|m| m.device.device_id == id.device_id)
                .map(|m| m.device.name.clone())
        })
    });
    let is_owner = identity.as_ref().is_some_and(|id| {
        roster
            .as_ref()
            .is_some_and(|r| r.roster.owner.device_id == id.device_id)
    });
    let is_processor = identity
        .as_ref()
        .is_some_and(|id| id.processor_device_id == id.device_id);
    let active_peers = if let Some(id) = identity.as_ref() {
        store.connection.query_row(
            "SELECT COUNT(DISTINCT device_id) FROM sync_members WHERE active=1 AND device_id!=?1",
            [&id.device_id],
            |r| r.get(0),
        )?
    } else {
        0
    };
    let pending_operations = store.connection.query_row(
        "SELECT COUNT(*) FROM sync_outbox WHERE acknowledged=0",
        [],
        |r| r.get(0),
    )?;
    Ok(LocalSyncStateDto {
        state: state.into(),
        installation_id: vault.as_ref().map(|v| v.0.clone()),
        vault_revision: vault.map(|v| v.1).unwrap_or(0),
        dataset_id: identity.as_ref().map(|i| i.dataset_id.clone()),
        device_id: identity.map(|i| i.device_id),
        device_name,
        is_owner,
        is_processor,
        active_peers,
        pending_operations,
    })
}

/// UI creation requires an explicit disclosure that pairing shares Provider keys.
/// Unlocking an existing installation does not repeat or change that authorization.
pub fn prepare_sync_key_request(
    name: Option<String>,
    platform: Option<String>,
    acknowledge_provider_key_sharing: bool,
) -> Result<SyncKeyRequestDto> {
    let config = AppConfig::load()?;
    let state = get_local_sync_state()?;
    if state.state == "not_configured" {
        ensure!(
            acknowledge_provider_key_sharing,
            "sync_provider_key_consent_required"
        );
    }
    let request = runtime::prepare_key(&config, name.as_deref(), platform.as_deref())?;
    Ok(SyncKeyRequestDto {
        token: request.token,
        installation_id: request.installation_id,
        revision: request.revision,
        creating: request.creating,
    })
}

/// Secret-bearing input, never returned in a settings DTO or diagnostic string.
pub fn install_sync_key(
    token: String,
    key: Vec<u8>,
    protected_revision: i64,
) -> Result<SyncKeyCommitDto> {
    let commit = runtime::install_key(&AppConfig::load()?, &token, key, protected_revision)?;
    Ok(SyncKeyCommitDto {
        token: commit.token,
        installation_id: commit.installation_id,
        revision: commit.revision,
    })
}

pub fn confirm_sync_key(token: String, protected_revision: i64) -> Result<LocalSyncStateDto> {
    runtime::confirm_key(&AppConfig::load()?, &token, protected_revision)?;
    get_local_sync_state()
}

pub fn release_sync_key_request(token: String) -> Result<bool> {
    Ok(runtime::release_request(&AppConfig::load()?, &token))
}

pub fn lock_local_sync() -> Result<LocalSyncStateDto> {
    let config = AppConfig::load().context("sync_workspace_unavailable")?;
    runtime::lock(&config);
    get_local_sync_state()
}

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
    pub acknowledgement_failed: bool,
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
            acknowledgement_failed: report.acknowledgement_failed,
        }),
        None => Ok(SyncTickResultDto {
            ran: false,
            staged_operations: 0,
            uploaded: 0,
            pulled_accepted: 0,
            pulled_waiting: 0,
            pulled_rejected: 0,
            acknowledgement_failed: false,
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

/// Read-only catalog for the selection UI. Secret-bearing material is never
/// part of a dataset entry; repair of a lost identity goes through the
/// ordinary P2 restore/join flows against the chosen dataset.
pub fn list_sync_datasets() -> Result<(Vec<SyncDatasetEntryDto>, i64)> {
    let config = AppConfig::load()?;
    let catalog = crate::sync::datasets::DatasetCatalog::open(&config.data_dir)?;
    let selected_id = catalog.selected_id()?;
    let revision = catalog.revision()?;
    let entries = catalog
        .list()?
        .into_iter()
        .map(|entry| SyncDatasetEntryDto {
            selected: selected_id.as_deref() == Some(entry.id.as_str()),
            entry_id: entry.id,
            dataset_id: entry.dataset_id,
            installation_id: entry.installation_id,
            name: entry.name,
            ready: entry.ready,
        })
        .collect();
    Ok((entries, revision))
}

/// Commits the user's selection of an existing catalog entry. The selection
/// must be the exact preview the UI displayed: a concurrent repair, staging
/// change or vault re-confirmation invalidates it instead of being selected
/// silently. Takes effect on the next process start; the pinned in-process
/// database never moves and no open SQLite/WAL pair is renamed.
pub fn select_sync_dataset(entry_id: String) -> Result<()> {
    let config = AppConfig::load()?;
    let catalog = crate::sync::datasets::DatasetCatalog::open(&config.data_dir)?;
    let selection = catalog.prepare_selection(&entry_id)?;
    // The committed selection needs the native-confirmed master of the exact
    // vault revision the user saw; without a live confirmed session the user
    // must unlock the runtime again instead of silently using stale material.
    let master = runtime::session_master(&config, selection.vault_revision)
        .context("sync_dataset_selection_requires_unlock")?;
    catalog.select_import(&selection, &master, selection.vault_revision)
}

/// Returns to the legacy default database. Same revision contract as
/// `select_sync_dataset`: the UI must commit the revision it displayed.
pub fn select_default_sync_dataset(expected_catalog_revision: i64) -> Result<()> {
    let config = AppConfig::load()?;
    let catalog = crate::sync::datasets::DatasetCatalog::open(&config.data_dir)?;
    catalog.select_default(expected_catalog_revision)
}
