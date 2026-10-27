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
    /// relay 信箱配额打满（发送侧收到 507）。UI 据此说清"不是卡在传输，
    /// 是在等对端腾空间"，而不是让用户以为链路坏了。
    pub storage_blocked: bool,
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
        storage_blocked: false,
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
                // joiner 的回执不会被任何人 ack：它是这条链路的终点，没有人会再回执
                // 它，所以 outbox 行投递成功后永远停在 pending。因此「通知是否完成」
                // 只能看 relay 有没有分配 relay_id（enrollment_wire_state 第二列），
                // 用 state=='acknowledged' 会永远判成未完成。
                result.notification_pending = {
                    let state = store.enrollment_wire_state(&record.peer.device_id, &receipt)?;
                    !state.is_some_and(|(_, delivered)| delivered)
                };
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
                // Owner 端进度：已**写进 relay 数据面**的片数（`blob_index`，逐片
                // durable）。这不再是「对端回执了几封信」—— 分片不走信箱，接收方
                // 的消费进度只有它自己知道；UI 对 owner 的措辞是「正在发送初始
                // 快照」，报「已上传 N 片」是诚实的，报「已完成 N 片」不是。
                result.received_chunks = store.connection.query_row(
                    "SELECT blob_index FROM sync_snapshot_exports WHERE id=?1",
                    [&snapshot.id],
                    |r| r.get::<_, i64>(0),
                )?;
                // 全部上传完但还没被确认：进度就停在总数上（不再是 0 或跳变）。
                if result.received_chunks > result.total_chunks {
                    result.received_chunks = result.total_chunks;
                }
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
    // 配额阻塞是发送侧在 507 时落的标记（见 `block_sync_wire`）：
    // 它只描述"现在发不出去"，与阶段、与是否有确认令牌都无关，所以单独成字段，
    // 让面板能加一行说明而不必自己解释卡住的阶段。
    result.storage_blocked = store.connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sync_wire_outbox WHERE recipient_device_id=?1 AND state='pending' AND blocked_reason='relay_quota')",
        rusqlite::params![record.peer.device_id],
        |r| r.get(0),
    )?;
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
        // 两个角色的「已收尾」判据不同：owner 的 completion 信令会被对端回执，
        // 所以能等 acknowledged；joiner 的回执行是链路终点、永远不会被 ack，
        // 只能以 relay 分配了 relay_id（已送达）作为完成。
        let settled = store
            .enrollment_wire_state(&record.peer.device_id, &wire)?
            .is_some_and(|(state, delivered)| {
                if role == EnrollmentRole::Owner {
                    state == "acknowledged"
                } else {
                    delivered
                }
            });
        if !ids.contains(&record.invitation_id) && !settled {
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
