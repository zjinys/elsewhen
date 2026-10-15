//! Relay addresses are authenticated by the Noise prologue and signed claims;
//! they are routing metadata, never a membership grant or mailbox owner secret.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

use super::PairingInvitation;

pub(super) fn validate_join_route(
    invitation: &PairingInvitation,
    mailbox: Option<&str>,
) -> Result<()> {
    match (invitation.route(), mailbox) {
        (None, None) => Ok(()),
        (Some(route), Some(mailbox)) => {
            route.validate()?;
            validate_mailbox(mailbox)?;
            ensure!(
                mailbox != route.issuer_mailbox && mailbox != route.request_sender,
                "pairing_mailbox_collision"
            );
            Ok(())
        }
        _ => anyhow::bail!("pairing_relay_route_mismatch"),
    }
}

pub(super) fn make_paired_route(
    invitation: &PairingInvitation,
    mailbox: Option<&str>,
    issuer: bool,
) -> Result<Option<PairedRoute>> {
    validate_join_route(invitation, mailbox)?;
    match (invitation.route(), mailbox) {
        (Some(route), Some(mailbox)) => {
            let (local, peer) = if issuer {
                (route.issuer_mailbox.as_str(), mailbox)
            } else {
                (mailbox, route.issuer_mailbox.as_str())
            };
            let paired = PairedRoute {
                relay_url: route.relay_url.clone(),
                local_mailbox: local.into(),
                peer_mailbox: peer.into(),
            };
            paired.validate()?;
            Ok(Some(paired))
        }
        _ => Ok(None),
    }
}

/// Temporary recipient-scoped capability, not the invitation PSK or a content
/// key. Persist only in the encrypted pairing journal, never an ordinary DTO.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairingDelivery {
    pub relay_url: String,
    pub recipient_mailbox: String,
    pub sender_mailbox: String,
    pub expires_at_ms: i64,
    credential: [u8; 32],
}
impl std::fmt::Debug for PairingDelivery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PairingDelivery([REDACTED])")
    }
}
impl PairingDelivery {
    pub(crate) fn validate(&self) -> Result<()> {
        validate_relay(&self.relay_url)?;
        validate_mailbox(&self.recipient_mailbox)?;
        validate_mailbox(&self.sender_mailbox)?;
        ensure!(
            self.recipient_mailbox != self.sender_mailbox && self.expires_at_ms > 0,
            "pairing_delivery_invalid"
        );
        Ok(())
    }
    pub(crate) fn credential(&self) -> &[u8; 32] {
        &self.credential
    }
}

pub(super) fn make_pairing_delivery(
    invitation: &PairingInvitation,
    mailbox: Option<&str>,
    issuer: bool,
) -> Result<Option<PairingDelivery>> {
    validate_join_route(invitation, mailbox)?;
    let (Some(route), Some(mailbox)) = (invitation.route(), mailbox) else {
        return Ok(None);
    };
    let invitation_credential = invitation.relay_credential()?;
    let (sender, recipient, credential) = if issuer {
        (
            route.issuer_mailbox.as_str(),
            mailbox,
            crate::sync::transport_relay::RelayClient::derive_sender_credential(
                &invitation_credential,
                &route.issuer_mailbox,
                mailbox,
            )?,
        )
    } else {
        (
            route.request_sender.as_str(),
            route.issuer_mailbox.as_str(),
            invitation_credential,
        )
    };
    let delivery = PairingDelivery {
        relay_url: route.relay_url.clone(),
        recipient_mailbox: recipient.into(),
        sender_mailbox: sender.into(),
        expires_at_ms: invitation.expires_at_ms(),
        credential,
    };
    delivery.validate()?;
    Ok(Some(delivery))
}

pub(crate) fn validate_mailbox(value: &str) -> Result<()> {
    ensure!(
        value.len() == 64
            && value
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
        "pairing_mailbox_invalid"
    );
    Ok(())
}
pub(crate) fn validate_relay(value: &str) -> Result<()> {
    ensure!(value.len() <= 512, "pairing_relay_url_invalid");
    let url =
        reqwest::Url::parse(value).map_err(|_| anyhow::anyhow!("pairing_relay_url_invalid"))?;
    // 与 transport_relay::RelayClient 同一口径：http 仅限回环/私网地址
    // （自建 NAS 中继），公网域名必须 HTTPS。
    let local = url.host_str().is_some_and(|host| {
        host == "localhost"
            || host
                .trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| match ip {
                    std::net::IpAddr::V4(v4) => v4.is_loopback() || v4.is_private(),
                    std::net::IpAddr::V6(v6) => {
                        v6.is_loopback() || (v6.segments()[0] & 0xfe00) == 0xfc00
                    }
                })
    });
    ensure!(
        (url.scheme() == "https" || (url.scheme() == "http" && local))
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "pairing_relay_url_invalid"
    );
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InvitationRoute {
    pub relay_url: String,
    pub issuer_mailbox: String,
    pub request_sender: String,
}
impl InvitationRoute {
    pub(crate) fn validate(&self) -> Result<()> {
        validate_relay(&self.relay_url)?;
        validate_mailbox(&self.issuer_mailbox)?;
        validate_mailbox(&self.request_sender)?;
        ensure!(
            self.issuer_mailbox != self.request_sender,
            "pairing_mailbox_collision"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairedRoute {
    pub relay_url: String,
    pub local_mailbox: String,
    pub peer_mailbox: String,
}
impl PairedRoute {
    pub(crate) fn validate(&self) -> Result<()> {
        validate_relay(&self.relay_url)?;
        validate_mailbox(&self.local_mailbox)?;
        validate_mailbox(&self.peer_mailbox)?;
        ensure!(
            self.local_mailbox != self.peer_mailbox,
            "pairing_mailbox_collision"
        );
        Ok(())
    }
}
