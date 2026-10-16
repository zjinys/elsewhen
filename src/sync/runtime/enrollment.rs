//! Bounded enrollment ticks under the existing workspace session. This is not a
//! general synchronization worker and never selects a different application DB.
use super::*;
use crate::{
    storage::sync::PairingCommit,
    sync::{
        engine::{self, enrollment as transfer},
        enrollment::{EnrollmentRecord, EnrollmentRole, completion_id, offer_id},
        pairing::InvitationRoute,
        transport_relay::RelayClient,
        vault::LocalVault,
    },
};
use base64::{Engine, engine::general_purpose::STANDARD};

#[derive(Clone, Debug)]
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

fn scope(config: &AppConfig, session: &Session) -> Result<(Store, EnrollmentRole)> {
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

fn view(config: &AppConfig, session: &Session, invitation: &str) -> Result<EnrollmentView> {
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

struct EnrollmentLoan {
    _lease: Arc<WorkspaceLease>,
    config: AppConfig,
    database: PathBuf,
    operation: String,
    installation: String,
    revision: i64,
    master: [u8; 32],
    role: EnrollmentRole,
    invitation: String,
    record: Option<EnrollmentRecord>,
    bootstrap: InvitationRoute,
    proof_deadline: i64,
    owner_credential: [u8; 32],
}
impl Drop for EnrollmentLoan {
    fn drop(&mut self) {
        self.master.fill(0);
        self.owner_credential.fill(0);
        let mut all = sessions().lock().unwrap_or_else(|p| p.into_inner());
        if let Some(session) = all.get_mut(&self.config.data_dir) {
            if session.operation == self.operation {
                session.enrollment_busy = false;
            }
        }
    }
}
impl EnrollmentLoan {
    fn store(&self) -> Result<Store> {
        Ok(Store::open(&self.database)?.with_provider_master(self.master))
    }
    fn current<'a>(&self, all: &'a mut HashMap<PathBuf, Session>) -> Result<&'a mut Session> {
        let session = all
            .get_mut(&self.config.data_dir)
            .context("sync_enrollment_session_missing")?;
        ensure!(
            session.operation == self.operation
                && session.database == self.database
                && session.revision == self.revision
                && session.confirmed
                && session.joining.is_some() == (self.role == EnrollmentRole::Joiner),
            "sync_enrollment_request_stale"
        );
        Ok(session)
    }
    fn check(&self) -> Result<()> {
        let mut all = sessions().lock().unwrap_or_else(|p| p.into_inner());
        self.current(&mut all)?;
        if let Some(record) = &self.record {
            record.require_live(now())?;
            self.store()?.checked_enrollment(
                &self.installation,
                &self.master,
                self.revision,
                &self.invitation,
            )?;
        } else {
            ensure!(now() < self.proof_deadline, "pairing_confirmation_expired");
        }
        Ok(())
    }
    fn pause(session: &mut Session, invitation: &str, role: EnrollmentRole, commit: PairingCommit) {
        if session.revision != commit.vault_revision {
            session.revision = commit.vault_revision;
            session.confirmed = false;
            session.token = uuid::Uuid::new_v4().to_string();
            session.expires_at = now() + 10 * 60 * 1000;
            session.enrollment_confirmation = Some((invitation.into(), role));
            if role == EnrollmentRole::Joiner {
                joining::pause_for_enrollment(session);
            }
        }
    }
    fn mutate(&self, operation: impl FnOnce(&Store) -> Result<PairingCommit>) -> Result<()> {
        let mut all = sessions().lock().unwrap_or_else(|p| p.into_inner());
        let session = self.current(&mut all)?;
        ensure!(
            !busy()
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .contains(&self.config.data_dir),
            "sync_runtime_in_use"
        );
        let store = self.store()?;
        let tx = store.begin_write_transaction()?;
        store.require_background_quiescent_for_sync()?;
        let commit = operation(&store)?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Self::pause(session, &self.invitation, self.role, commit);
        Ok(())
    }
    fn result(&self) -> Result<EnrollmentView> {
        let all = sessions().lock().unwrap_or_else(|p| p.into_inner());
        let session = all
            .get(&self.config.data_dir)
            .context("sync_enrollment_session_missing")?;
        ensure!(
            session.operation == self.operation,
            "sync_enrollment_request_stale"
        );
        view(&self.config, session, &self.invitation)
    }
}

fn begin(
    config: &AppConfig,
    role: &str,
    operation: &str,
    invitation: &str,
) -> Result<EnrollmentLoan> {
    let mut all = sessions().lock().unwrap_or_else(|p| p.into_inner());
    let session = all
        .get_mut(&config.data_dir)
        .context("sync_enrollment_session_missing")?;
    ensure!(
        session.operation == operation && session.confirmed && session.master.is_some(),
        "sync_enrollment_locked"
    );
    ensure!(
        !session.enrollment_busy
            && (session.joining.is_none() || joining::enrollment_available(session)),
        "sync_enrollment_busy"
    );
    let (store, kind) = scope(config, session)?;
    ensure!(role == kind.name(), "sync_enrollment_role_mismatch");
    let vault = store.load_sync_vault(
        &session.installation,
        session.master.as_ref().unwrap(),
        session.revision,
    )?;
    ensure!(
        vault.revision == session.revision,
        "sync_runtime_vault_changed"
    );
    let pairing = vault.pairing(invitation)?;
    let bootstrap = pairing
        .bootstrap
        .clone()
        .context("sync_enrollment_bootstrap_missing_requires_repair")?;
    let record = if let Ok(record) = vault.enrollment(invitation) {
        let (_, checked) = store.checked_enrollment(
            &session.installation,
            session.master.as_ref().unwrap(),
            session.revision,
            invitation,
        )?;
        ensure!(record.role == kind, "sync_enrollment_role_mismatch");
        checked.require_live(now())?;
        Some(checked)
    } else {
        ensure!(
            kind == EnrollmentRole::Owner
                && store.sync_roster_anchor()?.owner == *vault.device()
                && pairing.status(now()).state == "awaiting_installation",
            "sync_enrollment_proof_not_pending"
        );
        None
    };
    let result = EnrollmentLoan {
        _lease: session.lease.clone(),
        config: AppConfig {
            data_dir: config.data_dir.clone(),
            database_path: config.database_path.clone(),
        },
        database: session.database.clone(),
        operation: operation.into(),
        installation: session.installation.clone(),
        revision: session.revision,
        master: *session.master.as_ref().unwrap(),
        role: kind,
        invitation: invitation.into(),
        record,
        bootstrap,
        proof_deadline: pairing.expires_at_ms,
        owner_credential: *vault.mailbox_owner(),
    };
    session.enrollment_busy = true;
    Ok(result)
}

fn advance_proof(loan: &EnrollmentLoan, client: &RelayClient) -> Result<()> {
    loan.check()?;
    let store = loan.store()?;
    let mailbox = &loan.bootstrap.issuer_mailbox;
    let cursor = store.relay_scan_cursor(mailbox)?;
    let page = client.fetch_page(mailbox, &loan.owner_credential, &cursor, 3)?;
    accept_proof_page(loan, page)
}

fn accept_proof_page(
    loan: &EnrollmentLoan,
    page: crate::sync::transport_relay::RelayPage,
) -> Result<()> {
    loan.check()?;
    let store = loan.store()?;
    let mailbox = &loan.bootstrap.issuer_mailbox;
    let vault = store.load_sync_vault(&loan.installation, &loan.master, loan.revision)?;
    ensure!(
        vault.revision == loan.revision,
        "sync_runtime_vault_changed"
    );
    let pending = vault.pairing(&loan.invitation)?;
    for mail in &page.envelopes {
        loan.check()?;
        if mail.sender_mb == loan.bootstrap.request_sender && mail.ciphertext.len() <= 24 * 1024 {
            if let Ok(bytes) = STANDARD.decode(&mail.ciphertext) {
                // Authentication failures belong to this mail, not to the scan.
                // Validate before the mutation so storage failures still propagate.
                if pending.confirmed_pair(&bytes, now()).is_ok() {
                    loan.mutate(|store| {
                        store.confirm_sync_pairing_installation(
                            &loan.installation,
                            &loan.master,
                            loan.revision,
                            &loan.invitation,
                            &bytes,
                            now(),
                        )
                    })?;
                    // A duplicate pass after native confirmation will ack the proof.
                    store.save_relay_scan_cursor(mailbox, &mail.id)?;
                    return Ok(());
                }
            }
        }
    }
    loan.check()?;
    store.save_relay_scan_cursor(
        mailbox,
        if page.has_more {
            &page.next_cursor
        } else {
            "0"
        },
    )?;
    Ok(())
}

fn prepare_snapshot(
    loan: &EnrollmentLoan,
    store: &Store,
    vault: &LocalVault,
    record: &EnrollmentRecord,
) -> Result<()> {
    loan.check()?;
    // SnapshotExporter requires every current peer's Provider version to have an
    // encrypted durable delivery. Staging is not uploading these business deltas.
    let roster = store
        .current_signed_roster()?
        .context("sync_authority_not_ready")?;
    let peers: Vec<_> = roster
        .roster
        .members
        .iter()
        .filter(|m| m.active && m.device.device_id != record.local_device_id)
        .map(|m| {
            vault
                .peer_keys(&record.dataset_id, &m.device.device_id)
                .map(|(key, _, epoch)| (m.device.device_id.clone(), key, epoch))
        })
        .collect::<Result<_>>()?;
    let encryption: Vec<_> = peers
        .iter()
        .map(|(id, key, epoch)| engine::EncryptionPeer {
            device_id: id,
            key_version: *epoch,
            key,
        })
        .collect();
    engine::stage_provider_updates(store, &encryption, vault.identity())?;
    let (key, _, _) = vault.peer_keys(&record.dataset_id, &record.peer.device_id)?;
    engine::snapshot::stage_enrollment_snapshot(
        store,
        &record.peer.device_id,
        &record.snapshot.as_ref().unwrap().id,
        record.key_version,
        &key,
        vault.identity(),
    )?;
    loan.check()?;
    Ok(())
}

fn pull_transfer(
    loan: &EnrollmentLoan,
    client: &RelayClient,
    record: &EnrollmentRecord,
    receive: &crate::sync::crypto::ContentKey,
) -> Result<()> {
    let store = loan.store()?;
    let mailbox = &record.route.local_mailbox;
    let cursor = store.relay_scan_cursor(mailbox)?;
    loan.check()?;
    let page = client.fetch_page(mailbox, &loan.owner_credential, &cursor, 3)?;
    for mail in &page.envelopes {
        loan.check()?;
        let accepted = if loan.role == EnrollmentRole::Owner
            && mail.sender_mb == record.bootstrap.request_sender
            && mail.ciphertext.len() <= 24 * 1024
        {
            STANDARD.decode(&mail.ciphertext).ok().is_some_and(|bytes| {
                store
                    .load_sync_vault(&loan.installation, &loan.master, loan.revision)
                    .and_then(|v| v.pairing(&loan.invitation)?.authorized(&bytes))
                    .unwrap_or(false)
            })
        } else if mail.sender_mb == record.route.peer_mailbox {
            let mut all = sessions().lock().unwrap_or_else(|p| p.into_inner());
            let session = loan.current(&mut all)?;
            match transfer::accept(
                &store,
                &loan.installation,
                &loan.master,
                loan.revision,
                record,
                &mail.ciphertext,
                receive,
            ) {
                Ok(transfer::ReceiveOutcome::Durable(commit)) => {
                    if let Some(commit) = commit {
                        let changed = commit.vault_revision != loan.revision;
                        EnrollmentLoan::pause(session, &loan.invitation, loan.role, commit);
                        if changed {
                            store.save_relay_scan_cursor(mailbox, &mail.id)?;
                            return Ok(());
                        }
                    }
                    true
                }
                Ok(transfer::ReceiveOutcome::Waiting) => false,
                Err(error) if error.is::<rusqlite::Error>() || error.is::<std::io::Error>() => {
                    return Err(error);
                }
                Err(_) => false,
            }
        } else {
            false
        };
        if accepted {
            loan.check()?;
            // Failed relay deletion does not undo durable application or the receipt.
            let _ = client.ack(
                mailbox,
                &loan.owner_credential,
                std::slice::from_ref(&mail.id),
            );
        }
    }
    loan.check()?;
    store.save_relay_scan_cursor(
        mailbox,
        if page.has_more {
            &page.next_cursor
        } else {
            "0"
        },
    )?;
    Ok(())
}

fn grant_failure(error: anyhow::Error, unconfirmed: &str) -> anyhow::Error {
    if error
        .downcast_ref::<crate::sync::transport_relay::RelayHttpError>()
        .is_some_and(|error| matches!(error.0.as_u16(), 401 | 403 | 409))
    {
        anyhow::anyhow!("sync_enrollment_authorization_changed_requires_repair")
    } else {
        anyhow::anyhow!("{unconfirmed}")
    }
}

pub(crate) fn advance(
    config: &AppConfig,
    role: &str,
    operation: &str,
    invitation: &str,
) -> Result<EnrollmentView> {
    let loan = begin(config, role, operation, invitation)?;
    let client = RelayClient::new(&loan.bootstrap.relay_url)?;
    if loan.record.is_none() {
        advance_proof(&loan, &client)?;
        return loan.result();
    }
    let record = loan.record.as_ref().unwrap();
    loan.check()?;
    if loan.role == EnrollmentRole::Joiner
        && record.roster_message_id.is_none()
        && now() < loan.proof_deadline
    {
        // P1 may have lost the send response. Retry its persisted proof even after
        // the receive grant changes phase; never renew an expired invitation.
        let dispatch = loan.store()?.sync_pairing_dispatch(
            &loan.installation,
            &loan.master,
            loan.revision,
            invitation,
            now(),
        )?;
        let delivery = dispatch.delivery;
        loan.check()?;
        let _ = client.send(
            &delivery.recipient_mailbox,
            &delivery.sender_mailbox,
            delivery.credential(),
            0,
            &STANDARD.encode(&dispatch.frame),
            delivery.expires_at_ms,
        );
        loan.check()?;
        // The owner may already have revoked this temporary sender after proof
        // verification. Its failure must not block receiving the formal roster.
    }
    if !record.receive_ready {
        let grant = &record.receive;
        let result = client
            .register_sender(
                &grant.mailbox,
                &loan.owner_credential,
                &grant.sender,
                &grant.credential,
                grant.expires_at_ms,
                grant.expected_revision,
            )
            .map_err(|error| grant_failure(error, "sync_enrollment_grant_unconfirmed"))?;
        loan.mutate(|store| {
            store.confirm_enrollment_grant(
                &loan.installation,
                &loan.master,
                loan.revision,
                invitation,
                result,
            )
        })?;
        return loan.result();
    }
    if loan.role == EnrollmentRole::Owner && !record.temporary_revoked {
        let revoke = record
            .revoke
            .as_ref()
            .context("sync_enrollment_revoke_missing_requires_repair")?;
        let result = client
            .revoke_sender(
                &revoke.mailbox,
                &loan.owner_credential,
                &revoke.sender,
                revoke.expected_revision,
            )
            .map_err(|error| grant_failure(error, "sync_enrollment_revocation_unconfirmed"))?;
        loan.mutate(|store| {
            store.confirm_enrollment_revoke(
                &loan.installation,
                &loan.master,
                loan.revision,
                invitation,
                result,
            )
        })?;
        return loan.result();
    }
    let store = loan.store()?;
    if loan.role == EnrollmentRole::Owner {
        let export = record
            .snapshot
            .as_ref()
            .map(|s| store.enrollment_export_state(&s.id))
            .transpose()?
            .flatten();
        if record.snapshot.is_none() || matches!(export.as_deref(), Some("building" | "aborted")) {
            loan.mutate(|store| {
                store.reserve_enrollment_snapshot(
                    &loan.installation,
                    &loan.master,
                    loan.revision,
                    invitation,
                )
            })?;
            return loan.result();
        }
        if export.is_none() {
            let vault = store.load_sync_vault(&loan.installation, &loan.master, loan.revision)?;
            prepare_snapshot(&loan, &store, &vault, record)?;
            return loan.result();
        }
        if export.as_deref() == Some("acknowledged") && !record.owner_confirmed {
            loan.mutate(|store| {
                store.complete_enrollment(
                    &loan.installation,
                    &loan.master,
                    loan.revision,
                    invitation,
                    None,
                )
            })?;
            return loan.result();
        }
    }
    let vault = store.load_sync_vault(&loan.installation, &loan.master, loan.revision)?;
    let (send, receive, _) = vault.peer_keys(&record.dataset_id, &record.peer.device_id)?;
    if loan.role == EnrollmentRole::Owner {
        if record.owner_confirmed {
            transfer::stage_offer(&store, record, &send, vault.identity(), true)?;
        } else if let Some(roster) = transfer::snapshot_roster_id(&store, record)? {
            if transfer::wire_acknowledged(&store, &record.peer.device_id, &roster)? {
                transfer::stage_offer(&store, record, &send, vault.identity(), false)?;
            }
        }
    }
    loan.check()?;
    transfer::push_due(
        &store,
        &client,
        record,
        &send,
        vault.identity(),
        now(),
        || loan.check(),
    )?;
    pull_transfer(&loan, &client, record, &receive)?;
    loan.result()
}

pub(crate) fn confirm(
    config: &AppConfig,
    role: &str,
    operation: &str,
    invitation: &str,
    token: &str,
    revision: i64,
) -> Result<EnrollmentView> {
    {
        let all = sessions().lock().unwrap_or_else(|p| p.into_inner());
        let session = all
            .get(&config.data_dir)
            .context("sync_enrollment_session_missing")?;
        let (_, kind) = scope(config, session)?;
        ensure!(
            session.operation == operation
                && kind.name() == role
                && session.token == token
                && session.revision == revision
                && !session.enrollment_busy
                && session
                    .enrollment_confirmation
                    .as_ref()
                    .is_some_and(|(id, r)| id == invitation && *r == kind),
            "sync_enrollment_confirmation_stale"
        );
    }
    if role == "joiner" {
        joining::confirm_enrollment_key(config, token, revision)?;
    } else {
        super::confirm_enrollment_key(config, token, revision)?;
    }
    let all = sessions().lock().unwrap_or_else(|p| p.into_inner());
    let session = all
        .get(&config.data_dir)
        .context("sync_enrollment_session_missing")?;
    ensure!(
        session.operation == operation && session.token == token,
        "sync_enrollment_request_stale"
    );
    // Keep the binding for an exact retry after a lost confirmation response.
    // A later vault mutation replaces it together with its token.
    view(config, session, invitation)
}

#[cfg(test)]
mod tests;
