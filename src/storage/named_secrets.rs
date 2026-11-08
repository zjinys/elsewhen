//! Local credential definitions; list operations never return secret values.
use super::Store;
use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, params};

#[derive(Clone, Debug)]
pub struct NamedSecret {
    pub name: String,
    pub title: String,
    pub references: Vec<String>,
}
fn valid(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
impl Store {
    fn secret_context(&self, id: &str, name: &str) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(&(
            "elsewhen.named-secret.v1",
            self.sync_identity()?
                .context("secret_identity_missing")?
                .dataset_id,
            self.local_vault_installation()?,
            id,
            name,
        ))?)
    }
    pub fn save_named_secret(&self, name: &str, title: &str, value: &str) -> Result<()> {
        ensure!(
            valid(name),
            "密钥代号只能包含字母、数字、下划线和短横线（最多128字符）"
        );
        ensure!(
            !value.is_empty() && value.len() <= 65536,
            "密钥不能为空且不能超过64KB"
        );
        let tx = self.begin_write_transaction()?;
        let id: String = self
            .connection
            .query_row("SELECT id FROM named_secrets WHERE name=?1", [name], |r| {
                r.get(0)
            })
            .optional()?
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let cipher = if self.provider_keys_sealed()? {
            Some(super::provider_keys::seal(
                &self.provider_master_key()?,
                &self.secret_context(&id, name)?,
                value,
            )?)
        } else {
            None
        };
        self.connection.execute("INSERT INTO named_secrets(id,name,title,ciphertext,plaintext,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,datetime('now'),datetime('now')) ON CONFLICT(name) DO UPDATE SET title=excluded.title,ciphertext=excluded.ciphertext,plaintext=excluded.plaintext,updated_at=excluded.updated_at", params![id,name,title,cipher,if cipher.is_none(){Some(value)}else{None}])?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }
    /// Metadata-only edits must work without decrypting/replacing the existing value.
    pub fn edit_named_secret(&self, name: &str, title: &str, value: Option<&str>) -> Result<()> {
        ensure!(
            !title.trim().is_empty() && title.chars().count() <= 128,
            "名称必填，最多128字"
        );
        let tx = self.begin_write_transaction()?;
        let exists: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM named_secrets WHERE name=?1)",
            [name],
            |r| r.get(0),
        )?;
        ensure!(exists, "密钥不存在，请刷新列表");
        if let Some(value) = value {
            self.save_named_secret(name, title.trim(), value)?;
        } else {
            self.connection.execute(
                "UPDATE named_secrets SET title=?2,updated_at=datetime('now') WHERE name=?1",
                params![name, title.trim()],
            )?;
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }
    pub fn read_named_secret(&self, name: &str) -> Result<String> {
        let (id, cipher, plain): (String, Option<Vec<u8>>, Option<String>) = self
            .connection
            .query_row(
                "SELECT id,ciphertext,plaintext FROM named_secrets WHERE name=?1",
                [name],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?
            .with_context(|| format!("未找到密钥 {name}，请在设置→密钥管理中配置"))?;
        match cipher {
            Some(bytes) => super::provider_keys::open(
                &self.provider_master_key()?,
                &self.secret_context(&id, name)?,
                &bytes,
            ),
            None => plain.context("secret_value_missing"),
        }
    }
    pub(super) fn seal_named_secrets(&self) -> Result<()> {
        let rows = {
            let mut q = self.connection.prepare(
                "SELECT name,title,plaintext FROM named_secrets WHERE plaintext IS NOT NULL",
            )?;
            q.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
        };
        for (name, title, value) in rows {
            self.save_named_secret(&name, &title, &value)?;
        }
        Ok(())
    }
    pub fn named_secret_references(&self, name: &str) -> Result<Vec<String>> {
        let needle = format!("{{{{secret:{name}}}}}");
        let mut refs = Vec::new();
        for s in self.list_mcp_servers()? {
            if s.env_json.contains(&needle) || s.headers_json.contains(&needle) {
                refs.push(format!("MCP: {}", s.name));
            }
        }
        let mut q = self
            .connection
            .prepare("SELECT id,name,api_key_source FROM ai_provider_configs")?;
        let providers = q
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (id, title, source) in providers {
            if source == format!("named_secret:{name}")
                || self
                    .read_provider_key(&id)
                    .ok()
                    .flatten()
                    .is_some_and(|v| v.contains(&needle))
            {
                refs.push(format!("AI Provider: {title}"));
            }
        }
        if self.web_search_config()?.api_key.contains(&needle) {
            refs.push("外网搜索".into());
        }
        Ok(refs)
    }
    pub fn list_named_secrets(&self) -> Result<Vec<NamedSecret>> {
        let mut q = self
            .connection
            .prepare("SELECT name,title FROM named_secrets ORDER BY name")?;
        let rows = q
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|(name, title)| {
                Ok(NamedSecret {
                    references: self.named_secret_references(&name)?,
                    name,
                    title,
                })
            })
            .collect()
    }
    pub fn delete_named_secret(&self, name: &str, force: bool) -> Result<()> {
        ensure!(
            force || self.named_secret_references(name)?.is_empty(),
            "密钥仍被引用，请确认删除"
        );
        self.connection
            .execute("DELETE FROM named_secrets WHERE name=?1", [name])?;
        Ok(())
    }
    pub fn resolve_secret_placeholders(&self, raw: &str) -> Result<String> {
        let mut tail = raw;
        let mut out = String::new();
        while let Some(i) = tail.find("{{secret:") {
            out.push_str(&tail[..i]);
            tail = &tail[i + 9..];
            let end = tail.find("}}").context("密钥引用缺少结束括号")?;
            let name = &tail[..end];
            ensure!(valid(name), "密钥引用代号无效");
            out.push_str(&self.read_named_secret(name)?);
            tail = &tail[end + 2..];
        }
        out.push_str(tail);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn store() -> Store {
        Store::open(
            &std::env::temp_dir().join(format!("named-secrets-{}.db", uuid::Uuid::new_v4())),
        )
        .unwrap()
    }
    #[test]
    fn edit_secret_metadata_preserves_value_and_requires_title() {
        let s = store();
        s.save_named_secret("stable_code", "Old", "original")
            .unwrap();
        s.edit_named_secret("stable_code", "New", None).unwrap();
        assert_eq!(s.read_named_secret("stable_code").unwrap(), "original");
        assert_eq!(s.list_named_secrets().unwrap()[0].title, "New");
        assert!(s.edit_named_secret("stable_code", " ", None).is_err());
        s.edit_named_secret("stable_code", "New", Some("rotated"))
            .unwrap();
        assert_eq!(s.read_named_secret("stable_code").unwrap(), "rotated");
        assert!(s.edit_named_secret("missing", "New", None).is_err());
    }
    #[test]
    fn named_secret_crud_resolution_and_rotation() {
        let s = store();
        s.save_named_secret("shared", "共享", "abc").unwrap();
        assert_eq!(
            s.resolve_secret_placeholders("Bearer {{secret:shared}}/{{secret:shared}}!")
                .unwrap(),
            "Bearer abc/abc!"
        );
        s.save_named_secret("shared", "更新", "xyz").unwrap();
        assert_eq!(s.read_named_secret("shared").unwrap(), "xyz");
        assert_eq!(s.list_named_secrets().unwrap().len(), 1);
        s.delete_named_secret("shared", false).unwrap();
        assert!(s.resolve_secret_placeholders("{{secret:shared}}").is_err());
    }
    #[test]
    fn named_secret_invalid_references_fail_closed() {
        let s = store();
        for raw in [
            "{{secret:missing}}",
            "{{secret:bad name}}",
            "{{secret:unterminated",
        ] {
            assert!(s.resolve_secret_placeholders(raw).is_err());
        }
        assert!(s.save_named_secret("bad name", "", "abc").is_err());
        assert!(s.save_named_secret("ok", "", "").is_err());
        assert_eq!(s.resolve_secret_placeholders("plain").unwrap(), "plain");
    }
    #[test]
    fn named_secret_reference_delete_guard() {
        let s = store();
        s.save_named_secret("shared", "", "abc").unwrap();
        s.web_search_config().unwrap();
        s.connection
            .execute(
                "UPDATE web_search_configs SET api_key='{{secret:shared}}'",
                [],
            )
            .unwrap();
        assert_eq!(
            s.named_secret_references("shared").unwrap(),
            vec!["外网搜索"]
        );
        assert!(s.delete_named_secret("shared", false).is_err());
        s.delete_named_secret("shared", true).unwrap();
    }
    #[test]
    fn named_secret_sealed_locked_and_aad_bound() {
        let s = store().with_provider_master([7; 32]);
        let vault = s
            .create_sync_vault("installation", &[7; 32], "device", "test")
            .unwrap();
        let device = vault.device();
        s.initialize_sync_replica(&crate::storage::sync::ReplicaIdentity {
            dataset_id: "secret-test".into(),
            device_id: device.device_id.clone(),
            stream_id: device.stream_id.clone(),
            processor_device_id: device.device_id.clone(),
        })
        .unwrap();
        s.connection
            .execute("INSERT INTO provider_key_policy(singleton) VALUES(1)", [])
            .unwrap();
        s.save_named_secret("sealed", "", "sensitive-value")
            .unwrap();
        assert_eq!(s.read_named_secret("sealed").unwrap(), "sensitive-value");
        let plain: Option<String> = s
            .connection
            .query_row("SELECT plaintext FROM named_secrets", [], |r| r.get(0))
            .unwrap();
        assert!(plain.is_none());
        assert!(
            Store::open(&s.path)
                .unwrap()
                .read_named_secret("sealed")
                .is_err()
        );
        s.connection
            .execute("UPDATE named_secrets SET name='renamed'", [])
            .unwrap();
        assert!(s.read_named_secret("renamed").is_err());
    }
}
