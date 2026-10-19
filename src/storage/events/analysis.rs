//! 分析作业生命周期：claim/complete/fail、退避与重启恢复、可记录性决策。

use super::*;

impl Store {
    pub fn claim_analysis_job(&self) -> Result<Option<AnalysisJob>> {
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        if !self.background_processing_allowed()? {
            transaction.commit()?;
            return Ok(None);
        }
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
        if !self.background_processing_allowed()? {
            return Ok(0);
        }
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
        self.require_background_processor()?;
        let now = chrono::Utc::now().to_rfc3339();
        let result_id = Uuid::new_v4().to_string();
        transaction.execute("INSERT INTO event_analyses (id,event_id,prompt_version,result_json,created_at) VALUES (?1,?2,?3,?4,?5)", params![result_id, job.event_id, prompt_version, result_json, now])?;
        transaction.execute(
            "UPDATE analysis_jobs SET status='succeeded', updated_at=?2 WHERE id=?1",
            params![job.id, now],
        )?;
        transaction.execute(
            "UPDATE events SET status='processed', processed_at=?2, updated_at=?2 WHERE id=?1",
            params![job.event_id, now],
        )?;
        self.sync_record_analysis(&result_id)?;
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
}
