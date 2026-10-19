//! crypto 内嵌测试平移。

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
        let error = EncryptedEnvelope::from_bytes(&serde_json::to_vec(&wire).unwrap()).unwrap_err();
        assert!(!error.to_string().contains("SECRET_SENTINEL"));
    }
    let mut duplicate = String::from_utf8(bytes).unwrap();
    duplicate.insert_str(1, "\"nonce\":\"000000000000000000000000\",");
    assert!(EncryptedEnvelope::from_bytes(duplicate.as_bytes()).is_err());
}

#[test]
fn handoff_signature_domain_separates_and_verifies() {
    let identity = DeviceIdentity::generate().unwrap();
    let payload = b"{\"dataset_id\":\"ds\",\"from\":\"dev-a\",\"to\":\"dev-b\"}";
    let signature = identity.sign_handoff(payload).unwrap();
    assert!(verify_handoff_signature(&identity.public_key(), payload, &signature).is_ok());
    let stranger = DeviceIdentity::generate().unwrap();
    assert!(verify_handoff_signature(&stranger.public_key(), payload, &signature).is_err());
    assert!(verify_membership_signature(&identity.public_key(), payload, &signature).is_err());
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
