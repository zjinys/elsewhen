//! Credential administration; no plaintext in list DTOs.
use anyhow::Result;
#[derive(Clone)]
pub struct NamedSecretDto {
    pub name: String,
    pub title: String,
    pub references: Vec<String>,
}
fn store() -> Result<crate::storage::Store> {
    crate::storage::Store::open(&crate::config::AppConfig::load()?.database_path)
}
fn invalidate(store: &crate::storage::Store) -> Result<()> {
    for server in store.list_mcp_servers()? {
        crate::ai::mcp::invalidate_server(&server.id);
    }
    Ok(())
}
pub fn list_named_secrets() -> Result<Vec<NamedSecretDto>> {
    Ok(store()?
        .list_named_secrets()?
        .into_iter()
        .map(|s| NamedSecretDto {
            name: s.name,
            title: s.title,
            references: s.references,
        })
        .collect())
}
pub fn save_named_secret(name: String, title: String, value: String) -> Result<()> {
    anyhow::ensure!(
        !title.trim().is_empty() && title.chars().count() <= 128,
        "名称必填，最多128字"
    );
    let s = store()?;
    let tx = s.begin_write_transaction()?;
    anyhow::ensure!(
        !s.list_named_secrets()?.iter().any(|item| item.name == name),
        "该代号已存在，请使用其他代号"
    );
    s.save_named_secret(&name, title.trim(), &value)?;
    if let Some(tx) = tx {
        tx.commit()?;
    }
    invalidate(&s)
}
pub fn edit_named_secret(name: String, title: String, value: Option<String>) -> Result<()> {
    let s = store()?;
    s.edit_named_secret(&name, &title, value.as_deref())?;
    invalidate(&s)
}
pub fn delete_named_secret(name: String, force: bool) -> Result<()> {
    let s = store()?;
    s.delete_named_secret(&name, force)?;
    invalidate(&s)
}
/// Explicit extraction replaces exact matching JSON values only, in one transaction.
pub fn extract_named_secret(name: String, title: String, value: String) -> Result<u32> {
    let s = store()?;
    anyhow::ensure!(
        !title.trim().is_empty() && title.chars().count() <= 128,
        "名称必填，最多128字"
    );
    let count = extract_into(&s, &name, title.trim(), &value)?;
    invalidate(&s)?;
    Ok(count)
}
fn extract_into(s: &crate::storage::Store, name: &str, title: &str, value: &str) -> Result<u32> {
    anyhow::ensure!(!value.contains("{{secret:"), "不能提取已有引用");
    anyhow::ensure!(
        !s.list_named_secrets()?.iter().any(|item| item.name == name),
        "该密钥代号已存在，请使用新代号提取"
    );
    let tx = s.connection.unchecked_transaction()?;
    s.save_named_secret(&name, &title, &value)?;
    let replacement = format!("{{{{secret:{name}}}}}");
    let mut count = 0;
    for server in s.list_mcp_servers()? {
        for (column, raw) in [
            ("env_json", server.env_json),
            ("headers_json", server.headers_json),
        ] {
            let mut map: std::collections::HashMap<String, String> = serde_json::from_str(&raw)?;
            let mut changed = false;
            for v in map.values_mut() {
                if *v == value {
                    *v = replacement.clone();
                    count += 1;
                    changed = true;
                }
            }
            if changed {
                s.connection.execute(
                    &format!("UPDATE mcp_servers SET {column}=?1 WHERE id=?2"),
                    rusqlite::params![serde_json::to_string(&map)?, server.id],
                )?;
            }
        }
    }
    count += s.connection.execute(
        "UPDATE web_search_configs SET api_key=?1 WHERE api_key=?2",
        rusqlite::params![replacement, value],
    )? as u32;
    tx.commit()?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn named_secret_extraction_replaces_exact_values_and_rejects_overwrite() {
        let s = crate::storage::Store::open(
            &std::env::temp_dir().join(format!("secret-extract-{}.db", uuid::Uuid::new_v4())),
        )
        .unwrap();
        let id = s
            .save_mcp_server(
                None,
                "example",
                "",
                "all",
                "stdio",
                "node",
                "[]",
                r#"{"TOKEN":"original","OTHER":"keep"}"#,
                "",
                "{}",
                true,
                false,
            )
            .unwrap();
        s.web_search_config().unwrap();
        s.connection
            .execute("UPDATE web_search_configs SET api_key='original'", [])
            .unwrap();
        assert_eq!(extract_into(&s, "shared", "Shared", "original").unwrap(), 2);
        let server = s.get_mcp_server(&id).unwrap().unwrap();
        assert!(server.env_json.contains("{{secret:shared}}"));
        assert!(server.env_json.contains("keep"));
        assert_eq!(s.read_named_secret("shared").unwrap(), "original");
        assert_eq!(s.web_search_config().unwrap().api_key, "{{secret:shared}}");
        assert!(extract_into(&s, "shared", "", "different").is_err());
        assert_eq!(s.read_named_secret("shared").unwrap(), "original");
    }
}
