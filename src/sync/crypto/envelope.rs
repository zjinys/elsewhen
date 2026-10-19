//! 内容加密信封：ContentKey、EncryptedEnvelope wire 格式与 seal/open。

use super::*;

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

    pub(crate) fn aead_key(&self) -> Result<aead::LessSafeKey> {
        let key = aead::UnboundKey::new(&aead::CHACHA20_POLY1305, &self.0)
            .map_err(|_| anyhow::anyhow!("invalid sync content key"))?;
        Ok(aead::LessSafeKey::new(key))
    }
}

/// A signed encrypted envelope. A parsed header is still untrusted until
/// `open` succeeds; callers must not derive trust or execute work from it.
#[derive(Clone)]
pub struct EncryptedEnvelope {
    pub(crate) header: EnvelopeHeader,
    pub(crate) nonce: [u8; NONCE_BYTES],
    pub(crate) ciphertext: Vec<u8>,
    pub(crate) signature: [u8; SIGNATURE_BYTES],
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
pub(crate) struct WireEnvelope {
    pub(crate) header: EnvelopeHeader,
    pub(crate) nonce: String,
    pub(crate) ciphertext: String,
    pub(crate) signature: String,
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

    pub(crate) fn validate(&self) -> Result<()> {
        self.header.validate()?;
        ensure!(
            (TAG_BYTES..=MAX_PLAINTEXT_BYTES + TAG_BYTES).contains(&self.ciphertext.len()),
            "invalid sync ciphertext length"
        );
        Ok(())
    }

    pub(crate) fn signed_bytes(&self, aad: &[u8]) -> Vec<u8> {
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

pub(crate) fn append_frame(target: &mut Vec<u8>, bytes: &[u8]) {
    target.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    target.extend_from_slice(bytes);
}

pub(crate) fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 15) as usize] as char);
    }
    encoded
}

pub(crate) fn decode_hex(encoded: &str, max_bytes: usize) -> Result<Vec<u8>> {
    ensure!(
        encoded.len().is_multiple_of(2) && encoded.len() / 2 <= max_bytes,
        "invalid sync hex length"
    );
    pub(crate) fn nibble(byte: u8) -> Result<u8> {
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
