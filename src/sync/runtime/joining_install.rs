//! 加入流程的安装段：verify_installed、install_key 与 confirm_key(+inner)。

use super::*;

fn verify_installed(
    store: &Store,
    vault: &LocalVault,
    join: &mut JoinState,
    master: &[u8; 32],
) -> Result<()> {
    let identity = store
        .sync_identity()?
        .context("sync_join_requires_new_invitation")?;
    ensure!(
        identity.dataset_id == join.dataset
            && identity.device_id == vault.device().device_id
            && identity.stream_id == vault.device().stream_id,
        "sync_join_staging_changed"
    );
    let statuses =
        store.sync_pairing_statuses(&vault.installation_id, master, vault.revision, now())?;
    ensure!(
        statuses.len() == 1 && matches!(statuses[0].state, "awaiting_membership" | "joined"),
        "sync_join_not_recoverable"
    );
    let record = vault.pairing(&statuses[0].invitation_id)?;
    let anchor = store.sync_roster_anchor()?;
    ensure!(
        anchor.owner == record.peer && anchor.dataset_id == join.dataset,
        "sync_join_authority_changed"
    );
    ensure!(
        join.peer.as_ref().is_none_or(|p| *p == record.peer)
            && (join.invitation_id.is_empty() || join.invitation_id == record.invitation_id),
        "sync_join_authority_changed"
    );
    if statuses[0].state == "joined" {
        let (_, enrollment) = store.checked_enrollment(
            &vault.installation_id,
            master,
            vault.revision,
            &record.invitation_id,
        )?;
        ensure!(
            enrollment.role == crate::sync::enrollment::EnrollmentRole::Joiner
                && enrollment.roster_message_id.is_some()
                && store.local_sync_membership_active()?,
            "sync_join_handoff_context_missing"
        );
    } else {
        ensure!(
            !store.local_sync_membership_active()?,
            "sync_join_already_active"
        );
    }
    let route = vault.peer_route(&join.dataset, &record.peer.device_id)?;
    join.peer = Some(record.peer.clone());
    join.invitation_id = record.invitation_id.clone();
    join.relay_url = route.relay_url.clone();
    join.deadline = record.expires_at_ms;
    Ok(())
}

pub fn install_key(
    config: &AppConfig,
    token: &str,
    mut bytes: Vec<u8>,
    floor: i64,
) -> Result<JoinView> {
    let key: Result<[u8; 32]> = bytes
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("sync_master_key_invalid"));
    bytes.fill(0);
    let key = key?;
    let mut all = sessions().lock().unwrap_or_else(|p| p.into_inner());
    let session = session(&mut all, config)?;
    ensure!(
        session.token == token
            && session.expires_at > now()
            && session.joining.as_ref().unwrap().phase == Phase::KeyRequired,
        "sync_join_key_request_stale"
    );
    let store = staging_store(config, session)?.with_provider_master(key);
    let vault = if session.creating {
        ensure!(floor == 1, "sync_new_installation_revision_not_reserved");
        store.create_sync_join_vault(
            &session.installation,
            &key,
            &session.name,
            &session.platform,
        )?
    } else {
        let vault = store.load_sync_vault(&session.installation, &key, floor)?;
        ensure!(
            vault.revision == session.revision,
            "sync_runtime_vault_changed"
        );
        verify_installed(&store, &vault, session.joining.as_mut().unwrap(), &key)?;
        vault
    };
    session.creating = false;
    session.master = Some(key);
    session.revision = vault.revision;
    session.confirmed = false;
    session.token = uuid::Uuid::new_v4().to_string();
    session.expires_at = now() + 10 * 60 * 1000;
    let join = session.joining.as_mut().unwrap();
    join.fingerprint = Some(vault.device().fingerprint());
    join.phase = Phase::KeyConfirmation;
    Ok(view(session))
}

pub fn confirm_key(config: &AppConfig, token: &str, revision: i64) -> Result<JoinView> {
    confirm_key_inner(config, token, revision, false)
}

pub(in crate::sync::runtime) fn confirm_enrollment_key(
    config: &AppConfig,
    token: &str,
    revision: i64,
) -> Result<JoinView> {
    confirm_key_inner(config, token, revision, true)
}

fn confirm_key_inner(
    config: &AppConfig,
    token: &str,
    revision: i64,
    enrollment: bool,
) -> Result<JoinView> {
    let mut all = sessions().lock().unwrap_or_else(|p| p.into_inner());
    let session = session(&mut all, config)?;
    ensure!(
        session.token == token
            && session.expires_at > now()
            && session.revision == revision
            && session.master.is_some(),
        "sync_join_confirmation_stale"
    );
    if session.confirmed {
        return Ok(view(session));
    }
    let phase = session.joining.as_ref().unwrap().phase;
    ensure!(
        if enrollment {
            phase == Phase::EnrollmentConfirmation
        } else {
            matches!(
                phase,
                Phase::KeyConfirmation | Phase::InstallationConfirmation
            )
        },
        "sync_join_confirmation_stale"
    );
    let store = staging_store(config, session)?;
    let vault = store.load_sync_vault(
        &session.installation,
        session.master.as_ref().unwrap(),
        revision,
    )?;
    ensure!(vault.revision == revision, "sync_runtime_vault_changed");
    let join = session.joining.as_mut().unwrap();
    if join.restoring || phase == Phase::InstallationConfirmation || enrollment {
        verify_installed(&store, &vault, join, session.master.as_ref().unwrap())?;
        join.phase = if enrollment
            || vault
                .enrollment(&join.invitation_id)
                .is_ok_and(|e| e.receive_ready)
        {
            Phase::Enrollment
        } else {
            Phase::AwaitingMembership
        };
    } else {
        ensure!(
            store.sync_identity()?.is_none() && !store.has_sync_authority()?,
            "sync_join_staging_changed"
        );
        join.phase = Phase::Ready;
    }
    session.confirmed = true;
    // In particular, do not call owner confirm_key or recover any processing queue.
    Ok(view(session))
}

pub(in crate::sync::runtime) fn enrollment_scope(
    config: &AppConfig,
    session: &Session,
) -> Result<Option<(Store, String)>> {
    let join = session
        .joining
        .as_ref()
        .context("sync_join_wrong_session_kind")?;
    ensure!(
        join.scope == config.database_path,
        "sync_join_scope_changed"
    );
    if join.entry.is_none() || session.master.is_none() {
        return Ok(None);
    }
    let store = staging_store(config, session)?;
    if !store.has_sync_authority()? {
        return Ok(None);
    }
    Ok(Some((store, join.invitation_id.clone())))
}

/// Read-only session inspection for runtime-level guards (e.g. the repair
/// handoff in `session_master`); never exposes key material.
pub(in crate::sync::runtime) fn join_scope(session: &Session) -> Option<&std::path::Path> {
    session.joining.as_ref().map(|j| j.scope.as_path())
}

pub(in crate::sync::runtime) fn enrollment_available(session: &Session) -> bool {
    session.joining.as_ref().is_some_and(|join| {
        !join.network_busy
            && matches!(
                join.phase,
                Phase::AwaitingMembership
                    | Phase::ProofSent
                    | Phase::Enrollment
                    | Phase::EnrollmentConfirmation
                    // 终态只是**界面**的终点，不是交接握手的终点：收尾回执/迟到重投
                    // 仍要走 `enroll_tick`，所以这里必须继续放行。若把 `Joined`
                    // 关掉，就会在「看起来完成」之后截断握手 —— 正是 C7 ⑩ 那类
                    // 「对端永远等不到回执」的翻版。
                    | Phase::Joined
            )
    })
}

pub(in crate::sync::runtime) fn pause_for_enrollment(session: &mut Session) {
    if let Some(join) = session.joining.as_mut() {
        join.phase = Phase::EnrollmentConfirmation;
    }
}
