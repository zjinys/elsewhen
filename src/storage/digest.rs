//! 知识消化队列（FR-PES-004 阶段 1）：事件级持久任务 + 批次运行日志。
//!
//! - 入队：与事件同一事务（[`enqueue_digest_job_on`]），沉淀窗到期后才可领取，
//!   让事件分析先完成、用户有时间修改可记录性，也让同时段的事件自然成批；
//! - settle：不可记录的事件（人工分类 ?? 分析结果 ?? 默认可记录）标为 skipped，
//!   分析尚未终态的继续等待；冷却期满的 failed 自动回队（不提供手动触发）；
//! - claim：IMMEDIATE 事务按 recorded_at 领取一批（条数 + 字符预算），同时写一条
//!   running 运行日志；
//! - 完成：知识页写入、wiki_log 与任务确认在同一事务（[`Store::apply_digest_batch`]）；
//!   失败整批指数退避，连续失败 [`DIGEST_MAX_ATTEMPTS`] 次后进入冷却。
//!
//! 进度以「每个事件的任务状态」为准，不再依赖时间游标，迟到、同时间戳、积压
//! 都不会漏；证据按事件 id 并集去重，重试不会重复累加。

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use uuid::Uuid;

use super::{ContentPolicy, EventRecord, Store, WikiPageDraft};

/// 任务版本：提示词或合并语义有不兼容变化时升级，旧版本任务不再领取。
pub const DIGEST_VERSION: &str = "wiki-digest-v2";
/// 事件入队后的沉淀窗（秒）。
pub const DIGEST_SETTLE_SECS: i64 = 600;
/// 连续失败达到上限后的冷却时长（秒），期满自动回队重试。
pub const DIGEST_FAILED_COOLDOWN_SECS: i64 = 6 * 3600;
/// 单轮最多尝试次数（含首次），超过即进入冷却。
pub const DIGEST_MAX_ATTEMPTS: i64 = 5;
/// 运行日志里保留的模型返回片段长度（字符），不记录事件原文。
const RUN_ERROR_MAX_CHARS: usize = 300;

/// 已领取、等待消化的一批事件。
#[derive(Debug, Clone)]
pub struct DigestBatch {
    pub id: String,
    pub started_at: String,
    pub events: Vec<EventRecord>,
}

/// 一批消化成功后写入的页面结果（进运行日志）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DigestBatchOutcome {
    pub created: Vec<String>,
    pub updated: Vec<String>,
    /// 人工持有页：正文未动、只累加证据
    pub protected: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DigestJobStats {
    pub pending: i64,
    pub running: i64,
    pub retry: i64,
    pub succeeded: i64,
    pub failed: i64,
    pub skipped: i64,
    pub last_success_at: Option<String>,
    pub last_error: Option<String>,
    pub last_error_at: Option<String>,
}

/// 队列明细一行（GUI 只读查看）。
#[derive(Debug, Clone)]
pub struct DigestJobRow {
    pub job_id: String,
    pub event_id: String,
    pub event_excerpt: String,
    pub recorded_at: String,
    pub status: String,
    pub attempts: i64,
    pub failed_rounds: i64,
    pub available_at: String,
    pub last_error: Option<String>,
    pub skip_reason: Option<String>,
    pub batch_id: Option<String>,
    pub updated_at: String,
}

/// 运行日志一行（每个被领取的批次一行，空闲 tick 不写）。
#[derive(Debug, Clone)]
pub struct DigestRunRow {
    pub id: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub status: String,
    pub event_count: i64,
    pub model: Option<String>,
    pub duration_ms: Option<i64>,
    pub created_slugs: Vec<String>,
    pub updated_slugs: Vec<String>,
    pub protected_slugs: Vec<String>,
    pub error: Option<String>,
}

/// 与事件写入同事务登记消化任务（`INSERT OR IGNORE`，重复登记无副作用）。
pub(crate) fn enqueue_digest_job_on(
    connection: &Connection,
    event_id: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<()> {
    let available = now + chrono::Duration::seconds(DIGEST_SETTLE_SECS);
    let now_text = now.to_rfc3339();
    connection.execute(
        "INSERT OR IGNORE INTO knowledge_digest_jobs
         (id,event_id,digest_version,status,attempts,failed_rounds,available_at,created_at,updated_at)
         VALUES (?1,?2,?3,'pending',0,0,?4,?5,?5)",
        params![
            Uuid::new_v4().to_string(),
            event_id,
            DIGEST_VERSION,
            available.to_rfc3339(),
            now_text
        ],
    )?;
    Ok(())
}

/// 截断到指定字符数（按 char，避免切坏多字节字符）。
fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let mut out: String = text.chars().take(max).collect();
        out.push('…');
        out
    }
}

impl Store {
    /// 队列整理：①冷却期满的 failed 回队；②已到期任务按可记录性决定跳过或等待。
    /// 返回本次标为 skipped 的数量。
    pub fn settle_digest_queue(&self) -> Result<usize> {
        let now = chrono::Utc::now().to_rfc3339();
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        tx.execute(
            "UPDATE knowledge_digest_jobs
         SET status='retry', attempts=0, failed_rounds=failed_rounds+1, updated_at=?1
         WHERE digest_version=?2 AND status='failed' AND available_at<=?1",
            params![now, DIGEST_VERSION],
        )?;
        // 有效可记录性 = 最近一次人工分类 ?? 最近一次分析结果 ?? 默认可记录；
        // 没有人工分类且分析还没到终态（pending/running/retry）时先等待。
        let candidates: Vec<(
            String,
            Option<bool>,
            Option<String>,
            Option<bool>,
            Option<String>,
        )> = {
            let mut statement = tx.prepare(
                "SELECT j.id,
                    (SELECT d.recordable FROM event_recordability_decisions d
                      WHERE d.event_id=j.event_id ORDER BY d.created_at DESC, d.id DESC LIMIT 1),
                    (SELECT d.kind FROM event_recordability_decisions d
                      WHERE d.event_id=j.event_id ORDER BY d.created_at DESC, d.id DESC LIMIT 1),
                    (SELECT json_extract(a.result_json,'$.recordable') FROM event_analyses a
                      WHERE a.event_id=j.event_id ORDER BY a.created_at DESC, a.id DESC LIMIT 1),
                    (SELECT json_extract(a.result_json,'$.kind') FROM event_analyses a
                      WHERE a.event_id=j.event_id ORDER BY a.created_at DESC, a.id DESC LIMIT 1)
             FROM knowledge_digest_jobs j
             WHERE j.digest_version=?1 AND j.status IN ('pending','retry') AND j.available_at<=?2",
            )?;
            let rows = statement.query_map(params![DIGEST_VERSION, now], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let mut skipped = 0;
        for (job_id, manual, manual_kind, analyzed, analyzed_kind) in candidates {
            let reason = match (manual, analyzed) {
                (Some(false), _) => Some(format!(
                    "人工标记为不可记录（{}）",
                    manual_kind.unwrap_or_default()
                )),
                (Some(true), _) => None,
                (None, Some(false)) => Some(format!(
                    "分析判定为不可记录（{}）",
                    analyzed_kind.unwrap_or_default()
                )),
                _ => None,
            };
            if let Some(reason) = reason {
                skipped += tx.execute(
                    "UPDATE knowledge_digest_jobs
                 SET status='skipped', skip_reason=?2, batch_id=NULL, updated_at=?3
                 WHERE id=?1",
                    params![job_id, reason, now],
                )?;
            }
        }
        tx.commit()?;
        Ok(skipped)
    }

    /// 领取一批到期、可消化的任务（按记录时间，受条数与字符预算约束），并写一条
    /// running 运行日志。没有可领取任务时返回 None（不写日志）。
    /// 至少领取一条：单条超出字符预算时按单条成批，由提示词侧截断展示。
    pub fn claim_digest_batch(
        &self,
        max_events: usize,
        max_chars: usize,
        model: Option<&str>,
    ) -> Result<Option<DigestBatch>> {
        let now = chrono::Utc::now().to_rfc3339();
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        // 可领取：已到期；且有人工分类，或分析已终态（succeeded/failed/无分析任务）。
        let rows: Vec<(String, EventRecord)> = {
            let mut statement = tx.prepare(
                "SELECT j.id, e.id, e.recorded_at, e.raw_text
             FROM knowledge_digest_jobs j
             JOIN events e ON e.id=j.event_id
             LEFT JOIN analysis_jobs aj ON aj.event_id=j.event_id
             WHERE j.digest_version=?1 AND j.status IN ('pending','retry') AND j.available_at<=?2
               AND (
                 EXISTS (SELECT 1 FROM event_recordability_decisions d WHERE d.event_id=j.event_id)
                 OR aj.status IS NULL OR aj.status IN ('succeeded','failed')
               )
             ORDER BY e.recorded_at ASC, e.id ASC
             LIMIT ?3",
            )?;
            let rows =
                statement.query_map(params![DIGEST_VERSION, now, max_events as i64], |row| {
                    Ok((
                        row.get(0)?,
                        EventRecord {
                            id: row.get(1)?,
                            recorded_at: row.get(2)?,
                            raw_text: row.get(3)?,
                        },
                    ))
                })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let mut picked: Vec<(String, EventRecord)> = Vec::new();
        let mut used = 0usize;
        for (job_id, event) in rows {
            let cost = event.raw_text.chars().count();
            if !picked.is_empty() && used + cost > max_chars {
                break;
            }
            used += cost;
            picked.push((job_id, event));
        }
        if picked.is_empty() {
            tx.commit()?;
            return Ok(None);
        }
        let batch_id = Uuid::new_v4().to_string();
        for (job_id, _) in &picked {
            tx.execute(
                "UPDATE knowledge_digest_jobs
             SET status='running', attempts=attempts+1, batch_id=?2, updated_at=?3
             WHERE id=?1",
                params![job_id, batch_id, now],
            )?;
        }
        tx.execute(
            "INSERT INTO knowledge_digest_runs (id,started_at,status,event_count,model)
         VALUES (?1,?2,'running',?3,?4)",
            params![batch_id, now, picked.len() as i64, model],
        )?;
        tx.commit()?;
        Ok(Some(DigestBatch {
            id: batch_id,
            started_at: now,
            events: picked.into_iter().map(|(_, event)| event).collect(),
        }))
    }

    /// 把一批校验通过的页面草案写入知识库，并在同一事务里写 wiki_log、确认任务、
    /// 补全运行日志。任一步失败整批回滚（任务仍是 running，由调用方转入失败重试）。
    pub fn apply_digest_batch(
        &self,
        batch: &DigestBatch,
        drafts: &[WikiPageDraft],
    ) -> Result<DigestBatchOutcome> {
        let now = chrono::Utc::now();
        let now_text = now.to_rfc3339();
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        let mut outcome = DigestBatchOutcome::default();
        for draft in drafts {
            let result = self.upsert_wiki_page_in_tx(draft, ContentPolicy::PreserveHumanEdits)?;
            let bucket = if result.protected {
                &mut outcome.protected
            } else if result.created {
                &mut outcome.created
            } else {
                &mut outcome.updated
            };
            if !bucket.contains(&draft.slug) {
                bucket.push(draft.slug.clone());
            }
        }
        if !outcome.created.is_empty() || !outcome.updated.is_empty() {
            let date = now.format("%Y-%m-%d");
            let entry = if outcome.updated.is_empty() {
                format!(
                    "## [{date}] digest | created: {}",
                    outcome.created.join(", ")
                )
            } else {
                format!(
                    "## [{date}] digest | created: {}; updated: {}",
                    outcome.created.join(", "),
                    outcome.updated.join(", ")
                )
            };
            tx.execute(
                "INSERT INTO wiki_log (ts, entry) VALUES (?1, ?2)",
                params![now_text, entry],
            )?;
        }
        let confirmed = tx.execute(
            "UPDATE knowledge_digest_jobs
         SET status='succeeded', last_error=NULL, updated_at=?2
         WHERE batch_id=?1 AND status='running'",
            params![batch.id, now_text],
        )?;
        if confirmed != batch.events.len() {
            anyhow::bail!(
                "消化批次 {} 的任务状态已变化（期望 {} 条 running，实际 {} 条）",
                batch.id,
                batch.events.len(),
                confirmed
            );
        }
        tx.execute(
            "UPDATE knowledge_digest_runs
         SET status='succeeded', finished_at=?2, duration_ms=?3,
             created_slugs=?4, updated_slugs=?5, protected_slugs=?6, error=NULL
         WHERE id=?1",
            params![
                batch.id,
                now_text,
                duration_ms(&batch.started_at, now),
                serde_json::to_string(&outcome.created)?,
                serde_json::to_string(&outcome.updated)?,
                serde_json::to_string(&outcome.protected)?,
            ],
        )?;
        tx.commit()?;
        Ok(outcome)
    }

    /// 整批失败：任务按指数退避回 retry；本轮尝试次数用尽则 failed 并进入冷却。
    /// 运行日志记录失败原因（截断，不含事件原文）。
    pub fn fail_digest_batch(&self, batch: &DigestBatch, error: &str) -> Result<()> {
        let now = chrono::Utc::now();
        let now_text = now.to_rfc3339();
        let error = truncate_chars(error.trim(), RUN_ERROR_MAX_CHARS);
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        let attempts: Vec<(String, i64)> = {
            let mut statement = tx.prepare(
            "SELECT id, attempts FROM knowledge_digest_jobs WHERE batch_id=?1 AND status='running'",
        )?;
            let rows = statement.query_map([&batch.id], |row| Ok((row.get(0)?, row.get(1)?)))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        for (job_id, attempts) in attempts {
            let (status, available) = if attempts >= DIGEST_MAX_ATTEMPTS {
                (
                    "failed",
                    now + chrono::Duration::seconds(DIGEST_FAILED_COOLDOWN_SECS),
                )
            } else {
                (
                    "retry",
                    now + chrono::Duration::seconds(2_i64.pow(attempts.clamp(0, 8) as u32) * 30),
                )
            };
            tx.execute(
                "UPDATE knowledge_digest_jobs
             SET status=?2, last_error=?3, available_at=?4, updated_at=?5
             WHERE id=?1",
                params![job_id, status, error, available.to_rfc3339(), now_text],
            )?;
        }
        tx.execute(
            "UPDATE knowledge_digest_runs
         SET status='failed', finished_at=?2, duration_ms=?3, error=?4
         WHERE id=?1",
            params![
                batch.id,
                now_text,
                duration_ms(&batch.started_at, now),
                error
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// 进程中断遗留的 running 任务回队重试，对应运行日志标记失败。
    /// 与分析队列一致：启动时与每次 tick 前调用（同一时刻只有一个 worker）。
    pub fn recover_interrupted_digest_jobs(&self) -> Result<usize> {
        let now = chrono::Utc::now().to_rfc3339();
        let tx = self.connection.unchecked_transaction()?;
        let recovered = tx.execute(
            "UPDATE knowledge_digest_jobs
         SET status='retry', last_error='应用退出时消化尚未完成', available_at=?1, updated_at=?1
         WHERE status='running'",
            [&now],
        )?;
        tx.execute(
            "UPDATE knowledge_digest_runs
         SET status='failed', finished_at=?1, error='应用退出时消化尚未完成'
         WHERE status='running'",
            [&now],
        )?;
        tx.commit()?;
        Ok(recovered)
    }

    /// 用户把事件改回「可记录」时，被跳过的消化任务重新排队（立即可领取：
    /// 分类已由用户明确，不再需要沉淀窗）。
    pub(crate) fn requeue_skipped_digest_job_on(
        connection: &Connection,
        event_id: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        let now_text = now.to_rfc3339();
        connection.execute(
            "UPDATE knowledge_digest_jobs
         SET status='pending', skip_reason=NULL, attempts=0, available_at=?2, updated_at=?2
         WHERE event_id=?1 AND digest_version=?3 AND status='skipped'",
            params![event_id, now_text, DIGEST_VERSION],
        )?;
        Ok(())
    }

    pub fn digest_job_stats(&self) -> Result<DigestJobStats> {
        let mut stats = self
            .connection
            .query_row(
                "SELECT
                COALESCE(SUM(status='pending'),0), COALESCE(SUM(status='running'),0),
                COALESCE(SUM(status='retry'),0), COALESCE(SUM(status='succeeded'),0),
                COALESCE(SUM(status='failed'),0), COALESCE(SUM(status='skipped'),0)
             FROM knowledge_digest_jobs WHERE digest_version=?1",
                [DIGEST_VERSION],
                |row| {
                    Ok(DigestJobStats {
                        pending: row.get(0)?,
                        running: row.get(1)?,
                        retry: row.get(2)?,
                        succeeded: row.get(3)?,
                        failed: row.get(4)?,
                        skipped: row.get(5)?,
                        ..DigestJobStats::default()
                    })
                },
            )
            .context("aggregate knowledge digest job stats")?;
        stats.last_success_at = self
            .connection
            .query_row(
                "SELECT finished_at FROM knowledge_digest_runs
             WHERE status='succeeded' ORDER BY finished_at DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        if let Some((error, at)) = self
            .connection
            .query_row(
                "SELECT error, finished_at FROM knowledge_digest_runs
             WHERE status='failed' ORDER BY finished_at DESC LIMIT 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                    ))
                },
            )
            .optional()?
        {
            stats.last_error = error;
            stats.last_error_at = at;
        }
        Ok(stats)
    }

    /// 队列明细（按最近更新倒序）。`status` 为 None 时返回全部状态。
    pub fn list_digest_jobs(
        &self,
        status: Option<&str>,
        limit: usize,
    ) -> Result<Vec<DigestJobRow>> {
        let mut statement = self.connection.prepare(
        "SELECT j.id, j.event_id, e.raw_text, e.recorded_at, j.status, j.attempts,
                j.failed_rounds, j.available_at, j.last_error, j.skip_reason, j.batch_id, j.updated_at
         FROM knowledge_digest_jobs j JOIN events e ON e.id=j.event_id
         WHERE j.digest_version=?1 AND (?2 IS NULL OR j.status=?2)
         ORDER BY j.updated_at DESC, e.recorded_at DESC
         LIMIT ?3",
    )?;
        let rows = statement.query_map(params![DIGEST_VERSION, status, limit as i64], |row| {
            let raw: String = row.get(2)?;
            Ok(DigestJobRow {
                job_id: row.get(0)?,
                event_id: row.get(1)?,
                event_excerpt: truncate_chars(&raw.replace('\n', " "), 80),
                recorded_at: row.get(3)?,
                status: row.get(4)?,
                attempts: row.get(5)?,
                failed_rounds: row.get(6)?,
                available_at: row.get(7)?,
                last_error: row.get(8)?,
                skip_reason: row.get(9)?,
                batch_id: row.get(10)?,
                updated_at: row.get(11)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// 最近的运行日志（按开始时间倒序）。
    pub fn list_digest_runs(&self, limit: usize) -> Result<Vec<DigestRunRow>> {
        let mut statement = self.connection.prepare(
            "SELECT id, started_at, finished_at, status, event_count, model, duration_ms,
                created_slugs, updated_slugs, protected_slugs, error
         FROM knowledge_digest_runs ORDER BY started_at DESC LIMIT ?1",
        )?;
        let rows = statement.query_map([limit as i64], |row| {
            let slugs = |index: usize| -> rusqlite::Result<Vec<String>> {
                let raw: String = row.get(index)?;
                Ok(serde_json::from_str(&raw).unwrap_or_default())
            };
            Ok(DigestRunRow {
                id: row.get(0)?,
                started_at: row.get(1)?,
                finished_at: row.get(2)?,
                status: row.get(3)?,
                event_count: row.get(4)?,
                model: row.get(5)?,
                duration_ms: row.get(6)?,
                created_slugs: slugs(7)?,
                updated_slugs: slugs(8)?,
                protected_slugs: slugs(9)?,
                error: row.get(10)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }
}

fn duration_ms(started_at: &str, now: chrono::DateTime<chrono::Utc>) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(started_at)
        .ok()
        .map(|start| {
            (now - start.with_timezone(&chrono::Utc))
                .num_milliseconds()
                .max(0)
        })
}
