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
