//! Durable chat generation state. UI state never authorizes a transition.
use super::{Store, knowledge::content_hash};
use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRun {
    pub id: String,
    pub conversation_id: String,
    pub input_message_id: String,
    pub status: String,
    pub phase: String,
    pub sequence: i64,
    pub request_count: i64,
    pub provider_basis_json: String,
    pub content: String,
    pub error_kind: Option<String>,
    pub error_text: Option<String>,
    pub final_message_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRunEvent {
    pub run_id: String,
    pub sequence: i64,
    pub kind: String,
    pub payload_json: String,
}

#[derive(Debug)]
pub struct ChatRunStopped;
impl std::fmt::Display for ChatRunStopped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "生成已停止或运行租约失效")
    }
}
impl std::error::Error for ChatRunStopped {}

fn deadline() -> String {
    (chrono::Utc::now() + chrono::Duration::minutes(5)).to_rfc3339()
}

const RUN_COLUMNS: &str = "id,conversation_id,input_message_id,status,phase,last_sequence,request_count,provider_basis_json,response_text,error_kind,error_text,final_message_id,created_at,updated_at";

fn run_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChatRun> {
    Ok(ChatRun {
        id: row.get(0)?,
        conversation_id: row.get(1)?,
        input_message_id: row.get(2)?,
        status: row.get(3)?,
        phase: row.get(4)?,
        sequence: row.get(5)?,
        request_count: row.get(6)?,
        provider_basis_json: row.get(7)?,
        content: row.get(8)?,
        error_kind: row.get(9)?,
        error_text: row.get(10)?,
        final_message_id: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

impl Store {
    pub(crate) fn create_chat_run(
        &self,
        conversation: &str,
        input: &str,
        key: &str,
        owner: &str,
    ) -> Result<ChatRun> {
        ensure!(
            !key.trim().is_empty() && key.len() <= 200,
            "生成请求必须提供有效幂等键"
        );
        self.recover_expired_chat_runs()?;
        let tx = self.begin_write_transaction()?;
        let message: Option<(String,String)> = self.connection.query_row(
            "SELECT content,created_at FROM messages WHERE id=?1 AND conversation_id=?2 AND role='user'",
            params![input,conversation], |r| Ok((r.get(0)?,r.get(1)?)),
        ).optional()?;
        let (text, time) = message.context("生成输入不存在或不属于此会话")?;
        let hash = content_hash(&json!([input, text, time]).to_string());
        let existing: Option<(String,String,String,String)> = self.connection.query_row(
            "SELECT id,conversation_id,input_message_id,input_hash FROM chat_runs WHERE request_key=?1", [key],
            |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)),
        ).optional()?;
        if let Some((id, cid, mid, old_hash)) = existing {
            ensure!(
                cid == conversation && mid == input && old_hash == hash,
                "幂等键已经用于不同的生成输入"
            );
            if let Some(tx) = tx {
                tx.commit()?;
            }
            return self.chat_run(&id)?.context("既有运行缺失");
        }
        let injected: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM chat_input_queue WHERE input_message_id=?1 AND status='injected')", [input], |r| r.get(0))?;
        ensure!(
            !injected,
            "此输入已加入原任务上下文；请重试原任务，不能再次独立生成"
        );
        let unsupported: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM chat_input_queue WHERE input_message_id=?1 AND context_json<>'{}')", [input], |r|r.get(0))?;
        ensure!(
            !unsupported,
            "此输入包含尚未支持的上下文；保留原始材料，不能只取文本生成"
        );
        let active: bool = self.connection.query_row("SELECT EXISTS(SELECT 1 FROM chat_runs WHERE conversation_id=?1 AND status IN ('preparing','running','cancelling'))", [conversation], |r| r.get(0))?;
        ensure!(!active, "此会话已有进行中的生成，请等待或停止当前运行");
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        self.connection.execute("INSERT INTO chat_runs(id,request_key,conversation_id,input_message_id,input_hash,status,phase,executor_id,lease_until,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,'preparing','preparing',?6,?7,?8,?8)",
            params![id,key,conversation,input,hash,owner,deadline(),now])?;
        self.connection.execute(
            "UPDATE chat_input_queue SET status='started',run_id=?1,updated_at=?2 WHERE conversation_id=?3 AND input_message_id=?4 AND status='pending'",
            params![id, now, conversation, input],
        )?;
        let context = self.context_for_input(conversation, input)?;
        let context_basis = context
            .iter()
            .map(|m| json!([m.id, m.parent_message_id, m.role, m.content, m.created_at]))
            .collect::<Vec<_>>();
        let context_hash = content_hash(&serde_json::to_string(&context_basis)?);
        self.record_run_event_in_tx(&id, "created", &json!({"input_message_id":input,"context_hash":context_hash,"message_count":context.len()}))?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        self.chat_run(&id)?.context("创建运行后读取失败")
    }

    /// Match the worker's exact frozen history to the basis committed with creation.
    /// Older runs without a basis cannot safely restart against today's history.
    pub(crate) fn validate_chat_run_context(&self, run: &str, conversation: &str) -> Result<()> {
        let payload: String = self.connection.query_row(
            "SELECT payload_json FROM chat_run_events WHERE run_id=?1 AND kind='created' ORDER BY sequence LIMIT 1",
            [run], |r| r.get(0),
        )?;
        let value: Value = serde_json::from_str(&payload)?;
        let expected = value["context_hash"]
            .as_str()
            .context("生成运行缺少历史基底，请重新生成")?;
        let messages = self.list_messages(conversation)?;
        let basis = messages
            .iter()
            .map(|m| json!([m.id, m.parent_message_id, m.role, m.content, m.created_at]))
            .collect::<Vec<_>>();
        ensure!(
            content_hash(&serde_json::to_string(&basis)?) == expected,
            "生成历史基底已经变化，请重新生成"
        );
        Ok(())
    }

    pub fn chat_run(&self, id: &str) -> Result<Option<ChatRun>> {
        let sql = format!("SELECT {RUN_COLUMNS} FROM chat_runs WHERE id=?1");
        let mut run = self.connection.query_row(&sql, [id], run_row).optional()?;
        if let Some(run) = run.as_mut() {
            if matches!(run.status.as_str(), "preparing" | "running" | "cancelling") {
                run.content = self.chat_run_draft(id, run.sequence)?;
            }
        }
        Ok(run)
    }

    pub(crate) fn chat_run_metadata(&self, id: &str) -> Result<Option<ChatRun>> {
        let sql = format!(
            "SELECT {} FROM chat_runs WHERE id=?1",
            RUN_COLUMNS.replace("response_text", "''")
        );
        Ok(self.connection.query_row(&sql, [id], run_row).optional()?)
    }

    pub fn latest_chat_run(&self, conversation: &str) -> Result<Option<ChatRun>> {
        self.recover_expired_chat_runs()?;
        let id: Option<String> = self.connection.query_row("SELECT id FROM chat_runs WHERE conversation_id=?1 ORDER BY created_at DESC,id DESC LIMIT 1",[conversation],|r|r.get(0)).optional()?;
        id.map(|id| self.chat_run(&id))
            .transpose()
            .map(Option::flatten)
    }

    /// Bounded metadata; full response content is loaded only for a chosen run.
    pub fn list_chat_runs(
        &self,
        conversation: &str,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<ChatRun>> {
        self.recover_expired_chat_runs()?;
        let sql = format!(
            "SELECT {} FROM chat_runs WHERE conversation_id=?1 ORDER BY created_at DESC,id DESC LIMIT ?2 OFFSET ?3",
            RUN_COLUMNS.replace("response_text", "''")
        );
        Ok(self
            .connection
            .prepare(&sql)?
            .query_map(params![conversation, limit.clamp(1, 100), offset], run_row)?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn chat_run_events(&self, run: &str, after: i64, limit: u32) -> Result<Vec<ChatRunEvent>> {
        Ok(self.connection.prepare("SELECT run_id,sequence,kind,payload_json FROM chat_run_events WHERE run_id=?1 AND sequence>?2 ORDER BY sequence LIMIT ?3")?
            .query_map(params![run,after.max(0),limit.clamp(1,256)],|r| Ok(ChatRunEvent{run_id:r.get(0)?,sequence:r.get(1)?,kind:r.get(2)?,payload_json:r.get(3)?}))?.collect::<rusqlite::Result<_>>()?)
    }

    fn chat_run_draft(&self, run: &str, through: i64) -> Result<String> {
        let mut text = String::new();
        let mut stmt=self.connection.prepare("SELECT kind,payload_json FROM chat_run_events WHERE run_id=?1 AND sequence<=?2 AND kind IN ('draft_reset','text_delta') ORDER BY sequence")?;
        for row in stmt.query_map(params![run, through], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })? {
            let (kind, payload) = row?;
            if kind == "draft_reset" {
                text.clear();
            } else if let Some(delta) = serde_json::from_str::<Value>(&payload)?["text"].as_str() {
                text.push_str(delta);
            }
        }
        Ok(text)
    }

    fn record_run_event_in_tx(
        &self,
        run: &str,
        kind: &str,
        payload: &Value,
    ) -> Result<ChatRunEvent> {
        ensure!(!self.connection.is_autocommit(), "运行事件必须在事务内写入");
        let now = chrono::Utc::now().to_rfc3339();
        ensure!(
            self.connection.execute(
                "UPDATE chat_runs SET last_sequence=last_sequence+1,updated_at=?2 WHERE id=?1",
                params![run, now]
            )? == 1,
            "运行已删除"
        );
        let sequence: i64 = self.connection.query_row(
            "SELECT last_sequence FROM chat_runs WHERE id=?1",
            [run],
            |r| r.get(0),
        )?;
        let payload_json = payload.to_string();
        self.connection.execute("INSERT INTO chat_run_events(run_id,sequence,kind,payload_json,created_at) VALUES(?1,?2,?3,?4,?5)",params![run,sequence,kind,payload_json,now])?;
        Ok(ChatRunEvent {
            run_id: run.into(),
            sequence,
            kind: kind.into(),
            payload_json,
        })
    }

    pub(crate) fn check_chat_run_owner(&self, run: &str, owner: &str) -> Result<()> {
        let valid:bool=self.connection.query_row("SELECT EXISTS(SELECT 1 FROM chat_runs r JOIN conversations c ON c.id=r.conversation_id WHERE r.id=?1 AND r.executor_id=?2 AND r.status IN ('preparing','running') AND r.lease_until>?3)",params![run,owner,chrono::Utc::now().to_rfc3339()],|r|r.get(0))?;
        if !valid {
            return Err(ChatRunStopped.into());
        }
        Ok(())
    }

    /// usage 锚定用：run 归属的会话 id（不存在返回 None）。
    pub(crate) fn chat_run_conversation(&self, run: &str) -> Option<String> {
        self.connection
            .query_row(
                "SELECT conversation_id FROM chat_runs WHERE id=?1",
                [run],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .ok()
            .flatten()
    }

    pub(crate) fn renew_chat_run(&self, run: &str, owner: &str) -> Result<bool> {
        Ok(self.connection.execute("UPDATE chat_runs SET lease_until=?1 WHERE id=?2 AND executor_id=?3 AND status IN ('preparing','running') AND lease_until>?4",params![deadline(),run,owner,chrono::Utc::now().to_rfc3339()])?==1)
    }

    pub(crate) fn append_chat_run_event(
        &self,
        run: &str,
        owner: &str,
        kind: &str,
        payload: &Value,
    ) -> Result<ChatRunEvent> {
        let tx = self.begin_write_transaction()?;
        self.check_chat_run_owner(run, owner)?;
        if kind == "phase" {
            let phase = payload["phase"].as_str().context("阶段缺失")?;
            self.connection.execute(
                "UPDATE chat_runs SET status='running',phase=?2 WHERE id=?1",
                params![run, phase],
            )?;
        } else if kind == "provider_basis" {
            self.connection.execute(
                "UPDATE chat_runs SET provider_basis_json=?2 WHERE id=?1",
                params![run, payload.to_string()],
            )?;
        } else if kind == "request_budget" {
            // 降级构成观测（P1 验收口径 2）：每次装配报告覆盖写，终态行
            // 留下最后一次装配的占位化条数；重试链上取末次即最终形态。
            if let Some(request_id) = payload["request_id"].as_str() {
                if let Some(pruned) = payload["report"]["pruned_tool_results"].as_i64() {
                    self.connection.execute(
                        "UPDATE chat_run_requests SET pruned_tool_results=?2 WHERE id=?1 AND run_id=?3",
                        params![request_id, pruned, run],
                    )?;
                }
            }
        }
        let event = self.record_run_event_in_tx(run, kind, payload)?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(event)
    }

    /// A worker may flush already-vetted preview bytes during cancellation, but
    /// never after terminal state or lease loss. This grants no business writes.
    pub(crate) fn flush_chat_run_checkpoint(
        &self,
        run: &str,
        owner: &str,
        text: &str,
    ) -> Result<()> {
        ensure!(text.len() <= 2048, "检查点超过增量上限");
        let tx = self.begin_write_transaction()?;
        let valid: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM chat_runs WHERE id=?1 AND executor_id=?2
             AND status IN ('preparing','running','cancelling') AND lease_until>?3)",
            params![run, owner, chrono::Utc::now().to_rfc3339()],
            |r| r.get(0),
        )?;
        if !valid {
            return Err(ChatRunStopped.into());
        }
        self.record_run_event_in_tx(run, "text_delta", &json!({"text":text,"validated":false}))?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    pub(crate) fn start_chat_request(
        &self,
        run: &str,
        owner: &str,
        provider: &str,
        model: Option<&str>,
    ) -> Result<(String, ChatRunEvent)> {
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        self.check_chat_run_owner(run, owner)?;
        let id = uuid::Uuid::new_v4().to_string();
        self.connection.execute("INSERT INTO chat_run_requests(id,run_id,provider_name,model,status,started_at) VALUES(?1,?2,?3,?4,'running',?5)",params![id,run,provider,model,chrono::Utc::now().to_rfc3339()])?;
        self.connection.execute("UPDATE chat_runs SET status='running',phase='requesting',request_count=request_count+1 WHERE id=?1",[run])?;
        let event = self.record_run_event_in_tx(
            run,
            "request_started",
            &json!({"request_id":id,"provider":provider,"model":model}),
        )?;
        tx.commit()?;
        Ok((id, event))
    }

    pub(crate) fn finish_chat_request(
        &self,
        run: &str,
        owner: &str,
        request: &str,
        status: &str,
        model: Option<&str>,
        usage: Option<(u64, u64)>,
        cached_tokens: Option<u64>,
    ) -> Result<ChatRunEvent> {
        ensure!(matches!(status, "completed" | "failed"), "请求终态无效");
        let stored_usage = usage.and_then(|(prompt, completion)| {
            i64::try_from(prompt)
                .ok()
                .zip(i64::try_from(completion).ok())
        });
        let stored_cached = cached_tokens.and_then(|v| i64::try_from(v).ok());
        let usage_source = match (usage, stored_usage) {
            (Some(_), Some(_)) => "provider",
            (Some(_), None) => "invalid",
            (None, _) => "unknown",
        };
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        self.check_chat_run_owner(run, owner)?;
        let changed=self.connection.execute("UPDATE chat_run_requests SET status=?1,model=COALESCE(?2,model),prompt_tokens=?3,completion_tokens=?4,usage_source=?5,prompt_cached_tokens=?6,finished_at=?7 WHERE id=?8 AND run_id=?9 AND status='running'",
            params![status,model,stored_usage.map(|u|u.0),stored_usage.map(|u|u.1),usage_source,stored_cached,chrono::Utc::now().to_rfc3339(),request,run])?;
        ensure!(changed == 1, "请求已处理或不属于此运行");
        let event = self.record_run_event_in_tx(
            run,
            "request_finished",
            &json!({"request_id":request,"status":status,"model":model,"usage":stored_usage,"usage_source":usage_source,"prompt_cached_tokens":stored_cached}),
        )?;
        tx.commit()?;
        Ok(event)
    }

    pub fn cancel_chat_run(&self, conversation: &str, run: &str) -> Result<Option<ChatRunEvent>> {
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        let belongs: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM chat_runs WHERE id=?1 AND conversation_id=?2)",
            params![run, conversation],
            |r| r.get(0),
        )?;
        if belongs {
            self.pause_chat_queue(conversation)?;
        }
        let changed=self.connection.execute("UPDATE chat_runs SET status='cancelling',phase='cancelling' WHERE id=?1 AND conversation_id=?2 AND status IN ('preparing','running')",params![run,conversation])?;
        let event = if changed == 1 {
            Some(self.record_run_event_in_tx(run, "cancelling", &json!({}))?)
        } else {
            None
        };
        tx.commit()?;
        Ok(event)
    }

    fn mark_unfinished_chat_tools(&self, run: &str) -> Result<()> {
        let pending = self.connection.prepare(
            "SELECT s.payload_json FROM chat_run_events s WHERE s.run_id=?1 AND s.kind='tool_started'
             AND NOT EXISTS(SELECT 1 FROM chat_run_events r WHERE r.run_id=s.run_id
             AND r.kind IN ('tool_result','tool_uncertain')
             AND json_extract(r.payload_json,'$.call_id')=json_extract(s.payload_json,'$.call_id'))",
        )?.query_map([run], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        for payload in pending {
            let mut value: Value = serde_json::from_str(&payload)?;
            value["outcome"] = json!("uncertain");
            value["content"] = json!("工具未留下完成回执；结果待核实，不会自动重做");
            self.record_run_event_in_tx(run, "tool_uncertain", &value)?;
        }
        Ok(())
    }

    /// The final assistant message and terminal run state share a transaction.
    /// Failed/cancelled draft text remains run data, not a fabricated final reply.
    pub(crate) fn finish_chat_run(
        &self,
        run: &str,
        owner: &str,
        status: &str,
        content: Option<&str>,
        error: Option<(&str, &str)>,
    ) -> Result<ChatRunEvent> {
        ensure!(
            matches!(status, "completed" | "failed" | "cancelled"),
            "生成终态无效"
        );
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        let row:Option<(String,String,String)>=self.connection.query_row("SELECT conversation_id,status,input_message_id FROM chat_runs WHERE id=?1 AND executor_id=?2 AND lease_until>?3",params![run,owner,chrono::Utc::now().to_rfc3339()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        let (conversation, current, input) = row.ok_or(ChatRunStopped)?;
        if !matches!(current.as_str(), "preparing" | "running" | "cancelling")
            || (status == "completed" && current == "cancelling")
        {
            return Err(ChatRunStopped.into());
        }
        let status = if current == "cancelling" {
            "cancelled"
        } else {
            status
        };
        let text = match content {
            Some(content) => content.to_owned(),
            None => self.chat_run_draft(run, i64::MAX)?,
        };
        let message = if status == "completed" {
            Some(self.send_message_with_input(
                &conversation,
                "assistant",
                &text,
                None,
                Some(&input),
            )?)
        } else {
            None
        };
        self.connection.execute("UPDATE chat_runs SET status=?1,phase=?1,response_text=?2,error_kind=?3,error_text=?4,final_message_id=?5 WHERE id=?6",
            params![status,text,error.map(|e|e.0),error.map(|e|e.1),message,run])?;
        self.connection.execute("UPDATE chat_run_requests SET status=?1,finished_at=?2 WHERE run_id=?3 AND status='running'",params![if status=="cancelled"{"cancelled"}else{"interrupted"},chrono::Utc::now().to_rfc3339(),run])?;
        self.mark_unfinished_chat_tools(run)?;
        self.close_chat_supplements(run)?;
        let event=self.record_run_event_in_tx(run,status,&json!({"message_id":message,"content":text,"error_kind":error.map(|e|e.0),"error":error.map(|e|e.1)}))?;
        tx.commit()?;
        Ok(event)
    }

    pub fn recover_expired_chat_runs(&self) -> Result<usize> {
        let now = chrono::Utc::now().to_rfc3339();
        let expired:bool=self.connection.query_row("SELECT EXISTS(SELECT 1 FROM chat_runs WHERE status IN ('preparing','running','cancelling') AND lease_until<=?1)",[&now],|r|r.get(0))?;
        if !expired {
            return Ok(0);
        }
        let tx = self.begin_write_transaction()?;
        let ids=self.connection.prepare("SELECT id,status FROM chat_runs WHERE status IN ('preparing','running','cancelling') AND lease_until<=?1")?
            .query_map([&now],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        for (id, state) in &ids {
            let status = if state == "cancelling" {
                "cancelled"
            } else {
                "interrupted"
            };
            let draft = self.chat_run_draft(id, i64::MAX)?;
            self.connection.execute("UPDATE chat_runs SET status=?1,phase=?1,response_text=?2,error_kind='interrupted',error_text='生成中断；已保存输入和业务结果保留，不自动重放' WHERE id=?3",params![status,draft,id])?;
            self.connection.execute("UPDATE chat_run_requests SET status='interrupted',finished_at=?1 WHERE run_id=?2 AND status='running'",params![now,id])?;
            self.mark_unfinished_chat_tools(id)?;
            self.close_chat_supplements(id)?;
            self.record_run_event_in_tx(id, status, &json!({"reason":"lease_expired"}))?;
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(ids.len())
    }
}

#[cfg(test)]
mod tests;
