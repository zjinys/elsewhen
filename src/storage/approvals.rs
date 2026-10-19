//! Version-bound chat proposals and durable, at-most-once execution claims.
use super::{PendingAction, Store};
use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::sync::{OnceLock, mpsc};
use std::time::Duration;

fn executor_id() -> &'static str {
    static ID: OnceLock<String> = OnceLock::new();
    ID.get_or_init(|| uuid::Uuid::new_v4().to_string())
}

fn lease_deadline() -> String {
    (chrono::Utc::now() + chrono::Duration::minutes(5)).to_rfc3339()
}

pub(crate) struct ProposalLease(mpsc::Sender<()>);
impl Drop for ProposalLease {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

/// R3 审批续跑指令的确定性前缀：装配与节流扫描都依赖它识别
/// 「系统代发的自动续跑输入」，别改字面量。
pub const FOLLOWUP_MARK: &str = "〔审批后续跑〕";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatProposal {
    pub kind: String,
    pub id: String,
    pub conversation_id: Option<String>,
    pub action: String,
    pub args_json: String,
    pub version: i64,
    pub created_at: String,
    pub approval_token: String,
    pub target_basis_json: String,
}

impl ChatProposal {
    pub(crate) fn pending_action(&self) -> Result<PendingAction> {
        if self.kind != "action" {
            bail!("不是待执行动作");
        }
        Ok(PendingAction {
            id: self.id.clone(),
            conversation_id: self.conversation_id.clone().context("动作缺少来源会话")?,
            action: self.action.clone(),
            args_json: self.args_json.clone(),
            created_at: self.created_at.clone(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatDecision {
    pub kind: String,
    pub id: String,
    pub conversation_id: Option<String>,
    pub version: i64,
    pub approval_token: String,
    pub decision: String,
    pub status: String,
    pub result: String,
    pub action: String,
    pub args_json: String,
}

pub(crate) enum ProposalClaim {
    Ready(ChatProposal),
    Existing(ChatDecision),
}

fn collect_slugs(value: &Value, slugs: &mut BTreeSet<String>) {
    match value {
        Value::Object(fields) => {
            for (key, value) in fields {
                if matches!(
                    key.as_str(),
                    "slug"
                        | "page_slug"
                        | "source_slug"
                        | "upstream_slug"
                        | "related_wiki_slug"
                        | "parent_slug"
                        | "from_slug"
                        | "to_slug"
                ) {
                    if let Some(slug) = value.as_str() {
                        slugs.insert(slug.to_owned());
                    }
                }
                collect_slugs(value, slugs);
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_slugs(value, slugs);
            }
        }
        _ => {}
    }
}

fn editable_field(action: &str, key: &str) -> bool {
    match action {
        "activate_rule" => key == "content",
        "create_todo" => matches!(key, "title" | "note" | "due_at" | "priority"),
        "record_event" => key == "raw_text",
        "save_knowledge_draft" | "save_wiki_revision" => {
            matches!(key, "title" | "content_md" | "change_note")
        }
        "rename_wiki_page" => key == "new_title",
        "propose_people_relations" => matches!(key, "people" | "relations"),
        "import_url_to_wiki" => key == "title",
        _ => false,
    }
}

fn proposal_row(row: &rusqlite::Row<'_>, kind: &str) -> rusqlite::Result<ChatProposal> {
    Ok(ChatProposal {
        kind: kind.to_owned(),
        id: row.get(0)?,
        conversation_id: row.get(1)?,
        action: row.get(2)?,
        args_json: row.get(3)?,
        version: row.get(4)?,
        created_at: row.get(5)?,
        approval_token: String::new(),
        target_basis_json: String::new(),
    })
}

impl Store {
    pub fn chat_proposal(
        &self,
        kind: &str,
        id: &str,
        conversation_id: Option<&str>,
    ) -> Result<Option<ChatProposal>> {
        let sql = match kind {
            "action" => {
                "SELECT id,conversation_id,action,args_json,review_version,created_at FROM pending_actions WHERE id=?1 AND conversation_id IS ?2 AND status='pending'"
            }
            "rule" => {
                "SELECT id,conversation_id,'activate_rule',json_object('content',content),review_version,created_at FROM rules WHERE id=?1 AND conversation_id IS ?2 AND status='pending'"
            }
            _ => bail!("未知提议类型"),
        };
        let proposal = self
            .connection
            .query_row(sql, params![id, conversation_id], |r| proposal_row(r, kind))
            .optional()?;
        proposal
            .map(|mut proposal| {
                let args = serde_json::from_str::<Value>(&proposal.args_json).unwrap_or(Value::Null);
                let targets = self.chat_proposal_targets(&proposal.action, &args)?;
                proposal.target_basis_json = targets.to_string();
                proposal.approval_token = super::knowledge::content_hash(&json!({
                "kind":proposal.kind,"id":proposal.id,"conversation":proposal.conversation_id,
                "action":proposal.action,"version":proposal.version,"args_json":proposal.args_json,"targets":targets,
            }).to_string());
                Ok(proposal)
            })
            .transpose()
    }

    pub fn list_chat_proposals(
        &self,
        conversation_id: Option<&str>,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<ChatProposal>> {
        let mut statement = self.connection.prepare(
            "SELECT kind,id FROM (
                SELECT 'action' AS kind,id,created_at FROM pending_actions WHERE conversation_id IS ?1 AND status='pending'
                UNION ALL SELECT 'rule',id,created_at FROM rules WHERE conversation_id IS ?1 AND status='pending'
             ) p WHERE NOT EXISTS(SELECT 1 FROM chat_proposal_decisions d WHERE d.proposal_kind=p.kind AND d.proposal_id=p.id)
             ORDER BY created_at,id LIMIT ?2 OFFSET ?3"
        )?;
        let ids = statement
            .query_map(params![conversation_id, limit.clamp(1, 100), offset], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ids.into_iter()
            .filter_map(|(kind, id)| self.chat_proposal(&kind, &id, conversation_id).transpose())
            .collect()
    }

    fn chat_proposal_targets(&self, action: &str, args: &Value) -> Result<Value> {
        let mut slugs = BTreeSet::new();
        collect_slugs(args, &mut slugs);
        let mut targets = Vec::new();
        if action == "propose_people_relations" {
            let mut names = BTreeSet::new();
            for person in args["people"].as_array().into_iter().flatten() {
                if let Some(name) = person["name"].as_str() {
                    names.insert(name.trim().to_owned());
                }
            }
            for relation in args["relations"].as_array().into_iter().flatten() {
                for key in ["person", "target"] {
                    if let Some(name) = relation[key].as_str() {
                        names.insert(name.trim().to_owned());
                    }
                }
            }
            if names.len() > 64 {
                bail!("联系人提议过大，请拆分审阅");
            }
            for name in names {
                if name.is_empty() {
                    continue;
                }
                let mut matches = self.find_wiki_pages_by_title_or_alias(&name)?;
                matches.sort_by(|a, b| a.slug.cmp(&b.slug));
                slugs.extend(matches.iter().map(|p| p.slug.clone()));
                targets.push(json!({"name":name,"candidates":matches.iter().map(|p| json!({"id":p.id,"slug":p.slug,"kind":p.kind})).collect::<Vec<_>>()}));
            }
        }
        if slugs.len() > 64 {
            bail!("提议引用过多，请缩小审阅范围");
        }
        for slug in slugs {
            let page = self.connection.query_row(
                "SELECT id,title,content_md,updated_at,status,knowledge_epoch FROM wiki_pages WHERE slug=?1",
                [&slug], |r| Ok(json!({"id":r.get::<_,String>(0)?,"title":r.get::<_,String>(1)?,
                    "content_hash":super::knowledge::content_hash(&r.get::<_,String>(2)?),
                    "updated_at":r.get::<_,String>(3)?,"status":r.get::<_,String>(4)?,"epoch":r.get::<_,i64>(5)?})),
            ).optional()?;
            targets.push(json!({"slug":slug,"page":page}));
        }
        if action == "save_knowledge_draft" {
            if let Some(title) = args["title"].as_str() {
                let existing = self.find_wiki_page_by_title(title)?;
                targets.push(json!({"existing_title":title,"page":existing.map(|p| json!({"id":p.id,"updated_at":p.updated_at,"hash":super::knowledge::content_hash(&p.content_md)}))}));
            }
        }
        if action == "archive_conversations_by_title" {
            if let Some(ids) = args["ids"].as_array() {
                if ids.len() > 100 {
                    bail!("一次最多审阅 100 个对话");
                }
                for id in ids.iter().filter_map(Value::as_str) {
                    let conversation = self.get_conversation(id)?;
                    targets.push(json!({"conversation":id,"current":conversation.map(|c| json!({"title":c.title,"updated_at":c.updated_at,"archived":c.archived,"wiki_page_slug":c.wiki_page_slug,"message_count":c.message_count}))}));
                }
            }
        }
        Ok(Value::Array(targets))
    }

    pub(crate) fn validate_chat_proposal_targets(&self, proposal: &ChatProposal) -> Result<()> {
        anyhow::ensure!(
            !self.connection.is_autocommit(),
            "目标核验必须与业务写入处于同一事务"
        );
        let args: Value = serde_json::from_str(&proposal.args_json)?;
        let reviewed: Value = serde_json::from_str(&proposal.target_basis_json)?;
        anyhow::ensure!(
            self.chat_proposal_targets(&proposal.action, &args)? == reviewed,
            "审阅目标在确认后发生变化，未执行，请重新审阅"
        );
        Ok(())
    }

    pub fn chat_decision(
        &self,
        kind: &str,
        id: &str,
        conversation_id: Option<&str>,
    ) -> Result<Option<ChatDecision>> {
        Ok(self.connection.query_row(
            "SELECT proposal_kind,proposal_id,conversation_id,proposal_version,approval_token,decision,status,result,action,args_json FROM chat_proposal_decisions WHERE proposal_kind=?1 AND proposal_id=?2 AND conversation_id IS ?3",
            params![kind,id,conversation_id], |r| Ok(ChatDecision {kind:r.get(0)?,id:r.get(1)?,conversation_id:r.get(2)?,version:r.get(3)?,approval_token:r.get(4)?,decision:r.get(5)?,status:r.get(6)?,result:r.get(7)?,action:r.get(8)?,args_json:r.get(9)?}),
        ).optional()?)
    }

    pub fn list_chat_decisions(
        &self,
        conversation_id: Option<&str>,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<ChatDecision>> {
        self.recover_chat_proposal_decisions()?;
        let mut statement = self.connection.prepare("SELECT proposal_kind,proposal_id FROM chat_proposal_decisions WHERE conversation_id IS ?1 ORDER BY started_at DESC,proposal_id LIMIT ?2 OFFSET ?3")?;
        let ids = statement
            .query_map(params![conversation_id, limit.clamp(1, 100), offset], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ids.into_iter()
            .filter_map(|(kind, id)| self.chat_decision(&kind, &id, conversation_id).transpose())
            .collect()
    }

    /// Returns the claimed proposal, or a receipt for an identical prior decision.
    /// The claim commits before execution: an uncertain result is never replayed.
    pub(crate) fn claim_chat_proposal(
        &self,
        kind: &str,
        id: &str,
        conversation_id: Option<&str>,
        version: i64,
        approval_token: &str,
        decision: &str,
    ) -> Result<ProposalClaim> {
        if !matches!(decision, "accept" | "reject") {
            bail!("决定必须是接受或拒绝");
        }
        self.recover_chat_proposal_decisions()?;
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        if let Some(receipt) = self.chat_decision(kind, id, conversation_id)? {
            if receipt.version != version
                || receipt.approval_token != approval_token
                || receipt.decision != decision
            {
                bail!("提议已经处理，不能使用另一版本或决定重复执行");
            }
            tx.commit()?;
            return Ok(ProposalClaim::Existing(receipt));
        }
        let proposal = self
            .chat_proposal(kind, id, conversation_id)?
            .context("提议不存在、归属不符或已处理")?;
        if proposal.version != version || proposal.approval_token != approval_token {
            bail!("提议或目标已变化，请重新打开审阅后确认");
        }
        if decision == "accept"
            && !serde_json::from_str::<Value>(&proposal.args_json).is_ok_and(|v| v.is_object())
        {
            bail!("提议格式异常，请拒绝或重新草拟，不能直接执行");
        }
        let status = if decision == "accept" {
            "executing"
        } else {
            "declined"
        };
        let now = chrono::Utc::now().to_rfc3339();
        self.connection.execute(
            "INSERT INTO chat_proposal_decisions(proposal_kind,proposal_id,conversation_id,proposal_version,approval_token,decision,status,action,args_json,result,started_at,finished_at,executor_id,lease_until) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
            params![kind,id,conversation_id,version,approval_token,decision,status,proposal.action,proposal.args_json,
                if decision == "reject" { "已拒绝，未执行业务操作" } else { "正在执行；结果未知时不得重复执行" },
                now, if decision == "reject" { Some(&now) } else { None }, executor_id(), lease_deadline()],
        )?;
        if kind == "action" {
            self.connection.execute(
                "UPDATE pending_actions SET status=?1 WHERE id=?2",
                params![
                    if decision == "accept" {
                        "done"
                    } else {
                        "declined"
                    },
                    id
                ],
            )?;
        } else if decision == "reject" {
            self.connection
                .execute("DELETE FROM rules WHERE id=?1 AND status='pending'", [id])?;
        }
        tx.commit()?;
        if decision == "reject" {
            return Ok(ProposalClaim::Existing(
                self.chat_decision(kind, id, conversation_id)?
                    .context("拒绝回执缺失")?,
            ));
        }
        Ok(ProposalClaim::Ready(proposal))
    }

    pub(crate) fn finish_chat_proposal(
        &self,
        proposal: &ChatProposal,
        result: Result<String>,
    ) -> Result<ChatDecision> {
        let (status, message) = match result {
            Ok(message) => ("succeeded", message),
            Err(error) => (
                "uncertain",
                format!("操作未确认完成，可能有部分结果，请核对后另建提议；不会自动重试：{error}"),
            ),
        };
        self.write_chat_receipt(proposal, status, &message)
    }

    pub(crate) fn fail_chat_proposal_rolled_back(
        &self,
        proposal: &ChatProposal,
        error: anyhow::Error,
    ) -> Result<ChatDecision> {
        self.write_chat_receipt(
            proposal,
            "failed",
            &format!("执行失败，事务已回滚，本次没有写入；请重新审阅或草拟：{error}"),
        )
    }

    fn write_chat_receipt(
        &self,
        proposal: &ChatProposal,
        status: &str,
        message: &str,
    ) -> Result<ChatDecision> {
        let now = chrono::Utc::now().to_rfc3339();
        let changed = self.connection.execute(
            "UPDATE chat_proposal_decisions SET status=?1,result=?2,finished_at=?3 WHERE proposal_kind=?4 AND proposal_id=?5 AND status='executing' AND executor_id=?6 AND lease_until>?3",
            params![status,message,now,proposal.kind,proposal.id,executor_id()],
        )?;
        anyhow::ensure!(
            changed == 1,
            "执行租约或状态已改变，不能覆盖回执；请核对结果"
        );
        self.chat_decision(
            &proposal.kind,
            &proposal.id,
            proposal.conversation_id.as_deref(),
        )?
        .context("执行回执缺失")
    }

    pub(crate) fn keep_chat_proposal_alive(
        &self,
        proposal: &ChatProposal,
    ) -> Result<ProposalLease> {
        let owned: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM chat_proposal_decisions WHERE proposal_kind=?1 AND proposal_id=?2 AND proposal_version=?3 AND approval_token=?4 AND status='executing' AND executor_id=?5 AND lease_until>?6)",
            params![proposal.kind,proposal.id,proposal.version,proposal.approval_token,executor_id(),chrono::Utc::now().to_rfc3339()], |r| r.get(0),
        )?;
        anyhow::ensure!(owned, "执行租约已失效，未发起业务操作");
        let path = self.path.clone();
        let kind = proposal.kind.clone();
        let id = proposal.id.clone();
        let owner = executor_id().to_owned();
        let (stop, stopped) = mpsc::channel();
        std::thread::Builder::new().name("chat-approval-lease".into()).spawn(move || {
            while matches!(stopped.recv_timeout(Duration::from_secs(5)), Err(mpsc::RecvTimeoutError::Timeout)) {
                let renewed = (|| -> Result<usize> {
                    let connection = rusqlite::Connection::open(&path)?;
                    connection.busy_timeout(Duration::from_secs(2))?;
                    Ok(connection.execute("UPDATE chat_proposal_decisions SET lease_until=?1 WHERE proposal_kind=?2 AND proposal_id=?3 AND executor_id=?4 AND status='executing' AND lease_until>?5",
                        params![lease_deadline(),kind,id,owner,chrono::Utc::now().to_rfc3339()])?)
                })();
                if matches!(renewed, Ok(0)) { break; }
            }
        })?;
        Ok(ProposalLease(stop))
    }

    pub(crate) fn activate_reviewed_rule(&self, proposal: &ChatProposal) -> Result<String> {
        let changed = self.connection.execute(
            "UPDATE rules SET status='active',updated_at=?1 WHERE id=?2 AND conversation_id IS ?3 AND review_version=?4 AND status='pending'",
            params![chrono::Utc::now().to_rfc3339(),proposal.id,proposal.conversation_id,proposal.version],
        )?;
        if changed != 1 {
            bail!("规则已变化，未转正");
        }
        Ok("个人规则已确认并生效".into())
    }

    pub fn edit_chat_proposal(
        &self,
        kind: &str,
        id: &str,
        conversation_id: Option<&str>,
        version: i64,
        approval_token: &str,
        edited_json: &str,
    ) -> Result<ChatProposal> {
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        if self.chat_decision(kind, id, conversation_id)?.is_some() {
            bail!("已处理提议不能修改，请新建提议");
        }
        let proposal = self
            .chat_proposal(kind, id, conversation_id)?
            .context("提议不存在或归属不符")?;
        if proposal.version != version || proposal.approval_token != approval_token {
            bail!("提议或目标已变化，请重新审阅");
        }
        let old: Value = serde_json::from_str(&proposal.args_json)?;
        let edited: Value = serde_json::from_str(edited_json)?;
        let fields = edited.as_object().context("提议内容必须是 JSON 对象")?;
        let old_fields = old.as_object().context("原提议不是 JSON 对象")?;
        for key in old_fields.keys().chain(fields.keys()) {
            if !editable_field(&proposal.action, key) && old.get(key) != edited.get(key) {
                bail!("来源、目标或版本依据不可通过正文编辑修改：{key}");
            }
        }
        if kind == "rule" {
            let content = edited["content"]
                .as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .context("规则正文不能为空")?;
            self.connection.execute(
                "UPDATE rules SET content=?1,updated_at=?2 WHERE id=?3",
                params![content, chrono::Utc::now().to_rfc3339(), id],
            )?;
        } else {
            self.update_pending_action_args(id, &edited.to_string())?;
        }
        let updated = self
            .chat_proposal(kind, id, conversation_id)?
            .context("修改后提议缺失")?;
        tx.commit()?;
        Ok(updated)
    }

    /// Expired execution leases need inspection, never replay. Healthy claims,
    /// including those owned by another process, are not changed by startup.
    pub fn recover_chat_proposal_decisions(&self) -> Result<usize> {
        Ok(self.connection.execute(
            "UPDATE chat_proposal_decisions SET status='uncertain',result='执行租约已过期，结果需要核对；不会自动重试',finished_at=?1 WHERE status='executing' AND lease_until<=?1",
            [chrono::Utc::now().to_rfc3339()],
        )?)
    }
}

#[cfg(test)]
mod tests;
