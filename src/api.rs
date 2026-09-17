use crate::event::NewEvent;
use crate::storage::{RuleStatus, Store};
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
    pub id: String,
    pub name: String,
    pub provider_type: String,
    pub base_url: String,
    pub model: String,
    pub api_key_source: String,
    /// 明文密钥只在保存时上行；读取时不回传（用 api_key_source 判断是否已配置）
    pub api_key: String,
    pub is_active: bool,
    pub temperature: f64,
    pub max_tokens: Option<i64>,
}

fn dto_from_active(p: crate::storage::AiProviderConfig) -> AiProviderConfigDto {
    AiProviderConfigDto {
        id: p.id,
        name: p.name,
        provider_type: p.provider_type,
        base_url: p.base_url,
        model: p.model,
        api_key_source: p.api_key_source,
        api_key: String::new(),
        is_active: p.is_active,
        temperature: p.temperature,
        max_tokens: p.max_tokens,
    }
}

/// Get the active AI provider full config (for settings page prefill)
pub fn get_ai_provider_config() -> Result<Option<AiProviderConfigDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    Ok(store
        .active_ai_provider_config()?
        .map(dto_from_active))
}

/// 列出全部 AI provider 配置（多配置，仅一个 is_active=true）
pub fn list_ai_provider_configs() -> Result<Vec<AiProviderConfigDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    Ok(store
        .list_ai_provider_configs()?
        .into_iter()
        .map(|r| AiProviderConfigDto {
            id: r.id,
            name: r.name,
            provider_type: r.provider_type,
            base_url: r.base_url,
            model: r.model,
            api_key_source: r.api_key_source,
            api_key: String::new(),
            is_active: r.is_active,
            temperature: r.temperature,
            max_tokens: r.max_tokens,
        })
        .collect())
}

/// 新增或编辑 AI provider 配置；返回配置 id。
/// provider.id 为空表示新建；api_key 传空串表示保留原 key 不变（新建则必填）。
pub fn save_ai_provider_config(provider: AiProviderConfigDto) -> Result<String> {
    if provider.name.trim().is_empty() {
        anyhow::bail!("配置名称不能为空");
    }
    if provider.base_url.trim().is_empty() || provider.model.trim().is_empty() {
        anyhow::bail!("base_url 和 model 均不能为空");
    }
    if provider.id.trim().is_empty() && provider.api_key.trim().is_empty() {
        anyhow::bail!("新建配置时必须填写 API Key");
    }
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let id = if provider.id.trim().is_empty() {
        None
    } else {
        Some(provider.id.trim())
    };
    store.save_ai_provider_config(
        id,
        provider.name.trim(),
        &provider.provider_type,
        provider.base_url.trim(),
        provider.model.trim(),
        provider.api_key.trim(),
        provider.temperature,
        provider.max_tokens,
    )
}

/// 将指定配置设为激活（唯一激活项）
pub fn set_active_ai_provider_config(id: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.set_active_ai_provider_config(&id)?;
    Ok(())
}

/// 删除一个 AI provider 配置；若删除的是激活项，剩余第一条自动激活
pub fn delete_ai_provider_config(id: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.delete_ai_provider_config(&id)?;
    Ok(())
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

/// Create a new conversation
/// 个人经验规则 DTO
#[derive(Clone, Debug)]
pub struct RuleDto {
    pub id: String,
    pub content: String,
    pub status: String,
    pub created_at: String,
}

/// 列出规则库（含已生效与待确认）
pub fn list_rules() -> Result<Vec<RuleDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let rules = store.list_rules(None, None)?;
    Ok(rules
        .into_iter()
        .map(|r| RuleDto {
            id: r.id,
            content: r.content,
            status: r.status.as_str().to_string(),
            created_at: r.created_at,
        })
        .collect())
}

/// 新增一条规则（手动添加，直接生效）
pub fn add_rule(content: String) -> Result<String> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    store.add_rule(&content, RuleStatus::Active, None)
}

/// 删除一条规则
pub fn delete_rule(rule_id: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    store.delete_rule(&rule_id)?;
    Ok(())
}

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
pub fn send_message(conversation_id: String, role: String, content: String, parent_message_id: Option<String>) -> Result<MessageDto> {
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
    pub source_url: Option<String>,
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
            source_url: p.source_url,
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

/// 更新知识页标签（应用内整理元数据用；传空数组即清空）。返回更新后的页面。
pub fn update_wiki_tags(slug: String, tags: Vec<String>) -> Result<WikiPageDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let page = store.update_wiki_tags(&slug, &tags)?;
    Ok(WikiPageDto::from(page))
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
    let page = crate::wiki::save_tweet_page(
        &t,
        Some(&format!("https://x.com/i/status/{}", t.tweet_id)),
        &store,
    )?;
    Ok(WikiPageDto::from(page))
}

/// 把用户粘贴的纯文本保存为知识库页面（kind=topic），返回该页。
/// content_md 保留全文，不截断；tags 可选（页面保留「note」锚点标签）。
pub fn save_text_page(
    text: String,
    title: Option<String>,
    tags: Vec<String>,
) -> Result<WikiPageDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let page = crate::wiki::save_text_page(&text, title.as_deref(), &tags, &store)?;
    Ok(WikiPageDto::from(page))
}

/// 判断一个网址是否需要走专用抓取 API（当前：x.com/twitter.com 推文 → fxtwitter）。
/// 返回 "tweet" 或 "web"，供导入入口统一分发，避免前端各自猜测。
pub fn guess_import_kind(url: String) -> Result<String> {
    let trimmed = url.trim();
    if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        anyhow::bail!("仅支持 http/https 链接");
    }
    Ok(if crate::wiki::is_tweet_url(trimmed) {
        "tweet".to_string()
    } else {
        "web".to_string()
    })
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

// ── 个人待办（todo） ──────────────────────────────────────────────────

/// 待办 DTO
#[derive(Clone, Debug)]
pub struct TodoDto {
    pub id: String,
    pub title: String,
    pub status: String,
    pub priority: String,
    pub due_at: Option<String>,
    pub related_event_id: Option<String>,
    pub related_wiki_slug: Option<String>,
    pub note: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<crate::storage::Todo> for TodoDto {
    fn from(t: crate::storage::Todo) -> Self {
        Self {
            id: t.id,
            title: t.title,
            status: t.status.as_str().to_string(),
            priority: t.priority,
            due_at: t.due_at,
            related_event_id: t.related_event_id,
            related_wiki_slug: t.related_wiki_slug,
            note: t.note,
            created_at: t.created_at,
            updated_at: t.updated_at,
        }
    }
}

/// 列出待办（status 过滤：open/done/archived；None 时列出 open+done）
pub fn list_todos(status: Option<String>) -> Result<Vec<TodoDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let todos = store.list_todos(status.as_deref())?;
    Ok(todos.into_iter().map(TodoDto::from).collect())
}

/// 新建待办（用户手动创建，直接生效）
pub fn create_todo(
    title: String,
    due_at: Option<String>,
    priority: Option<String>,
    related_wiki_slug: Option<String>,
    note: Option<String>,
) -> Result<TodoDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    if title.trim().is_empty() {
        anyhow::bail!("待办内容不能为空");
    }
    let t = store.create_todo(
        title.trim(),
        priority.as_deref().unwrap_or("normal"),
        due_at.as_deref(),
        None,
        related_wiki_slug.as_deref(),
        note.as_deref(),
    )?;
    Ok(TodoDto::from(t))
}

/// 更新待办状态（open/done/archived）
pub fn update_todo_status(id: String, status: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.update_todo_status(&id, crate::storage::TodoStatus::parse(&status))
}

/// 更新待办的可编辑字段（标题 / 补充 / 优先级 / 截止时间）。
/// 可选字段传 None 表示清除（如结束拖延、去掉截止时间）。
pub fn update_todo(
    id: String,
    title: String,
    note: Option<String>,
    priority: Option<String>,
    due_at: Option<String>,
) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.update_todo(
        &id,
        &title,
        note.as_deref(),
        priority.as_deref(),
        due_at.as_deref(),
    )
}

/// 删除一条待办
pub fn delete_todo(id: String) -> Result<bool> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.delete_todo(&id)
}

// ── 任意 URL 导入 ──────────────────────────────────────────────────────

/// 任意 URL 抓取结果 DTO（推文或普通页面，只解析不入库）
#[derive(Clone, Debug)]
pub struct ImportUrlDto {
    pub source_url: String,
    /// "tweet" | "webpage"
    pub source_kind: String,
    pub title: Option<String>,
    pub content_md: String,
    pub author_name: Option<String>,
    pub screen_name: Option<String>,
}

/// 抓取任意 URL 的内容（推文走 fxtwitter，普通页面走 HTML 文本提取）。
/// 只解析不写库，由后续「保存」动作决定。
pub fn fetch_import_url(url: String) -> Result<ImportUrlDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    if !url.starts_with("http://") && !url.starts_with("https://") {
        anyhow::bail!("仅支持 http/https 链接");
    }
    let service = store
        .get_meta("tweet_fetch_service")?
        .unwrap_or_else(|| "fxtwitter".to_string());
    if service != "fxtwitter" && crate::wiki::is_tweet_url(&url) {
        anyhow::bail!("暂不支持的推文抓取服务: {service}");
    }
    let c = crate::wiki::fetch_import_url(&url)?;
    Ok(ImportUrlDto {
        source_url: c.source_url,
        source_kind: c.source_kind,
        title: c.title,
        content_md: c.content_md,
        author_name: c.author_name,
        screen_name: c.screen_name,
    })
}

/// 把抓取到的 URL 内容保存为知识库页面（kind=source，带 source_url 溯源）。
/// 用户点击「保存」才走这里入库。
pub fn save_imported_page(
    title: String,
    content_md: String,
    source_url: String,
    source_kind: String,
    tags: Vec<String>,
) -> Result<WikiPageDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let trimmed = content_md.trim();
    if trimmed.is_empty() {
        anyhow::bail!("内容为空，无法保存");
    }
    let title = if title.trim().is_empty() {
        "未命名导入".to_string()
    } else {
        title.trim().to_string()
    };
    let summary: String = trimmed.chars().take(120).collect();
    // URL 去重：同一条 source_url 已导入过 → 走更新而不是复制新页
    let existing_slug = store
        .find_wiki_page_by_source_url(&source_url)?
        .map(|p| p.slug);
    let slug = match existing_slug {
        Some(slug) => slug,
        None => format!(
            "{}-{}",
            if source_kind == "tweet" { "tweet" } else { "import" },
            &uuid::Uuid::new_v4().to_string()[..8]
        ),
    };
    let mut all_tags = tags;
    all_tags.push("import".to_string());
    if source_kind == "webpage" {
        all_tags.push("web".to_string());
    } else {
        all_tags.push("tweet".to_string());
    }
    all_tags.sort();
    all_tags.dedup();
    let draft = crate::storage::WikiPageDraft {
        slug,
        kind: "source".to_string(),
        title,
        summary,
        content_md: trimmed.to_string(),
        tags: all_tags,
        source_event_ids: vec![],
        status: "active".to_string(),
        reason: format!("从 {source_url} 导入"),
        source_url: Some(source_url),
    };
    let outcome = store.upsert_wiki_page(&draft)?;
    Ok(WikiPageDto::from(outcome.page))
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
        None => store.create_wiki_chat_conversation(
            &page_slug,
            &format!("[知识页] {}", page.title),
        )?,
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
