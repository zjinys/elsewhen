use super::*;
use crate::sync::transport_relay::{RelayClient, RelayPage};
use base64::{Engine, engine::general_purpose::STANDARD};

#[derive(Clone, Copy)]
enum Action {
    Request,
    Poll,
    Proof,
}
struct NetworkLease {
    _lease: Arc<WorkspaceLease>,
    operation: String,
    entry: String,
    revision: i64,
    relay_url: String,
    mailbox: String,
    owner_credential: [u8; 32],
    recipient: String,
    sender: String,
    credential: [u8; 32],
    response_credential: [u8; 32],
    expires_at: i64,
    cursor: String,
    frame: Vec<u8>,
    approval_mail: Option<String>,
}
impl Drop for NetworkLease {
    fn drop(&mut self) {
        self.owner_credential.fill(0);
        self.credential.fill(0);
        self.response_credential.fill(0);
    }
}

fn begin(config: &AppConfig, operation: &str, action: Action) -> Result<NetworkLease> {
    let mut all = sessions().lock().unwrap_or_else(|p| p.into_inner());
    let session = session(&mut all, config)?;
    ensure!(
        session.operation == operation && session.confirmed && session.master.is_some(),
        "sync_join_runtime_locked"
    );
    let join = session.joining.as_ref().unwrap();
    ensure!(
        !join.network_busy && !session.enrollment_busy,
        "sync_join_network_busy"
    );
    let allowed = match action {
        Action::Request => matches!(join.phase, Phase::Ready | Phase::AwaitingApproval),
        Action::Poll => join.phase == Phase::AwaitingApproval,
        Action::Proof => matches!(join.phase, Phase::AwaitingMembership | Phase::ProofSent),
    };
    ensure!(
        allowed && now() < join.deadline,
        "sync_join_network_phase_stale"
    );
    let store = staging_store(config, session)?;
    let vault = store.load_sync_vault(
        &session.installation,
        session.master.as_ref().unwrap(),
        session.revision,
    )?;
    ensure!(
        vault.revision == session.revision,
        "sync_runtime_vault_changed"
    );
    let mailbox = RelayClient::mailbox_key(vault.mailbox_owner());
    let dispatch = if matches!(action, Action::Proof) {
        Some(store.sync_pairing_dispatch(
            &session.installation,
            session.master.as_ref().unwrap(),
            session.revision,
            &join.invitation_id,
            now(),
        )?)
    } else {
        None
    };
    let join = session.joining.as_mut().unwrap();
    let (recipient, sender, credential, response_credential, frame) = if let Some(dispatch) =
        dispatch
    {
        (
            dispatch.delivery.recipient_mailbox.clone(),
            dispatch.delivery.sender_mailbox.clone(),
            *dispatch.delivery.credential(),
            [0; 32],
            dispatch.frame,
        )
    } else {
        let invitation = join
            .invitation
            .as_ref()
            .context("sync_join_requires_new_invitation")?;
        let route = invitation.route().context("pairing_relay_route_missing")?;
        let credential = invitation.relay_credential()?;
        let response =
            RelayClient::derive_sender_credential(&credential, &route.issuer_mailbox, &mailbox)?;
        if join.pending.is_none() {
            ensure!(
                matches!(action, Action::Request) && join.phase == Phase::Ready,
                "sync_join_requires_new_invitation"
            );
            let (pending, frame) = PendingJoin::begin_for_relay(
                invitation.clone(),
                vault.device().clone(),
                vault.identity(),
                now(),
                mailbox.clone(),
            )?;
            join.pending = Some(pending);
            join.request = frame;
        }
        (
            route.issuer_mailbox.clone(),
            route.request_sender.clone(),
            credential,
            response,
            join.request.clone(),
        )
    };
    let result = NetworkLease {
        _lease: session.lease.clone(),
        operation: operation.into(),
        entry: join.entry.as_ref().unwrap().id.clone(),
        revision: session.revision,
        relay_url: join.relay_url.clone(),
        mailbox,
        owner_credential: *vault.mailbox_owner(),
        recipient,
        sender,
        credential,
        response_credential,
        expires_at: join.deadline,
        cursor: join.cursor.clone(),
        frame,
        approval_mail: join.approval_mail.clone(),
    };
    join.network_busy = true;
    if matches!(action, Action::Request) {
        join.phase = Phase::AwaitingApproval;
    }
    Ok(result)
}

fn current<'a>(
    all: &'a mut HashMap<PathBuf, Session>,
    config: &AppConfig,
    network: &NetworkLease,
) -> Result<&'a mut Session> {
    let session = session(all, config)?;
    ensure!(
        session.operation == network.operation
            && session.revision == network.revision
            && session.confirmed
            && session
                .joining
                .as_ref()
                .unwrap()
                .entry
                .as_ref()
                .is_some_and(|e| e.id == network.entry),
        "sync_join_request_stale"
    );
    Ok(session)
}
fn check_current(config: &AppConfig, network: &NetworkLease) -> Result<()> {
    let mut all = sessions().lock().unwrap_or_else(|p| p.into_inner());
    current(&mut all, config, network)?;
    ensure!(now() < network.expires_at, "pairing_confirmation_expired");
    Ok(())
}

pub(crate) fn send_request(config: &AppConfig, operation: &str) -> Result<JoinView> {
    let network = begin(config, operation, Action::Request)?;
    let result = (|| -> Result<()> {
        let client = RelayClient::new(&network.relay_url)?;
        check_current(config, &network)?;
        ensure!(
            client.create_mailbox(&network.owner_credential)? == network.mailbox,
            "sync_join_mailbox_changed"
        );
        check_current(config, &network)?;
        // Retry the original expected revision and exact capability, never a newly
        // fetched revision. The Worker recognizes the exact committed post-state.
        client.register_sender(
            &network.mailbox,
            &network.owner_credential,
            &network.recipient,
            &network.response_credential,
            network.expires_at,
            0,
        )?;
        check_current(config, &network)?;
        client.send(
            &network.recipient,
            &network.sender,
            &network.credential,
            0,
            &STANDARD.encode(&network.frame),
            network.expires_at,
        )?;
        Ok(())
    })();
    let mut all = sessions().lock().unwrap_or_else(|p| p.into_inner());
    let session = current(&mut all, config, &network)?;
    session.joining.as_mut().unwrap().network_busy = false;
    result.map_err(|_| anyhow::anyhow!("sync_join_request_delivery_unconfirmed"))?;
    Ok(view(session))
}

fn accept_page(
    config: &AppConfig,
    network: &NetworkLease,
    page: Result<RelayPage>,
) -> Result<JoinView> {
    let mut all = sessions().lock().unwrap_or_else(|p| p.into_inner());
    let session = current(&mut all, config, network)?;
    session.joining.as_mut().unwrap().network_busy = false;
    let page = page.map_err(|_| anyhow::anyhow!("sync_join_relay_unavailable"))?;
    ensure!(now() < network.expires_at, "pairing_confirmation_expired");
    let envelope = page
        .envelopes
        .into_iter()
        .find(|e| e.sender_mb == network.recipient);
    session.joining.as_mut().unwrap().cursor = page.next_cursor;
    let Some(envelope) = envelope else {
        return Ok(view(session));
    };
    let store = staging_store(config, session)?;
    let join = session.joining.as_mut().unwrap();
    ensure!(
        join.phase == Phase::AwaitingApproval,
        "sync_join_network_phase_stale"
    );
    // Consume an untrusted response only once; on any failure without a committed
    // journal, a new invitation is needed rather than pretending Noise can resume.
    join.phase = Phase::Failed;
    let pending = join
        .pending
        .take()
        .context("sync_join_requires_new_invitation")?;
    let result = (|| {
        ensure!(
            envelope.ciphertext.len() <= 4 * (16 * 1024_usize).div_ceil(3),
            "pairing_frame_size_invalid"
        );
        let bytes = STANDARD
            .decode(&envelope.ciphertext)
            .map_err(|_| anyhow::anyhow!("pairing_approval_invalid"))?;
        let pair = pending.finish(&bytes, 0, now())?;
        store.install_sync_pairing_join(
            &session.installation,
            session.master.as_ref().unwrap(),
            session.revision,
            pair,
            now(),
        )
    })();
    join.invitation = None;
    join.request.clear();
    let commit = result
        .map_err(|_| anyhow::anyhow!("sync_join_installation_failed_requires_new_invitation"))?;
    session.revision = commit.vault_revision;
    session.confirmed = false;
    session.token = uuid::Uuid::new_v4().to_string();
    session.expires_at = now() + 10 * 60 * 1000;
    join.phase = Phase::InstallationConfirmation;
    join.approval_mail = Some(envelope.id);
    // No ack or proof can leave before the native revision confirmation.
    Ok(view(session))
}

pub(crate) fn poll(config: &AppConfig, operation: &str) -> Result<JoinView> {
    let network = begin(config, operation, Action::Poll)?;
    let page = RelayClient::new(&network.relay_url).and_then(|client| {
        check_current(config, &network)?;
        client.fetch_page(
            &network.mailbox,
            &network.owner_credential,
            &network.cursor,
            100,
        )
    });
    accept_page(config, &network, page)
}

pub(crate) fn send_proof(config: &AppConfig, operation: &str) -> Result<JoinView> {
    let network = begin(config, operation, Action::Proof)?;
    let result = (|| -> Result<bool> {
        let client = RelayClient::new(&network.relay_url)?;
        check_current(config, &network)?;
        client.send(
            &network.recipient,
            &network.sender,
            &network.credential,
            0,
            &STANDARD.encode(&network.frame),
            network.expires_at,
        )?;
        let acknowledged = if let Some(id) = &network.approval_mail {
            check_current(config, &network)?;
            client
                .ack(
                    &network.mailbox,
                    &network.owner_credential,
                    std::slice::from_ref(id),
                )
                .is_ok()
        } else {
            true
        };
        Ok(acknowledged)
    })();
    let mut all = sessions().lock().unwrap_or_else(|p| p.into_inner());
    let session = current(&mut all, config, &network)?;
    let join = session.joining.as_mut().unwrap();
    join.network_busy = false;
    let acknowledged =
        result.map_err(|_| anyhow::anyhow!("sync_join_proof_delivery_unconfirmed"))?;
    join.phase = Phase::ProofSent;
    if acknowledged {
        join.approval_mail = None;
    }
    Ok(view(session))
}

#[cfg(test)]
mod tests;
