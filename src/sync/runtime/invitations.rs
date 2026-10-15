//! Volatile owner invitations. Only a verified, explicitly approved request is
//! promoted to the encrypted journal. Network calls retain the workspace lease,
//! but never hold the process-wide runtime mutex.

use super::*;
use crate::sync::{
    pairing::{
        InvitationRoute, PairingDevice, PairingInvitation, PendingApproval, PendingInvitation,
    },
    transport_relay::{RelayClient, RelayPage},
};
use base64::{Engine, engine::general_purpose::STANDARD};

pub(super) struct OwnerInvitation {
    invitation: PairingInvitation,
    pending: Option<PendingInvitation>,
    approval: Option<PendingApproval>,
    preview_token: Option<String>,
    offered: bool,
    polling: bool,
    cursor: String,
}

/// The code is a secret-bearing, explicitly requested QR/copy result. Do not
/// derive Debug or include it in the ordinary settings state/diagnostics.
pub(crate) struct InvitationOffer {
    pub invitation_id: String,
    pub code: String,
    pub expires_at_ms: i64,
}
#[derive(Clone, Debug)]
pub(crate) struct InvitationPreview {
    pub invitation_id: String,
    pub token: String,
    pub device: PairingDevice,
    pub expires_at_ms: i64,
}

struct NetworkLease {
    _lease: Arc<WorkspaceLease>,
    operation: String,
    invitation_id: String,
    relay_url: String,
    mailbox: String,
    sender: String,
    owner_credential: [u8; 32],
    invitation_credential: [u8; 32],
    expires_at_ms: i64,
    cursor: String,
}
impl Drop for NetworkLease {
    fn drop(&mut self) {
        self.owner_credential.fill(0);
        self.invitation_credential.fill(0);
    }
}

fn owner_session<'a>(
    sessions: &'a mut HashMap<PathBuf, Session>,
    config: &AppConfig,
) -> Result<&'a mut Session> {
    let session = sessions
        .get_mut(&config.data_dir)
        .context("sync_runtime_request_missing")?;
    ensure!(
        session.database == config.database_path && session.confirmed && session.master.is_some(),
        "sync_pairing_runtime_locked"
    );
    let store = Store::open(&session.database)?;
    let vault = store.load_sync_vault(
        &session.installation,
        session.master.as_ref().unwrap(),
        session.revision,
    )?;
    ensure!(
        vault.revision == session.revision,
        "sync_runtime_vault_changed"
    );
    ensure!(store.has_sync_authority()?, "sync_pairing_has_no_authority");
    store.require_sync_signer(vault.identity())?;
    ensure!(
        store.sync_roster_anchor()?.owner == *vault.device(),
        "sync_membership_owner_required"
    );
    Ok(session)
}
fn network_lease(session: &Session) -> Result<NetworkLease> {
    let exchange = session
        .owner_invitation
        .as_ref()
        .context("sync_invitation_missing")?;
    let invitation = &exchange.invitation;
    let route = invitation.route().context("pairing_relay_route_missing")?;
    let store = Store::open(&session.database)?;
    let vault = store.load_sync_vault(
        &session.installation,
        session.master.as_ref().unwrap(),
        session.revision,
    )?;
    Ok(NetworkLease {
        _lease: session.lease.clone(),
        operation: session.operation.clone(),
        invitation_id: invitation.invitation_id().into(),
        relay_url: route.relay_url.clone(),
        mailbox: route.issuer_mailbox.clone(),
        sender: route.request_sender.clone(),
        owner_credential: *vault.mailbox_owner(),
        invitation_credential: invitation.relay_credential()?,
        expires_at_ms: invitation.expires_at_ms(),
        cursor: exchange.cursor.clone(),
    })
}
fn current_exchange<'a>(
    session: &'a mut Session,
    request: &NetworkLease,
) -> Result<&'a mut OwnerInvitation> {
    ensure!(
        session.operation == request.operation,
        "sync_invitation_request_stale"
    );
    let exchange = session
        .owner_invitation
        .as_mut()
        .context("sync_invitation_missing")?;
    ensure!(
        exchange.invitation.invitation_id() == request.invitation_id,
        "sync_invitation_request_stale"
    );
    Ok(exchange)
}

fn prepare(config: &AppConfig, relay_url: &str, consent: bool) -> Result<NetworkLease> {
    ensure!(consent, "sync_provider_key_consent_required");
    let mut sessions = sessions().lock().unwrap_or_else(|p| p.into_inner());
    let session = owner_session(&mut sessions, config)?;
    ensure!(
        session
            .owner_invitation
            .as_ref()
            .is_none_or(|v| v.invitation.expires_at_ms() <= now()),
        "sync_invitation_already_open"
    );
    let store = Store::open(&session.database)?;
    let vault = store.load_sync_vault(
        &session.installation,
        session.master.as_ref().unwrap(),
        session.revision,
    )?;
    let temporary = crate::sync::crypto::ContentKey::generate()?;
    let (invitation, pending) = PairingInvitation::issue_for_relay(
        &store
            .sync_identity()?
            .context("sync_not_initialized")?
            .dataset_id,
        vault.device().clone(),
        vault.identity(),
        now(),
        InvitationRoute {
            relay_url: relay_url.into(),
            issuer_mailbox: RelayClient::mailbox_key(vault.mailbox_owner()),
            request_sender: RelayClient::mailbox_key(&temporary.to_bytes()),
        },
    )?;
    session.owner_invitation = Some(OwnerInvitation {
        invitation,
        pending: Some(pending),
        approval: None,
        preview_token: None,
        offered: false,
        polling: false,
        cursor: "0".into(),
    });
    network_lease(session)
}
fn registered(
    config: &AppConfig,
    request: &NetworkLease,
    successful: bool,
) -> Result<InvitationOffer> {
    let mut sessions = sessions().lock().unwrap_or_else(|p| p.into_inner());
    let session = owner_session(&mut sessions, config)?;
    let exchange = current_exchange(session, request)?;
    if !successful || now() >= exchange.invitation.expires_at_ms() {
        session.owner_invitation = None;
        anyhow::bail!("sync_invitation_registration_unconfirmed");
    }
    exchange.offered = true;
    Ok(InvitationOffer {
        invitation_id: request.invitation_id.clone(),
        code: exchange.invitation.encode()?,
        expires_at_ms: exchange.invitation.expires_at_ms(),
    })
}

pub(crate) fn issue(config: &AppConfig, relay_url: &str, consent: bool) -> Result<InvitationOffer> {
    let request = prepare(config, relay_url, consent)?;
    let result = (|| -> Result<()> {
        let relay = RelayClient::new(&request.relay_url)?;
        ensure!(
            relay.create_mailbox(&request.owner_credential)? == request.mailbox,
            "sync_invitation_mailbox_changed"
        );
        relay.register_sender(
            &request.mailbox,
            &request.owner_credential,
            &request.sender,
            &request.invitation_credential,
            request.expires_at_ms,
            0,
        )?;
        Ok(())
    })();
    // Unknown network outcomes leave only a short-lived sender grant, never an
    // active dataset member. No invite is displayed before registration succeeds.
    registered(config, &request, result.is_ok())
}

fn preview(exchange: &OwnerInvitation) -> Result<Option<InvitationPreview>> {
    match (&exchange.approval, &exchange.preview_token) {
        (Some(approval), Some(token)) => Ok(Some(InvitationPreview {
            invitation_id: exchange.invitation.invitation_id().into(),
            token: token.clone(),
            device: approval.preview().clone(),
            expires_at_ms: exchange.invitation.expires_at_ms(),
        })),
        (None, None) => Ok(None),
        _ => anyhow::bail!("sync_invitation_state_invalid"),
    }
}
fn accept_page(
    config: &AppConfig,
    request: &NetworkLease,
    page: Result<RelayPage>,
) -> Result<Option<InvitationPreview>> {
    let mut sessions = sessions().lock().unwrap_or_else(|p| p.into_inner());
    {
        let session = sessions
            .get_mut(&config.data_dir)
            .context("sync_runtime_request_missing")?;
        ensure!(
            session.database == config.database_path,
            "sync_invitation_request_stale"
        );
        current_exchange(session, request)?.polling = false;
    }
    let session = owner_session(&mut sessions, config)?;
    let exchange = current_exchange(session, request)?;
    ensure!(
        now() < exchange.invitation.expires_at_ms(),
        "pairing_confirmation_expired"
    );
    let page = page.map_err(|_| anyhow::anyhow!("sync_invitation_relay_unavailable"))?;
    exchange.cursor = page.next_cursor;
    if let Some(envelope) = page
        .envelopes
        .into_iter()
        .find(|e| e.sender_mb == request.sender)
    {
        ensure!(
            envelope.ciphertext.len() <= 4 * (16 * 1024_usize).div_ceil(3),
            "pairing_frame_size_invalid"
        );
        let bytes = STANDARD
            .decode(envelope.ciphertext)
            .map_err(|_| anyhow::anyhow!("pairing_request_invalid"))?;
        let pending = exchange
            .pending
            .take()
            .context("sync_invitation_already_consumed")?;
        match pending.receive(&bytes, now()) {
            Ok(approval) => {
                exchange.approval = Some(approval);
                exchange.preview_token = Some(uuid::Uuid::new_v4().to_string());
            }
            Err(_) => {
                session.owner_invitation = None;
                anyhow::bail!("pairing_request_invalid");
            }
        }
    }
    // Mail is deliberately not acked at preview time: no durable approval exists.
    preview(exchange)
}

pub(crate) fn poll(config: &AppConfig, invitation_id: &str) -> Result<Option<InvitationPreview>> {
    let request = {
        let mut sessions = sessions().lock().unwrap_or_else(|p| p.into_inner());
        let session = owner_session(&mut sessions, config)?;
        let exchange = session
            .owner_invitation
            .as_mut()
            .context("sync_invitation_missing")?;
        ensure!(
            exchange.invitation.invitation_id() == invitation_id && exchange.offered,
            "sync_invitation_request_stale"
        );
        ensure!(
            now() < exchange.invitation.expires_at_ms(),
            "pairing_confirmation_expired"
        );
        if exchange.approval.is_some() {
            return preview(exchange);
        }
        ensure!(!exchange.polling, "sync_invitation_poll_in_progress");
        let request = network_lease(session)?;
        session.owner_invitation.as_mut().unwrap().polling = true;
        request
    };
    let result = RelayClient::new(&request.relay_url).and_then(|relay| {
        relay.fetch_page(
            &request.mailbox,
            &request.owner_credential,
            &request.cursor,
            100,
        )
    });
    accept_page(config, &request, result)
}

pub(crate) fn approve(
    config: &AppConfig,
    invitation_id: &str,
    preview_token: &str,
    consent: bool,
) -> Result<super::pairing::PairingKeyCommit> {
    ensure!(consent, "sync_provider_key_consent_required");
    super::pairing::mutate(config, |store, session| {
        let exchange = session
            .owner_invitation
            .as_ref()
            .context("sync_invitation_missing")?;
        ensure!(
            exchange.invitation.invitation_id() == invitation_id
                && exchange.preview_token.as_deref() == Some(preview_token)
                && exchange.offered
                && now() < exchange.invitation.expires_at_ms(),
            "sync_invitation_approval_stale"
        );
        let mut exchange = session.owner_invitation.take().unwrap();
        let approval = exchange
            .approval
            .take()
            .context("sync_invitation_not_reviewed")?;
        store.approve_sync_pairing(
            &session.installation,
            session.master.as_ref().unwrap(),
            session.revision,
            approval,
            now(),
        )
    })
}

pub(crate) fn cancel(config: &AppConfig, invitation_id: &str) -> Result<()> {
    let request = {
        let mut sessions = sessions().lock().unwrap_or_else(|p| p.into_inner());
        let session = owner_session(&mut sessions, config)?;
        let request = network_lease(session)?;
        ensure!(
            request.invitation_id == invitation_id,
            "sync_invitation_request_stale"
        );
        session.owner_invitation = None;
        request
    };
    RelayClient::new(&request.relay_url)
        .and_then(|relay| {
            relay.revoke_sender(
                &request.mailbox,
                &request.owner_credential,
                &request.sender,
                1,
            )
        })
        .map_err(|_| anyhow::anyhow!("sync_invitation_revocation_unconfirmed"))?;
    Ok(())
}

pub(crate) fn send_approval(config: &AppConfig, invitation_id: &str) -> Result<()> {
    let (_lease, dispatch) = {
        let mut sessions = sessions().lock().unwrap_or_else(|p| p.into_inner());
        let session = owner_session(&mut sessions, config)?;
        let store = Store::open(&session.database)?;
        let dispatch = store.sync_pairing_dispatch(
            &session.installation,
            session.master.as_ref().unwrap(),
            session.revision,
            invitation_id,
            now(),
        )?;
        (session.lease.clone(), dispatch)
    };
    let delivery = &dispatch.delivery;
    RelayClient::new(&delivery.relay_url)
        .and_then(|relay| {
            relay.send(
                &delivery.recipient_mailbox,
                &delivery.sender_mailbox,
                delivery.credential(),
                0,
                &STANDARD.encode(&dispatch.frame),
                delivery.expires_at_ms,
            )
        })
        .map_err(|_| anyhow::anyhow!("sync_pairing_delivery_unconfirmed"))?;
    Ok(())
}

#[cfg(test)]
mod tests;
