//! Explicit supplements are consumed only at a verified model-request boundary.
use super::{Store, knowledge::content_hash};
use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};

#[derive(Clone, Debug)]
pub(crate) struct ChatSupplement {
    pub queue_id: i64,
    pub input_id: String,
    pub content: String,
    pub created_at: String,
    pub basis: String,
    pub context_json: String,
}

fn basis(id: &str, text: &str, created: &str) -> String {
    content_hash(&json!([id, text, created]).to_string())
}

impl Store {
    pub(crate) fn enqueue_chat_supplement(
        &self,
        conversation: &str,
        input: &str,
        run: &str,
        context_json: &str,
    ) -> Result<i64> {
        ensure!(context_json.len() <= 16384, "补充材料元数据过大");
        let envelope: Value = serde_json::from_str(context_json)?;
        ensure!(envelope.is_object(), "补充材料必须为对象");
        let normalized = envelope.to_string();
        let tx = self.begin_write_transaction()?;
        let id = self.enqueue_chat_input(conversation, input)?;
        let (status, mode, previous_run, previous_context): (
            String,
            String,
            Option<String>,
            String,
        ) = self.connection.query_row(
            "SELECT status,mode,target_run_id,context_json FROM chat_input_queue WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
        if mode == "steer" {
            ensure!(
                previous_run.as_deref() == Some(run) && previous_context == normalized,
                "此输入已用于不同的补充请求"
            );
        } else {
            ensure!(status == "pending", "输入已消费或撤回，不能改成补充要求");
            let target: Option<(String, String, String)> = self
                .connection
                .query_row(
                    "SELECT conversation_id,input_message_id,status FROM chat_runs WHERE id=?1",
                    [run],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?;
            let (owner, original, state) = target.context("补充目标运行不存在")?;
            ensure!(
                owner == conversation && original != input,
                "补充输入或会话身份不匹配"
            );
            let (text, time): (String,String) = self.connection.query_row(
                "SELECT content,created_at FROM messages WHERE id=?1 AND conversation_id=?2 AND role='user'", params![input,conversation], |r|Ok((r.get(0)?,r.get(1)?)))?;
            let reason = self
                .supplement_context_reason_for(input, run, &envelope)?
                .or_else(|| {
                    (!matches!(state.as_str(), "preparing" | "running")).then_some("target_closed")
                });
            self.connection.execute(
                "UPDATE chat_input_queue SET mode='steer',target_run_id=?2,input_basis=?3,context_json=?4,defer_reason=?5 WHERE id=?1",
                params![id,run,basis(input,&text,&time),normalized,reason])?;
            // A completed/cancelled target never licenses an automatic new run.
            if !matches!(state.as_str(), "preparing" | "running") {
                self.pause_chat_queue(conversation)?;
            }
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(id)
    }

    /// Closing a target preserves unconsumed inputs but requires an explicit
    /// queue continuation before they can become independent questions.
    pub(crate) fn close_chat_supplements(&self, run: &str) -> Result<()> {
        ensure!(
            !self.connection.is_autocommit(),
            "补充回退必须与运行终态共同提交"
        );
        let changed = self.connection.execute(
            "UPDATE chat_input_queue SET defer_reason=CASE
                 WHEN defer_reason IN ('unsupported_context','input_changed','context_changed') THEN defer_reason
                 ELSE 'target_closed' END,updated_at=?2
             WHERE target_run_id=?1 AND mode='steer' AND status='pending'",
            params![run, chrono::Utc::now().to_rfc3339()],
        )?;
        if changed > 0 {
            let conversation: String = self.connection.query_row(
                "SELECT conversation_id FROM chat_runs WHERE id=?1",
                [run],
                |r| r.get(0),
            )?;
            self.pause_chat_queue(&conversation)?;
        }
        Ok(())
    }

    pub(crate) fn inherited_chat_supplements(
        &self,
        conversation: &str,
        run: &str,
    ) -> Result<Vec<ChatSupplement>> {
        // An explicit retry of the original input inherits consumed requirements,
        // not pending ones. The queue row remains owned by its first consumer.
        Ok(self.connection.prepare(
            "SELECT q.id,q.input_message_id,m.content,m.created_at,q.input_basis,q.context_json FROM chat_input_queue q JOIN messages m ON m.id=q.input_message_id JOIN chat_runs original ON original.id=q.run_id JOIN chat_runs current ON current.id=?2 WHERE q.conversation_id=?1 AND current.conversation_id=?1 AND q.status='injected' AND original.input_message_id=current.input_message_id ORDER BY q.id",
        )?.query_map(params![conversation,run], |r| Ok(ChatSupplement {
            queue_id:r.get(0)?,input_id:r.get(1)?,content:r.get(2)?,created_at:r.get(3)?,basis:r.get(4)?,context_json:r.get(5)?,
        }))?.collect::<rusqlite::Result<_>>()?)
    }

    pub(crate) fn next_chat_supplement(
        &self,
        conversation: &str,
        run: &str,
    ) -> Result<Option<ChatSupplement>> {
        Ok(self.connection.query_row(
            "SELECT q.id,q.input_message_id,m.content,m.created_at,q.input_basis,q.context_json FROM chat_input_queue q JOIN messages m ON m.id=q.input_message_id WHERE q.conversation_id=?1 AND q.target_run_id=?2 AND q.mode='steer' AND q.status='pending' ORDER BY q.id LIMIT 1",
            params![conversation,run], |r|Ok(ChatSupplement { queue_id:r.get(0)?,input_id:r.get(1)?,content:r.get(2)?,created_at:r.get(3)?,basis:r.get(4)?,context_json:r.get(5)? })).optional()?)
    }

    pub(crate) fn defer_chat_supplement(&self, id: i64, run: &str, reason: &str) -> Result<()> {
        self.connection.execute("UPDATE chat_input_queue SET defer_reason=?3 WHERE id=?1 AND target_run_id=?2 AND status='pending'", params![id,run,reason])?;
        Ok(())
    }

    fn supplement_context_reason_for(
        &self,
        input: &str,
        run: &str,
        context: &Value,
    ) -> Result<Option<&'static str>> {
        if !context.as_object().is_some_and(|map| map.is_empty()) {
            return Ok(Some("unsupported_context"));
        }
        let input_task: Option<String> = self
            .connection
            .query_row(
                "SELECT task_json FROM chat_page_tasks WHERE input_message_id=?1",
                [input],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(input_task) = input_task {
            let target_task: Option<String> = self.connection.query_row(
                "SELECT t.task_json FROM chat_page_tasks t JOIN chat_runs r ON r.input_message_id=t.input_message_id WHERE r.id=?1", [run], |r| r.get(0),
            ).optional()?;
            if target_task.as_deref() != Some(input_task.as_str()) {
                return Ok(Some("context_changed"));
            }
        }
        Ok(None)
    }

    pub(crate) fn supplement_defer_reason(
        &self,
        supplement: &ChatSupplement,
    ) -> Result<Option<&'static str>> {
        let context: Value = serde_json::from_str(&supplement.context_json)?;
        let target: String = self.connection.query_row(
            "SELECT target_run_id FROM chat_input_queue WHERE id=?1",
            [supplement.queue_id],
            |r| r.get(0),
        )?;
        self.supplement_context_reason_for(&supplement.input_id, &target, &context)
    }

    pub(crate) fn supplement_is_plain(&self, supplement: &ChatSupplement) -> Result<bool> {
        Ok(self.supplement_defer_reason(supplement)?.is_none())
    }

    pub(crate) fn consume_chat_supplement(
        &self,
        run: &str,
        owner: &str,
        supplement: &ChatSupplement,
    ) -> Result<bool> {
        let tx = self.begin_write_transaction()?;
        self.check_chat_run_owner(run, owner)?;
        let live = self.next_chat_supplement(
            &self
                .chat_run_metadata(run)?
                .context("运行不存在")?
                .conversation_id,
            run,
        )?;
        let Some(live) = live else {
            return Ok(false);
        };
        if live.queue_id != supplement.queue_id
            || live.basis != supplement.basis
            || live.context_json != supplement.context_json
        {
            return Ok(false);
        }
        ensure!(
            basis(&live.input_id, &live.content, &live.created_at) == live.basis
                && live.content == supplement.content,
            "补充输入版本已变化，仍保留队列"
        );
        if let Some(reason) = self.supplement_defer_reason(&live)? {
            self.defer_chat_supplement(live.queue_id, run, reason)?;
            if let Some(tx) = tx {
                tx.commit()?;
            }
            return Ok(false);
        }
        self.connection.execute("UPDATE chat_input_queue SET status='injected',run_id=?2,defer_reason=NULL,updated_at=?3 WHERE id=?1 AND status='pending'", params![live.queue_id,run,chrono::Utc::now().to_rfc3339()])?;
        self.append_chat_run_event(run,owner,"input_injected", &json!({"queue_id":live.queue_id,"input_message_id":live.input_id,"input_basis":live.basis,"meaning":"included_for_next_request_not_execution"}))?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests;
