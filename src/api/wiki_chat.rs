//! 知识页内 AI 会话 FRB 门面。

use super::*;

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
