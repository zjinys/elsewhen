//! 事件 / 输入记录 / 分析队列存取（从 `Store` 抽出）。核心写路径。
//!
//! - insert_event：唯一提交边界，单事务写入 event + analysis_job（+ input_record）。
//!   原始事件不可变由 DB trigger 强制，分析失败不阻塞落盘（进 retry）。
//! - input_records：统一输入关联层，幂等键 INSERT OR IGNORE + 回读（原子）。
//! - analysis_jobs：claim/complete/fail + 指数退避 available_at，重启恢复 running→retry。

use anyhow::{Context, Result};
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use uuid::Uuid;

use super::adapter::AnalysisJob;
use super::digest::enqueue_digest_job_on;
use super::{
    map_input_record, AnalysisJobStats, DailyEntry, DailyReviewRecord, EventAnalysisDetail,
    EventRecordabilityDecision, EventSummary, InputRecord, InsightSummary, NewEvent, Store,
};

impl Store {
    pub fn insert_event(&self, event: NewEvent<'_>) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now_at = chrono::Utc::now();
        let now = now_at.to_rfc3339();
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute(
            "INSERT INTO events
         (id, occurred_at, recorded_at, raw_text, source, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6, ?6)",
            params![
                id,
                event.occurred_at.to_rfc3339(),
                event.recorded_at.to_rfc3339(),
                event.raw_text,
                event.source,
                now,
            ],
        )?;
        transaction.execute(
        "INSERT INTO analysis_jobs (id, event_id, status, attempts, available_at, created_at, updated_at)
         VALUES (?1, ?2, 'pending', 0, ?3, ?3, ?3)",
        params![Uuid::new_v4().to_string(), id, now],
    )?;
        enqueue_digest_job_on(&transaction, &id, now_at)?;
        transaction.commit()?;
        Ok(id)
    }

    pub fn create_input_record(
        &self,
        raw_text: &str,
        source: &str,
        idempotency_key: Option<&str>,
    ) -> Result<InputRecord> {
        let raw_text = raw_text.trim();
        let source = source.trim();
        if raw_text.is_empty() {
            anyhow::bail!("input raw_text 不能为空");
        }
        if source.is_empty() {
            anyhow::bail!("input source 不能为空");
        }
        let key = idempotency_key.map(str::trim).filter(|key| !key.is_empty());
        if let Some(key) = key {
            // 幂等写入原子化（P2-3）：不做「先查后插」（check-then-insert 有并发窗口，
            // 两个同键请求会同时判定不存在并双双 INSERT），直接用部分唯一索引兜底——
            // 同名键并发时 INSERT OR IGNORE 让后写者静默跳过，再回读既有记录返回，
            // 且首个请求按 pending 落库，与原来的插入行为一致。
            self.connection.execute(
                "INSERT OR IGNORE INTO input_records
             (id,raw_text,source,route_status,idempotency_key,created_at,updated_at)
             VALUES (?1,?2,?3,'pending',?4,?5,?5)",
                params![
                    Uuid::new_v4().to_string(),
                    raw_text,
                    source,
                    key,
                    chrono::Utc::now().to_rfc3339()
                ],
            )?;
            return self
                .get_input_record_by_idempotency_key(key)?
                .context("input record 幂等键写入后读取既有记录失败");
        }

        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        self.connection.execute(
            "INSERT INTO input_records
         (id,raw_text,source,route_status,idempotency_key,created_at,updated_at)
         VALUES (?1,?2,?3,'pending',?4,?5,?5)",
            params![id, raw_text, source, key, now],
        )?;
        self.get_input_record(&id)?
            .context("input record 创建后读取失败")
    }

    /// 普通个人输入的最小统一提交路径：input record、不可变 event 与分析任务
    /// 在同一事务内提交。网络和 AI 均不参与此路径。
    pub fn submit_input_as_event(
        &self,
        raw_text: &str,
        source: &str,
        idempotency_key: Option<&str>,
    ) -> Result<InputRecord> {
        let raw_text = raw_text.trim();
        let source = source.trim();
        if raw_text.is_empty() || source.is_empty() {
            anyhow::bail!("input raw_text 和 source 不能为空");
        }
        let key = idempotency_key.map(str::trim).filter(|key| !key.is_empty());
        if let Some(key) = key {
            if let Some(existing) = self.get_input_record_by_idempotency_key(key)? {
                return Ok(existing);
            }
        }

        let input_id = Uuid::new_v4().to_string();
        let event_id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now();
        let now_text = now.to_rfc3339();
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute(
            "INSERT INTO events
         (id,occurred_at,recorded_at,raw_text,source,status,created_at,updated_at)
         VALUES (?1,?2,?2,?3,?4,'pending',?2,?2)",
            params![event_id, now_text, raw_text, source],
        )?;
        transaction.execute(
            "INSERT INTO analysis_jobs
         (id,event_id,status,attempts,available_at,created_at,updated_at)
         VALUES (?1,?2,'pending',0,?3,?3,?3)",
            params![Uuid::new_v4().to_string(), event_id, now_text],
        )?;
        enqueue_digest_job_on(&transaction, &event_id, now)?;
        // 幂等键的并发窗口（先查后插存在时间差）由部分唯一索引兜底：后到的
        // 重复键在这里触发 UNIQUE 冲突——必须回滚整个事务（否则 event+job 已成
        // 孤儿行），再按幂等键回读既有记录返回（P2-3）。
        let inserted = transaction.execute(
            "INSERT INTO input_records
         (id,raw_text,source,route_status,idempotency_key,event_id,created_at,updated_at)
         VALUES (?1,?2,?3,'routed',?4,?5,?6,?6)",
            params![input_id, raw_text, source, key, event_id, now_text],
        );
        match inserted {
            Ok(_) => {}
            Err(e) if e.to_string().contains("input_records.idempotency_key") && key.is_some() => {
                transaction.rollback()?;
                return self
                    .get_input_record_by_idempotency_key(key.unwrap())?
                    .context("并发重复提交：回滚后读取既有 input record 失败");
            }
            Err(e) => return Err(e.into()),
        }
        transaction.commit()?;
        self.get_input_record(&input_id)?
            .context("统一输入提交后读取失败")
    }

    /// 主对话输入：同一事务保存用户消息与个人事件，并用 input record 关联。
    pub fn submit_conversation_input(
        &self,
        conversation_id: &str,
        raw_text: &str,
        idempotency_key: Option<&str>,
    ) -> Result<InputRecord> {
        let raw_text = raw_text.trim();
        if raw_text.is_empty() {
            anyhow::bail!("input raw_text 不能为空");
        }
        let key = idempotency_key.map(str::trim).filter(|key| !key.is_empty());
        if let Some(key) = key {
            if let Some(existing) = self.get_input_record_by_idempotency_key(key)? {
                return Ok(existing);
            }
        }
        if self.get_conversation(conversation_id)?.is_none() {
            anyhow::bail!("Conversation not found: {conversation_id}");
        }

        let input_id = Uuid::new_v4().to_string();
        let event_id = Uuid::new_v4().to_string();
        let message_id = Uuid::new_v4().to_string();
        let now_at = chrono::Utc::now();
        let now = now_at.to_rfc3339();
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute(
            "INSERT INTO events
         (id,occurred_at,recorded_at,raw_text,source,status,created_at,updated_at)
         VALUES (?1,?2,?2,?3,'conversation','pending',?2,?2)",
            params![event_id, now, raw_text],
        )?;
        transaction.execute(
            "INSERT INTO analysis_jobs
         (id,event_id,status,attempts,available_at,created_at,updated_at)
         VALUES (?1,?2,'pending',0,?3,?3,?3)",
            params![Uuid::new_v4().to_string(), event_id, now],
        )?;
        enqueue_digest_job_on(&transaction, &event_id, now_at)?;
        transaction.execute(
            "INSERT INTO messages (id,conversation_id,role,content,created_at)
         VALUES (?1,?2,'user',?3,?4)",
            params![message_id, conversation_id, raw_text, now],
        )?;
        transaction.execute(
            "UPDATE conversations SET updated_at=?1 WHERE id=?2",
            params![now, conversation_id],
        )?;
        // 与 submit_input_as_event 相同的幂等并发兜底（P2-3）：重复键冲突时
        // 回滚整个事务（event+job+message 一并撤销），再回读既有记录。
        let inserted = transaction.execute(
            "INSERT INTO input_records
         (id,raw_text,source,route_status,idempotency_key,event_id,message_id,created_at,updated_at)
         VALUES (?1,?2,'conversation','routed',?3,?4,?5,?6,?6)",
            params![input_id, raw_text, key, event_id, message_id, now],
        );
        match inserted {
            Ok(_) => {}
            Err(e) if e.to_string().contains("input_records.idempotency_key") && key.is_some() => {
                transaction.rollback()?;
                return self
                    .get_input_record_by_idempotency_key(key.unwrap())?
                    .context("并发重复提交：回滚后读取既有 input record 失败");
            }
            Err(e) => return Err(e.into()),
        }
        transaction.commit()?;
        self.get_input_record(&input_id)?
            .context("对话统一输入提交后读取失败")
    }

    pub fn get_input_record(&self, id: &str) -> Result<Option<InputRecord>> {
        self.connection
            .query_row(
                "SELECT id,raw_text,source,route_status,idempotency_key,event_id,message_id,
                    wiki_page_slug,todo_id,created_at,updated_at
             FROM input_records WHERE id=?1",
                [id],
                map_input_record,
            )
            .optional()
            .context("read input record")
    }

    pub fn latest_event_id_for_conversation(
        &self,
        conversation_id: &str,
    ) -> Result<Option<String>> {
        let event_id = self
            .connection
            .query_row(
                "SELECT i.event_id FROM messages m
         LEFT JOIN input_records i ON i.message_id=m.id
         WHERE m.conversation_id=?1 AND m.role='user'
         ORDER BY m.created_at DESC LIMIT 1",
                [conversation_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(anyhow::Error::from)?;
        Ok(event_id.flatten())
    }

    pub fn conversation_id_for_event(&self, event_id: &str) -> Result<Option<String>> {
        self.connection
            .query_row(
                "SELECT m.conversation_id FROM input_records i
             JOIN messages m ON m.id=i.message_id
             WHERE i.event_id=?1 LIMIT 1",
                [event_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    fn get_input_record_by_idempotency_key(&self, key: &str) -> Result<Option<InputRecord>> {
        self.connection
            .query_row(
                "SELECT id,raw_text,source,route_status,idempotency_key,event_id,message_id,
                    wiki_page_slug,todo_id,created_at,updated_at
             FROM input_records WHERE idempotency_key=?1",
                [key],
                map_input_record,
            )
            .optional()
            .context("read input record by idempotency key")
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_input_route(
        &self,
        id: &str,
        route_status: &str,
        event_id: Option<&str>,
        message_id: Option<&str>,
        wiki_page_slug: Option<&str>,
        todo_id: Option<&str>,
    ) -> Result<InputRecord> {
        if !matches!(
            route_status,
            "pending" | "routed" | "needs_confirmation" | "failed"
        ) {
            anyhow::bail!("非法 input route_status: {route_status}");
        }
        let changed = self.connection.execute(
            "UPDATE input_records
         SET route_status=?2,event_id=COALESCE(?3,event_id),message_id=COALESCE(?4,message_id),
             wiki_page_slug=COALESCE(?5,wiki_page_slug),todo_id=COALESCE(?6,todo_id),
             updated_at=?7 WHERE id=?1",
            params![
                id,
                route_status,
                event_id,
                message_id,
                wiki_page_slug,
                todo_id,
                chrono::Utc::now().to_rfc3339()
            ],
        )?;
        if changed == 0 {
            anyhow::bail!("input record 不存在: {id}");
        }
        self.get_input_record(id)?
            .context("input route 更新后读取失败")
    }

    pub fn claim_analysis_job(&self) -> Result<Option<AnalysisJob>> {
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        let job = transaction.query_row(
        "SELECT j.id, j.event_id, e.raw_text, j.attempts FROM analysis_jobs j JOIN events e ON e.id=j.event_id
         WHERE j.status IN ('pending','retry') AND j.available_at <= ?1 ORDER BY j.created_at LIMIT 1",
        [chrono::Utc::now().to_rfc3339()],
        |row| Ok(AnalysisJob { id: row.get(0)?, event_id: row.get(1)?, raw_text: row.get(2)?, attempts: row.get(3)? }),
    ).optional()?;
        if let Some(ref job) = job {
            transaction.execute("UPDATE analysis_jobs SET status='running', attempts=attempts+1, updated_at=?2 WHERE id=?1", params![job.id, chrono::Utc::now().to_rfc3339()])?;
        }
        transaction.commit()?;
        Ok(job)
    }

    /// Return jobs left running by a previous process to the durable queue.
    /// This is called once at bridge startup, never from `Store::open`, so it
    /// cannot steal work from a live worker in the current process.
    pub fn recover_interrupted_analysis_jobs(&self) -> Result<usize> {
        let now = chrono::Utc::now().to_rfc3339();
        Ok(self.connection.execute(
            "UPDATE analysis_jobs
         SET status='retry', last_error='应用退出时分析尚未完成',
             available_at=?1, updated_at=?1
         WHERE status='running'",
            [now],
        )?)
    }

    pub fn complete_analysis(
        &self,
        job: &AnalysisJob,
        prompt_version: &str,
        result_json: &str,
    ) -> Result<()> {
        let transaction = self.connection.unchecked_transaction()?;
        let now = chrono::Utc::now().to_rfc3339();
        transaction.execute("INSERT INTO event_analyses (id,event_id,prompt_version,result_json,created_at) VALUES (?1,?2,?3,?4,?5)", params![Uuid::new_v4().to_string(), job.event_id, prompt_version, result_json, now])?;
        transaction.execute(
            "UPDATE analysis_jobs SET status='succeeded', updated_at=?2 WHERE id=?1",
            params![job.id, now],
        )?;
        transaction.execute(
            "UPDATE events SET status='processed', processed_at=?2, updated_at=?2 WHERE id=?1",
            params![job.event_id, now],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn fail_analysis(&self, job: &AnalysisJob, error: &str) -> Result<()> {
        let delay = 2_i64.pow((job.attempts as u32).min(8));
        let available = chrono::Utc::now() + chrono::Duration::seconds(delay);
        self.connection.execute("UPDATE analysis_jobs SET status=CASE WHEN attempts >= 5 THEN 'failed' ELSE 'retry' END, last_error=?2, available_at=?3, updated_at=?4 WHERE id=?1", params![job.id, error, available.to_rfc3339(), chrono::Utc::now().to_rfc3339()])?;
        Ok(())
    }

    /// Aggregate the durable analysis queue by its complete status vocabulary.
    /// Missing statuses are returned as zero so callers can render a stable UI.
    pub fn analysis_job_stats(&self) -> Result<AnalysisJobStats> {
        self.connection
            .query_row(
                "SELECT
                COALESCE(SUM(status = 'pending'), 0),
                COALESCE(SUM(status = 'running'), 0),
                COALESCE(SUM(status = 'retry'), 0),
                COALESCE(SUM(status = 'succeeded'), 0),
                COALESCE(SUM(status = 'failed'), 0)
             FROM analysis_jobs",
                [],
                |row| {
                    Ok(AnalysisJobStats {
                        pending: row.get(0)?,
                        running: row.get(1)?,
                        retry: row.get(2)?,
                        succeeded: row.get(3)?,
                        failed: row.get(4)?,
                    })
                },
            )
            .context("aggregate analysis job stats")
    }

    pub fn event_analysis_detail(&self, event_id: &str) -> Result<Option<EventAnalysisDetail>> {
        self.connection
            .query_row(
                "SELECT e.id,e.raw_text,e.source,e.recorded_at,e.status,
                    j.status,j.attempts,j.last_error,j.available_at,
                    a.prompt_version,a.result_json,a.created_at
             FROM events e
             JOIN analysis_jobs j ON j.event_id=e.id
             LEFT JOIN event_analyses a ON a.id=(
               SELECT latest.id FROM event_analyses latest
               WHERE latest.event_id=e.id
               ORDER BY latest.created_at DESC LIMIT 1
             )
             WHERE e.id=?1",
                [event_id],
                |row| {
                    Ok(EventAnalysisDetail {
                        event_id: row.get(0)?,
                        raw_text: row.get(1)?,
                        source: row.get(2)?,
                        recorded_at: row.get(3)?,
                        event_status: row.get(4)?,
                        job_status: row.get(5)?,
                        attempts: row.get(6)?,
                        last_error: row.get(7)?,
                        available_at: row.get(8)?,
                        prompt_version: row.get(9)?,
                        result_json: row.get(10)?,
                        analysis_created_at: row.get(11)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn latest_event_recordability_decision(
        &self,
        event_id: &str,
    ) -> Result<Option<EventRecordabilityDecision>> {
        self.connection
            .query_row(
                "SELECT event_id,recordable,kind,reason,created_at
             FROM event_recordability_decisions
             WHERE event_id=?1 ORDER BY created_at DESC,id DESC LIMIT 1",
                [event_id],
                |row| {
                    Ok(EventRecordabilityDecision {
                        event_id: row.get(0)?,
                        recordable: row.get(1)?,
                        kind: row.get(2)?,
                        reason: row.get(3)?,
                        created_at: row.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn event_id_for_message(&self, message_id: &str) -> Result<Option<String>> {
        self.connection
        .query_row(
            "SELECT event_id FROM input_records WHERE message_id=?1 AND event_id IS NOT NULL LIMIT 1",
            [message_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn set_event_recordability(
        &self,
        event_id: &str,
        recordable: bool,
        reason: &str,
    ) -> Result<EventRecordabilityDecision> {
        if self.event_analysis_detail(event_id)?.is_none() {
            anyhow::bail!("事件不存在");
        }
        let kind = if recordable { "event" } else { "discussion" };
        let now_at = chrono::Utc::now();
        let now = now_at.to_rfc3339();
        let tx = self.connection.unchecked_transaction()?;
        tx.execute(
        "INSERT INTO event_recordability_decisions (id,event_id,recordable,kind,reason,created_at)
         VALUES (?1,?2,?3,?4,?5,?6)",
        params![Uuid::new_v4().to_string(), event_id, recordable, kind, reason.trim(), now],
    )?;
        if recordable {
            // 改回可记录：之前被跳过的知识消化任务重新排队
            Self::requeue_skipped_digest_job_on(&tx, event_id, now_at)?;
        }
        if !recordable {
            tx.execute(
                "DELETE FROM entity_facts WHERE source_event_id=?1",
                [event_id],
            )?;
            tx.execute("DELETE FROM relations WHERE source_event_id=?1", [event_id])?;
            tx.execute(
                "UPDATE pending_actions SET status='declined'
             WHERE status='pending' AND args_json LIKE '%' || ?1 || '%'",
                [event_id],
            )?;
            tx.execute(
                "UPDATE conversations SET tag='discussion',updated_at=?2
             WHERE id IN (
               SELECT m.conversation_id FROM input_records i
               JOIN messages m ON m.id=i.message_id WHERE i.event_id=?1
             ) AND (tag IS NULL OR tag='general')",
                params![event_id, now],
            )?;
        }
        tx.commit()?;
        self.latest_event_recordability_decision(event_id)?
            .context("记录人工分类后读取失败")
    }

    pub fn requeue_event_analysis(&self, event_id: &str) -> Result<bool> {
        let now = chrono::Utc::now().to_rfc3339();
        let affected = self.connection.execute(
        "UPDATE analysis_jobs SET status='pending',attempts=0,last_error=NULL,available_at=?2,updated_at=?2
         WHERE event_id=?1 AND status<>'running'",
        params![event_id, now],
    )?;
        if affected > 0 {
            self.connection.execute(
                "UPDATE events SET status='pending',processed_at=NULL,updated_at=?2 WHERE id=?1",
                params![event_id, now],
            )?;
        }
        Ok(affected > 0)
    }

    pub fn save_daily_review(
        &self,
        date: chrono::NaiveDate,
        prompt_version: &str,
        result_json: &str,
        source_event_ids: &[String],
    ) -> Result<String> {
        let prompt_version = prompt_version.trim();
        if prompt_version.is_empty() {
            anyhow::bail!("daily review prompt_version 不能为空");
        }
        if source_event_ids.is_empty() {
            anyhow::bail!("daily review 必须引用至少一条来源事件");
        }
        let daily_event_ids = self
            .daily_entries(date)?
            .into_iter()
            .map(|entry| entry.event_id)
            .collect::<Vec<_>>();
        for event_id in source_event_ids {
            if !daily_event_ids
                .iter()
                .any(|candidate| candidate == event_id)
            {
                anyhow::bail!("daily review 来源不属于目标日期: {event_id}");
            }
        }

        let transaction = self.connection.unchecked_transaction()?;
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        transaction.execute(
            "INSERT INTO daily_reviews
         (id,review_date,prompt_version,result_json,created_at)
         VALUES (?1,?2,?3,?4,?5)",
            params![
                id,
                date.format("%Y-%m-%d").to_string(),
                prompt_version,
                result_json,
                now
            ],
        )?;
        for event_id in source_event_ids {
            transaction.execute(
                "INSERT OR IGNORE INTO daily_review_sources (review_id,event_id)
             VALUES (?1,?2)",
                params![id, event_id],
            )?;
        }
        transaction.commit()?;
        Ok(id)
    }

    pub fn latest_daily_review(
        &self,
        date: chrono::NaiveDate,
    ) -> Result<Option<DailyReviewRecord>> {
        let review = self
            .connection
            .query_row(
                "SELECT id,review_date,prompt_version,result_json,created_at
             FROM daily_reviews WHERE review_date=?1
             ORDER BY created_at DESC,rowid DESC LIMIT 1",
                [date.format("%Y-%m-%d").to_string()],
                |row| {
                    Ok(DailyReviewRecord {
                        id: row.get(0)?,
                        date: row.get(1)?,
                        prompt_version: row.get(2)?,
                        result_json: row.get(3)?,
                        source_event_ids: Vec::new(),
                        created_at: row.get(4)?,
                    })
                },
            )
            .optional()?;
        let Some(mut review) = review else {
            return Ok(None);
        };
        let mut statement = self.connection.prepare(
            "SELECT event_id FROM daily_review_sources
         WHERE review_id=?1 ORDER BY rowid",
        )?;
        review.source_event_ids = statement
            .query_map([&review.id], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(Some(review))
    }

    pub fn list_events(&self) -> Result<Vec<EventSummary>> {
        let mut statement = self.connection.prepare(
            "SELECT id, recorded_at, raw_text, source, status
         FROM events ORDER BY recorded_at DESC, id DESC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(EventSummary {
                id: row.get(0)?,
                recorded_at: row.get(1)?,
                raw_text: row.get(2)?,
                source: row.get(3)?,
                status: row.get(4)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// 按 id 删除事件（AI 尝试失败时回滚 WriteDirect 写入用，见 conversation.rs）。
    /// events 的防改触发器只拦 UPDATE 特定列，不拦 DELETE。
    pub fn delete_event(&self, id: &str) -> Result<bool> {
        let n = self
            .connection
            .execute("DELETE FROM events WHERE id=?1", [id])?;
        Ok(n > 0)
    }

    /// 查某个「本地日历日」记录的事件（按 recorded_at，本地时区日界 → UTC 区间，倒序）。
    /// 用户说「6月20日有哪些事件」→ 用本地日界解释，跨时区也正确。
    pub fn events_on_date(&self, date: chrono::NaiveDate) -> Result<Vec<EventSummary>> {
        use chrono::{Local, TimeZone};
        let day_edges = |d: chrono::NaiveDate| {
            let naive_local = d.and_hms_opt(0, 0, 0).expect("midnight is valid");
            match Local
                .from_local_datetime(&naive_local)
                .single()
                .or_else(|| Local.from_local_datetime(&naive_local).earliest())
            {
                Some(dt) => dt.with_timezone(&chrono::Utc).to_rfc3339(),
                // DST 空洞等罕见情形：按 UTC 同名时刻兜底，避免 panic
                None => naive_local.and_utc().to_rfc3339(),
            }
        };
        let start_utc = day_edges(date);
        let end_utc = day_edges(date + chrono::Duration::days(1));
        let mut statement = self.connection.prepare(
            "SELECT id, recorded_at, raw_text, source, status FROM events
         WHERE recorded_at >= ?1 AND recorded_at < ?2
         ORDER BY recorded_at DESC, id DESC",
        )?;
        let rows = statement.query_map(params![start_utc, end_utc], |row| {
            Ok(EventSummary {
                id: row.get(0)?,
                recorded_at: row.get(1)?,
                raw_text: row.get(2)?,
                source: row.get(3)?,
                status: row.get(4)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// 统一日流：events 是权威全集；input_records 仅补充新流程的关联信息。
    /// 因此历史事件与新 Capture/对话输入都会出现，且每个 event 只返回一次。
    pub fn daily_entries(&self, date: chrono::NaiveDate) -> Result<Vec<DailyEntry>> {
        use chrono::{Local, TimeZone};
        let edge = |day: chrono::NaiveDate| {
            let local_midnight = day.and_hms_opt(0, 0, 0).expect("midnight is valid");
            Local
                .from_local_datetime(&local_midnight)
                .single()
                .or_else(|| Local.from_local_datetime(&local_midnight).earliest())
                .map(|value| value.with_timezone(&chrono::Utc).to_rfc3339())
                .unwrap_or_else(|| local_midnight.and_utc().to_rfc3339())
        };
        let start = edge(date);
        let end = edge(date + chrono::Duration::days(1));
        let mut statement = self.connection.prepare(
            "SELECT e.id,i.id,i.message_id,e.raw_text,e.source,e.status,e.recorded_at
         FROM events e LEFT JOIN input_records i ON i.event_id=e.id
         WHERE e.recorded_at>=?1 AND e.recorded_at<?2
         ORDER BY e.recorded_at DESC,e.id DESC",
        )?;
        let rows = statement.query_map(params![start, end], |row| {
            Ok(DailyEntry {
                event_id: row.get(0)?,
                input_id: row.get(1)?,
                message_id: row.get(2)?,
                raw_text: row.get(3)?,
                source: row.get(4)?,
                event_status: row.get(5)?,
                recorded_at: row.get(6)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// 最近 N 天内的最多 limit 条事件（按记录时间倒序）
    pub fn recent_events(&self, days: i64, limit: usize) -> Result<Vec<EventSummary>> {
        let since = (chrono::Utc::now() - chrono::Duration::days(days)).to_rfc3339();
        let mut statement = self.connection.prepare(
            "SELECT id, recorded_at, raw_text, source, status FROM events
         WHERE recorded_at >= ?1 ORDER BY recorded_at DESC, id DESC LIMIT ?2",
        )?;
        let rows = statement.query_map(params![since, limit as i64], |row| {
            Ok(EventSummary {
                id: row.get(0)?,
                recorded_at: row.get(1)?,
                raw_text: row.get(2)?,
                source: row.get(3)?,
                status: row.get(4)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// 保存一条 AI 生成的认知洞察（derived data，不影响原始事件）
    pub fn insert_insight(
        &self,
        window_days: i64,
        prompt_version: &str,
        lens: &str,
        title: &str,
        observation: &str,
        related_events: &[String],
        action: Option<&str>,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let related_raw = serde_json::to_string(related_events)?;
        self.connection.execute(
        "INSERT INTO insights
         (id, created_at, window_days, prompt_version, lens, title, observation, related_raw, action, status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'new')",
        params![
            id,
            now,
            window_days,
            prompt_version,
            lens,
            title,
            observation,
            related_raw,
            action,
        ],
    )?;
        Ok(id)
    }

    /// 列出全部已存洞察（新的在前）
    pub fn list_insights(&self) -> Result<Vec<InsightSummary>> {
        let mut statement = self.connection.prepare(
            "SELECT id, created_at, window_days, prompt_version, lens, title, observation,
                related_raw, action, status
         FROM insights ORDER BY created_at DESC",
        )?;
        let rows = statement.query_map([], |row| {
            let related_raw: String = row.get(7)?;
            let related_events: Vec<String> =
                serde_json::from_str(&related_raw).unwrap_or_default();
            Ok(InsightSummary {
                id: row.get(0)?,
                created_at: row.get(1)?,
                window_days: row.get(2)?,
                prompt_version: row.get(3)?,
                lens: row.get(4)?,
                title: row.get(5)?,
                observation: row.get(6)?,
                related_events,
                action: row.get(8)?,
                status: row.get(9)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }
}
