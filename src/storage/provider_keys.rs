//! Provider secrets are sealed before SQL binding on protected installations.
//! Public configuration metadata remains queryable while the native key is locked.

use super::Store;
use anyhow::{Context, Result, ensure};
use ring::{
    aead,
    rand::{SecureRandom, SystemRandom},
};
use rusqlite::{OptionalExtension, params};

const MAX_KEY_BYTES: usize = 64 * 1024;

impl Store {
    /// Explicit capability for a coordinator-owned staging/read-view connection.
    /// This key never enters a content snapshot, DTO or stored configuration.
    pub(crate) fn with_provider_master(mut self, master: [u8; 32]) -> Self {
        self.provider_master = Some(master);
        self
    }
    pub(crate) fn provider_keys_sealed(&self) -> Result<bool> {
        Ok(self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM provider_key_policy)",
            [],
            |r| r.get(0),
        )?)
    }
    fn provider_master_key(&self) -> Result<[u8; 32]> {
        if let Some(key) = self.provider_master {
            return Ok(key);
        }
        let revision: i64 = self.connection.query_row(
            "SELECT revision FROM sync_local_vault WHERE singleton=1",
            [],
            |r| r.get(0),
        )?;
        crate::sync::runtime::provider_master(&self.path, revision)
            .context("provider_key_locked（请先在设置→数据中解锁本机同步密钥）")
    }
    fn provider_key_context(&self, id: &str) -> Result<Vec<u8>> {
        let (url, kind): (String, String) = self.connection.query_row(
            "SELECT base_url,provider_type FROM ai_provider_configs WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        ensure!(
            url.len() <= 16 * 1024 && kind.len() <= 256,
            "provider_key_context_too_large"
        );
        let identity = self
            .sync_identity()?
            .context("provider_key_identity_missing")?;
        let installation: String = self.connection.query_row(
            "SELECT installation_id FROM sync_local_vault WHERE singleton=1",
            [],
            |r| r.get(0),
        )?;
        Ok(serde_json::to_vec(&(
            "elsewhen.provider-key.v1",
            identity.dataset_id,
            installation,
            id,
            url,
            kind,
        ))?)
    }
    pub(crate) fn read_provider_key(&self, id: &str) -> Result<Option<String>> {
        let plaintext: Option<String> = self.connection.query_row(
            "SELECT api_key FROM ai_provider_configs WHERE id=?1",
            [id],
            |r| r.get(0),
        )?;
        let ciphertext: Option<Vec<u8>> = self
            .connection
            .query_row(
                "SELECT ciphertext FROM provider_key_secrets WHERE config_id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(bytes) = ciphertext {
            ensure!(plaintext.is_none(), "provider_key_storage_inconsistent");
            let key = self.provider_master_key()?;
            return Ok(Some(open(&key, &self.provider_key_context(id)?, &bytes)?));
        }
        ensure!(
            !self.provider_keys_sealed()? || plaintext.is_none(),
            "provider_key_storage_inconsistent"
        );
        Ok(plaintext)
    }
    fn seal_provider_key_on(&self, id: &str, value: &str) -> Result<()> {
        ensure!(
            !self.connection.is_autocommit(),
            "provider_key_requires_transaction"
        );
        let master = self.provider_master_key()?;
        let bytes = seal(&master, &self.provider_key_context(id)?, value)?;
        self.connection.execute(
            "UPDATE ai_provider_configs SET api_key=NULL WHERE id=?1",
            [id],
        )?;
        self.connection.execute("INSERT INTO provider_key_secrets(config_id,ciphertext) VALUES(?1,?2) ON CONFLICT(config_id) DO UPDATE SET ciphertext=excluded.ciphertext",params![id,bytes])?;
        Ok(())
    }
    pub(crate) fn write_provider_key_on(&self, id: &str, value: Option<&str>) -> Result<()> {
        ensure!(
            !self.connection.is_autocommit(),
            "provider_key_requires_transaction"
        );
        if self.provider_keys_sealed()?
            && let Some(value) = value
        {
            return self.seal_provider_key_on(id, value);
        }
        self.connection
            .execute("DELETE FROM provider_key_secrets WHERE config_id=?1", [id])?;
        self.connection.execute(
            "UPDATE ai_provider_configs SET api_key=?2 WHERE id=?1",
            params![id, value],
        )?;
        Ok(())
    }
    pub(crate) fn enable_provider_key_sealing(&self) -> Result<()> {
        let tx = self.begin_write_transaction()?;
        let master = self.provider_master_key()?;
        let installation: String = self.connection.query_row(
            "SELECT installation_id FROM sync_local_vault WHERE singleton=1",
            [],
            |r| r.get(0),
        )?;
        let vault = self.load_sync_vault(&installation, &master, 1)?;
        self.require_sync_signer(vault.identity())?;
        if !self.provider_keys_sealed()? {
            let oversized:bool=self.connection.query_row("SELECT EXISTS(SELECT 1 FROM ai_provider_configs WHERE length(CAST(api_key AS BLOB))>?1)",[MAX_KEY_BYTES as i64],|r|r.get(0))?;
            ensure!(!oversized, "provider_key_too_large");
            let mut statement = self.connection.prepare(
                "SELECT id,api_key FROM ai_provider_configs WHERE api_key IS NOT NULL LIMIT 65",
            )?;
            let rows = statement
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            ensure!(rows.len() <= 64, "provider_key_profile_limit");
            for (id, value) in rows {
                self.seal_provider_key_on(&id, &value)?;
            }
            self.connection
                .execute("INSERT INTO provider_key_policy(singleton) VALUES(1)", [])?;
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }
}

/// 掩码后的 API Key：UTF-8 char 边界安全。仅用于冲突审阅展示，
/// 绝不进入 DTO 之外的存储、日志或诊断字符串。
pub(crate) fn mask_api_key(key: &str) -> String {
    if key.is_empty() {
        return String::new();
    }
    let count = key.chars().count();
    if count <= 8 {
        "*".repeat(count)
    } else {
        let prefix: String = key.chars().take(3).collect();
        let suffix: String = {
            let mut rev: Vec<char> = key.chars().rev().take(4).collect();
            rev.reverse();
            rev.into_iter().collect()
        };
        format!("{prefix}****{suffix}")
    }
}

fn cipher(key: &[u8; 32]) -> Result<aead::LessSafeKey> {
    Ok(aead::LessSafeKey::new(
        aead::UnboundKey::new(&aead::CHACHA20_POLY1305, key)
            .map_err(|_| anyhow::anyhow!("provider_key_invalid"))?,
    ))
}
fn seal(key: &[u8; 32], context: &[u8], value: &str) -> Result<Vec<u8>> {
    ensure!(value.len() <= MAX_KEY_BYTES, "provider_key_too_large");
    let mut nonce = [0; 12];
    SystemRandom::new()
        .fill(&mut nonce)
        .map_err(|_| anyhow::anyhow!("provider_key_random_failed"))?;
    let mut body = value.as_bytes().to_vec();
    cipher(key)?
        .seal_in_place_append_tag(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(context),
            &mut body,
        )
        .map_err(|_| anyhow::anyhow!("provider_key_seal_failed"))?;
    let mut bytes = vec![1];
    bytes.extend(nonce);
    bytes.extend(body);
    Ok(bytes)
}
fn open(key: &[u8; 32], context: &[u8], bytes: &[u8]) -> Result<String> {
    ensure!(
        (29..=MAX_KEY_BYTES + 29).contains(&bytes.len()) && bytes[0] == 1,
        "provider_key_encoding_invalid"
    );
    let mut body = bytes[13..].to_vec();
    let plaintext = cipher(key)?
        .open_in_place(
            aead::Nonce::try_assume_unique_for_key(&bytes[1..13])
                .map_err(|_| anyhow::anyhow!("provider_key_encoding_invalid"))?,
            aead::Aad::from(context),
            &mut body,
        )
        .map_err(|_| anyhow::anyhow!("provider_key_authentication_failed"))?;
    Ok(std::str::from_utf8(plaintext)
        .map_err(|_| anyhow::anyhow!("provider_key_encoding_invalid"))?
        .to_owned())
}

#[cfg(test)]
mod tests;
