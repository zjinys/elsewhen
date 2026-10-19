//! 报名面：会话作用域、邀请视图解析与本地列表。

use super::*;

pub(crate) struct EnrollmentView {
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

pub(super) fn scope(config: &AppConfig, session: &Session) -> Result<(Store, EnrollmentRole)> {
    let master = session.master.context("sync_enrollment_locked")?;
    if session.joining.is_some() {
        let (store, _) =
            joining::enrollment_scope(config, session)?.context("sync_enrollment_not_started")?;
        Ok((store.with_provider_master(master), EnrollmentRole::Joiner))
    } else {
        ensure!(
            session.database == config.database_path,
            "sync_enrollment_scope_changed"
        );
        Ok((
            Store::open(&session.database)?.with_provider_master(master),
            EnrollmentRole::Owner,
        ))
    }
}

pub(super) fn view(
    config: &AppConfig,
    session: &Session,
    invitation: &str,
) -> Result<EnrollmentView> {
    let (store, role) = scope(config, session)?;
    let vault = store.load_sync_vault(
        &session.installation,
        session.master.as_ref().unwrap(),
        session.revision,
    )?;
    ensure!(
        vault.revision == session.revision,
        "sync_runtime_vault_changed"
    );
    let mut result = EnrollmentView {
        role: role.name().into(),
        operation_id: session.operation.clone(),
        invitation_id: invitation.into(),
        installation_id: session.installation.clone(),
        revision: session.revision,
        confirmation_token: session
            .enrollment_confirmation
            .as_ref()
            .filter(|(id, kind)| id == invitation && *kind == role && !session.confirmed)
            .map(|_| session.token.clone()),
        phase: "awaiting_proof".into(),
        snapshot_id: None,
        received_chunks: 0,
        total_chunks: 0,
        local_applied: false,
        remote_applied: false,
        notification_pending: false,
    };
    let record = match vault.enrollment(invitation) {
        Ok(record) => record,
        Err(_) => {
            let pairing = vault.pairing(invitation)?;
            result.phase = if pairing.bootstrap.is_none() {
                "requires_repair"
            } else if pairing.status(now()).state == "awaiting_installation"
                && role == EnrollmentRole::Owner
            {
                "awaiting_proof"
            } else if pairing.status(now()).state == "expired" {
                "expired"
            } else {
                "requires_repair"
            }
            .into();
            return Ok(result);
        }
    };
    if let Err(error) = store.checked_enrollment(
        &session.installation,
        session.master.as_ref().unwrap(),
        session.revision,
        invitation,
    ) {
        result.phase = if error.to_string().contains("revoked") {
            "revoked"
        } else {
            "requires_repair"
        }
        .into();
        return Ok(result);
    }
    ensure!(record.role == role, "sync_enrollment_role_mismatch");
    if let Some(snapshot) = &record.snapshot {
        result.snapshot_id = Some(snapshot.id.clone());
        result.total_chunks = snapshot.chunks;
        if role == EnrollmentRole::Joiner {
            result.local_applied =
                store.enrollment_import_state(&snapshot.id)?.as_deref() == Some("applied");
            result.received_chunks = if result.local_applied {
                snapshot.chunks
            } else {
                store.connection.query_row(
                    "SELECT COUNT(*) FROM sync_snapshot_chunks WHERE snapshot_id=?1",
                    [&snapshot.id],
                    |r| r.get(0),
                )?
            };
            result.remote_applied = record.owner_confirmed;
            if record.owner_confirmed {
                let receipt = format!(
                    "envelope-receipt-{}",
                    crate::storage::sync::hash(completion_id(invitation, &snapshot.id).as_bytes())
                );
                result.notification_pending = !store
                    .enrollment_wire_state(&record.peer.device_id, &receipt)?
                    .is_some_and(|(s, _)| s == "acknowledged");
            }
        } else {
            result.remote_applied =
                store.enrollment_export_state(&snapshot.id)?.as_deref() == Some("acknowledged");
            if let Ok((send, _, _)) = vault.peer_keys(&record.dataset_id, &record.peer.device_id) {
                if let Ok(offer) = transfer::owner_offer(&store, record, &send, vault.identity()) {
                    result.total_chunks = offer.chunks;
                }
            }
            if result.remote_applied {
                result.received_chunks = result.total_chunks;
            } else if result.total_chunks > 0 {
                // Owner 端发送进度：对端已回执的快照分片信封数量（消息 id 形如
                // snapshot:{snapshot_id}:{index}，见 storage/sync/delivery.rs）。
                result.received_chunks = store.connection.query_row(
                    "SELECT COUNT(*) FROM sync_wire_outbox WHERE recipient_device_id=?1 AND message_id LIKE 'snapshot:'||?2||':%' AND state='acknowledged'",
                    rusqlite::params![record.peer.device_id, snapshot.id],
                    |r| r.get(0),
                )?;
            }
            if record.owner_confirmed {
                result.notification_pending = !store
                    .enrollment_wire_state(
                        &record.peer.device_id,
                        &completion_id(invitation, &snapshot.id),
                    )?
                    .is_some_and(|(state, _)| state == "acknowledged");
            }
        }
    }
    result.phase = if record.owner_confirmed {
        "complete"
    } else if !record.receive_ready || !record.temporary_revoked {
        "preparing_channel"
    } else if role == EnrollmentRole::Joiner {
        if record.roster_message_id.is_none() {
            "awaiting_roster"
        } else if record.snapshot.is_none() {
            "awaiting_snapshot_selection"
        } else if result.local_applied {
            "awaiting_receipt"
        } else {
            "receiving_snapshot"
        }
    } else if let Some(snapshot) = &record.snapshot {
        match store.enrollment_export_state(&snapshot.id)?.as_deref() {
            Some("ready") => {
                if let Some(roster) = transfer::snapshot_roster_id(&store, record)? {
                    if !transfer::wire_acknowledged(&store, &record.peer.device_id, &roster)? {
                        "awaiting_roster"
                    } else if !transfer::wire_acknowledged(
                        &store,
                        &record.peer.device_id,
                        &offer_id(invitation, &snapshot.id),
                    )? {
                        "awaiting_snapshot_selection"
                    } else {
                        "sending_snapshot"
                    }
                } else {
                    "requires_repair"
                }
            }
            Some("acknowledged") => "awaiting_receipt",
            _ => "preparing_snapshot",
        }
    } else {
        "preparing_snapshot"
    }
    .into();
    if now() >= record.receive.expires_at_ms
        && (!record.owner_confirmed || result.notification_pending)
    {
        result.phase = "expired".into();
    }
    Ok(result)
}

pub(crate) fn list(config: &AppConfig) -> Result<Vec<EnrollmentView>> {
    let all = sessions().lock().unwrap_or_else(|p| p.into_inner());
    let Some(session) = all.get(&config.data_dir) else {
        return Ok(Vec::new());
    };
    if session.master.is_none() || (!session.confirmed && session.enrollment_confirmation.is_none())
    {
        return Ok(Vec::new());
    }
    if session.joining.is_some() && joining::enrollment_scope(config, session)?.is_none() {
        return Ok(Vec::new());
    }
    let (store, role) = scope(config, session)?;
    let vault = store.load_sync_vault(
        &session.installation,
        session.master.as_ref().unwrap(),
        session.revision,
    )?;
    if role == EnrollmentRole::Owner && store.sync_roster_anchor()?.owner != *vault.device() {
        return Ok(Vec::new());
    }
    let mut ids = Vec::new();
    // Proofs have the short invitation deadline; completed recent transfers must
    // not hide an older unfinished enrollment from the recovery inventory.
    for pending in vault.pairing_statuses(now()) {
        if matches!(
            pending.state,
            "awaiting_installation" | "awaiting_membership"
        ) {
            ids.push(pending.invitation_id);
        }
    }
    for record in vault
        .enrollments()
        .iter()
        .filter(|e| e.role == role && !e.owner_confirmed)
    {
        if !ids.contains(&record.invitation_id) {
            ids.push(record.invitation_id.clone());
        }
    }
    // Snapshot application is not completion-notice delivery. Give live pending
    // notices priority over already settled history when restoring the inventory.
    for record in vault
        .enrollments()
        .iter()
        .filter(|e| e.role == role && e.owner_confirmed && e.receive.expires_at_ms > now())
    {
        if ids.len() >= 16 {
            break;
        }
        let Some(snapshot) = &record.snapshot else {
            continue;
        };
        let subject = completion_id(&record.invitation_id, &snapshot.id);
        let wire = if role == EnrollmentRole::Owner {
            subject
        } else {
            format!(
                "envelope-receipt-{}",
                crate::storage::sync::hash(subject.as_bytes())
            )
        };
        if !ids.contains(&record.invitation_id)
            && !store
                .enrollment_wire_state(&record.peer.device_id, &wire)?
                .is_some_and(|(state, _)| state == "acknowledged")
        {
            ids.push(record.invitation_id.clone());
        }
    }
    for record in vault.enrollments().iter().rev().filter(|e| e.role == role) {
        if ids.len() >= 16 {
            break;
        }
        if !ids.contains(&record.invitation_id) {
            ids.push(record.invitation_id.clone());
        }
    }
    ensure!(ids.len() <= 16, "sync_enrollment_inventory_limit");
    ids.into_iter()
        .map(|id| view(config, session, &id))
        .collect()
}
