//! Boundary-only task steering. Queue ownership remains the durable arbiter.
use super::{
    memory::{ContextMessage, OmissionPolicy, SegmentOrigin, SegmentPurpose},
    provider::AiProvider,
    tool::ToolSpec,
};
use crate::storage::{Store, chat_supplements::ChatSupplement};
use anyhow::{Result, ensure};
use serde_json::json;

fn valid_basis(input: &ChatSupplement) -> bool {
    crate::storage::knowledge::content_hash(
        &json!([input.input_id, input.content, input.created_at]).to_string(),
    ) == input.basis
}

fn context(input: &ChatSupplement) -> ContextMessage {
    ContextMessage::new("system", format!(
        "【用户显式补充当前任务】\n以下是用户已保存的补充要求，发送时间 {}，只在安全工具轮边界加入（显式重试沿用），不改变原问题及其发送时间。它不构成任何业务动作或规则的确认；现有草稿如需改变必须重新生成并审阅。\n{}",
        input.created_at, input.content,
    )).with_segment(SegmentPurpose::TaskSupplement, SegmentOrigin::StoredMessage,
        Some(&input.input_id), OmissionPolicy::Required)
}

pub(crate) fn restore(
    store: &Store,
    conversation: &str,
    messages: &mut Vec<ContextMessage>,
) -> Result<()> {
    let Some(run) = &store.chat_run_identity else {
        return Ok(());
    };
    for input in store.inherited_chat_supplements(conversation, &run.id)? {
        super::generation::check(store)?;
        ensure!(
            valid_basis(&input) && store.supplement_is_plain(&input)?,
            "原任务的补充材料已经变化，不能静默沿用，请重新提交任务"
        );
        messages.push(context(&input));
        super::generation::event(
            store,
            "input_restored",
            json!({
                "queue_id":input.queue_id,"input_message_id":input.input_id,"input_basis":input.basis,
                "meaning":"included_for_next_request_not_execution",
            }),
        )?;
    }
    Ok(())
}

pub(crate) fn inject_next(
    store: &Store,
    conversation: &str,
    provider: &dyn AiProvider,
    request_context: &[ContextMessage],
    tools: Option<&[ToolSpec]>,
) -> Result<Option<ContextMessage>> {
    let Some(run) = &store.chat_run_identity else {
        return Ok(None);
    };
    super::generation::check(store)?;
    let Some(input) = store.next_chat_supplement(conversation, &run.id)? else {
        return Ok(None);
    };
    if let Some(reason) = store.supplement_defer_reason(&input)? {
        store.defer_chat_supplement(input.queue_id, &run.id, reason)?;
        return Ok(None);
    }
    if !valid_basis(&input) {
        store.defer_chat_supplement(input.queue_id, &run.id, "input_changed")?;
        return Ok(None);
    }
    let context = context(&input);
    let mut candidate = request_context.to_vec();
    candidate.push(context.clone());
    if !provider.accepts_supplement(&candidate, tools)? {
        store.defer_chat_supplement(input.queue_id, &run.id, "budget_or_protocol")?;
        return Ok(None);
    }
    if store.consume_chat_supplement(&run.id, &run.owner, &input)? {
        Ok(Some(context))
    } else {
        Ok(None)
    }
}
