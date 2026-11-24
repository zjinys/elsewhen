use crate::api::chat_proposals::{ChatDecisionDto, ChatProposalDto, ChatReviewRequest};
use crate::storage::Store;
use anyhow::{Result, ensure};

fn store() -> Result<Store> {
    Store::open(&crate::config::AppConfig::load()?.database_path)
}

pub fn list_personal_memories(filter_json: String, offset: u32, limit: u32) -> Result<String> {
    ensure!(filter_json.len() <= 8192, "筛选参数过长");
    Ok(store()?
        .memory_management_list(&serde_json::from_str(&filter_json)?, offset, limit)?
        .to_string())
}

pub fn get_personal_memory_detail(claim_id: String) -> Result<String> {
    Ok(store()?.memory_management_detail(&claim_id)?.to_string())
}

pub fn edit_personal_memory(claim_id: String, expected_version: i64, content: String) -> Result<()> {
    store()?.edit_direct_memory(&claim_id, expected_version, &content)
}

pub fn prepare_personal_memory_action(
    claim_id: String,
    expected_version: i64,
    detail_token: String,
    args_json: String,
) -> Result<ChatProposalDto> {
    ensure!(args_json.len() <= 8192, "操作参数过长");
    Ok(store()?
        .prepare_memory_management_action(
            &claim_id,
            expected_version,
            &detail_token,
            &serde_json::from_str(&args_json)?,
        )?
        .into())
}

pub fn decide_personal_memory_action(
    request: ChatReviewRequest,
    accept: bool,
) -> Result<ChatDecisionDto> {
    ensure!(request.kind == "action", "不是记忆审阅卡");
    Ok(store()?
        .decide_memory_management_action(
            &request.proposal_id,
            request
                .conversation_id
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("缺少所属会话"))?,
            request.expected_version,
            &request.approval_token,
            accept,
        )?
        .into())
}

pub fn inspect_personal_memory_maintenance(conversation_id: String) -> Result<String> {
    Ok(store()?.memory_maintenance(&conversation_id)?.to_string())
}
