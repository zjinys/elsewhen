//! Explicit UI decisions are the only entry to executing a chat proposal.
use crate::storage::Store;
use crate::storage::approvals::{ChatDecision, ChatProposal, ProposalClaim};
use anyhow::Result;

pub fn resolve_chat_proposal(
    store: &Store,
    kind: &str,
    id: &str,
    conversation_id: Option<&str>,
    version: i64,
    approval_token: &str,
    decision: &str,
) -> Result<ChatDecision> {
    let proposal = match store.claim_chat_proposal(
        kind,
        id,
        conversation_id,
        version,
        approval_token,
        decision,
    )? {
        ProposalClaim::Existing(receipt) => return Ok(receipt),
        ProposalClaim::Ready(proposal) => proposal,
    };
    execute_claimed_proposal(store, &proposal)
}

pub(crate) fn execute_claimed_proposal(
    store: &Store,
    proposal: &ChatProposal,
) -> Result<ChatDecision> {
    let _lease = store.keep_chat_proposal_alive(proposal)?;
    let execute = || {
        if proposal.kind == "rule" {
            store.activate_reviewed_rule(proposal)
        } else {
            proposal
                .pending_action()
                .and_then(|action| super::tool::execute_pending_action(store, &action))
        }
    };
    // These local writers join one transaction: recheck the reviewed targets,
    // perform the write and commit its receipt atomically. Directory imports
    // retain their own per-source commit rules and the durable uncertain guard.
    let atomic = proposal.kind == "rule"
        || matches!(
            proposal.action.as_str(),
            "create_todo"
                | "save_knowledge_draft"
                | "save_wiki_revision"
                | "rename_wiki_page"
                | "propose_people_relations"
                | "archive_conversations_by_title"
                | "import_url_to_wiki"
        );
    if !atomic {
        return store.finish_chat_proposal(proposal, execute());
    }
    let tx = rusqlite::Transaction::new_unchecked(
        &store.connection,
        rusqlite::TransactionBehavior::Immediate,
    )?;
    let result = store
        .validate_chat_proposal_targets(proposal)
        .and_then(|_| execute());
    match result {
        Ok(message) => {
            let receipt = store.finish_chat_proposal(proposal, Ok(message))?;
            tx.commit()?;
            Ok(receipt)
        }
        Err(error) => {
            drop(tx);
            store.fail_chat_proposal_rolled_back(proposal, error)
        }
    }
}

/// R3 审批续跑闭环：accept 成功后向会话注入一条续跑指令（user 角色、
/// 自明系统代发），并入队一次生成，让模型接着完成剩余步骤并交付答复。
/// 封顶：每次审批只触发一轮；续跑文案明示「本轮后不再自动继续」，
/// 链式推进的每一环都要用户手动批准。5 分钟窗口内同会话自动续跑
/// 超过 2 条则节流放弃（防误点连批导致连环生成）。
/// 返回 true 表示已入队续跑。
pub fn enqueue_approval_followup(
    store: &Store,
    proposal: &ChatProposal,
    decision: &ChatDecision,
) -> Result<bool> {
    use crate::storage::approvals::FOLLOWUP_MARK;
    if decision.decision != "accept" || decision.status != "succeeded" {
        return Ok(false);
    }
    let Some(conversation_id) = proposal
        .conversation_id
        .as_deref()
        .map(str::to_string)
        .or_else(|| decision.conversation_id.clone())
    else {
        // 全局提案（如规则库）无对话语境，不续跑。
        return Ok(false);
    };
    // 节流：最近 5 分钟内已注入 2 条续跑指令时放弃。
    let messages = store.list_messages(&conversation_id)?;
    let window = chrono::Duration::minutes(5);
    let now = chrono::Utc::now();
    let recent = messages
        .iter()
        .rev()
        .filter(|m| {
            m.role == "user"
                && m.content.starts_with(FOLLOWUP_MARK)
                && chrono::DateTime::parse_from_rfc3339(&m.created_at)
                    .map(|t| now.signed_duration_since(t.with_timezone(&chrono::Utc)) <= window)
                    .unwrap_or(false)
        })
        .count();
    if recent >= 2 {
        super::generation::event(
            store,
            "approval_followup_throttled",
            serde_json::json!({"conversation_id": conversation_id, "recent": recent}),
        )?;
        return Ok(false);
    }
    let result = {
        let mut text = decision.result.clone();
        if text.chars().count() > 200 {
            text = text.chars().take(200).collect();
        }
        text
    };
    let content = format!(
        "{FOLLOWUP_MARK}你此前提出的待确认动作「{}」已由用户在评审界面批准并执行成功（结果：{result}）。\n         请继续完成该任务的剩余步骤，并给出面向用户的最终答复。这是系统代发的自动续跑指令，\n         本轮结束后不会再自动继续；不要重复提出同一个提案，也不要把这段话当作新任务。\n         若草稿仍待用户在界面单独审阅，如实说明等待审阅，不要声称已保存。",
        proposal.action
    );
    let message_id = store.send_message(&conversation_id, "user", &content, None)?;
    store.enqueue_chat_input(&conversation_id, &message_id)?;
    Ok(true)
}

/// API 层便捷入口：决策已产生（proposal 已被 claim 消费），从存储回查
/// proposal（终态保留）再续跑。查不到（被清理）时静默跳过。
pub fn enqueue_approval_followup_claimed(
    store: &Store,
    kind: &str,
    id: &str,
    conversation_id: Option<&str>,
    decision: &ChatDecision,
) -> Result<bool> {
    match store.chat_proposal(kind, id, conversation_id)? {
        Some(proposal) => enqueue_approval_followup(store, &proposal, decision),
        None => Ok(false),
    }
}
