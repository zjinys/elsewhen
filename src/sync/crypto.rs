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
mod envelope;
mod identity;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod vector_tests;

pub use envelope::{ContentKey, EncryptedEnvelope, open, seal};
#[cfg(test)]
pub(crate) use envelope::{WireEnvelope, decode_hex, encode_hex};
pub use identity::{DeviceIdentity, EnvelopeHeader};
pub(crate) use identity::{
    verify_handoff_signature, verify_membership_signature, verify_pairing_identity,
};
