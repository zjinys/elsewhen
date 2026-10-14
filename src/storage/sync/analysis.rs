//! Analysis is produced only by the designated processor. Receiving terminal
//! results updates read models, never grants generation or business execution.

use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, params};

use super::{Apply, Store, hash};
use crate::sync::protocol::{AnalysisRecord, SyncMutation, SyncOperation, analysis_task_id};

impl Store {
    pub(crate) fn sync_enqueue_imported_event(&self, event_id: &str) -> Result<()> {
        if !self.background_processing_allowed()? {
            return Ok(());
        }
        let identity = self.sync_identity()?.context("sync_not_initialized")?;
        let event = self
            .sync_event_record(event_id)?
            .context("sync_event_missing")?;
        let now = chrono::Utc::now();
        let now_text = now.to_rfc3339();
        let task = analysis_task_id(
            &identity.dataset_id,
            event_id,
            &hash(event.raw_text.as_bytes()),
            "event-analysis",
        );
        self.connection.execute(
            "INSERT INTO analysis_jobs(id,event_id,status,attempts,available_at,created_at,updated_at) SELECT ?1,?2,'pending',0,?3,?3,?3 WHERE NOT EXISTS(SELECT 1 FROM analysis_jobs WHERE event_id=?2) AND NOT EXISTS(SELECT 1 FROM event_analyses WHERE event_id=?2)",
            params![task,event_id,now_text],
        )?;
        super::super::digest::enqueue_digest_job_on(&self.connection, event_id, now)?;
        Ok(())
    }

    pub(crate) fn sync_record_analysis(&self, id: &str) -> Result<()> {
        let Some(identity) = self.sync_identity()? else {
            return Ok(());
        };
        self.require_background_processor()?;
        let (event_id,prompt_version,result_json,created_at): (String,String,String,String) = self.connection.query_row(
            "SELECT event_id,prompt_version,result_json,created_at FROM event_analyses WHERE id=?1",[id],
            |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)),
        )?;
        let event = self
            .sync_event_record(&event_id)?
            .context("sync_event_missing")?;
        let input_hash = hash(event.raw_text.as_bytes());
        let task_id = analysis_task_id(
            &identity.dataset_id,
            &event_id,
            &input_hash,
            &prompt_version,
        );
        self.append_sync_mutation(SyncMutation::AnalysisCompleted {
            analysis: AnalysisRecord {
                id: id.into(),
                event_id,
                task_id,
                input_hash,
                prompt_version,
                result_json,
                created_at,
            },
        })?;
        Ok(())
    }

    pub(super) fn apply_sync_analysis(
        &self,
        op: &SyncOperation,
        analysis: &AnalysisRecord,
    ) -> Result<Apply> {
        let identity = self.sync_identity()?.context("sync_not_initialized")?;
        // Analysis may originate from the current processor, or from a previous
        // processor within the cryptographic handoff boundary (delayed ops
        // authored before the yield are legitimate; beyond it they are not).
        if op.origin_device_id != identity.processor_device_id {
            let handoff_sequence: Option<i64> = self
                .connection
                .query_row(
                    "SELECT MAX(from_sequence) FROM sync_processor_handoffs WHERE from_device_id=?1",
                    [&op.origin_device_id],
                    |r| r.get(0),
                )
                .optional()?;
            let within_handoff = handoff_sequence.is_some_and(|max| op.origin_sequence <= max);
            ensure!(within_handoff, "sync_analysis_wrong_processor");
        }
        let Some(event) = self.sync_event_record(&analysis.event_id)? else {
            return Ok(Apply::Waiting);
        };
        ensure!(
            hash(event.raw_text.as_bytes()) == analysis.input_hash
                && analysis.task_id
                    == analysis_task_id(
                        &identity.dataset_id,
                        &analysis.event_id,
                        &analysis.input_hash,
                        &analysis.prompt_version
                    ),
            "sync_analysis_input_mismatch"
        );
        let existing: Option<(String,String,String,String)> = self.connection.query_row(
            "SELECT event_id,prompt_version,result_json,created_at FROM event_analyses WHERE id=?1",[&analysis.id],
            |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)),
        ).optional()?;
        if let Some(existing) = existing {
            ensure!(
                existing
                    == (
                        analysis.event_id.clone(),
                        analysis.prompt_version.clone(),
                        analysis.result_json.clone(),
                        analysis.created_at.clone()
                    ),
                "sync_analysis_identity_conflict"
            );
        } else {
            self.connection.execute("INSERT INTO event_analyses(id,event_id,prompt_version,result_json,created_at) VALUES(?1,?2,?3,?4,?5)",
                params![analysis.id,analysis.event_id,analysis.prompt_version,analysis.result_json,analysis.created_at])?;
        }
        // Project completion for event detail queries without creating a claim,
        // dispatch, AI run or approval. The source operation order determines the
        // sequence of results, not the receiver's wall clock.
        let running: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM analysis_jobs WHERE event_id=?1 AND status='running')",
            [&analysis.event_id],
            |r| r.get(0),
        )?;
        ensure!(!running, "sync_analysis_local_execution_conflict");
        self.connection.execute("INSERT INTO analysis_jobs(id,event_id,status,attempts,available_at,created_at,updated_at) SELECT ?1,?2,'succeeded',0,?3,?3,?3 WHERE NOT EXISTS(SELECT 1 FROM analysis_jobs WHERE event_id=?2)",
            params![analysis.task_id,analysis.event_id,analysis.created_at])?;
        self.connection.execute("UPDATE analysis_jobs SET status='succeeded',last_error=NULL,updated_at=?2 WHERE event_id=?1",
            params![analysis.event_id,analysis.created_at])?;
        self.connection.execute(
            "UPDATE events SET status='processed',processed_at=?2,updated_at=?2 WHERE id=?1",
            params![analysis.event_id, analysis.created_at],
        )?;
        Ok(Apply::Applied)
    }
}

#[cfg(test)]
mod tests;
