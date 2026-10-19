//! 数据集选择与提供方冲突：目录/默认集、provider 冲突列表与裁决、设备名册。

use super::*;

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

// ---------------------------------------------------------------------------
// P4b: Provider sync conflict review. Keys are masked, choices are explicit
// (keep local / accept remote whole-bundle), and both bindings must match the
// revisions the user actually reviewed.

#[derive(Clone, Debug)]
pub struct ProviderConflictProfileDto {
    pub id: String,
    pub name: String,
    pub provider_type: String,
    pub base_url: String,
    pub model: String,
    pub api_key_masked: String,
    pub api_key_present: bool,
    pub enabled: bool,
    pub is_active: bool,
    pub temperature: f64,
    pub max_tokens: Option<i64>,
    pub context_window: Option<i64>,
    pub reachability_hints: Vec<ReachabilityHint>,
}

#[derive(Clone, Debug)]
pub struct ProviderSyncConflictDto {
    pub revision_id: String,
    pub base_revision: Option<String>,
    pub sender_device_id: String,
    pub sender_device_name: Option<String>,
    pub local_revision: String,
    pub remote_profiles: Vec<ProviderConflictProfileDto>,
    pub remote_active_id: Option<String>,
    pub remote_failover_order: Vec<String>,
    pub local_profiles: Vec<ProviderConflictProfileDto>,
    pub local_active_id: Option<String>,
}

fn conflict_profile_dto(
    p: crate::storage::sync::provider_conflicts::ProviderConflictProfile,
) -> ProviderConflictProfileDto {
    ProviderConflictProfileDto {
        reachability_hints: assess_base_url(&p.base_url),
        id: p.id,
        name: p.name,
        provider_type: p.provider_type,
        base_url: p.base_url,
        model: p.model,
        api_key_masked: p.api_key_masked,
        api_key_present: p.api_key_present,
        enabled: p.enabled,
        is_active: p.is_active,
        temperature: p.temperature,
        max_tokens: p.max_tokens,
        context_window: p.context_window,
    }
}

/// 列出待审阅的 Provider 同步冲突；密钥已掩码，锁定状态失败关闭。
/// UI 必须保存 local_revision / revision_id，裁决时原样回传。
pub fn list_sync_provider_conflicts() -> Result<Vec<ProviderSyncConflictDto>> {
    let config = AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let reviews = store.list_sync_provider_conflicts()?;
    let roster = store.current_signed_roster()?;
    Ok(reviews
        .into_iter()
        .map(|r| {
            let sender_device_name = roster.as_ref().and_then(|roster| {
                roster
                    .roster
                    .members
                    .iter()
                    .find(|m| m.device.device_id == r.sender_device_id)
                    .map(|m| m.device.name.clone())
            });
            ProviderSyncConflictDto {
                revision_id: r.revision_id,
                base_revision: r.base_revision,
                sender_device_id: r.sender_device_id.clone(),
                sender_device_name,
                local_revision: r.local_revision,
                remote_profiles: r
                    .remote_profiles
                    .into_iter()
                    .map(conflict_profile_dto)
                    .collect(),
                remote_active_id: r.remote_active_id,
                remote_failover_order: r.remote_failover_order,
                local_profiles: r
                    .local_profiles
                    .into_iter()
                    .map(conflict_profile_dto)
                    .collect(),
                local_active_id: r.local_active_id,
            }
        })
        .collect())
}

/// 显式裁决一条 Provider 同步冲突。
/// `expected_local_revision` 与 `expected_remote_revision` 必须原样回传审阅时
/// 展示的值；任一不匹配都失败关闭，避免竞态覆盖审阅后的新变更。
/// `accept_remote` 为 false 时保留本地并把当前本地状态重新广播给对端。
pub fn resolve_sync_provider_conflict(
    revision_id: String,
    expected_local_revision: String,
    expected_remote_revision: String,
    accept_remote: bool,
) -> Result<()> {
    let config = AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.resolve_sync_provider_conflict(
        &revision_id,
        &expected_local_revision,
        &expected_remote_revision,
        if accept_remote {
            ProviderConflictResolution::AcceptRemote
        } else {
            ProviderConflictResolution::KeepLocal
        },
    )
}

/// 名册设备列表（只读公开信息：设备名/平台/角色，不含密钥）。
#[derive(Clone, Debug)]
pub struct SyncRosterMemberDto {
    pub device_id: String,
    pub name: String,
    pub platform: String,
    pub active: bool,
    pub is_owner: bool,
    pub is_processor: bool,
    pub is_local: bool,
}

#[derive(Clone, Debug)]
pub struct SyncRosterDto {
    pub revision: i64,
    pub dataset_id: String,
    pub owner_device_id: String,
    pub processor_device_id: String,
    pub local_device_id: Option<String>,
    pub members: Vec<SyncRosterMemberDto>,
}

/// 查询当前名册设备列表。非活跃（已撤销）成员也展示，供 owner 审计。
pub fn get_sync_roster() -> Result<SyncRosterDto> {
    let config = AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let identity = store.sync_identity()?;
    let signed = store
        .current_signed_roster()?
        .context("sync_authority_not_ready")?;
    let roster = &signed.roster;
    Ok(SyncRosterDto {
        revision: roster.revision as i64,
        dataset_id: roster.dataset_id.clone(),
        owner_device_id: roster.owner.device_id.clone(),
        processor_device_id: roster.processor_device_id.clone(),
        local_device_id: identity.as_ref().map(|i| i.device_id.clone()),
        members: roster
            .members
            .iter()
            .map(|m| SyncRosterMemberDto {
                is_owner: m.device.device_id == roster.owner.device_id,
                is_processor: m.device.device_id == roster.processor_device_id,
                is_local: identity
                    .as_ref()
                    .is_some_and(|i| i.device_id == m.device.device_id),
                active: m.active,
                device_id: m.device.device_id.clone(),
                name: m.device.name.clone(),
                platform: m.device.platform.clone(),
            })
            .collect(),
    })
}

/// 名册 owner 撤销成员设备（需本机已解锁）。修订号必须与 UI 审阅时所见一致；
/// 撤销 owner/加工设备会被 sync_membership_handoff_required 硬拦。
pub fn revoke_sync_member(device_id: String, expected_revision: i64) -> Result<()> {
    ensure!(expected_revision > 0, "sync_membership_revision_invalid");
    // 期望修订号恒为正，i64 上限无需再比（clippy::absurd_extreme_comparisons）。
    let config = AppConfig::load()?;
    runtime::handoff::revoke_member(
        &config,
        &device_id,
        u64::try_from(expected_revision)
            .map_err(|_| anyhow::anyhow!("sync_membership_revision_invalid"))?,
    )
}

/// owner 改设备显示名（含本机）：vault + 签名 roster + 锚原子更新，
/// 广播由 membership tick 接管。仅 owner 本机可调（签名权所在）。
pub fn rename_sync_device(
    device_id: String,
    new_name: String,
    expected_vault_revision: i64,
) -> Result<i64> {
    let config = AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let vault: (String, i64) = store
        .connection
        .query_row(
            "SELECT installation_id,revision FROM sync_local_vault WHERE singleton=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .context("sync_not_configured")?;
    ensure!(
        vault.1 == expected_vault_revision,
        "sync_vault_revision_changed"
    );
    let master =
        runtime::session_master(&config, vault.1).context("sync_rename_requires_unlock")?;
    ensure!(
        !new_name.trim().is_empty()
            && new_name.len() <= 256
            && !new_name.chars().any(char::is_control),
        "pairing_device_name_invalid"
    );
    let revision =
        store.rename_sync_vault_device(&vault.0, &master, vault.1, &device_id, &new_name)?;
    i64::try_from(revision).map_err(|_| anyhow::anyhow!("sync_membership_revision_invalid"))
}

/// 查询本机加工移交状态（独立 DTO，不影响 LocalSyncStateDto）。
pub fn get_processor_handoff_state() -> Result<ProcessorHandoffStateDto> {
    let config = AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let state = store.get_processor_handoff_state()?;
    Ok(ProcessorHandoffStateDto {
        state: state.state,
        from_device_id: state.from_device_id,
        to_device_id: state.to_device_id,
        final_sequence: state.final_sequence,
        can_confirm: state.can_confirm,
    })
}

/// 旧加工端让出加工权：生成移交证明（需本机已解锁会话）。
/// 后继修订号始终取当前名册的下一修订，调用方不可指定。
pub fn yield_sync_processor(to_device_id: String) -> Result<ProcessingYieldProofDto> {
    let config = AppConfig::load()?;
    let proof = runtime::handoff::yield_processor(&config, &to_device_id)?;
    Ok(ProcessingYieldProofDto {
        dataset_id: proof.dataset_id,
        from_device_id: proof.from_device_id,
        to_device_id: proof.to_device_id,
        roster_revision: proof.roster_revision as i64,
        final_sequence: proof.final_sequence as i64,
        signature: proof.signature,
    })
}

/// 名册 owner 审批旧加工端的移交证明并签发后继名册（需 owner 本机已解锁）。
pub fn approve_processor_handoff(proof: ProcessingYieldProofDto) -> Result<()> {
    let config = AppConfig::load()?;
    let proof = runtime::handoff::proof_from_parts(
        proof.dataset_id,
        proof.from_device_id,
        proof.to_device_id,
        proof.roster_revision,
        proof.final_sequence,
        proof.signature,
    )?;
    runtime::handoff::approve_handoff(&config, &proof)
}

/// 新加工端确认接管（frontier 追平后调用）。
pub fn confirm_processor_takeover() -> Result<()> {
    let config = AppConfig::load()?;
    runtime::handoff::confirm_takeover(&config)
}

// ---------------------------------------------------------------------------
// S05: Wiki 页同步冲突审阅。三种候选语义（删除后离线编辑 / 两端各自改名 /
// 新 slug 撞名）并列展示双端状态，裁决显式二选一；绑定失败关闭与 Provider
// 冲突审阅同款。

#[derive(Clone, Debug)]
pub struct WikiPageConflictSideDto {
    pub slug: String,
    pub title: String,
    pub content_md_len: i64,
    pub updated_at: String,
}

#[derive(Clone, Debug)]
pub struct WikiPageConflictDto {
    pub op_id: String,
    pub object_id: String,
    pub reason: String,
    pub sender_device_id: String,
    pub sender_device_name: Option<String>,
    pub incoming_slug: String,
    pub incoming_title: String,
    pub incoming_deleted: bool,
    pub incoming_renamed_from: Option<String>,
    pub incoming_summary: String,
    pub incoming_content_md_len: i64,
    pub incoming_updated_at: String,
    pub local: Option<WikiPageConflictSideDto>,
    pub local_head_deleted: bool,
}
