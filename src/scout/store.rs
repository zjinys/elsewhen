//! Scout findings 本地镜像（`scout_findings_mirror` / `scout_settings`，迁移 85）。
//!
//! 服务端（CF Workers D1）是权威数据源；两张表都是本机派生物，不进同步
//! 快照，可随时清空重建。唯一归本地所有的状态是 `read_at`（已读态）与
//! `last_polled_at`（增量拉取水位）。
//!
//! 幂等：`upsert_findings` 以服务端 findings.id 为主键，重复拉到同一 id
//! 时只补齐 payload，**不覆盖 read_at**——已读态是用户的操作痕迹。

use crate::storage::Store;
use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, params};

/// scout 连接设置（单行表，行不存在 = 未配置）。
#[derive(Clone, Debug)]
pub struct ScoutSettings {
    pub base_url: String,
    /// 命名密钥代号（明文 token 存 named_secrets，此处只存引用）。
    /// None = 服务未开鉴权（本地/内网部署）。
    pub token_secret: Option<String>,
    pub poll_interval_minutes: i64,
    pub enabled: bool,
    pub last_polled_at: Option<String>,
}

/// 镜像里的一条 finding（UI 消费形状；payload 保持 JSON 字符串，
/// 由 FRB 层解析成结构化 DTO，避免本层依赖 payload 的内部形状）。
#[derive(Clone, Debug)]
pub struct ScoutFindingRow {
    pub id: i64,
    pub source_id: String,
    pub snapshot_id: i64,
    pub kind: String,
    pub item_key: String,
    pub payload_json: String,
    /// 字段级变化（仅 changed）；None = 无明细（new/gone 或存量数据）。
    pub delta_json: Option<String>,
    pub detected_at: String,
    pub read_at: Option<String>,
}

/// 按源聚合的镜像摘要（发现页分组卡 / 总览卡共用）。
/// 计数含已读，`unread` 单独给；`latest_detected_at` 是最后一次发现时间。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScoutSourceSummary {
    pub source_id: String,
    pub unread: i64,
    pub new_count: i64,
    pub changed_count: i64,
    pub gone_count: i64,
    pub total: i64,
    pub latest_detected_at: Option<String>,
}

impl Store {
    /// 读取 scout 设置；未配置返回 Ok(None)。
    pub fn scout_settings(&self) -> Result<Option<ScoutSettings>> {
        self.connection
            .query_row(
                "SELECT base_url, token_secret, poll_interval_minutes, enabled, last_polled_at
                 FROM scout_settings WHERE id = 1",
                [],
                |r| {
                    Ok(ScoutSettings {
                        base_url: r.get(0)?,
                        token_secret: r.get(1)?,
                        poll_interval_minutes: r.get(2)?,
                        enabled: r.get::<_, i64>(3)? != 0,
                        last_polled_at: r.get(4)?,
                    })
                },
            )
            .optional()
            .context("read scout_settings")
    }

    /// 保存 scout 设置（upsert 单行）。token_secret 只存命名密钥**代号**，
    /// 调用方须先把明文 token 存进 named_secrets（API 层负责）。
    pub fn save_scout_settings(&self, settings: &ScoutSettings) -> Result<()> {
        let base =
            reqwest::Url::parse(&settings.base_url).context("scout base_url 不是合法 URL")?;
        ensure!(
            base.host_str().is_some() && base.query().is_none() && base.fragment().is_none(),
            "scout base_url 不能包含查询串或片段"
        );
        ensure!(settings.poll_interval_minutes >= 5, "拉取间隔最少 5 分钟");
        if let Some(secret) = &settings.token_secret {
            ensure!(
                !secret.trim().is_empty() && secret.trim().len() <= 128,
                "密钥代号无效"
            );
        }
        self.connection.execute(
            "INSERT INTO scout_settings(id, base_url, token_secret, poll_interval_minutes, enabled, last_polled_at, updated_at)
             VALUES(1, ?1, ?2, ?3, ?4, ?5, datetime('now'))
             ON CONFLICT(id) DO UPDATE SET
                base_url=excluded.base_url,
                token_secret=excluded.token_secret,
                poll_interval_minutes=excluded.poll_interval_minutes,
                enabled=excluded.enabled,
                last_polled_at=excluded.last_polled_at,
                updated_at=datetime('now')",
            params![
                settings.base_url.trim().trim_end_matches('/'),
                settings.token_secret.as_deref().map(str::trim),
                settings.poll_interval_minutes,
                settings.enabled as i64,
                settings.last_polled_at,
            ],
        )?;
        Ok(())
    }

    /// 批量镜像 findings（单事务）。返回实际新插入的条数——已存在的 id
    /// 只补 payload，不碰 read_at（已读态归本地）。dedupe 靠主键幂等。
    pub fn upsert_findings(&self, findings: &[crate::scout::ScoutFinding]) -> Result<usize> {
        let tx = self.begin_write_transaction()?;
        let mut inserted = 0usize;
        for f in findings {
            let payload_json = f.payload_json.clone();
            let delta_json = f.delta_json.clone();
            let exists: bool = self.connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM scout_findings_mirror WHERE id = ?1)",
                [f.id],
                |r| r.get(0),
            )?;
            self.connection.execute(
                "INSERT INTO scout_findings_mirror(id, source_id, snapshot_id, kind, item_key, payload_json, delta_json, detected_at)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(id) DO UPDATE SET
                    payload_json=excluded.payload_json,
                    delta_json=excluded.delta_json,
                    snapshot_id=excluded.snapshot_id,
                    kind=excluded.kind",
                params![f.id, f.source_id, f.snapshot_id, f.kind, f.item_key, payload_json, delta_json, f.detected_at],
            )?;
            if !exists {
                inserted += 1;
            }
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(inserted)
    }

    /// findings 时间倒序分页；kind / source_id / unread 可选过滤。
    pub fn list_scout_findings(
        &self,
        kind: Option<&str>,
        source_id: Option<&str>,
        unread_only: bool,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ScoutFindingRow>> {
        ensure!(
            kind.is_none_or(|k| matches!(k, "new" | "changed" | "gone")),
            "kind 无效"
        );
        let limit = limit.clamp(1, 500);
        let mut sql = String::from(
            "SELECT id, source_id, snapshot_id, kind, item_key, payload_json, delta_json, detected_at, read_at
             FROM scout_findings_mirror WHERE 1=1",
        );
        let mut binds: Vec<String> = Vec::new();
        if let Some(k) = kind {
            sql.push_str(" AND kind = ?");
            binds.push(k.to_string());
        }
        if let Some(s) = source_id {
            sql.push_str(" AND source_id = ?");
            binds.push(s.to_string());
        }
        if unread_only {
            sql.push_str(" AND read_at IS NULL");
        }
        sql.push_str(" ORDER BY detected_at DESC, id DESC LIMIT ? OFFSET ?");
        let mut stmt = self.connection.prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params_from_iter(
                binds
                    .iter()
                    .map(|b| b as &dyn rusqlite::ToSql)
                    .chain(std::iter::once(&limit as &dyn rusqlite::ToSql))
                    .chain(std::iter::once(&offset as &dyn rusqlite::ToSql)),
            ),
            |r| {
                Ok(ScoutFindingRow {
                    id: r.get(0)?,
                    source_id: r.get(1)?,
                    snapshot_id: r.get(2)?,
                    kind: r.get(3)?,
                    item_key: r.get(4)?,
                    payload_json: r.get(5)?,
                    delta_json: r.get(6)?,
                    detected_at: r.get(7)?,
                    read_at: r.get(8)?,
                })
            },
        )?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .context("query findings")
    }

    /// 未读计数（侧栏徽标 + 今天栏卡片判断）。
    pub fn scout_unread_count(&self) -> Result<i64> {
        self.connection
            .query_row(
                "SELECT COUNT(*) FROM scout_findings_mirror WHERE read_at IS NULL",
                [],
                |r| r.get(0),
            )
            .context("count unread findings")
    }

    /// 标记已读。ids 为空 = 全部标已读（「全部已读」按钮）。
    pub fn mark_findings_read(&self, ids: &[i64]) -> Result<usize> {
        let tx = self.begin_write_transaction()?;
        let mut changed = 0usize;
        if ids.is_empty() {
            changed = self.connection.execute(
                "UPDATE scout_findings_mirror SET read_at = datetime('now')
                 WHERE read_at IS NULL",
                [],
            )?;
        } else {
            for id in ids {
                changed += self.connection.execute(
                    "UPDATE scout_findings_mirror SET read_at = datetime('now')
                     WHERE id = ?1 AND read_at IS NULL",
                    [id],
                )?;
            }
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(changed)
    }

    /// 清空镜像（设置页「重新同步」用）：保留设置行，下次拉取全量重建。
    pub fn clear_scout_mirror(&self) -> Result<()> {
        self.connection
            .execute_batch("DELETE FROM scout_findings_mirror; DELETE FROM scout_sync_cursor;")?;
        Ok(())
    }

    /// 各源未读数（按源分组徽标 / Dashboard 摘要卡）。
    pub fn scout_unread_by_source(&self) -> Result<Vec<(String, i64)>> {
        let mut stmt = self.connection.prepare(
            "SELECT source_id, COUNT(*) FROM scout_findings_mirror
             WHERE read_at IS NULL GROUP BY source_id ORDER BY source_id",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .context("unread by source")
    }

    /// 按源汇总（发现页分组卡片 / 总览卡片共用）：每源的未读、各 kind
    /// 计数与最近一次发现时间。一次 GROUP BY 拿全，避免 UI 逐源查询。
    pub fn scout_source_summaries(&self) -> Result<Vec<ScoutSourceSummary>> {
        let mut stmt = self.connection.prepare(
            "SELECT source_id,
                    COUNT(*) FILTER (WHERE read_at IS NULL) AS unread,
                    COUNT(*) FILTER (WHERE kind = 'new') AS new_count,
                    COUNT(*) FILTER (WHERE kind = 'changed') AS changed_count,
                    COUNT(*) FILTER (WHERE kind = 'gone') AS gone_count,
                    COUNT(*) AS total,
                    MAX(detected_at) AS latest
             FROM scout_findings_mirror
             GROUP BY source_id
             ORDER BY unread DESC, latest DESC, source_id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(ScoutSourceSummary {
                source_id: r.get(0)?,
                unread: r.get(1)?,
                new_count: r.get(2)?,
                changed_count: r.get(3)?,
                gone_count: r.get(4)?,
                total: r.get(5)?,
                latest_detected_at: r.get(6)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .context("summarize scout sources")
    }

    pub fn scout_cursor(&self) -> Result<Option<(String, i64, Option<i64>)>> {
        Ok(self
            .connection
            .query_row(
                "SELECT instance_id,after_id,upper_id FROM scout_sync_cursor WHERE id=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?)
    }
    pub fn apply_scout_page(
        &self,
        page: &super::client::FindingPage,
        expected_after: i64,
    ) -> Result<usize> {
        let tx = self.begin_write_transaction()?;
        if let Some((instance, after, _)) = self.scout_cursor()? {
            ensure!(
                instance == page.instance_id,
                "scout_instance_changed_reconfigure"
            );
            ensure!(after == expected_after, "scout_cursor_changed_retry");
        } else {
            ensure!(expected_after == 0, "scout_cursor_changed_retry");
        }
        let inserted = self.upsert_findings(&page.findings()?)?;
        self.connection.execute(
            "INSERT INTO scout_sync_cursor(id,instance_id,after_id,upper_id) VALUES(1,?1,?2,?3)
            ON CONFLICT(id) DO UPDATE SET after_id=excluded.after_id,upper_id=excluded.upper_id",
            params![
                page.instance_id,
                page.next,
                if page.has_more {
                    Some(page.upper)
                } else {
                    None
                }
            ],
        )?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(inserted)
    }
    pub fn cache_scout_products(&self, key: &str, instance: &str, document: &str) -> Result<()> {
        let tx = self.begin_write_transaction()?;
        let mut cache: serde_json::Value = serde_json::from_str(&self.cached_scout_products()?)?;
        if !cache.is_object() {
            cache = serde_json::json!({});
        }
        let map = cache.as_object_mut().unwrap();
        if map.len() >= 32 && !map.contains_key(key) {
            if let Some(first) = map.keys().next().cloned() {
                map.remove(&first);
            }
        }
        map.insert(key.to_string(), serde_json::from_str(document)?);
        if let Some((known, _, _)) = self.scout_cursor()? {
            ensure!(known == instance, "scout_instance_changed_reconfigure");
        }
        self.connection.execute("INSERT INTO scout_sync_cursor(id,instance_id,after_id,products_json,products_fetched_at) VALUES(1,?1,0,?2,datetime('now'))
          ON CONFLICT(id) DO UPDATE SET products_json=excluded.products_json,products_fetched_at=excluded.products_fetched_at",params![instance,cache.to_string()])?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }
    pub fn cached_scout_products(&self) -> Result<String> {
        Ok(self
            .connection
            .query_row(
                "SELECT products_json FROM scout_sync_cursor WHERE id=1",
                [],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or_else(|| "{}".into()))
    }

    /// 更新拉取水位（成功拉取后调用）。
    pub fn touch_scout_polled_at(&self, iso: &str) -> Result<()> {
        self.connection.execute(
            "UPDATE scout_settings SET last_polled_at = ?1, updated_at = datetime('now')
             WHERE id = 1",
            [iso],
        )?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
