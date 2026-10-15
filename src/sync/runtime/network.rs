//! Bounded incremental sync tick for the selected dataset. Unlike the
//! enrollment coordinator this module never prepares keys or snapshots: it only
//! drains already-authorized peers' encrypted deltas through the durable
//! outbox/receipt machinery. A tick is not a pairing, selection or knowledge
//! trigger, and a failed tick never blocks local capture.

use anyhow::{Context, Result, ensure};

use crate::{config::AppConfig, storage::Store};

use super::{background_guard, now, sessions};

/// What a single bounded tick actually did. Counts never prove that the peer
/// has applied anything; receipt travel is itself part of the next ticks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct NetworkTickReport {
    pub staged_operations: usize,
    pub uploaded: usize,
    pub pulled_accepted: usize,
    pub pulled_waiting: usize,
    /// 拒绝里因协议版本不匹配（对端需要升级）的数量。
    pub pulled_version_mismatch: usize,
    /// 本轮为该成员补发了当前名册修订（owner 收敛）。
    pub roster_converged: bool,
    pub pulled_rejected: usize,
    pub acknowledgement_failed: bool,
    /// Re-snapshot exports staged this tick for lagging members.
    pub resnapshot_staged: usize,
    /// Re-snapshot requests completed this tick (export ack applied).
    pub resnapshot_completed: usize,
    /// Members still flagged `needs_resnapshot` after this tick.
    pub resnapshot_pending: Vec<String>,
}

impl NetworkTickReport {
    fn merge(&mut self, other: &NetworkTickReport) {
        self.staged_operations += other.staged_operations;
        self.uploaded += other.uploaded;
        self.pulled_accepted += other.pulled_accepted;
        self.pulled_waiting += other.pulled_waiting;
        self.pulled_version_mismatch += other.pulled_version_mismatch;
        self.roster_converged |= other.roster_converged;
        self.pulled_rejected += other.pulled_rejected;
        self.acknowledgement_failed |= other.acknowledgement_failed;
    }
}

/// Most peers/pages a single tick touches; a backlog resumes on the next tick
/// instead of one call blocking the caller.
const MAX_PEERS_PER_TICK: usize = 8;
const MAX_PAGES_PER_TICK: u32 = 4;

/// Runs one bounded push/pull cycle for every active, non-revoked, routed peer
/// of the pinned database. Returns `Ok(None)` when a tick must not run now:
/// no confirmed local session, another background caller holds the workspace,
/// a snapshot import is still staging, or this device lost its membership.
/// Hard failures (vault revision drift, relay rejection of a valid grant,
/// missing peer key material) surface as errors so the UI can show them
/// instead of retrying silently.
pub(crate) fn tick(config: &AppConfig) -> Result<Option<NetworkTickReport>> {
    let store = Store::open(&config.database_path)?;
    if store.has_sync_authority()? {
        // A restored staging database must finish its snapshot publication
        // before ordinary deltas move, matching the engine's receive-side hold.
        if store.snapshot_import_pending()? {
            return Ok(None);
        }
    } else if store.sync_identity()?.is_none() {
        // Sync was never initialized here: nothing to synchronize.
        return Ok(None);
    }
    // Owner devices pass via their pinned authority; a joined device must
    // still be an active roster member before it may touch the relay.
    store.require_local_sync_membership()?;
    let (master, revision, installation) = {
        let all = sessions().lock().unwrap_or_else(|p| p.into_inner());
        let Some(session) = all.get(&config.data_dir) else {
            return Ok(None);
        };
        if session.joining.is_some()
            || session.database != config.database_path
            || !session.confirmed
            || session.master.is_none()
            || session.enrollment_busy
        {
            return Ok(None);
        }
        (
            session.master.unwrap(),
            session.revision,
            session.installation.clone(),
        )
    };
    let vault = store.load_sync_vault(&installation, &master, revision)?;
    // The session's confirmed revision must still be the durable vault
    // revision; native-confirmed P2 changes invalidate older sessions.
    ensure!(vault.revision == revision, "sync_runtime_vault_changed");
    let Some(_guard) = background_guard(config, &store)? else {
        return Ok(None);
    };
    let identity = store.sync_identity()?.context("sync_not_initialized")?;
    let local_device = vault.device().clone();
    ensure!(
        identity.device_id == local_device.device_id,
        "sync_network_identity_mismatch"
    );
    let roster = store
        .current_signed_roster()?
        .context("sync_authority_not_ready")?;
    let peers: Vec<(String, [u8; 32])> = roster
        .roster
        .members
        .iter()
        .filter(|m| m.active && m.device.device_id != local_device.device_id)
        .take(MAX_PEERS_PER_TICK)
        .map(|m| (m.device.device_id.clone(), m.device.signing_key))
        .collect();
    let mut report = NetworkTickReport::default();
    for (peer_id, peer_signing) in &peers {
        // A roster member without usable local key material cannot be
        // contacted; this is a repair signal, not a skippable peer.
        let (send_key, receive_key, key_version) =
            vault.peer_keys(&identity.dataset_id, &peer_id)?;
        let route = vault.peer_route(&identity.dataset_id, &peer_id)?.clone();
        route.validate()?;
        ensure!(
            route.local_mailbox
                == crate::sync::transport_relay::RelayClient::mailbox_key(vault.mailbox_owner()),
            "sync_network_route_mismatch"
        );
        let client = crate::sync::transport_relay::RelayClient::new(&route.relay_url)?;
        let mut peer_report = NetworkTickReport::default();
        // Stage first: new local operations and provider versions become due
        // wire envelopes before any network call, so a crash after staging
        // never loses the intent to send.
        peer_report.staged_operations = crate::sync::engine::stage_operations(
            &store,
            &peer_id,
            key_version,
            &send_key,
            vault.identity(),
        )?;
        let encryption = [crate::sync::engine::EncryptionPeer {
            device_id: &peer_id,
            key_version,
            key: &send_key,
        }];
        crate::sync::engine::stage_provider_updates(&store, &encryption, vault.identity())?;
        // Application receipts ride the same durable outbox; without staging
        // them the peer's applied operations never clear and would resend.
        crate::sync::engine::stage_receipts(
            &store,
            &peer_id,
            key_version,
            &send_key,
            vault.identity(),
        )?;
        // P4e 名册收敛：owner 对未确认当前名册修订的活跃成员自动补发。
        // 长离线成员回连后的恢复通道；撤销/移交后新修订由此到达其余成员。
        if crate::sync::engine::stage_membership_roster_if_stale(
            &store,
            &peer_id,
            key_version,
            &send_key,
            vault.identity(),
        )? {
            peer_report.roster_converged = true;
        }
        // The durable send credential for this direction was agreed at
        // enrollment; never re-derive it from newer relay state.
        let credential = crate::sync::transport_relay::RelayClient::derive_sender_credential(
            &send_key.to_bytes(),
            &route.local_mailbox,
            &route.peer_mailbox,
        )?;
        let pushed = crate::sync::engine::push_due(
            &store,
            &client,
            &peer_id,
            &route.peer_mailbox,
            &route.local_mailbox,
            &credential,
            now(),
        )?;
        peer_report.uploaded = pushed.uploaded;
        let receiving = [crate::sync::engine::ReceivingPeer {
            device_id: &peer_id,
            mailbox_id: &route.peer_mailbox,
            key_version,
            key: &receive_key,
            public_key: &peer_signing,
        }];
        for _ in 0..MAX_PAGES_PER_TICK {
            let pulled = crate::sync::engine::pull_page(
                &store,
                &client,
                &route.local_mailbox,
                vault.mailbox_owner(),
                &receiving,
            )?;
            peer_report.pulled_accepted += pulled.accepted;
            peer_report.pulled_waiting += pulled.waiting;
            peer_report.pulled_rejected += pulled.rejected;
            peer_report.pulled_version_mismatch += pulled.rejected_version_mismatch;
            peer_report.acknowledgement_failed |= pulled.acknowledgement_failed;
            if pulled.accepted + pulled.waiting + pulled.rejected < 3 {
                break;
            }
        }
        report.merge(&peer_report);
    }
    // Re-snapshot scheduling: GC flagged members get exactly one in-flight
    // export each, built with a fresh authority roster frame; the durable
    // wire outbox owns the upload retries. Completion is what un-freezes GC.
    report.resnapshot_completed = store.finish_sync_resnapshot_requests()?;
    let _gc = store.run_sync_gc()?;
    let pending = store.sync_resnapshot_pending()?;
    for member in &pending {
        // The roster peers are also the routed peers; a flagged member whose
        // route is absent cannot be served by this device (a joiner only
        // routes to the owner).
        if !peers.iter().any(|(id, _)| id == member) {
            continue;
        }
        let Some(claim) = store.claim_sync_resnapshot(member)? else {
            continue;
        };
        let (send_key, _, key_version) = vault.peer_keys(&identity.dataset_id, member)?;
        crate::sync::engine::snapshot::stage_enrollment_snapshot(
            &store,
            member,
            &claim.snapshot_id,
            key_version,
            &send_key,
            vault.identity(),
        )?;
        report.resnapshot_staged += 1;
    }
    // The receiver of a re-snapshot cannot import it into its live database;
    // it acknowledges the transfer so the sender's request completes and GC
    // un-freezes. The snapshot itself stays a transport copy for the restore
    // flow, never an in-place overwrite.
    for (peer_id, _) in &peers {
        let acked: Vec<String> = {
            let mut q = store.connection.prepare(
                "SELECT DISTINCT e.id FROM sync_snapshot_exports e
                 JOIN sync_snapshot_export_wires w
                   ON w.snapshot_id = e.id AND w.recipient_device_id = ?1
                 JOIN sync_wire_outbox o
                   ON o.recipient_device_id = w.recipient_device_id
                  AND o.message_id = w.message_id
                 WHERE e.state = 'acknowledged' AND o.state = 'acknowledged'",
            )?;
            q.query_map([peer_id], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        for export_id in acked {
            let (_, receive_key, key_version) = vault.peer_keys(&identity.dataset_id, peer_id)?;
            crate::sync::engine::snapshot::ack_resnapshot(
                &store,
                peer_id,
                &export_id,
                key_version,
                &receive_key,
                vault.identity(),
            )?;
        }
    }
    report.resnapshot_pending = store.sync_resnapshot_pending()?;
    Ok(Some(report))
}

#[cfg(test)]
mod tests;
