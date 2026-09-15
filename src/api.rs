use crate::event::NewEvent;
use crate::storage::Store;
use anyhow::Result;

/// Event data transfer object for Flutter
#[derive(Clone, Debug)]
pub struct EventDto {
    pub id: String,
    pub raw_text: String,
    pub recorded_at: String,
    pub occurred_at: String,
    pub source: String,
    pub status: String,
}

/// Analysis result DTO
#[derive(Clone, Debug)]
pub struct AnalysisDto {
    pub event_type: String,
    pub confidence: f64,
    pub summary: String,
    pub clarifications: Vec<String>,
}

/// Initialize the bridge with database path
pub fn init_bridge(database_path: Option<String>) -> String {
    let config = match crate::config::AppConfig::load() {
        Ok(c) => c,
        Err(e) => return format!("Error loading config: {}", e),
    };

    let db_path = database_path
        .map(|p| std::path::PathBuf::from(p))
        .unwrap_or(config.database_path);

    db_path.display().to_string()
}

/// Record a new event
pub fn record_event(raw_text: String) -> Result<EventDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let new_event = NewEvent::now(&raw_text);
    let id = store.insert_event(new_event)?;

    // Query back the created event
    let events = store.list_events()?;
    let event = events.into_iter()
        .find(|e| e.raw_text == raw_text)
        .ok_or_else(|| anyhow::anyhow!("Event not found after insert"))?;

    Ok(EventDto {
        id,
        raw_text: event.raw_text,
        recorded_at: event.recorded_at.clone(),
        occurred_at: event.recorded_at,
        source: "flutter_gui".to_string(),
        status: "pending".to_string(),
    })
}

/// List all events
pub fn list_events() -> Result<Vec<EventDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let events = store.list_events()?;

    Ok(events.into_iter().map(|e| EventDto {
        id: uuid::Uuid::new_v4().to_string(), // TODO: Store should return ID
        raw_text: e.raw_text,
        recorded_at: e.recorded_at.clone(),
        occurred_at: e.recorded_at,
        source: "unknown".to_string(),
        status: "completed".to_string(),
    }).collect())
}

/// List completed analyses
pub fn list_analyses() -> Result<Vec<AnalysisDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let analyses = store.list_analyses()?;

    Ok(analyses.into_iter().map(|a| AnalysisDto {
        event_type: a.event_type,
        confidence: a.confidence,
        summary: a.raw_text,
        clarifications: serde_json::from_str(&a.clarifications).unwrap_or_default(),
    }).collect())
}

/// Get active AI provider info
pub fn get_ai_provider() -> Result<Option<String>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    if let Some(provider) = store.active_ai_provider_config()? {
        Ok(Some(format!("{} - {}", provider.model, provider.base_url)))
    } else {
        Ok(None)
    }
}

/// AI provider config DTO for Flutter settings page
#[derive(Clone, Debug)]
pub struct AiProviderConfigDto {
    pub provider_type: String,
    pub base_url: String,
    pub model: String,
    pub api_key: String,
}

/// Get the active AI provider full config (for settings page prefill)
pub fn get_ai_provider_config() -> Result<Option<AiProviderConfigDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    Ok(store.active_ai_provider_config()?.map(|p| AiProviderConfigDto {
        provider_type: p.provider_type,
        base_url: p.base_url,
        model: p.model,
        api_key: p.api_key,
    }))
}

/// Upsert the active AI provider config (settings page save)
pub fn update_ai_provider_config(
    base_url: String,
    model: String,
    api_key: String,
) -> Result<()> {
    if base_url.trim().is_empty() || model.trim().is_empty() || api_key.trim().is_empty() {
        anyhow::bail!("base_url、model 和 api_key 均不能为空");
    }
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.upsert_ai_provider_config(&base_url, &model, &api_key)?;
    Ok(())
}

/// Trigger AI analysis for pending events
/// Returns "success" or "error: <message>"
pub fn trigger_analysis() -> Result<String> {
    // TODO: Implement when AI analysis is needed
    Ok("success".to_string())
}

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
    })
}

/// List all conversations
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
        })
        .collect())
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
        }))
    } else {
        Ok(None)
    }
}

/// Send a message in a conversation
pub fn send_message(conversation_id: String, role: String, content: String, parent_message_id: Option<String>) -> Result<MessageDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let parent_ref = parent_message_id.as_deref();
    let message_id = store.send_message(&conversation_id, &role, &content, parent_ref)?;

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

/// Wiki page data transfer object for Flutter（知识库浏览）
#[derive(Clone, Debug)]
pub struct WikiPageDto {
    pub id: String,
    pub slug: String,
    pub kind: String,
    pub title: String,
    pub summary: String,
    pub content_md: String,
    pub tags: Vec<String>,
    pub source_event_ids: Vec<String>,
    pub evidence_count: i64,
    pub first_seen_at: String,
    pub last_seen_at: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

impl From<crate::storage::WikiPage> for WikiPageDto {
    fn from(p: crate::storage::WikiPage) -> Self {
        Self {
            id: p.id,
            slug: p.slug,
            kind: p.kind,
            title: p.title,
            summary: p.summary,
            content_md: p.content_md,
            tags: p.tags,
            source_event_ids: p.source_event_ids,
            evidence_count: p.evidence_count,
            first_seen_at: p.first_seen_at,
            last_seen_at: p.last_seen_at,
            status: p.status,
            created_at: p.created_at,
            updated_at: p.updated_at,
        }
    }
}

/// List wiki pages（可过滤 kind）；kind 为 None 时列出全部
pub fn list_wiki_pages(kind: Option<String>) -> Result<Vec<WikiPageDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let pages = store.list_wiki_pages(kind.as_deref())?;
    Ok(pages.into_iter().map(WikiPageDto::from).collect())
}

/// Get a single wiki page by slug
pub fn get_wiki_page(slug: String) -> Result<Option<WikiPageDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let page = store.get_wiki_page(&slug)?;
    Ok(page.map(WikiPageDto::from))
}

/// 抓取的推文内容 DTO（只解析，不入库）
#[derive(Clone, Debug)]
pub struct TweetFetchDto {
    pub tweet_id: String,
    pub url: String,
    pub text: String,
    /// 文章型推文的标题（article.title），普通推文为 None
    pub title: Option<String>,
    pub author_name: Option<String>,
    pub screen_name: Option<String>,
}

/// 从 x.com / twitter.com 推文链接抓取长文（只解析 json，不写库）。
/// 是否入库由后续「保存」动作决定。
pub fn fetch_tweet(url: String) -> Result<TweetFetchDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    // 按设置的抓取服务分发（当前仅 fxtwitter）
    let service = store
        .get_meta("tweet_fetch_service")?
        .unwrap_or_else(|| "fxtwitter".to_string());
    if service != "fxtwitter" {
        anyhow::bail!("暂不支持的推文抓取服务: {service}");
    }

    let t = crate::wiki::fetch_tweet_text(&url)?;
    Ok(TweetFetchDto {
        tweet_id: t.tweet_id,
        url,
        text: t.text,
        title: t.title,
        author_name: t.author_name,
        screen_name: t.screen_name,
    })
}

/// 把已抓取的推文内容保存为知识库页面（kind=source）并返回该页。
/// 只有用户点击「保存」才走这里入库存。
pub fn save_tweet_page(
    tweet_id: String,
    text: String,
    title: Option<String>,
    author_name: Option<String>,
    screen_name: Option<String>,
) -> Result<WikiPageDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let t = crate::wiki::TweetText {
        tweet_id,
        text,
        title,
        author_name,
        screen_name,
    };
    let page = crate::wiki::save_tweet_page(&t, &store)?;
    Ok(WikiPageDto::from(page))
}

/// 内容对话消息 DTO（临时讨论的一条消息）
#[derive(Clone, Debug)]
pub struct ContentChatMessageDto {
    pub role: String,
    pub content: String,
}

/// 针对一段抓取内容做一次性对话回复（不写库，供保存前与 AI 讨论内容）
pub fn generate_content_chat(
    content: String,
    messages: Vec<ContentChatMessageDto>,
) -> Result<String> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let msgs: Vec<crate::ai::ContentChatMessage> = messages
        .into_iter()
        .map(|m| crate::ai::ContentChatMessage {
            role: m.role,
            content: m.content,
        })
        .collect();
    crate::ai::generate_content_chat(&content, &msgs, &store)
}

/// 当前推文抓取服务（设置页读取；当前仅支持 fxtwitter）
pub fn get_tweet_fetch_service() -> Result<String> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    Ok(store
        .get_meta("tweet_fetch_service")?
        .unwrap_or_else(|| "fxtwitter".to_string()))
}

/// 更新推文抓取服务（设置页保存）
pub fn update_tweet_fetch_service(service: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.set_meta("tweet_fetch_service", &service)
}

/// 按推文链接查知识库是否已存在对应页面（URL 查重）。
/// 已保存过则返回该页 DTO（UI 直接打开、不再抓取）；否则返回 None。
/// 链接格式不合法 / 未找到都算「不存在」。
pub fn find_tweet_source_page(url: String) -> Result<Option<WikiPageDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let Some(id) = crate::wiki::extract_tweet_id(&url) else {
        return Ok(None);
    };
    let slug = format!("tweet-{id}");
    let page = store.get_wiki_page(&slug)?;
    Ok(page.map(WikiPageDto::from))
}

/// 主题偏好 DTO（设置页「外观」：模式 + 预设；存 app_meta）
#[derive(Clone, Debug)]
pub struct ThemePrefsDto {
    /// "light" | "dark" | "system"
    pub mode: String,
    /// 预设名，如 "amber" | "indigo" | "aqua" | "violet"
    pub preset: String,
}

/// 读取主题偏好（默认深色 + 琥珀，保留现有观感）
pub fn get_theme_prefs() -> Result<ThemePrefsDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let mode = store
        .get_meta("theme_mode")?
        .unwrap_or_else(|| "dark".to_string());
    let preset = store
        .get_meta("theme_preset")?
        .unwrap_or_else(|| "amber".to_string());
    Ok(ThemePrefsDto { mode, preset })
}

/// 保存主题偏好（设置页「外观」保存）
pub fn update_theme_prefs(mode: String, preset: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.set_meta("theme_mode", &mode)?;
    store.set_meta("theme_preset", &preset)
}
