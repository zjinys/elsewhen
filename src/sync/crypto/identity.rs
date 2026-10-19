//! 设备身份与成员签名：EnvelopeHeader、DeviceIdentity 与三类签名验证。

use super::envelope::append_frame;
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvelopeHeader {
    pub protocol_version: u32,
    pub dataset_id: String,
    pub sender_device_id: String,
    pub recipient_device_id: String,
    pub key_version: u64,
    pub message_id: String,
}

impl EnvelopeHeader {
    pub fn new(
        dataset_id: impl Into<String>,
        sender_device_id: impl Into<String>,
        recipient_device_id: impl Into<String>,
        key_version: u64,
        message_id: impl Into<String>,
    ) -> Result<Self> {
        let header = Self {
            protocol_version: ENVELOPE_VERSION,
            dataset_id: dataset_id.into(),
            sender_device_id: sender_device_id.into(),
            recipient_device_id: recipient_device_id.into(),
            key_version,
            message_id: message_id.into(),
        };
        header.validate()?;
        Ok(header)
    }

    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            self.protocol_version == ENVELOPE_VERSION,
            "unsupported sync envelope version"
        );
        ensure!(self.key_version > 0, "invalid sync key version");
        for value in [
            &self.dataset_id,
            &self.sender_device_id,
            &self.recipient_device_id,
            &self.message_id,
        ] {
            ensure!(
                !value.is_empty()
                    && value.len() <= MAX_IDENTIFIER_BYTES
                    && value.bytes().all(
                        |b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b':' | b'.')
                    ),
                "invalid sync envelope identifier"
            );
        }
        Ok(())
    }

    pub(crate) fn authenticated_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut bytes = AAD_DOMAIN.to_vec();
        bytes.extend_from_slice(&self.protocol_version.to_be_bytes());
        bytes.extend_from_slice(&self.key_version.to_be_bytes());
        for value in [
            &self.dataset_id,
            &self.sender_device_id,
            &self.recipient_device_id,
            &self.message_id,
        ] {
            append_frame(&mut bytes, value.as_bytes());
        }
        Ok(bytes)
    }
}

/// A device's signing identity. Export is explicit for a protected local
/// keystore; private material must never be put in content synchronization.
pub struct DeviceIdentity {
    pub(crate) key_pair: signature::Ed25519KeyPair,
    pkcs8: Vec<u8>,
}

impl fmt::Debug for DeviceIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DeviceIdentity([REDACTED])")
    }
}

impl DeviceIdentity {
    pub fn generate() -> Result<Self> {
        let document = signature::Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
            .map_err(|_| anyhow::anyhow!("cannot generate sync device identity"))?;
        Self::from_pkcs8(document.as_ref())
    }

    pub fn from_pkcs8(bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len() <= 1024, "invalid sync identity encoding");
        let key_pair = signature::Ed25519KeyPair::from_pkcs8(bytes)
            .map_err(|_| anyhow::anyhow!("invalid sync identity encoding"))?;
        Ok(Self {
            key_pair,
            pkcs8: bytes.to_vec(),
        })
    }

    pub fn public_key(&self) -> [u8; 32] {
        self.key_pair
            .public_key()
            .as_ref()
            .try_into()
            .expect("Ed25519 public keys have 32 bytes")
    }

    /// Secret: only pass this to the local protected identity store.
    pub fn to_pkcs8(&self) -> Vec<u8> {
        self.pkcs8.clone()
    }

    pub(crate) fn sign_membership(&self, payload: &[u8]) -> Result<Vec<u8>> {
        Ok(self
            .key_pair
            .sign(&membership_signature_bytes(payload)?)
            .as_ref()
            .to_vec())
    }

    pub(crate) fn sign_handoff(&self, payload: &[u8]) -> Result<Vec<u8>> {
        Ok(self
            .key_pair
            .sign(&handoff_signature_bytes(payload)?)
            .as_ref()
            .to_vec())
    }

    pub(crate) fn sign_pairing(&self, role: &str, payload: &[u8]) -> Result<Vec<u8>> {
        Ok(self
            .key_pair
            .sign(&pairing_signature_bytes(role, payload)?)
            .as_ref()
            .to_vec())
    }
}

fn membership_signature_bytes(payload: &[u8]) -> Result<Vec<u8>> {
    ensure!(
        payload.len() <= 256 * 1024,
        "sync_membership_payload_too_large"
    );
    let mut bytes = b"elsewhen.sync.membership.v1\0".to_vec();
    append_frame(&mut bytes, payload);
    Ok(bytes)
}

pub(crate) fn verify_membership_signature(
    public_key: &[u8; 32],
    payload: &[u8],
    proof: &[u8],
) -> Result<()> {
    signature::UnparsedPublicKey::new(&signature::ED25519, public_key)
        .verify(&membership_signature_bytes(payload)?, proof)
        .map_err(|_| anyhow::anyhow!("sync_membership_signature_invalid"))
}

fn handoff_signature_bytes(payload: &[u8]) -> Result<Vec<u8>> {
    ensure!(payload.len() <= 64 * 1024, "sync_handoff_payload_too_large");
    let mut bytes = b"elsewhen.sync.handoff.v1\0".to_vec();
    append_frame(&mut bytes, payload);
    Ok(bytes)
}

pub(crate) fn verify_handoff_signature(
    public_key: &[u8; 32],
    payload: &[u8],
    proof: &[u8],
) -> Result<()> {
    signature::UnparsedPublicKey::new(&signature::ED25519, public_key)
        .verify(&handoff_signature_bytes(payload)?, proof)
        .map_err(|_| anyhow::anyhow!("sync_handoff_signature_invalid"))
}

fn pairing_signature_bytes(role: &str, payload: &[u8]) -> Result<Vec<u8>> {
    ensure!(
        matches!(role, "join" | "approve") && payload.len() <= 16 * 1024,
        "invalid pairing identity payload"
    );
    let mut bytes = b"elsewhen.sync.pairing.identity.v1\0".to_vec();
    append_frame(&mut bytes, role.as_bytes());
    append_frame(&mut bytes, payload);
    Ok(bytes)
}

pub(crate) fn verify_pairing_identity(
    public_key: &[u8; 32],
    role: &str,
    payload: &[u8],
    signature: &[u8],
) -> Result<()> {
    signature::UnparsedPublicKey::new(&signature::ED25519, public_key)
        .verify(&pairing_signature_bytes(role, payload)?, signature)
        .map_err(|_| anyhow::anyhow!("pairing identity authentication failed"))
}
