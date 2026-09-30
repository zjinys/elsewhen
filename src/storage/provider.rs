//! AI provider 配置存取（从 `Store` 抽出，同 crate `impl Store` 块）。
//!
//! 多配置 + 单激活语义：`is_active` 由 partial unique index 强制最多一条 = 1；
//! `set_active`/`save` 用 IMMEDIATE 事务避免并发下的激活竞态（见 deep review B1）。

use anyhow::Result;
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use uuid::Uuid;

use super::adapter::{AiProviderConfig, AiProviderConfigRow};
use super::Store;

impl Store {
    pub fn upsert_ai_provider_config(
        &self,
        base_url: &str,
        model: &str,
        api_key: &str,
    ) -> Result<()> {
        // 兼容旧接口：以固定名 'default' 保存（若库中无激活配置则该条会自动激活）
        self.save_ai_provider_config(
            None,
            "default",
            "openai-compatible",
            base_url,
            model,
            api_key,
            0.7,
            None,
        )?;
        Ok(())
    }

    /// 列出全部 AI provider 配置（支持多配置，仅一个 is_active=1）
    pub fn list_ai_provider_configs(&self) -> Result<Vec<AiProviderConfigRow>> {
        let mut statement = self.connection.prepare(
        "SELECT id,name,provider_type,base_url,model,api_key_source,is_active,temperature,max_tokens,context_window
         FROM ai_provider_configs ORDER BY created_at ASC, id ASC",
    )?;
        let rows = statement
            .query_map([], |row| {
                Ok(AiProviderConfigRow {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    provider_type: row.get(2)?,
                    base_url: row.get(3)?,
                    model: row.get(4)?,
                    api_key_source: row.get(5)?,
                    is_active: row.get::<_, i64>(6)? != 0,
                    temperature: row.get(7)?,
                    max_tokens: row.get(8)?,
                    context_window: row.get(9)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Full provider configurations for runtime failover, including secrets.
    /// The active provider is returned first, followed by creation order.
    pub fn list_ai_provider_configs_for_runtime(&self) -> Result<Vec<AiProviderConfig>> {
        let mut statement = self.connection.prepare(
            "SELECT id,name,provider_type,base_url,model,api_key_source,
                COALESCE(api_key,''),is_active,temperature,max_tokens,context_window
         FROM ai_provider_configs
         ORDER BY is_active DESC, created_at ASC, id ASC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(AiProviderConfig {
                id: row.get(0)?,
                name: row.get(1)?,
                provider_type: row.get(2)?,
                base_url: row.get(3)?,
                model: row.get(4)?,
                api_key_source: row.get(5)?,
                api_key: row.get(6)?,
                is_active: row.get::<_, i64>(7)? != 0,
                temperature: row.get(8)?,
                max_tokens: row.get(9)?,
                context_window: row.get(10)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// 新增 / 编辑 AI provider 配置。id 为空则新建；
    /// 新建且当前无激活配置时自动激活。api_key 传空串表示保留已有 key 不变。
    pub fn save_ai_provider_config(
        &self,
        id: Option<&str>,
        name: &str,
        provider_type: &str,
        base_url: &str,
        model: &str,
        api_key: &str,
        temperature: f64,
        max_tokens: Option<i64>,
    ) -> Result<String> {
        let now = chrono::Utc::now().to_rfc3339();
        if let Some(pid) = id {
            if !pid.is_empty() {
                let changed = self.connection.execute(
                    "UPDATE ai_provider_configs SET
                   name=?1, provider_type=?2, base_url=?3, model=?4,
                   api_key_source='database',
                   api_key=CASE WHEN ?5='' THEN api_key ELSE ?5 END,
                   temperature=?6, max_tokens=?7, updated_at=?8
                 WHERE id=?9",
                    params![
                        name,
                        provider_type,
                        base_url,
                        model,
                        api_key,
                        temperature,
                        max_tokens,
                        now,
                        pid
                    ],
                )?;
                if changed == 0 {
                    anyhow::bail!("未找到要更新的配置（id={pid}）");
                }
                return Ok(pid.to_string());
            }
        }
        // 新建：若库中尚无激活配置，则自动激活（保证始终存在激活项）。
        // count 与 insert 必须同属一个写事务：两个并发「首次新建」若各自
        // 先查后写，会同时算出 has_active=0 并双双写入 is_active=1（P2-2）。
        let transaction = if self.connection.is_autocommit() {
            Some(Transaction::new_unchecked(
                &self.connection,
                TransactionBehavior::Immediate,
            )?)
        } else {
            None
        };
        let has_active: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM ai_provider_configs WHERE is_active=1",
            [],
            |r| r.get(0),
        )?;
        let new_id = Uuid::new_v4().to_string();
        let new_active = if has_active == 0 { 1 } else { 0 };
        self.connection
        .execute(
            "INSERT INTO ai_provider_configs
             (id,name,provider_type,base_url,model,api_key_source,api_key,is_active,temperature,max_tokens,created_at,updated_at)
             VALUES (?1,?2,?3,?4,?5,'database',?6,?7,?8,?9,?10,?10)",
            params![new_id, name, provider_type, base_url, model, api_key, new_active, temperature, max_tokens, now],
        )
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("UNIQUE constraint failed: ai_provider_configs.name") {
                anyhow::anyhow!("配置名「{name}」已存在，请换一个名称")
            } else if msg.contains("UNIQUE constraint failed: ai_provider_configs.is_active") {
                // IMMEDIATE 串行化后正常极难触发，保留友好映射兜底
                anyhow::anyhow!("已有激活配置，新建配置默认停用，可稍后手动激活")
            } else {
                anyhow::anyhow!("{e}")
            }
        })?;
        if let Some(transaction) = transaction {
            transaction.commit()?;
        }
        Ok(new_id)
    }

    /// 把指定配置设为激活（其余全部取消激活），保证有且仅有一个激活项
    pub fn set_active_ai_provider_config(&self, id: &str) -> Result<()> {
        // IMMEDIATE：先置全零再点亮目标行，两步写必须在拿到写锁后一次性完成，
        // 避免 DEFERRED 在 WAL 并发下升级锁时撞上 worker 的写事务报 database is locked。
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        transaction.execute(
            "UPDATE ai_provider_configs SET is_active=0, updated_at=?1",
            params![chrono::Utc::now().to_rfc3339()],
        )?;
        let changed = transaction.execute(
            "UPDATE ai_provider_configs SET is_active=1, updated_at=?1 WHERE id=?2",
            params![chrono::Utc::now().to_rfc3339(), id],
        )?;
        if changed == 0 {
            transaction.rollback()?;
            anyhow::bail!("未找到要激活的配置（id={id}）");
        }
        transaction.commit()?;
        Ok(())
    }

    /// 删除一条配置；若删除的恰是激活项，则自动把剩余第一条配置激活
    pub fn delete_ai_provider_config(&self, id: &str) -> Result<()> {
        let transaction = self.connection.unchecked_transaction()?;
        let was_active: i64 = transaction.query_row(
            "SELECT COUNT(*) FROM ai_provider_configs WHERE id=?1 AND is_active=1",
            params![id],
            |r| r.get(0),
        )?;
        let deleted =
            transaction.execute("DELETE FROM ai_provider_configs WHERE id=?1", params![id])?;
        if deleted == 0 {
            transaction.rollback()?;
            anyhow::bail!("未找到要删除的配置（id={id}）");
        }
        if was_active > 0 {
            transaction.execute(
                "UPDATE ai_provider_configs SET is_active=1, updated_at=?1
             WHERE id=(SELECT id FROM ai_provider_configs ORDER BY created_at ASC, id ASC LIMIT 1)",
                params![chrono::Utc::now().to_rfc3339()],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn active_ai_provider_config(&self) -> Result<Option<AiProviderConfig>> {
        self.connection
        .query_row(
            "SELECT id,name,provider_type,base_url,model,api_key_source,api_key,is_active,temperature,max_tokens,context_window
             FROM ai_provider_configs
             WHERE is_active=1 AND api_key IS NOT NULL LIMIT 1",
            [],
            |row| {
                Ok(AiProviderConfig {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    provider_type: row.get(2)?,
                    base_url: row.get(3)?,
                    model: row.get(4)?,
                    api_key_source: row.get(5)?,
                    api_key: row.get(6)?,
                    is_active: row.get::<_, i64>(7)? != 0,
                    temperature: row.get(8)?,
                    max_tokens: row.get(9)?,
                    context_window: row.get(10)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
    }
}
