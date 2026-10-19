use super::*;

#[test]
fn evidence_sanitization_removes_secrets_and_credentials() {
    let sanitized = sanitize_evidence(
        "API_KEY=super-secret\npassword: hunter2\nlet normal = true;\nAuthorization: Bearer abc",
    );
    assert!(!sanitized.contains("super-secret"));
    assert!(!sanitized.contains("hunter2"));
    assert!(!sanitized.contains("Bearer abc"));
    assert!(sanitized.contains("let normal = true;"));
    assert!(is_sensitive_evidence_path("credentials.json"));
    assert!(is_sensitive_evidence_path("signing.p12"));
    assert!(!is_sensitive_evidence_path("readme.md"));
}
