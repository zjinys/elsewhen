//! Queue selection and scheduling metadata are independent from node receipts.
use super::*;

impl Store {
    pub(crate) fn prefer_event_reading(&self) -> Result<bool> {
        Ok(self.connection.query_row(
            "SELECT prefer_reading FROM event_reading_schedule WHERE singleton=1",
            [],
            |r| r.get(0),
        )?)
    }
    pub(crate) fn set_digest_lane(&self, reading: bool) -> Result<()> {
        self.require_background_processor()?;
        self.connection.execute(
            "UPDATE event_reading_schedule SET prefer_reading=?1 WHERE singleton=1",
            [!reading],
        )?;
        Ok(())
    }

    pub(crate) fn next_event_reading(&self) -> Result<Option<ReadingTask>> {
        let now = chrono::Utc::now();
        let id:Option<String>=self.connection.query_row("SELECT t.id FROM event_reading_tasks t JOIN knowledge_digest_jobs j ON j.event_id=t.event_id AND j.digest_version=t.digest_version AND j.prompt_bundle_id=t.bundle_id WHERE t.status IN ('active','ready') AND t.available_at<=?1 AND (t.lease_token IS NULL OR t.lease_until<=?1 OR t.authority_generation<>(SELECT generation FROM event_reading_authority WHERE singleton=1)) AND j.status IN ('pending','retry') AND j.available_at<=?2 ORDER BY t.last_tick,t.created_at,t.id LIMIT 1",params![now.timestamp(),now.to_rfc3339()],|r|r.get(0)).optional()?;
        id.map(|id| self.event_reading_task(&id)).transpose()
    }

    /// Preparation has made no model request. Remove the provisional run and
    /// return every event without charging an attempt; preserve pinned bundles.
    pub(crate) fn park_digest_for_reading(&self, batch: &DigestBatch) -> Result<()> {
        let tx = self.reading_tx()?;
        self.require_background_processor()?;
        let changed=tx.execute("UPDATE knowledge_digest_jobs SET status='pending',batch_id=NULL,attempts=MAX(0,attempts-1),updated_at=?2 WHERE batch_id=?1 AND status='running'",params![batch.id,chrono::Utc::now().to_rfc3339()])?;
        ensure!(
            changed == batch.events.len(),
            "event_reading_digest_claim_stale"
        );
        tx.execute(
            "DELETE FROM knowledge_digest_runs WHERE id=?1 AND status='running'",
            [&batch.id],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn record_event_reading_tick(
        &self,
        id: &str,
        error: Option<&str>,
        waiting: bool,
    ) -> Result<()> {
        let tx = self.reading_tx()?;
        self.require_background_processor()?;
        let now = chrono::Utc::now().timestamp();
        let retry: i64 = if error.is_some() {
            tx.query_row("SELECT COALESCE(MAX(available_at),?2) FROM event_reading_nodes WHERE task_id=?1 AND status='pending'",params![id,now+120],|r|r.get::<_,i64>(0))?.max(now+120)
        } else if waiting {
            now + 5
        } else {
            now
        };
        tx.execute(
            "UPDATE event_reading_schedule SET ticks=ticks+1 WHERE singleton=1",
            [],
        )?;
        tx.execute("UPDATE event_reading_tasks SET last_tick=(SELECT ticks FROM event_reading_schedule WHERE singleton=1),updated_at=?2,available_at=?3,last_error=?4 WHERE id=?1 AND status IN ('active','ready')",params![id,now,retry,error.map(|e|e.chars().take(300).collect::<String>())])?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn claim_event_reading_batch(
        &self,
        task: &ReadingTask,
        model: Option<&str>,
    ) -> Result<Option<DigestBatch>> {
        let tx = self.reading_tx()?;
        self.require_reading_current(task)?;
        let now = chrono::Utc::now();
        let id = uuid::Uuid::new_v4().to_string();
        let count=tx.execute("UPDATE knowledge_digest_jobs SET status='running',attempts=attempts+1,batch_id=?2,updated_at=?3 WHERE event_id=?1 AND digest_version=?4 AND prompt_bundle_id=?5 AND status IN ('pending','retry') AND available_at<=?3",params![task.event_id,id,now.to_rfc3339(),super::super::DIGEST_VERSION,task.bundle_id])?;
        if count == 0 {
            return Ok(None);
        }
        tx.execute("INSERT INTO knowledge_digest_runs(id,started_at,status,event_count,model,prompt_bundle_id) VALUES(?1,?2,'running',1,?3,?4)",params![id,now.to_rfc3339(),model,task.bundle_id])?;
        let event = tx.query_row(
            "SELECT id,recorded_at,raw_text FROM events WHERE id=?1",
            [&task.event_id],
            |r| {
                Ok(super::super::EventRecord {
                    id: r.get(0)?,
                    recorded_at: r.get(1)?,
                    raw_text: r.get(2)?,
                })
            },
        )?;
        tx.commit()?;
        Ok(Some(DigestBatch {
            id,
            started_at: now.to_rfc3339(),
            events: vec![event],
        }))
    }
}
