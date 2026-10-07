//! Authenticated, end-to-end encrypted sync envelopes.
//!
//! Relay credentials are deliberately not used here. Callers obtain the trusted
//! sender public key and content key from a separately authenticated pairing
//! flow; neither key is accepted from an incoming envelope. This module does not
//! implement pairing, member authorization, replay storage or key persistence.

use anyhow::{Result, bail, ensure};
use ring::{
    aead,
    rand::{SecureRandom, SystemRandom},
    signature::{self, KeyPair},
};
use serde::{Deserialize, Serialize};
use std::fmt;

pub const ENVELOPE_VERSION: u32 = 1;
/// Larger snapshots must be split into independently identified chunks.
pub const MAX_PLAINTEXT_BYTES: usize = 1024 * 1024;
pub const MAX_ENVELOPE_BYTES: usize = (MAX_PLAINTEXT_BYTES + 16) * 2 + 16 * 1024;
const MAX_IDENTIFIER_BYTES: usize = 128;
const NONCE_BYTES: usize = 12;
const SIGNATURE_BYTES: usize = 64;
const TAG_BYTES: usize = 16;
const AAD_DOMAIN: &[u8] = b"elsewhen.sync.envelope.aad.v1\0";
const SIGNATURE_DOMAIN: &[u8] = b"elsewhen.sync.envelope.signature.v1\0";

/// Routing metadata is authenticated but not encrypted. Do not put content,
/// Provider names, addresses or credentials in these opaque identifiers.
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

    fn validate(&self) -> Result<()> {
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

    fn authenticated_bytes(&self) -> Result<Vec<u8>> {
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
    key_pair: signature::Ed25519KeyPair,
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

/// Per-recipient content key supplied by the authenticated key-management
/// layer. No serialization or implicit derivation from relay credentials.
pub struct ContentKey([u8; 32]);

impl fmt::Debug for ContentKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ContentKey([REDACTED])")
    }
}

impl ContentKey {
    pub fn generate() -> Result<Self> {
        let mut bytes = [0u8; 32];
        SystemRandom::new()
            .fill(&mut bytes)
            .map_err(|_| anyhow::anyhow!("cannot generate sync content key"))?;
        Ok(Self(bytes))
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Secret: only export for authenticated key distribution/local storage.
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0
    }

    fn aead_key(&self) -> Result<aead::LessSafeKey> {
        let key = aead::UnboundKey::new(&aead::CHACHA20_POLY1305, &self.0)
            .map_err(|_| anyhow::anyhow!("invalid sync content key"))?;
        Ok(aead::LessSafeKey::new(key))
    }
}

/// A signed encrypted envelope. A parsed header is still untrusted until
/// `open` succeeds; callers must not derive trust or execute work from it.
#[derive(Clone)]
pub struct EncryptedEnvelope {
    header: EnvelopeHeader,
    nonce: [u8; NONCE_BYTES],
    ciphertext: Vec<u8>,
    signature: [u8; SIGNATURE_BYTES],
}

impl fmt::Debug for EncryptedEnvelope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EncryptedEnvelope")
            .field("header", &self.header)
            .field("ciphertext_bytes", &self.ciphertext.len())
            .finish_non_exhaustive()
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireEnvelope {
    header: EnvelopeHeader,
    nonce: String,
    ciphertext: String,
    signature: String,
}

impl EncryptedEnvelope {
    /// Unauthenticated metadata, for routing to already-trusted key records.
    pub fn unverified_header(&self) -> &EnvelopeHeader {
        &self.header
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let wire = WireEnvelope {
            header: self.header.clone(),
            nonce: encode_hex(&self.nonce),
            ciphertext: encode_hex(&self.ciphertext),
            signature: encode_hex(&self.signature),
        };
        let bytes = serde_json::to_vec(&wire)?;
        ensure!(bytes.len() <= MAX_ENVELOPE_BYTES, "sync envelope too large");
        Ok(bytes)
    }

    /// Enforces the wire limit before parsing/allocating ciphertext. Parsing is
    /// not authentication; never apply its contents before calling `open`.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len() <= MAX_ENVELOPE_BYTES, "sync envelope too large");
        let wire: WireEnvelope = serde_json::from_slice(bytes)
            .map_err(|_| anyhow::anyhow!("invalid sync envelope encoding"))?;
        wire.header.validate()?;
        let nonce: [u8; NONCE_BYTES] = decode_hex(&wire.nonce, NONCE_BYTES)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid sync nonce length"))?;
        let signature: [u8; SIGNATURE_BYTES] = decode_hex(&wire.signature, SIGNATURE_BYTES)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid sync signature length"))?;
        let ciphertext = decode_hex(&wire.ciphertext, MAX_PLAINTEXT_BYTES + TAG_BYTES)?;
        let envelope = Self {
            header: wire.header,
            nonce,
            ciphertext,
            signature,
        };
        envelope.validate()?;
        Ok(envelope)
    }

    fn validate(&self) -> Result<()> {
        self.header.validate()?;
        ensure!(
            (TAG_BYTES..=MAX_PLAINTEXT_BYTES + TAG_BYTES).contains(&self.ciphertext.len()),
            "invalid sync ciphertext length"
        );
        Ok(())
    }

    fn signed_bytes(&self, aad: &[u8]) -> Vec<u8> {
        let mut bytes = SIGNATURE_DOMAIN.to_vec();
        append_frame(&mut bytes, aad);
        append_frame(&mut bytes, &self.nonce);
        append_frame(&mut bytes, &self.ciphertext);
        bytes
    }
}

/// Seal opaque data, including a Provider bundle, before persistent transport
/// storage. Repeated calls use fresh nonces; delivery retries should retain the
/// original serialized envelope and message identity.
pub fn seal(
    header: EnvelopeHeader,
    key: &ContentKey,
    sender: &DeviceIdentity,
    plaintext: &[u8],
) -> Result<EncryptedEnvelope> {
    let aad = header.authenticated_bytes()?;
    ensure!(
        plaintext.len() <= MAX_PLAINTEXT_BYTES,
        "sync plaintext too large; split into chunks"
    );
    let mut nonce = [0u8; NONCE_BYTES];
    SystemRandom::new()
        .fill(&mut nonce)
        .map_err(|_| anyhow::anyhow!("cannot generate sync envelope nonce"))?;
    let mut ciphertext = plaintext.to_vec();
    key.aead_key()?
        .seal_in_place_append_tag(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(&aad),
            &mut ciphertext,
        )
        .map_err(|_| anyhow::anyhow!("cannot encrypt sync envelope"))?;
    let mut envelope = EncryptedEnvelope {
        header,
        nonce,
        ciphertext,
        signature: [0u8; SIGNATURE_BYTES],
    };
    let signature = sender.key_pair.sign(&envelope.signed_bytes(&aad));
    envelope.signature.copy_from_slice(signature.as_ref());
    Ok(envelope)
}

/// Verify against a caller-supplied trusted identity/context, then decrypt.
/// Successful authentication does not grant replay/execution permission:
/// inbox deduplication, stream ordering and membership checks remain required.
pub fn open(
    envelope: &EncryptedEnvelope,
    expected_header: &EnvelopeHeader,
    key: &ContentKey,
    trusted_sender_public_key: &[u8; 32],
) -> Result<Vec<u8>> {
    envelope.validate()?;
    expected_header.validate()?;
    ensure!(
        &envelope.header == expected_header,
        "sync envelope context mismatch"
    );
    let aad = envelope.header.authenticated_bytes()?;
    signature::UnparsedPublicKey::new(&signature::ED25519, trusted_sender_public_key)
        .verify(&envelope.signed_bytes(&aad), &envelope.signature)
        .map_err(|_| anyhow::anyhow!("sync envelope authentication failed"))?;
    let mut plaintext = envelope.ciphertext.clone();
    let length = key
        .aead_key()?
        .open_in_place(
            aead::Nonce::assume_unique_for_key(envelope.nonce),
            aead::Aad::from(&aad),
            &mut plaintext,
        )
        .map_err(|_| anyhow::anyhow!("sync envelope authentication failed"))?
        .len();
    plaintext.truncate(length);
    Ok(plaintext)
}

fn append_frame(target: &mut Vec<u8>, bytes: &[u8]) {
    target.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    target.extend_from_slice(bytes);
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 15) as usize] as char);
    }
    encoded
}

fn decode_hex(encoded: &str, max_bytes: usize) -> Result<Vec<u8>> {
    ensure!(
        encoded.len().is_multiple_of(2) && encoded.len() / 2 <= max_bytes,
        "invalid sync hex length"
    );
    fn nibble(byte: u8) -> Result<u8> {
        match byte {
            b'0'..=b'9' => Ok(byte - b'0'),
            b'a'..=b'f' => Ok(byte - b'a' + 10),
            _ => bail!("invalid sync hex encoding"),
        }
    }
    encoded
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| Ok((nibble(pair[0])? << 4) | nibble(pair[1])?))
        .collect()
}

#[cfg(test)]
mod vector_tests;

#[cfg(test)]
mod tests {
    use super::*;

    fn header() -> EnvelopeHeader {
        EnvelopeHeader::new("dataset-1", "device-a", "device-b", 1, "stream-a:42").unwrap()
    }

    fn fixture() -> (DeviceIdentity, ContentKey, EncryptedEnvelope) {
        let identity = DeviceIdentity::generate().unwrap();
        let key = ContentKey::generate().unwrap();
        let sealed = seal(header(), &key, &identity, b"synthetic-provider-key").unwrap();
        (identity, key, sealed)
    }

    #[test]
    fn provider_payload_roundtrips_without_plaintext_in_wire() {
        let (identity, key, envelope) = fixture();
        let encoded = envelope.to_bytes().unwrap();
        assert!(!String::from_utf8_lossy(&encoded).contains("synthetic-provider-key"));
        let parsed = EncryptedEnvelope::from_bytes(&encoded).unwrap();
        assert_eq!(
            open(&parsed, &header(), &key, &identity.public_key()).unwrap(),
            b"synthetic-provider-key"
        );
        assert_eq!(encoded, parsed.to_bytes().unwrap());
    }

    #[test]
    fn empty_and_maximum_chunks_roundtrip() {
        let identity = DeviceIdentity::generate().unwrap();
        let key = ContentKey::generate().unwrap();
        for size in [0, MAX_PLAINTEXT_BYTES] {
            let data = vec![71; size];
            let envelope = seal(header(), &key, &identity, &data).unwrap();
            let wire = envelope.to_bytes().unwrap();
            assert!(wire.len() <= MAX_ENVELOPE_BYTES);
            let parsed = EncryptedEnvelope::from_bytes(&wire).unwrap();
            assert_eq!(
                open(&parsed, &header(), &key, &identity.public_key()).unwrap(),
                data
            );
        }
    }

    #[test]
    fn nonce_randomizes_repeated_plaintext() {
        let (identity, key, first) = fixture();
        let second = seal(header(), &key, &identity, b"synthetic-provider-key").unwrap();
        assert_ne!(first.nonce, second.nonce);
        assert_ne!(first.ciphertext, second.ciphertext);
        assert_ne!(first.to_bytes().unwrap(), second.to_bytes().unwrap());
    }

    #[test]
    fn device_identity_roundtrips_and_secrets_have_redacted_debug() {
        let identity = DeviceIdentity::generate().unwrap();
        let restored = DeviceIdentity::from_pkcs8(&identity.to_pkcs8()).unwrap();
        assert_eq!(identity.public_key(), restored.public_key());
        let key = ContentKey::from_bytes([7; 32]);
        assert_eq!(key.to_bytes(), [7; 32]);
        assert_eq!(format!("{identity:?}"), "DeviceIdentity([REDACTED])");
        assert_eq!(format!("{key:?}"), "ContentKey([REDACTED])");
        assert!(DeviceIdentity::from_pkcs8(b"not a secret key").is_err());
        assert!(DeviceIdentity::from_pkcs8(&[0; 1025]).is_err());
    }

    #[test]
    fn wrong_key_and_untrusted_sender_are_rejected() {
        let (identity, key, envelope) = fixture();
        let stranger = DeviceIdentity::generate().unwrap();
        assert!(open(&envelope, &header(), &key, &stranger.public_key()).is_err());
        assert!(
            open(
                &envelope,
                &header(),
                &ContentKey::generate().unwrap(),
                &identity.public_key()
            )
            .is_err()
        );
    }

    #[test]
    fn every_context_field_is_bound() {
        let (identity, key, envelope) = fixture();
        let variants = [
            EnvelopeHeader {
                dataset_id: "dataset-2".into(),
                ..header()
            },
            EnvelopeHeader {
                sender_device_id: "device-c".into(),
                ..header()
            },
            EnvelopeHeader {
                recipient_device_id: "device-c".into(),
                ..header()
            },
            EnvelopeHeader {
                key_version: 2,
                ..header()
            },
            EnvelopeHeader {
                message_id: "stream-a:43".into(),
                ..header()
            },
            EnvelopeHeader {
                protocol_version: 2,
                ..header()
            },
        ];
        for expected in variants {
            assert!(open(&envelope, &expected, &key, &identity.public_key()).is_err());
            let mut modified = envelope.clone();
            modified.header = expected.clone();
            assert!(open(&modified, &expected, &key, &identity.public_key()).is_err());
        }
    }

    #[test]
    fn modified_nonce_ciphertext_or_signature_is_rejected() {
        let (identity, key, envelope) = fixture();
        let mut variants = [envelope.clone(), envelope.clone(), envelope];
        variants[0].nonce[0] ^= 1;
        variants[1].ciphertext[0] ^= 1;
        variants[2].signature[0] ^= 1;
        for modified in variants {
            assert!(open(&modified, &header(), &key, &identity.public_key()).is_err());
        }
    }

    #[test]
    fn shared_content_key_does_not_allow_impersonating_sender() {
        let (identity, key, _) = fixture();
        let stranger = DeviceIdentity::generate().unwrap();
        let forged = seal(header(), &key, &stranger, b"forged configuration").unwrap();
        assert!(open(&forged, &header(), &key, &identity.public_key()).is_err());
    }

    #[test]
    fn invalid_identifiers_versions_and_oversized_data_are_rejected() {
        let identity = DeviceIdentity::generate().unwrap();
        let key = ContentKey::generate().unwrap();
        for id in [
            "".to_string(),
            "x".repeat(MAX_IDENTIFIER_BYTES + 1),
            "bad|id".into(),
            "bad\nid".into(),
            "设备".into(),
        ] {
            assert!(EnvelopeHeader::new(id, "a", "b", 1, "m").is_err());
        }
        assert!(EnvelopeHeader::new("d", "a", "b", 0, "m").is_err());
        assert!(seal(header(), &key, &identity, &vec![0; MAX_PLAINTEXT_BYTES + 1]).is_err());
        assert!(EncryptedEnvelope::from_bytes(&vec![0; MAX_ENVELOPE_BYTES + 1]).is_err());
    }

    #[test]
    fn malformed_wire_is_rejected_without_echoing_payload() {
        let (_, _, envelope) = fixture();
        let bytes = envelope.to_bytes().unwrap();
        for length in [0, 1, bytes.len() - 1] {
            assert!(EncryptedEnvelope::from_bytes(&bytes[..length]).is_err());
        }
        let original: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        for (field, value) in [
            ("nonce", serde_json::json!("00")),
            ("nonce", serde_json::json!("zz".repeat(NONCE_BYTES))),
            ("signature", serde_json::json!("00")),
            ("ciphertext", serde_json::json!("00".repeat(TAG_BYTES - 1))),
            ("ciphertext", serde_json::json!("f")),
            ("unexpected_secret", serde_json::json!("SECRET_SENTINEL")),
        ] {
            let mut wire = original.clone();
            wire[field] = value;
            let error =
                EncryptedEnvelope::from_bytes(&serde_json::to_vec(&wire).unwrap()).unwrap_err();
            assert!(!error.to_string().contains("SECRET_SENTINEL"));
        }
        let mut duplicate = String::from_utf8(bytes).unwrap();
        duplicate.insert_str(1, "\"nonce\":\"000000000000000000000000\",");
        assert!(EncryptedEnvelope::from_bytes(duplicate.as_bytes()).is_err());
    }

    #[test]
    fn header_framing_separates_adjacent_identifiers() {
        let left = EnvelopeHeader::new("ab", "c", "d", 1, "m").unwrap();
        let right = EnvelopeHeader::new("a", "bc", "d", 1, "m").unwrap();
        assert_ne!(
            left.authenticated_bytes().unwrap(),
            right.authenticated_bytes().unwrap()
        );
        assert!(left.authenticated_bytes().unwrap().starts_with(AAD_DOMAIN));
    }
}
