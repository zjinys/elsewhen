//! 知识页内 AI 会话 FRB 门面。

use super::*;

/// Material and its review basis are captured together, then displayed by the UI.
#[derive(Clone)]
pub struct WikiChatMaterialDto {
    pub slug: String,
    pub title: String,
    pub content_md: String,
    pub basis_json: String,
}

pub fn get_wiki_chat_material(page_slug: String) -> Result<WikiChatMaterialDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let tx = store.begin_write_transaction()?;
    let (page, basis) = store.page_task_basis(&page_slug)?;
    let result = WikiChatMaterialDto {
        slug: page.slug,
        title: page.title,
        content_md: page.content_md,
        basis_json: serde_json::to_string(&basis)?,
    };
    if let Some(tx) = tx {
        tx.commit()?;
    }
    Ok(result)
}

/// Restore the last saved input's exact task JSON; never refresh its revision tokens.
pub fn get_wiki_chat_task(conversation_id: String) -> Result<Option<String>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let conversation = store
        .get_conversation(&conversation_id)?
        .ok_or_else(|| anyhow::anyhow!("会话不存在"))?;
    anyhow::ensure!(
        conversation.wiki_page_slug.is_some(),
        "页面任务只能用于知识页会话"
    );
    store.latest_page_task_json(&conversation_id)
}

/// Save one version-bound wiki input, without creating a personal event.
pub fn submit_wiki_chat_input(
    conversation_id: String,
    content: String,
    idempotency_key: String,
    task_json: String,
) -> Result<MessageDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let task = serde_json::from_str(&task_json)?;
    let id = store.save_page_task_input(&conversation_id, &content, &idempotency_key, &task)?;
    let message = store
        .list_messages(&conversation_id)?
        .into_iter()
        .find(|m| m.id == id)
        .ok_or_else(|| anyhow::anyhow!("已保存消息不存在"))?;
    Ok(MessageDto {
        id: message.id,
        conversation_id: message.conversation_id,
        parent_message_id: message.parent_message_id,
        role: message.role,
        content: message.content,
        created_at: message.created_at,
        input_message_id: message.input_message_id,
        kind: message.kind,
        message_ref: message.message_ref,
    })
}

// ── 知识页内 AI 处理会话 ──────────────────────────────────────────────

/// 获取（不存在则创建）某个知识页的处理会话，返回会话 DTO。
/// 页内 AI 聊天通过该会话进行；生成时自动注入页面内容。
pub fn ensure_wiki_page_chat(page_slug: String) -> Result<ConversationDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let Some(page) = store.get_wiki_page(&page_slug)? else {
        anyhow::bail!("知识页不存在: {page_slug}");
    };
    let conversation_id = match store.find_wiki_chat_conversation(&page_slug)? {
        Some(id) => id,
        None => {
            store.create_wiki_chat_conversation(&page_slug, &format!("[知识页] {}", page.title))?
        }
    };
    let conversation = store
        .get_conversation(&conversation_id)?
        .ok_or_else(|| anyhow::anyhow!("会话创建失败"))?;
    Ok(ConversationDto {
        id: conversation.id,
        title: conversation.title,
        tag: conversation.tag,
        created_at: conversation.created_at,
        updated_at: conversation.updated_at,
        message_count: conversation.message_count,
        last_message_preview: conversation.last_message_preview,
        archived: conversation.archived,
        wiki_page_slug: conversation.wiki_page_slug,
    })
}

/// 删除一个知识页的处理会话（重建时用）
pub fn archive_wiki_page_chat(page_slug: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let Some(conversation_id) = store.find_wiki_chat_conversation(&page_slug)? else {
        return Ok(());
    };
    store.set_conversation_archived(&conversation_id, true)
}
