//! Event-scoped local reading cache. No model calls or event acknowledgement here.
use super::{DigestBatch, Store, knowledge::content_hash};
use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

pub(crate) const STRATEGY: &str = "event-reading-v1";
pub(crate) const MAX_NODES: i64 = 4096;
pub(crate) const MAX_CACHE_BYTES: i64 = 16 * 1024 * 1024;
pub(crate) const MAX_OUTPUT_BYTES: usize = 16384;
const LEASE_SECONDS: i64 = 600;
mod finalization;
mod progress;
mod scheduling;
mod tree;
pub(crate) use finalization::FinalReadingLease;

#[derive(Debug, Clone)]
pub(crate) struct ReadingTask {
    pub id: String,
    pub event_id: String,
    pub bundle_id: String,
    pub total_chars: usize,
    pub status: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ReadingNode {
    pub id: String,
    pub task_id: String,
    pub kind: String,
    pub level: usize,
    pub start: usize,
    pub end: usize,
    pub input_hash: String,
    pub output: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ReadingLease {
    pub node: ReadingNode,
    pub generation: i64,
    pub token: String,
    pub authority: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadingQuote {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadingOutput {
    pub summary: String,
    pub quotes: Vec<ReadingQuote>,
}

impl Store {
    fn reading_tx(&self) -> Result<Transaction<'_>> {
        Ok(Transaction::new_unchecked(
            &self.connection,
            TransactionBehavior::Immediate,
        )?)
    }

    pub(crate) fn event_reading_task(&self, id: &str) -> Result<ReadingTask> {
        Ok(self.connection.query_row(
            "SELECT id,event_id,bundle_id,total_chars,status FROM event_reading_tasks WHERE id=?1",
            [id],
            |r| {
                Ok(ReadingTask {
                    id: r.get(0)?,
                    event_id: r.get(1)?,
                    bundle_id: r.get(2)?,
                    total_chars: r.get::<_, i64>(3)? as usize,
                    status: r.get(4)?,
                })
            },
        )?)
    }

    /// Creation is tied to the claimed singleton's pinned bundle. Preparation
    /// never acknowledges the digest job, nor increments its failure counter.
    pub(crate) fn ensure_event_reading(&self, batch: &DigestBatch) -> Result<ReadingTask> {
        ensure!(
            batch.events.len() == 1,
            "event_reading_requires_single_event"
        );
        let tx = self.reading_tx()?;
        self.require_background_processor()?;
        let event = &batch.events[0];
        ensure!(
            self.recordable_event(&event.id)?,
            "event_reading_not_recordable"
        );
        let bundle = self.digest_prompt_bundle(&batch.id)?;
        let active: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM knowledge_digest_jobs WHERE event_id=?1 AND batch_id=?2 AND status='running' AND prompt_bundle_id=?3)",params![event.id,batch.id,bundle],|r|r.get(0))?;
        ensure!(active, "event_reading_digest_claim_stale");
        let original: String = tx.query_row(
            "SELECT raw_text FROM events WHERE id=?1",
            [&event.id],
            |r| r.get(0),
        )?;
        ensure!(original == event.raw_text, "event_reading_content_changed");
        let chars = original.chars().count();
        ensure!(
            (1..=2_000_000).contains(&chars),
            "event_reading_material_limit"
        );
        let fingerprint: String = tx.query_row(
            "SELECT bundle_hash FROM prompt_bundles WHERE id=?1",
            [&bundle],
            |r| r.get(0),
        )?;
        let dependency = content_hash(&format!("{STRATEGY}:{fingerprint}"));
        let hash = content_hash(&original);
        let existing: Option<String> = tx.query_row("SELECT id FROM event_reading_tasks WHERE event_id=?1 AND digest_version=?2 AND content_hash=?3 AND bundle_id=?4 AND strategy=?5",params![event.id,super::DIGEST_VERSION,hash,bundle,STRATEGY],|r|r.get(0)).optional()?;
        let id = existing.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let now = chrono::Utc::now().timestamp();
        tx.execute("INSERT OR IGNORE INTO event_reading_tasks(id,event_id,digest_version,content_hash,total_chars,bundle_id,strategy,dependency_hash,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?9)",params![id,event.id,super::DIGEST_VERSION,hash,chars as i64,bundle,STRATEGY,dependency,now])?;
        // An explicit new eligible claim can resume a cancelled cache, but can
        // never reuse a lease from before the classification change.
        tx.execute("UPDATE event_reading_nodes SET status='pending' WHERE task_id=?1 AND status='running' AND EXISTS(SELECT 1 FROM event_reading_tasks WHERE id=?1 AND status='cancelled')",[&id])?;
        tx.execute("UPDATE event_reading_tasks SET status='active',generation=generation+1,last_error=NULL,available_at=0,updated_at=?2 WHERE id=?1 AND status='cancelled'",params![id,now])?;
        tx.commit()?;
        self.event_reading_task(&id)
    }

    pub(crate) fn event_reading_text(&self, task: &ReadingTask) -> Result<String> {
        self.require_reading_current(task)?;
        let (text, hash): (String,String) = self.connection.query_row(
            "SELECT e.raw_text,t.content_hash FROM event_reading_tasks t JOIN events e ON e.id=t.event_id WHERE t.id=?1", [&task.id], |r|Ok((r.get(0)?,r.get(1)?)))?;
        ensure!(
            content_hash(&text) == hash && text.chars().count() == task.total_chars,
            "event_reading_content_changed"
        );
        Ok(text)
    }

    pub(crate) fn event_reading_nodes(&self, task: &str) -> Result<Vec<ReadingNode>> {
        Ok(self.connection.prepare("SELECT id,task_id,kind,level,start_char,end_char,input_hash,output_json FROM event_reading_nodes WHERE task_id=?1 ORDER BY level,start_char")?.query_map([task],|r|Ok(ReadingNode {
            id:r.get(0)?,task_id:r.get(1)?,kind:r.get(2)?,level:r.get::<_,i64>(3)? as usize,start:r.get::<_,i64>(4)? as usize,end:r.get::<_,i64>(5)? as usize,input_hash:r.get(6)?,output:r.get(7)?,
        }))?.collect::<rusqlite::Result<_>>()?)
    }

    fn require_reading_current(&self, task: &ReadingTask) -> Result<()> {
        self.require_background_processor()?;
        ensure!(
            matches!(task.status.as_str(), "active" | "ready"),
            "event_reading_task_inactive"
        );
        ensure!(
            self.recordable_event(&task.event_id)?,
            "event_reading_not_recordable"
        );
        let current: bool = self.connection.query_row("SELECT EXISTS(SELECT 1 FROM knowledge_digest_jobs WHERE event_id=?1 AND digest_version=?2 AND status IN ('pending','retry','running','failed') AND prompt_bundle_id=?3)",params![task.event_id,super::DIGEST_VERSION,task.bundle_id],|r|r.get(0))?;
        ensure!(current, "event_reading_digest_job_stale");
        Ok(())
    }

    /// Append one leaf, or form a parent from completed consecutive frontier
    /// nodes. Dependencies bind exact immutable output hashes, never lengths.
    pub(crate) fn prepare_event_reading_node(
        &self,
        task_id: &str,
        start: usize,
        end: usize,
        children: &[String],
    ) -> Result<ReadingNode> {
        let tx = self.reading_tx()?;
        let task = self.event_reading_task(task_id)?;
        self.require_reading_current(&task)?;
        ensure!(
            start < end && end <= task.total_chars,
            "event_reading_range_invalid"
        );
        ensure!(
            children.is_empty() || (2..=8).contains(&children.len()),
            "event_reading_fan_in_limit"
        );
        let nodes = self.event_reading_nodes(task_id)?;
        let mut dependencies = vec![];
        let (kind, level, input) = if children.is_empty() {
            let text: String = tx.query_row(
                "SELECT raw_text FROM events WHERE id=?1",
                [&task.event_id],
                |r| r.get(0),
            )?;
            (
                "leaf",
                0,
                text.chars()
                    .skip(start)
                    .take(end - start)
                    .collect::<String>(),
            )
        } else {
            let mut cursor = start;
            let mut level = 0;
            for id in children {
                let child = nodes
                    .iter()
                    .find(|n| &n.id == id)
                    .context("event_reading_dependency_missing")?;
                let output = child
                    .output
                    .as_ref()
                    .context("event_reading_dependency_unfinished")?;
                ensure!(
                    child.start == cursor,
                    "event_reading_dependency_gap_or_overlap"
                );
                cursor = child.end;
                level = level.max(child.level + 1);
                dependencies.push((id.clone(), content_hash(output)));
            }
            ensure!(
                cursor == end && level <= 16,
                "event_reading_dependency_coverage_or_depth"
            );
            ("merge", level, serde_json::to_string(&dependencies)?)
        };
        let hash = content_hash(&input);
        if let Some(n) = nodes
            .iter()
            .find(|n| n.kind == kind && n.level == level && n.start == start && n.end == end)
        {
            ensure!(n.input_hash == hash, "event_reading_node_identity_conflict");
            tx.commit()?;
            return Ok(n.clone());
        }
        ensure!(task.status == "active", "event_reading_already_ready");
        ensure!((nodes.len() as i64) < MAX_NODES, "event_reading_node_limit");
        if children.is_empty() {
            let frontier = nodes
                .iter()
                .filter(|n| n.kind == "leaf")
                .map(|n| n.end)
                .max()
                .unwrap_or(0);
            ensure!(start == frontier, "event_reading_leaf_gap_or_overlap");
        }
        let id = uuid::Uuid::new_v4().to_string();
        tx.execute("INSERT INTO event_reading_nodes(id,task_id,kind,level,start_char,end_char,input_hash) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![id,task_id,kind,level as i64,start as i64,end as i64,hash])?;
        for (position, (child, hash)) in dependencies.iter().enumerate() {
            tx.execute("INSERT INTO event_reading_dependencies(task_id,parent_id,position,child_id,output_hash) VALUES(?1,?2,?3,?4,?5)",params![task_id,id,position as i64,child,hash])?;
        }
        tx.commit()?;
        Ok(ReadingNode {
            id,
            task_id: task_id.into(),
            kind: kind.into(),
            level,
            start,
            end,
            input_hash: hash,
            output: None,
        })
    }

    pub(crate) fn claim_event_reading_node(
        &self,
        task_id: &str,
        now: i64,
    ) -> Result<Option<ReadingLease>> {
        let tx = self.reading_tx()?;
        let task = self.event_reading_task(task_id)?;
        self.require_reading_current(&task)?;
        // Recovery and claim share the write transaction. A healthy lease is
        // retained across application initialization and competing workers.
        let busy: bool = tx.query_row("SELECT lease_token IS NOT NULL AND lease_until>?2 AND authority_generation=(SELECT generation FROM event_reading_authority WHERE singleton=1) FROM event_reading_tasks WHERE id=?1",params![task_id,now],|r|r.get(0))?;
        if busy {
            return Ok(None);
        }
        tx.execute(
            "UPDATE event_reading_nodes SET status='pending' WHERE task_id=?1 AND status='running'",
            [task_id],
        )?;
        let id: Option<String> = tx.query_row("SELECT id FROM event_reading_nodes WHERE task_id=?1 AND status='pending' AND available_at<=?2 ORDER BY level,start_char LIMIT 1",params![task_id,now],|r|r.get(0)).optional()?;
        let Some(id) = id else {
            tx.execute("UPDATE event_reading_tasks SET lease_token=NULL,lease_until=NULL,generation=generation+1 WHERE id=?1 AND lease_token IS NOT NULL",[task_id])?;
            tx.commit()?;
            return Ok(None);
        };
        let authority: i64 = tx.query_row(
            "SELECT generation FROM event_reading_authority WHERE singleton=1",
            [],
            |r| r.get(0),
        )?;
        let token = uuid::Uuid::new_v4().to_string();
        tx.execute("UPDATE event_reading_tasks SET generation=generation+1,lease_token=?2,lease_until=?3,authority_generation=?4,updated_at=?5 WHERE id=?1",params![task_id,token,now+LEASE_SECONDS,authority,now])?;
        tx.execute(
            "UPDATE event_reading_nodes SET status='running',attempts=attempts+1 WHERE id=?1",
            [&id],
        )?;
        let generation: i64 = tx.query_row(
            "SELECT generation FROM event_reading_tasks WHERE id=?1",
            [task_id],
            |r| r.get(0),
        )?;
        let node = self
            .event_reading_nodes(task_id)?
            .into_iter()
            .find(|n| n.id == id)
            .context("event_reading_node_missing")?;
        tx.commit()?;
        Ok(Some(ReadingLease {
            node,
            generation,
            token,
            authority,
        }))
    }

    fn require_event_reading_lease(&self, lease: &ReadingLease, now: i64) -> Result<()> {
        let task = self.event_reading_task(&lease.node.task_id)?;
        self.require_reading_current(&task)?;
        let valid: bool = self.connection.query_row("SELECT EXISTS(SELECT 1 FROM event_reading_tasks t JOIN event_reading_authority a ON a.singleton=1 JOIN event_reading_nodes n ON n.task_id=t.id WHERE t.id=?1 AND t.generation=?2 AND t.lease_token=?3 AND t.lease_until>?4 AND t.authority_generation=?5 AND a.generation=?5 AND n.id=?6 AND n.status='running' AND n.kind=?7 AND n.level=?8 AND n.start_char=?9 AND n.end_char=?10 AND n.input_hash=?11)",params![task.id,lease.generation,lease.token,now,lease.authority,lease.node.id,lease.node.kind,lease.node.level as i64,lease.node.start as i64,lease.node.end as i64,lease.node.input_hash],|r|r.get(0))?;
        ensure!(valid, "event_reading_lease_stale");
        Ok(())
    }

    pub(crate) fn renew_event_reading_lease(&self, lease: &ReadingLease, now: i64) -> Result<()> {
        let tx = self.reading_tx()?;
        self.require_event_reading_lease(lease, now)?;
        tx.execute(
            "UPDATE event_reading_tasks SET lease_until=?2 WHERE id=?1",
            params![lease.node.task_id, now + LEASE_SECONDS],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn fail_event_reading_node(
        &self,
        lease: &ReadingLease,
        error: &str,
        now: i64,
    ) -> Result<()> {
        let tx = self.reading_tx()?;
        self.require_event_reading_lease(lease, now)?;
        let attempts: i64 = tx.query_row(
            "SELECT attempts FROM event_reading_nodes WHERE id=?1",
            [&lease.node.id],
            |r| r.get(0),
        )?;
        let delay = if attempts >= 5 {
            21600
        } else {
            60 * (1_i64 << attempts)
        };
        tx.execute("UPDATE event_reading_nodes SET status='pending',available_at=?2,last_error=?3 WHERE id=?1",params![lease.node.id,now+delay,error.chars().take(300).collect::<String>()])?;
        tx.execute("UPDATE event_reading_tasks SET lease_token=NULL,lease_until=NULL,updated_at=?2 WHERE id=?1",params![lease.node.task_id,now])?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn complete_event_reading_node(
        &self,
        lease: &ReadingLease,
        result: &ReadingOutput,
        now: i64,
    ) -> Result<()> {
        let output = serde_json::to_string(result)?;
        ensure!(
            output.len() <= MAX_OUTPUT_BYTES
                && !result.summary.trim().is_empty()
                && result.summary.chars().count() <= 1600
                && result.quotes.len() <= 8,
            "event_reading_output_limit"
        );
        let tx = self.reading_tx()?;
        let task = self.event_reading_task(&lease.node.task_id)?;
        self.require_reading_current(&task)?;
        let authority: i64 = tx.query_row(
            "SELECT generation FROM event_reading_authority WHERE singleton=1",
            [],
            |r| r.get(0),
        )?;
        ensure!(
            authority == lease.authority,
            "event_reading_authority_stale"
        );
        let hash = content_hash(&output);
        let receipt: Option<(String,String,i64)> = tx.query_row("SELECT output_hash,receipt_token,receipt_generation FROM event_reading_nodes WHERE id=?1 AND status='succeeded'",[&lease.node.id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        if let Some((old, token, generation)) = receipt {
            ensure!(
                old == hash && token == lease.token && generation == lease.generation,
                "event_reading_receipt_conflict"
            );
            tx.commit()?;
            return Ok(());
        }
        self.require_event_reading_lease(lease, now)?;
        let original: String = tx.query_row(
            "SELECT raw_text FROM events WHERE id=?1",
            [&task.event_id],
            |r| r.get(0),
        )?;
        let chars: Vec<char> = original.chars().collect();
        self.validate_event_reading_output(&lease.node, result, &chars)?;
        let used: i64 = tx.query_row("SELECT COALESCE(SUM(length(CAST(output_json AS BLOB))),0) FROM event_reading_nodes WHERE task_id=?1",[&task.id],|r|r.get(0))?;
        ensure!(
            used + output.len() as i64 <= MAX_CACHE_BYTES,
            "event_reading_cache_limit"
        );
        tx.execute("UPDATE event_reading_nodes SET status='succeeded',output_json=?2,output_hash=?3,receipt_token=?4,receipt_generation=?5,last_error=NULL WHERE id=?1",params![lease.node.id,output,hash,lease.token,lease.generation])?;
        tx.execute("UPDATE event_reading_tasks SET lease_token=NULL,lease_until=NULL,updated_at=?2 WHERE id=?1",params![task.id,now])?;
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
