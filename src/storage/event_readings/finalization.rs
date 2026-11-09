use super::*;

#[derive(Debug, Clone)]
pub(crate) struct FinalReadingLease {
    pub task_id: String,
    pub root_id: String,
    pub root_hash: String,
    pub generation: i64,
    pub token: String,
    pub authority: i64,
}

impl Store {
    pub(crate) fn claim_event_reading_final(
        &self,
        task_id: &str,
        batch: &DigestBatch,
        now: i64,
    ) -> Result<Option<FinalReadingLease>> {
        let tx = self.reading_tx()?;
        let task = self.event_reading_task(task_id)?;
        self.require_reading_current(&task)?;
        ensure!(task.status == "ready", "event_reading_root_incomplete");
        self.require_reading_batch(&task, batch)?;
        let root = self.event_reading_root(&task)?;
        let busy:bool=tx.query_row("SELECT lease_token IS NOT NULL AND lease_until>?2 AND authority_generation=(SELECT generation FROM event_reading_authority WHERE singleton=1) FROM event_reading_tasks WHERE id=?1",params![task_id,now],|r|r.get(0))?;
        if busy {
            return Ok(None);
        }
        let authority: i64 = tx.query_row(
            "SELECT generation FROM event_reading_authority WHERE singleton=1",
            [],
            |r| r.get(0),
        )?;
        let token = uuid::Uuid::new_v4().to_string();
        tx.execute("UPDATE event_reading_tasks SET generation=generation+1,lease_token=?2,lease_until=?3,authority_generation=?4,updated_at=?5 WHERE id=?1",params![task_id,token,now+LEASE_SECONDS,authority,now])?;
        let generation = tx.query_row(
            "SELECT generation FROM event_reading_tasks WHERE id=?1",
            [task_id],
            |r| r.get(0),
        )?;
        tx.commit()?;
        Ok(Some(FinalReadingLease {
            task_id: task_id.into(),
            root_id: root.id,
            root_hash: content_hash(root.output.as_ref().unwrap()),
            generation,
            token,
            authority,
        }))
    }

    fn require_reading_batch(&self, task: &ReadingTask, batch: &DigestBatch) -> Result<()> {
        ensure!(
            batch.events.len() == 1
                && batch.events[0].id == task.event_id
                && self.digest_prompt_bundle(&batch.id)? == task.bundle_id,
            "event_reading_digest_claim_stale"
        );
        let valid:bool=self.connection.query_row("SELECT EXISTS(SELECT 1 FROM knowledge_digest_jobs WHERE event_id=?1 AND batch_id=?2 AND status='running' AND prompt_bundle_id=?3)",params![task.event_id,batch.id,task.bundle_id],|r|r.get(0))?;
        ensure!(valid, "event_reading_digest_claim_stale");
        Ok(())
    }

    pub(crate) fn require_event_reading_final(
        &self,
        lease: &FinalReadingLease,
        batch: &DigestBatch,
        now: i64,
    ) -> Result<()> {
        let task = self.event_reading_task(&lease.task_id)?;
        self.require_reading_current(&task)?;
        self.require_reading_batch(&task, batch)?;
        let valid:bool=self.connection.query_row("SELECT EXISTS(SELECT 1 FROM event_reading_tasks t JOIN event_reading_authority a ON a.singleton=1 WHERE t.id=?1 AND t.status='ready' AND t.generation=?2 AND t.lease_token=?3 AND t.lease_until>?4 AND t.authority_generation=?5 AND a.generation=?5)",params![lease.task_id,lease.generation,lease.token,now,lease.authority],|r|r.get(0))?;
        ensure!(valid, "event_reading_final_lease_stale");
        let root = self.event_reading_root(&task)?;
        ensure!(
            root.id == lease.root_id
                && content_hash(root.output.as_ref().unwrap()) == lease.root_hash,
            "event_reading_root_changed"
        );
        Ok(())
    }

    pub(crate) fn renew_event_reading_final(
        &self,
        lease: &FinalReadingLease,
        batch: &DigestBatch,
        now: i64,
    ) -> Result<()> {
        let tx = self.reading_tx()?;
        self.require_event_reading_final(lease, batch, now)?;
        tx.execute(
            "UPDATE event_reading_tasks SET lease_until=?2 WHERE id=?1",
            params![lease.task_id, now + LEASE_SECONDS],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn release_event_reading_final(
        &self,
        lease: &FinalReadingLease,
        batch: &DigestBatch,
        now: i64,
    ) -> Result<()> {
        let tx = self.reading_tx()?;
        self.require_event_reading_final(lease, batch, now)?;
        tx.execute("UPDATE event_reading_tasks SET lease_token=NULL,lease_until=NULL,updated_at=?2 WHERE id=?1",params![lease.task_id,now])?;
        tx.commit()?;
        Ok(())
    }
}
