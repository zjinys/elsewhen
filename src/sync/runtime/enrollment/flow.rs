//! 报名主流程：EnrollmentLoan 租约、begin、证明交换、快照/传输、advance 与 confirm。

use super::*;
use std::collections::HashSet;
use std::sync::OnceLock;

/// enrollment 扫描单页上限。relay 的 fetch 允许 1..=100，且服务端把单页响应
/// 钳在 16MB 内，所以单页不会撑爆内存。老值 3 会把扫描卡在历轮配对留下
/// 的、数量只增不减且永远不被 accept 的积压邮件上（约 500 封 → 每个控制步
/// ack 都要多等数分钟）。30 仍然不够：死信堆到 800+ 时每拍的推进量追不上
/// 重推量，而交接必须扫到尾部才能取到对端回执。
const SCAN_PAGE_LIMIT: usize = 100;

/// enrollment 扫描的游标键。
///
/// 不能直接用信箱地址：普通增量扫描（`engine::pull`）用的是同一行
/// `sync_relay_cursors.after_id`，它扫到尾部就把游标写成 `"0"` 重新包绕。
/// 两个扫描器的页面大小和「什么算已处理」都不一样，于是互相把对方的进度
/// 回卷——信箱里一旦有几百封永不 ack 的死信（历轮配对留下的
/// `provider:` / `pair-complete:`），交接扫描就永远到不了尾部，对端的回执
/// 永远取不到，两端停在「等待成员授权与初始同步」。
pub(super) fn scan_key(mailbox: &str) -> String {
    format!("{mailbox}#enrollment")
}

/// 本进程内已对 joiner「响过一次」的重推邮件（mailbox, 邮件 id）。
/// 只为把第一处不可应用邮件错误浮出水面；重启后再次响一次。
fn transfer_drop_registry() -> &'static Mutex<HashSet<(String, String)>> {
    static REGISTRY: OnceLock<Mutex<HashSet<(String, String)>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashSet::new()))
}

pub(super) struct EnrollmentLoan {
    pub(super) _lease: Arc<WorkspaceLease>,
    pub(super) config: AppConfig,
    pub(super) database: PathBuf,
    pub(super) operation: String,
    pub(super) installation: String,
    pub(super) revision: i64,
    pub(super) master: [u8; 32],
    pub(super) role: EnrollmentRole,
    pub(super) invitation: String,
    pub(super) record: Option<EnrollmentRecord>,
    pub(super) bootstrap: InvitationRoute,
    pub(super) proof_deadline: i64,
    pub(super) owner_credential: [u8; 32],
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
    pub(super) fn store(&self) -> Result<Store> {
        Ok(Store::open(&self.database)?.with_provider_master(self.master))
    }
    pub(super) fn current<'a>(
        &self,
        all: &'a mut HashMap<PathBuf, Session>,
    ) -> Result<&'a mut Session> {
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
    pub(super) fn check(&self) -> Result<()> {
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
    pub(super) fn pause(
        session: &mut Session,
        invitation: &str,
        role: EnrollmentRole,
        commit: PairingCommit,
    ) {
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
    pub(super) fn mutate(
        &self,
        operation: impl FnOnce(&Store) -> Result<PairingCommit>,
    ) -> Result<()> {
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
    pub(super) fn result(&self) -> Result<EnrollmentView> {
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

pub(super) fn begin(
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

pub(super) fn advance_proof(loan: &EnrollmentLoan, client: &RelayClient) -> Result<()> {
    loan.check()?;
    let store = loan.store()?;
    let mailbox = &loan.bootstrap.issuer_mailbox;
    let cursor = store.relay_scan_cursor(&scan_key(mailbox))?;
    let page = client.fetch_page(mailbox, &loan.owner_credential, &cursor, SCAN_PAGE_LIMIT)?;
    accept_proof_page(loan, page)
}

pub(super) fn accept_proof_page(
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
                    store.save_relay_scan_cursor(&scan_key(mailbox), &mail.id)?;
                    return Ok(());
                }
            }
        }
    }
    loan.check()?;
    store.save_relay_scan_cursor(
        &scan_key(mailbox),
        if page.has_more {
            &page.next_cursor
        } else {
            "0"
        },
    )?;
    Ok(())
}

pub(super) fn prepare_snapshot(
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
    let upload =
        crate::sync::runtime::snapshot_upload(vault, &record.dataset_id, &record.peer.device_id)?;
    engine::snapshot::stage_enrollment_snapshot(
        store,
        &record.peer.device_id,
        &record.snapshot.as_ref().unwrap().id,
        record.key_version,
        &key,
        vault.identity(),
        upload.as_ref(),
    )?;
    loan.check()?;
    Ok(())
}

/// 一封 relay 邮件是不是数据面信封。
///
/// 同一个信箱里混着两种载荷：数据面信封（`crypto::seal(..).to_bytes()`，紧凑
/// JSON 对象，首字节必是 `{`）和配对帧（原始 AEAD 密文，随机字节）。配对阶段
/// 对端往本机信箱回过 response/proof-ack，这些邮件扫到交接时仍然在。它们不是
/// 损坏的信封，只是不属于这条收件路径 —— 当成「不是我的东西」跳过。
/// 只有「解出来必然为空或非 `{` 开头」才算外来帧；`{` 开头但解析失败的仍走
/// 响亮报错路径，因为那才真的是损坏或被篡改的信封。
fn looks_like_dataplane_envelope(ciphertext: &str) -> bool {
    STANDARD
        .decode(ciphertext)
        .ok()
        .and_then(|bytes| bytes.first().copied())
        == Some(b'{')
}

pub(super) fn pull_transfer(
    loan: &EnrollmentLoan,
    client: &RelayClient,
    record: &EnrollmentRecord,
    receive: &crate::sync::crypto::ContentKey,
) -> Result<()> {
    let store = loan.store()?;
    let mailbox = &record.route.local_mailbox;
    let cursor = store.relay_scan_cursor(&scan_key(mailbox))?;
    loan.check()?;
    let page = client.fetch_page(mailbox, &loan.owner_credential, &cursor, SCAN_PAGE_LIMIT)?;
    // 诊断：交接卡住时唯一可信的现场输入是「信箱里到底有什么」。ELSEWHEN_DEBUG 才开，
    // 只打主题/长度，不打正文。
    let trace = std::env::var("ELSEWHEN_DEBUG").is_ok();
    if trace {
        // 统一走 `wire_log`（单一门控 + 四字段）：`subject` 是本机信箱前缀，
        // 第三个槽位留 0 —— 这一行讲的是「信箱里有什么」，不是某一封的长度。
        crate::sync::engine::deliver::wire_log(
            "pull",
            &mailbox[..8.min(mailbox.len())],
            0,
            &format!(
                "role {:?} cursor {} n {} more {}",
                loan.role,
                &cursor[..8.min(cursor.len())],
                page.envelopes.len(),
                page.has_more
            ),
        );
    }
    let mut first_error: Option<anyhow::Error> = None;
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
                            store.save_relay_scan_cursor(&scan_key(mailbox), &mail.id)?;
                            return Ok(());
                        }
                    }
                    true
                }
                Ok(transfer::ReceiveOutcome::Waiting) => false,
                Err(error) if error.is::<rusqlite::Error>() || error.is::<std::io::Error>() => {
                    return Err(error);
                }
                Err(_) if !looks_like_dataplane_envelope(&mail.ciphertext) => {
                    // 配对帧 / 其它非信封载荷：本机信箱里配对阶段的残留。
                    // 不 accept、不 ack（ack 语义是「已应用」）、不算错误。
                    if trace {
                        crate::sync::engine::deliver::wire_log(
                            "pull-foreign-frame",
                            &mail.id,
                            mail.ciphertext.len(),
                            &format!(
                                "from {} skipped",
                                &mail.sender_mb[..8.min(mail.sender_mb.len())]
                            ),
                        );
                    }
                    false
                }
                Err(error) => {
                    // 发送端重推下「复现错误」的邮件不能永久阻塞整个交接：
                    // 第一次冒泡给 tick（joiner 日志/UI 可见），同一进程对同一
                    // 邮件只响一次，之后跳过并推进扫描光标——扫描会包绕回来
                    // 重访它，发送端的新导出/重置也不会被它饿死。
                    let key = (mailbox.clone(), mail.id.clone());
                    let loud = transfer_drop_registry()
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .insert(key);
                    if loud && first_error.is_none() {
                        first_error = Some(error);
                    }
                    false
                }
            }
        } else {
            false
        };
        if trace {
            let subject = STANDARD
                .decode(&mail.ciphertext)
                .ok()
                .and_then(|bytes| crate::sync::crypto::EncryptedEnvelope::from_bytes(&bytes).ok())
                .map(|envelope| envelope.unverified_header().message_id.clone());
            crate::sync::engine::deliver::wire_log(
                "pull-mail",
                &mail.id,
                mail.ciphertext.len(),
                &format!(
                    "from {} subject {:?} accepted {}",
                    &mail.sender_mb[..8.min(mail.sender_mb.len())],
                    subject,
                    accepted
                ),
            );
        }
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
    if let Some(error) = first_error {
        return Err(error);
    }
    store.save_relay_scan_cursor(
        &scan_key(mailbox),
        if page.has_more {
            &page.next_cursor
        } else {
            "0"
        },
    )?;
    Ok(())
}

pub(super) fn grant_failure(error: anyhow::Error, unconfirmed: &str) -> anyhow::Error {
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
    // blob 数据面：快照分片不在信箱里，必须主动去 blob 上拉。joiner 这一侧只有
    // 它自己的信箱 owner 凭据，正好是 `blob-get`/`blob-ack` 要求的那个身份。
    if loan.role == EnrollmentRole::Joiner
        && store.snapshot_import_pending()?
        && store.local_sync_membership_active()?
    {
        let download = crate::sync::engine::snapshot::BlobDownload {
            client: &client,
            mailbox: &record.route.local_mailbox,
            owner_credential: &loan.owner_credential,
            source: &record.peer.device_id,
            key_version: record.key_version,
            key: &receive,
            public: &record.peer.signing_key,
        };
        crate::sync::engine::snapshot::import_blob_snapshots(&store, &download, 2)?;
    }
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
