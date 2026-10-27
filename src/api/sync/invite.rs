//! 属主侧设备邀请：创建/审阅/批准/撤销与本地同步密钥安装、锁定。

use super::*;

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
    /// 当前生效的 relay 地址（本机已解锁并持有 peer route 时才有；锁定/未配置为 None）。
    pub relay_url: Option<String>,
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

/// 加工移交状态查询（独立 DTO，不影响 LocalSyncStateDto）。
#[derive(Clone, Debug)]
pub struct ProcessorHandoffStateDto {
    pub state: String, // idle | relinquished | pending_takeover | active
    pub from_device_id: Option<String>,
    pub to_device_id: Option<String>,
    pub final_sequence: Option<i64>,
    pub can_confirm: bool,
}

/// 加工移交证明（旧端签名，供 owner 签发新名册）。
#[derive(Clone, Debug)]
pub struct ProcessingYieldProofDto {
    pub dataset_id: String,
    pub from_device_id: String,
    pub to_device_id: String,
    pub roster_revision: i64,
    pub final_sequence: i64,
    pub signature: Vec<u8>,
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
    local_sync_state_for(&config, &store)
}

/// 状态判定的唯一实现。`get_local_sync_state` 与「清除本机同步身份」必须用同一份，
/// 否则出口的准入条件会和 UI 显示的状态漂移（UI 显示 `requires_repair` 却拒绝执行）。
pub(crate) fn local_sync_state_for(
    config: &AppConfig,
    store: &Store,
) -> Result<LocalSyncStateDto> {
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
    } else if runtime::unlocked(config, vault.as_ref().map(|v| v.1).unwrap_or(0)) {
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
    // relay 读面（设置页展示用）：本机已解锁才有 master 可解密 vault。
    // 锁定/加入中/未配置都是 None，UI 显示「不可见」而不是猜一个值。
    let relay_url = match (vault.as_ref(), identity.as_ref()) {
        (Some((installation, revision)), Some(identity)) => {
            runtime::session_master(config, *revision).and_then(|master| {
                store
                    .load_sync_vault(installation, &master, *revision)
                    .ok()
                    .and_then(|vault| vault.relay_url(&identity.dataset_id))
            })
        }
        _ => None,
    };
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
        relay_url,
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
