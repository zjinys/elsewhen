//! Durable generation eligibility for saved inputs. Withdrawing never edits history.
use super::{Store, chat_runs::ChatRun};
use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, params};

#[derive(Debug, Clone)]
pub struct QueuedChatInput {
    pub id: i64,
    pub input_message_id: String,
    pub status: String,
    pub mode: String,
    pub preview: String,
    pub defer_reason: Option<String>,
    pub run_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UnscheduledChatInput {
    pub input_message_id: String,
    pub preview: String,
    pub created_at: String,
}

impl Store {
    /// Absence of a durable run/queue association is not inferred from the next
    /// assistant message: an earlier run may finish after this input was saved.
    pub(crate) fn unscheduled_chat_inputs(
        &self,
        conversation: &str,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<UnscheduledChatInput>> {
        Ok(self
            .connection
            .prepare(
                "SELECT m.id,substr(m.content,1,160),m.created_at FROM messages m
             WHERE m.conversation_id=?1 AND m.role='user'
               AND NOT EXISTS(SELECT 1 FROM chat_runs r WHERE r.input_message_id=m.id)
               AND NOT EXISTS(SELECT 1 FROM chat_input_queue q WHERE q.input_message_id=m.id)
             ORDER BY m.created_at DESC,m.id DESC LIMIT ?2 OFFSET ?3",
            )?
            .query_map(params![conversation, limit.clamp(1, 100), offset], |r| {
                Ok(UnscheduledChatInput {
                    input_message_id: r.get(0)?,
                    preview: r.get(1)?,
                    created_at: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?)
    }

    /// The save has already committed. Eligibility and the idle/FIFO decision
    /// share one write transaction so a new question cannot jump the queue.
    pub(crate) fn schedule_chat_input(
        &self,
        conversation: &str,
        input: &str,
        dispatcher: &str,
        owner: &str,
    ) -> Result<Option<ChatRun>> {
        self.recover_expired_chat_runs()?;
        let tx = self.begin_write_transaction()?;
        let has_dispatch_state: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM chat_queue_dispatch WHERE conversation_id=?1)",
            [conversation],
            |r| r.get(0),
        )?;
        let paused = has_dispatch_state && self.chat_queue_paused(conversation, dispatcher)?;
        let id = self.enqueue_chat_input(conversation, input)?;
        let (status, run): (String, Option<String>) = self.connection.query_row(
            "SELECT status,run_id FROM chat_input_queue WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if status != "pending" {
            let existing = run.map(|id| self.chat_run(&id)).transpose()?.flatten();
            if let Some(tx) = tx {
                tx.commit()?;
            }
            return Ok(existing);
        }
        let blocked: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM chat_runs WHERE conversation_id=?1 AND status IN ('preparing','running','cancelling')) OR EXISTS(SELECT 1 FROM chat_input_queue WHERE conversation_id=?1 AND status='pending' AND id<?2)",
            params![conversation,id], |r| r.get(0))?;
        let run = if blocked || paused {
            None
        } else {
            self.resume_chat_queue(conversation, dispatcher)?;
            Some(self.create_chat_run(
                conversation,
                input,
                &format!("queued-input:{id}:{input}"),
                owner,
            )?)
        };
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(run)
    }

    pub(crate) fn recent_chat_injections(
        &self,
        conversation: &str,
    ) -> Result<Vec<QueuedChatInput>> {
        Ok(self.connection.prepare(
            "SELECT id,input_message_id,status,mode,run_id,(SELECT substr(content,1,120) FROM messages WHERE id=input_message_id),defer_reason FROM chat_input_queue WHERE conversation_id=?1 AND status='injected' ORDER BY id DESC LIMIT 10",
        )?.query_map([conversation], |r| Ok(QueuedChatInput {
            id:r.get(0)?,input_message_id:r.get(1)?,status:r.get(2)?,mode:r.get(3)?,run_id:r.get(4)?,preview:r.get(5)?,defer_reason:r.get(6)?,
        }))?.collect::<rusqlite::Result<_>>()?)
    }

    pub(crate) fn enqueue_chat_input(&self, conversation: &str, input: &str) -> Result<i64> {
        let tx = self.begin_write_transaction()?;
        let valid: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM messages WHERE id=?1 AND conversation_id=?2 AND role='user')",
            params![input, conversation], |r| r.get(0),
        )?;
        ensure!(valid, "排队输入不存在或不属于此会话");
        let existing: Option<i64> = self
            .connection
            .query_row(
                "SELECT id FROM chat_input_queue WHERE input_message_id=?1",
                [input],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(id) = existing {
            if let Some(tx) = tx {
                tx.commit()?;
            }
            return Ok(id);
        }
        let started: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM chat_runs WHERE conversation_id=?1 AND input_message_id=?2)",
            params![conversation, input], |r| r.get(0),
        )?;
        ensure!(
            !started,
            "此输入已有生成记录，请使用明确的生成重试而非再次排队"
        );
        let now = chrono::Utc::now().to_rfc3339();
        self.connection.execute(
            "INSERT INTO chat_input_queue(conversation_id,input_message_id,created_at,updated_at) VALUES(?1,?2,?3,?3)",
            params![conversation, input, now],
        )?;
        let id = self.connection.last_insert_rowid();
        self.connection.execute(
            "INSERT OR IGNORE INTO chat_queue_dispatch(conversation_id,paused,updated_at) VALUES(?1,1,?2)",
            params![conversation, now],
        )?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(id)
    }

    pub(crate) fn list_chat_input_queue(
        &self,
        conversation: &str,
        after: i64,
        limit: u32,
    ) -> Result<Vec<QueuedChatInput>> {
        Ok(self.connection.prepare(
            "SELECT id,input_message_id,status,mode,run_id,(SELECT substr(content,1,120) FROM messages WHERE id=input_message_id),defer_reason FROM chat_input_queue WHERE conversation_id=?1 AND id>?2 ORDER BY id LIMIT ?3",
        )?.query_map(params![conversation, after.max(0), limit.clamp(1,100)], |r| Ok(QueuedChatInput {
            id: r.get(0)?, input_message_id: r.get(1)?, status: r.get(2)?, mode: r.get(3)?, run_id: r.get(4)?, preview: r.get(5)?, defer_reason: r.get(6)?,
        }))?.collect::<rusqlite::Result<_>>()?)
    }

    pub(crate) fn pending_chat_input_queue(
        &self,
        conversation: &str,
        after: i64,
        limit: u32,
    ) -> Result<Vec<QueuedChatInput>> {
        Ok(self.connection.prepare(
            "SELECT id,input_message_id,status,mode,run_id,(SELECT substr(content,1,120) FROM messages WHERE id=input_message_id),defer_reason FROM chat_input_queue WHERE conversation_id=?1 AND id>?2 AND status='pending' ORDER BY id LIMIT ?3",
        )?.query_map(params![conversation, after.max(0), limit.clamp(1,100)], |r| Ok(QueuedChatInput {
            id: r.get(0)?, input_message_id: r.get(1)?, status: r.get(2)?, mode: r.get(3)?, run_id: r.get(4)?, preview: r.get(5)?, defer_reason: r.get(6)?,
        }))?.collect::<rusqlite::Result<_>>()?)
    }

    pub(crate) fn withdraw_chat_input(&self, conversation: &str, id: i64) -> Result<bool> {
        Ok(self.connection.execute(
            "UPDATE chat_input_queue SET status='withdrawn',updated_at=?3 WHERE id=?1 AND conversation_id=?2 AND status='pending'",
            params![id, conversation, chrono::Utc::now().to_rfc3339()],
        )? == 1)
    }

    pub(crate) fn pause_chat_queue(&self, conversation: &str) -> Result<()> {
        self.connection.execute(
            "INSERT INTO chat_queue_dispatch(conversation_id,paused,updated_at) VALUES(?1,1,?2) ON CONFLICT(conversation_id) DO UPDATE SET paused=1,owner=NULL,lease_until=NULL,updated_at=excluded.updated_at",
            params![conversation, chrono::Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    /// Only an explicit continue grants ownership. A new process cannot renew it.
    pub(crate) fn resume_chat_queue(&self, conversation: &str, owner: &str) -> Result<()> {
        ensure!(!owner.is_empty(), "派发身份不能为空");
        self.get_conversation(conversation)?.context("会话不存在")?;
        self.connection.execute(
            "INSERT INTO chat_queue_dispatch(conversation_id,paused,owner,lease_until,updated_at) VALUES(?1,0,?2,?3,?4) ON CONFLICT(conversation_id) DO UPDATE SET paused=0,owner=excluded.owner,lease_until=excluded.lease_until,updated_at=excluded.updated_at",
            params![conversation, owner, queue_deadline(), chrono::Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub(crate) fn chat_queue_paused(&self, conversation: &str, owner: &str) -> Result<bool> {
        let active: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM chat_queue_dispatch WHERE conversation_id=?1 AND paused=0 AND owner=?2 AND lease_until>?3)",
            params![conversation, owner, chrono::Utc::now().to_rfc3339()], |r| r.get(0),
        )?;
        Ok(!active)
    }

    /// Queue consumption and run creation commit together. A crash after this
    /// point leaves a recoverable run, never a still-pending item to execute twice.
    pub(crate) fn claim_next_chat_input(
        &self,
        conversation: &str,
        dispatcher: &str,
        executor: &str,
    ) -> Result<Option<ChatRun>> {
        self.recover_expired_chat_runs()?;
        let tx = self.begin_write_transaction()?;
        if self.chat_queue_paused(conversation, dispatcher)? {
            return Ok(None);
        }
        self.connection.execute(
            "UPDATE chat_queue_dispatch SET lease_until=?2 WHERE conversation_id=?1",
            params![conversation, queue_deadline()],
        )?;
        let active: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM chat_runs WHERE conversation_id=?1 AND status IN ('preparing','running','cancelling'))", [conversation], |r| r.get(0),
        )?;
        if active {
            if let Some(tx) = tx {
                tx.commit()?;
            }
            return Ok(None);
        }
        let pending: Option<(i64,String,String)> = self.connection.query_row(
            "SELECT id,input_message_id,context_json FROM chat_input_queue WHERE conversation_id=?1 AND status='pending' ORDER BY id LIMIT 1", [conversation], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)),
        ).optional()?;
        let Some((id, input, envelope)) = pending else {
            if let Some(tx) = tx {
                tx.commit()?;
            }
            return Ok(None);
        };
        if envelope != "{}" {
            self.connection.execute(
                "UPDATE chat_input_queue SET defer_reason='unsupported_context' WHERE id=?1",
                [id],
            )?;
            if let Some(tx) = tx {
                tx.commit()?;
            }
            return Ok(None);
        }
        // create_chat_run atomically consumes any pending eligibility for its
        // input, including when the user explicitly starts it outside this tick.
        let run = self.create_chat_run(
            conversation,
            &input,
            &format!("queued-input:{id}:{input}"),
            executor,
        )?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(Some(run))
    }
}

fn queue_deadline() -> String {
    (chrono::Utc::now() + chrono::Duration::seconds(30)).to_rfc3339()
}

#[cfg(test)]
mod tests;
