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
    let offer = runtime::invitations::issue(&AppConfig::load()?, &relay_url,
        acknowledge_provider_key_sharing)?;
    Ok(SyncInvitationOfferDto {
        invitation_id: offer.invitation_id, code: offer.code, expires_at_ms: offer.expires_at_ms,
    })
}

pub fn inspect_owner_sync_invitation(invitation_id: String) -> Result<Option<SyncInvitationPreviewDto>> {
    Ok(runtime::invitations::poll(&AppConfig::load()?, &invitation_id)?.map(|preview|
        SyncInvitationPreviewDto {
            invitation_id: preview.invitation_id, token: preview.token,
            fingerprint: preview.device.fingerprint(), device_name: preview.device.name,
            device_platform: preview.device.platform, expires_at_ms: preview.expires_at_ms,
        }))
}

/// Bind approval to the original displayed request. Saving the returned revision
/// in native storage and confirm_sync_key must precede send_owner_sync_approval.
pub fn approve_owner_sync_invitation(
    invitation_id: String,
    preview_token: String,
    acknowledge_provider_key_sharing: bool,
) -> Result<SyncKeyCommitDto> {
    let approved = runtime::invitations::approve(&AppConfig::load()?, &invitation_id,
        &preview_token, acknowledge_provider_key_sharing)?;
    Ok(SyncKeyCommitDto {
        token: approved.key.token, installation_id: approved.key.installation_id,
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
