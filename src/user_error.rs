//! Versioned presentation contract across an anyhow/FRB exception boundary.
//! Codes and bounded parameters describe the domain failure; diagnostics stay
//! in the original error chain and are never translated by string replacement.
use serde::Serialize;

#[derive(Debug, Serialize)]
pub(crate) struct UserError {
    version: u8,
    code: &'static str,
    params: serde_json::Value,
}

impl UserError {
    pub(crate) fn new(code: &'static str, params: serde_json::Value) -> Self {
        Self {
            version: 1,
            code,
            params,
        }
    }

    pub(crate) fn required(field: &'static str) -> Self {
        Self::new("field_required", serde_json::json!({"field":field}))
    }

    pub(crate) fn missing(resource: &'static str) -> Self {
        Self::new("not_found", serde_json::json!({"resource":resource}))
    }
}

impl std::fmt::Display for UserError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "elsewhen-error-v1:{}",
            serde_json::to_string(self).map_err(|_| std::fmt::Error)?
        )
    }
}
impl std::error::Error for UserError {}

/// Only exact machine codes are adapted here. Domain prose and third-party
/// messages keep their diagnostics and receive the localized generic fallback.
pub(crate) fn present(error: anyhow::Error) -> anyhow::Error {
    if error.chain().any(|e| e.is::<UserError>()) {
        return error;
    }
    for cause in error.chain() {
        let code = match cause.to_string().as_str() {
            "language_preferences_conflict" | "knowledge_language_changed" => "settings_changed",
            "knowledge_language_owner_required" => "owner_required",
            "knowledge_language_owner_signer_required" | "sync_runtime_locked" => {
                "sync_unlock_required"
            }
            "knowledge_language_capability_missing"
            | "knowledge_language_capability_stale"
            | "knowledge_language_processor_incompatible" => "language_capability_required",
            "knowledge_language_unsupported" | "ui_language_unsupported" => "language_unsupported",
            _ => continue,
        };
        return error.context(UserError::new(code, serde_json::json!({})));
    }
    error
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn contract_is_versioned_and_does_not_copy_diagnostics_into_parameters() {
        let error =
            anyhow::anyhow!("knowledge_language_changed").context("private diagnostic path");
        let converted = present(error);
        let value: serde_json::Value = serde_json::from_str(
            converted
                .to_string()
                .strip_prefix("elsewhen-error-v1:")
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            value,
            serde_json::json!({"version":1,"code":"settings_changed","params":{}})
        );
        assert!(format!("{converted:#}").contains("private diagnostic path"));
        assert_eq!(
            present(anyhow::anyhow!("unknown third party error")).to_string(),
            "unknown third party error"
        );
    }
}
