//! 推文抓取 / 内容对话 FRB 门面。

use super::*;

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
