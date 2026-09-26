//! 会话 / 消息 / token 用量 FRB 门面。

use super::*;

/// Conversation DTO for Flutter
#[derive(Clone, Debug)]
pub struct ConversationDto {
    pub id: String,
    pub title: Option<String>,
    pub tag: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: i32,
    pub last_message_preview: Option<String>,
    pub archived: bool,
    pub wiki_page_slug: Option<String>,
}

/// Message DTO for Flutter
#[derive(Clone, Debug)]
pub struct MessageDto {
    pub id: String,
    pub conversation_id: String,
    pub parent_message_id: Option<String>,
    pub role: String,
    pub content: String,
    pub created_at: String,
}

#[derive(Clone, Debug)]
pub struct PendingActionDto {
    pub id: String,
    pub action: String,
    pub args_json: String,
    pub created_at: String,
}

pub fn list_pending_actions(conversation_id: String) -> Result<Vec<PendingActionDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    Ok(store
        .pending_actions_for_conversation(&conversation_id)?
        .into_iter()
        .map(|item| PendingActionDto {
            id: item.id,
            action: item.action,
            args_json: item.args_json,
            created_at: item.created_at,
        })
        .collect())
}

pub fn update_pending_action_args(action_id: String, args_json: String) -> Result<bool> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let _: serde_json::Value = serde_json::from_str(&args_json)?;
    store.update_pending_action_args(&action_id, &args_json)
}

/// 从对话中的草稿预览单独保存一张知识页，不确认同会话的其他动作。
pub fn confirm_knowledge_draft(conversation_id: String, action_id: String) -> Result<String> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    crate::ai::tool::confirm_knowledge_draft(&store, &conversation_id, &action_id)
}

/// 只拒绝这一篇待入库知识草稿，不删除已保存页面或其他待确认动作。
pub fn decline_knowledge_draft(conversation_id: String, action_id: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    crate::ai::tool::decline_knowledge_draft(&store, &conversation_id, &action_id)
}

/// Create a new conversation
pub fn create_conversation(title: Option<String>, tag: Option<String>) -> Result<ConversationDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let title_ref = title.as_deref();
    let tag_ref = tag.as_deref();
    let id = store.create_conversation(title_ref, tag_ref)?;

    let conversation = store
        .get_conversation(&id)?
        .ok_or_else(|| anyhow::anyhow!("Conversation not found after creation"))?;

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

/// List all non-archived conversations
pub fn list_conversations() -> Result<Vec<ConversationDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let conversations = store.list_conversations()?;

    Ok(conversations
        .into_iter()
        .map(|c| ConversationDto {
            id: c.id,
            title: c.title,
            tag: c.tag,
            created_at: c.created_at,
            updated_at: c.updated_at,
            message_count: c.message_count,
            last_message_preview: c.last_message_preview,
            archived: c.archived,
            wiki_page_slug: c.wiki_page_slug,
        })
        .collect())
}

/// List archived conversations
pub fn list_archived_conversations() -> Result<Vec<ConversationDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let conversations = store.list_archived_conversations()?;

    Ok(conversations
        .into_iter()
        .map(|c| ConversationDto {
            id: c.id,
            title: c.title,
            tag: c.tag,
            created_at: c.created_at,
            updated_at: c.updated_at,
            message_count: c.message_count,
            last_message_preview: c.last_message_preview,
            archived: c.archived,
            wiki_page_slug: c.wiki_page_slug,
        })
        .collect())
}

/// Rename a conversation
pub fn rename_conversation(conversation_id: String, title: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.rename_conversation(&conversation_id, &title)
}

/// Archive or unarchive a conversation
pub fn set_conversation_archived(conversation_id: String, archived: bool) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.set_conversation_archived(&conversation_id, archived)
}

/// Delete an archived ordinary conversation.
pub fn delete_archived_conversation(conversation_id: String) -> Result<bool> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.delete_archived_conversation(&conversation_id)
}

/// Get a specific conversation
pub fn get_conversation(conversation_id: String) -> Result<Option<ConversationDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    if let Some(conversation) = store.get_conversation(&conversation_id)? {
        Ok(Some(ConversationDto {
            id: conversation.id,
            title: conversation.title,
            tag: conversation.tag,
            created_at: conversation.created_at,
            updated_at: conversation.updated_at,
            message_count: conversation.message_count,
            last_message_preview: conversation.last_message_preview,
            archived: conversation.archived,
            wiki_page_slug: conversation.wiki_page_slug,
        }))
    } else {
        Ok(None)
    }
}

/// Send a message in a conversation
pub fn send_message(
    conversation_id: String,
    role: String,
    content: String,
    parent_message_id: Option<String>,
) -> Result<MessageDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let parent_ref = parent_message_id.as_deref();
    let message_id = store.send_message(&conversation_id, &role, &content, parent_ref)?;

    // 首条用户消息到达时自动生成对话标题，避免列表里一屏「新对话」
    if role == "user" {
        if let Some(title) = crate::storage::derive_conversation_title(&content) {
            let untitled = store
                .get_conversation(&conversation_id)?
                .and_then(|c| c.title)
                .map_or(true, |t| t.trim().is_empty());
            if untitled {
                store.rename_conversation(&conversation_id, &title)?;
            }
        }
    }

    let messages = store.list_messages(&conversation_id)?;
    let message = messages
        .into_iter()
        .find(|m| m.id == message_id)
        .ok_or_else(|| anyhow::anyhow!("Message not found after creation"))?;

    Ok(MessageDto {
        id: message.id,
        conversation_id: message.conversation_id,
        parent_message_id: message.parent_message_id,
        role: message.role,
        content: message.content,
        created_at: message.created_at,
    })
}

/// List messages in a conversation
pub fn list_messages(conversation_id: String) -> Result<Vec<MessageDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let messages = store.list_messages(&conversation_id)?;

    Ok(messages
        .into_iter()
        .map(|m| MessageDto {
            id: m.id,
            conversation_id: m.conversation_id,
            parent_message_id: m.parent_message_id,
            role: m.role,
            content: m.content,
            created_at: m.created_at,
        })
        .collect())
}

/// Get child messages of a specific message (for branching conversations)
pub fn get_child_messages(parent_id: String) -> Result<Vec<MessageDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let messages = store.get_child_messages(&parent_id)?;

    Ok(messages
        .into_iter()
        .map(|m| MessageDto {
            id: m.id,
            conversation_id: m.conversation_id,
            parent_message_id: m.parent_message_id,
            role: m.role,
            content: m.content,
            created_at: m.created_at,
        })
        .collect())
}

/// Get the message chain from root to a specific message
pub fn get_message_chain(message_id: String) -> Result<Vec<MessageDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let messages = store.get_message_chain(&message_id)?;

    Ok(messages
        .into_iter()
        .map(|m| MessageDto {
            id: m.id,
            conversation_id: m.conversation_id,
            parent_message_id: m.parent_message_id,
            role: m.role,
            content: m.content,
            created_at: m.created_at,
        })
        .collect())
}

/// Generate AI reply for a conversation
pub fn generate_reply(
    conversation_id: String,
    provider_type: Option<String>,
    memory_type: Option<String>,
    memory_window_size: Option<u32>,
) -> Result<String> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let provider = match provider_type.as_deref().unwrap_or("openai_compatible") {
        "openai_compatible" => crate::ai::ProviderType::OpenAiCompatible,
        "ollama" => crate::ai::ProviderType::Ollama,
        _ => anyhow::bail!("Unsupported provider type"),
    };

    let memory = match memory_type.as_deref().unwrap_or("sliding_window") {
        "simple" => crate::ai::MemoryType::Simple {
            max_messages: memory_window_size.unwrap_or(10) as usize,
        },
        "sliding_window" => crate::ai::MemoryType::SlidingWindow {
            max_tokens: memory_window_size.unwrap_or(4096) as usize,
        },
        _ => anyhow::bail!("Unsupported memory type"),
    };

    let conv_config = crate::ai::ConversationConfig {
        provider_type: provider,
        memory_type: memory,
    };

    crate::ai::generate_conversation_reply(&conversation_id, &store, Some(conv_config))
}

/// Daily token usage DTO for Flutter（每日 token 使用统计）
#[derive(Clone, Debug)]
pub struct DailyTokenUsageDto {
    pub date: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub call_count: i64,
}

/// 获取最近 N 天的每日 token 用量统计（含当天，日期倒序）
pub fn get_daily_token_usage(days: u32) -> Result<Vec<DailyTokenUsageDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let rows = store.daily_token_usage(days)?;
    Ok(rows
        .into_iter()
        .map(|d| DailyTokenUsageDto {
            date: d.date,
            prompt_tokens: d.prompt_tokens,
            completion_tokens: d.completion_tokens,
            total_tokens: d.total_tokens,
            call_count: d.call_count,
        })
        .collect())
}
