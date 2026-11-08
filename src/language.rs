//! Installation-local display preference, independent of the selected dataset.
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;
use std::sync::RwLock;

pub(crate) mod policy;
pub(crate) mod runtime;

const FILE: &str = "language.json";
static RUNTIME_UI_LOCALE: RwLock<Option<String>> = RwLock::new(None);

#[derive(Serialize, Deserialize)]
struct Preference {
    locale: Option<String>,
}

fn display_locale(locale: &str) -> Result<String> {
    let normalized = crate::storage::prompts::normalize(locale)?;
    match normalized.as_str() {
        "zh-Hans" => Ok(normalized),
        "en" => Ok(normalized),
        _ if normalized.starts_with("en-") => Ok("en".into()),
        _ => anyhow::bail!("ui_language_unsupported"),
    }
}

pub(crate) fn read_ui_preference(root: &Path) -> Result<Option<String>> {
    let bytes = match std::fs::read(root.join(FILE)) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Some("zh-Hans".into())),
        Err(e) => return Err(e).context("ui_language_read_failed"),
    };
    // Invalid or legacy local configuration must preserve the Chinese default.
    Ok(match serde_json::from_slice::<Preference>(&bytes) {
        Ok(Preference { locale: None }) => None,
        Ok(Preference {
            locale: Some(locale),
        }) => Some(display_locale(&locale).unwrap_or_else(|_| "zh-Hans".into())),
        Err(_) => Some("zh-Hans".into()),
    })
}

pub(crate) fn write_ui_preference(root: &Path, locale: Option<&str>) -> Result<Option<String>> {
    let locale = locale.map(display_locale).transpose()?;
    let bytes = serde_json::to_vec(&Preference {
        locale: locale.clone(),
    })?;
    std::fs::create_dir_all(root)?;
    let temporary = root.join(format!(".language-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| -> Result<()> {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, root.join(FILE))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result.context("ui_language_write_failed")?;
    Ok(locale)
}

/// The frontend supplies its resolved display language, including OS changes.
/// This is copied into a bundle at run preparation, never read during that run.
pub(crate) fn set_runtime_ui_locale(locale: &str) -> Result<()> {
    let locale = display_locale(locale)?;
    *RUNTIME_UI_LOCALE.write().unwrap_or_else(|e| e.into_inner()) = Some(locale);
    Ok(())
}

pub(crate) fn runtime_ui_locale() -> String {
    RUNTIME_UI_LOCALE
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_else(|| "zh-Hans".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_preference_survives_reopen_without_becoming_dataset_data() {
        let root = std::env::temp_dir().join(format!("language-{}", uuid::Uuid::new_v4()));
        assert_eq!(
            read_ui_preference(&root).unwrap().as_deref(),
            Some("zh-Hans")
        );
        assert_eq!(
            write_ui_preference(&root, Some("en_GB"))
                .unwrap()
                .as_deref(),
            Some("en")
        );
        assert_eq!(read_ui_preference(&root).unwrap().as_deref(), Some("en"));
        // Dataset selection only changes the database path, not this root file.
        std::fs::create_dir_all(root.join("datasets/another-dataset")).unwrap();
        assert_eq!(read_ui_preference(&root).unwrap().as_deref(), Some("en"));
        write_ui_preference(&root, None).unwrap();
        assert_eq!(read_ui_preference(&root).unwrap(), None);
        assert!(write_ui_preference(&root, Some("fr")).is_err());
        assert_eq!(read_ui_preference(&root).unwrap(), None);
        std::fs::write(root.join(FILE), r#"{"locale":"invalid"}"#).unwrap();
        assert_eq!(
            read_ui_preference(&root).unwrap().as_deref(),
            Some("zh-Hans")
        );
        std::fs::write(root.join(FILE), "broken JSON").unwrap();
        assert_eq!(
            read_ui_preference(&root).unwrap().as_deref(),
            Some("zh-Hans")
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
