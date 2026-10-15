//! Version-bound review APIs shared by all chat entry points.
use crate::storage::Store;
use crate::storage::approvals::{ChatDecision, ChatProposal};
use anyhow::Result;

#[derive(Clone, Debug)]
pub struct ChatReviewRequest {
    pub kind: String,
    pub proposal_id: String,
    pub conversation_id: Option<String>,
    pub expected_version: i64,
    pub approval_token: String,
}

#[derive(Clone, Debug)]
pub struct ChatProposalDto {
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

impl From<ChatProposal> for ChatProposalDto {
    fn from(value: ChatProposal) -> Self {
        Self {
            kind: value.kind,
            id: value.id,
            conversation_id: value.conversation_id,
            action: value.action,
            args_json: value.args_json,
            version: value.version,
            created_at: value.created_at,
            approval_token: value.approval_token,
            target_basis_json: value.target_basis_json,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ChatDecisionDto {
    pub kind: String,
    pub id: String,
    pub conversation_id: Option<String>,
    pub version: i64,
    pub decision: String,
    pub status: String,
    pub result: String,
    pub action: String,
    pub args_json: String,
}

impl From<ChatDecision> for ChatDecisionDto {
    fn from(value: ChatDecision) -> Self {
        Self {
            kind: value.kind,
            id: value.id,
            conversation_id: value.conversation_id,
            version: value.version,
            decision: value.decision,
            status: value.status,
            result: value.result,
            action: value.action,
            args_json: value.args_json,
        }
    }
}

fn store() -> Result<Store> {
    Store::open(&crate::config::AppConfig::load()?.database_path)
}

/// None scopes the list to legacy global pending rules, not every conversation.
pub fn list_chat_proposals(
    conversation_id: Option<String>,
    offset: u32,
    limit: u32,
) -> Result<Vec<ChatProposalDto>> {
    Ok(store()?
        .list_chat_proposals(conversation_id.as_deref(), offset, limit)?
        .into_iter()
        .map(Into::into)
        .collect())
}

pub fn list_chat_decisions(
    conversation_id: Option<String>,
    offset: u32,
    limit: u32,
) -> Result<Vec<ChatDecisionDto>> {
    Ok(store()?
        .list_chat_decisions(conversation_id.as_deref(), offset, limit)?
        .into_iter()
        .map(Into::into)
        .collect())
}

pub fn edit_chat_proposal(
    request: ChatReviewRequest,
    args_json: String,
) -> Result<ChatProposalDto> {
    Ok(store()?
        .edit_chat_proposal(
            &request.kind,
            &request.proposal_id,
            request.conversation_id.as_deref(),
            request.expected_version,
            &request.approval_token,
            &args_json,
        )?
        .into())
}

/// Approval binds the exact preview token, never a natural-language keyword.
pub fn decide_chat_proposal(request: ChatReviewRequest, accept: bool) -> Result<ChatDecisionDto> {
    Ok(crate::ai::approval::resolve_chat_proposal(
        &store()?,
        &request.kind,
        &request.proposal_id,
        request.conversation_id.as_deref(),
        request.expected_version,
        &request.approval_token,
        if accept { "accept" } else { "reject" },
    )?
    .into())
}
