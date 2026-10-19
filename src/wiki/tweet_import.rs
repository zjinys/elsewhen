//! 自 wiki.rs 机械分段平移（行为零变化）。



use crate::storage::{ContentPolicy, Store, WikiPage, WikiPageDraft};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use crate::wiki::media_download::download_tweet_images;
// ── 外部来源导入：x.com 推文 → 知识库页面（kind=source） ────────────────

/// fxtwitter API 响应（只取需要的字段）
#[derive(Debug, Deserialize)]
struct FxTwitterResponse {
    code: Option<i64>,
    message: Option<String>,
    tweet: Option<FxTweet>,
}

#[derive(Debug, Deserialize)]
struct FxTweet {
    text: Option<String>,
    author: Option<FxAuthor>,
    /// 文章型推文（x.com/i/article/…）：长文在这里，tweet.text 只是文章链接
    article: Option<FxArticle>,
    /// 普通推文（非文章型）的配图：追加在正文末尾。
    media: Option<FxMedia>,
}

/// 普通推文的顶层媒体容器（只取照片；视频/GIF 不落盘）。
#[derive(Debug, Deserialize)]
struct FxMedia {
    #[serde(default)]
    photos: Vec<FxPhoto>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct FxPhoto {
    pub(crate) url: Option<String>,
}

/// fxtwitter 的 article 字段（note tweet / x 文章）
#[derive(Debug, Deserialize)]
struct FxArticle {
    title: Option<String>,
    content: Option<FxArticleContent>,
    /// 文章配图（atomic block 通过 entityRanges[0].key 1-based 索引关联到这里）。
    /// 图片直链在 media_info.original_img_url（pbs.twimg.com，可直接下载）。
    #[serde(default)]
    media_entities: Vec<FxMediaEntity>,
}

#[derive(Debug, Deserialize)]
struct FxArticleContent {
    #[serde(default)]
    blocks: Vec<FxArticleBlock>,
}

#[derive(Debug, Deserialize)]
struct FxArticleBlock {
    text: Option<String>,
    /// "unstyled" / "header-two" / "atomic"（图片占位）等；缺省视为普通段落。
    #[serde(rename = "type", default)]
    block_type: Option<String>,
    /// atomic block 的第一个 entityRange.key = media_entities 的 1-based 索引。
    #[serde(rename = "entityRanges", default)]
    entity_ranges: Vec<FxEntityRange>,
}

#[derive(Debug, Deserialize)]
struct FxEntityRange {
    key: Option<usize>,
}

/// 文章配图实体：只取媒体直链所需的字段。
#[derive(Debug, Deserialize)]
struct FxMediaEntity {
    media_info: Option<FxMediaInfo>,
}

#[derive(Debug, Deserialize)]
struct FxMediaInfo {
    original_img_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FxAuthor {
    name: Option<String>,
    screen_name: Option<String>,
}

/// 通用 URL 抓取结果（推文或普通页面）
#[derive(Debug, Clone)]
pub struct ImportedContent {
    pub source_url: String,
    pub source_kind: String, // "tweet" | "webpage"
    pub title: Option<String>,
    pub content_md: String,
    pub author_name: Option<String>,
    pub screen_name: Option<String>,
}

/// 判断是否推文 URL（x.com / twitter.com 含 status）
pub fn is_tweet_url(url: &str) -> bool {
    let lower = url.to_lowercase();
    (lower.contains("x.com/") || lower.contains("twitter.com/"))
        && (lower.contains("/status/") || lower.contains("/i/status/"))
}

/// 抓取任意 URL：推文走 fxtwitter，其他走 HTML 纯文本提取
pub fn fetch_import_url(url: &str) -> Result<ImportedContent> {
    if is_tweet_url(url) {
        let t = fetch_tweet_text(url)?;
        Ok(ImportedContent {
            source_url: url.to_string(),
            source_kind: "tweet".to_string(),
            title: t.title.clone(),
            content_md: t.text.clone(),
            author_name: t.author_name.clone(),
            screen_name: t.screen_name.clone(),
        })
    } else {
        // 普通网页
        let client = crate::http_proxy::with_outbound_proxy(reqwest::blocking::Client::builder())
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .context("创建 HTTP 客户端失败")?;
        let resp = client
            .get(url)
            .header(
                "User-Agent",
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36",
            )
            .send()
            .context("抓取页面失败")?;
        if !resp.status().is_success() {
            anyhow::bail!("页面返回 HTTP {}", resp.status());
        }
        let html = resp.text().context("读取页面内容失败")?;
        let title = extract_title(&html);
        let text = crate::ai::tool::html_to_text(&html);
        let text = text.trim().to_string();
        if text.is_empty() {
            anyhow::bail!("页面没有可读文本内容");
        }
        Ok(ImportedContent {
            source_url: url.to_string(),
            source_kind: "webpage".to_string(),
            title,
            content_md: text,
            author_name: None,
            screen_name: None,
        })
    }
}

/// 从 HTML 抽取 <title> 文本（不做 DOM 解析，简单正则）
fn extract_title(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let start = lower.find("<title")? + 6;
    // 跳过 <title> 标签（可能含属性后跟 >）
    let rest = &html[start..];
    let gt = rest.find('>')?;
    let inner = &rest[gt + 1..];
    let end = inner.find("</title>").or_else(|| inner.find("</TITLE>"))?;
    let title = inner[..end].trim().to_string();
    if title.is_empty() { None } else { Some(title) }
}

/// 抓取到的推文内容（纯数据，尚未入库；只有点击保存才写库）
#[derive(Debug, Clone)]
pub struct TweetText {
    pub tweet_id: String,
    /// 可读正文：普通推文为 tweet.text；文章型推文为 article.content.blocks 拼接
    pub text: String,
    /// 文章型推文的标题（article.title），普通推文为 None
    pub title: Option<String>,
    pub author_name: Option<String>,
    pub screen_name: Option<String>,
}

/// fxtwitter 内部解析产物
pub(crate) struct FxTweetContent {
    pub(crate) text: String,
    pub(crate) title: Option<String>,
    pub(crate) author_name: Option<String>,
    pub(crate) screen_name: Option<String>,
    /// 正文里出现的图片直链（atomic block 占位 `{{tweet-image-N}}` 的 N-1 索引对应这里）。
    /// 纯解析层只收集 URL；实际下载落盘在网络层完成。
    pub(crate) image_urls: Vec<String>,
}

/// 正文里的图片占位标记前缀（atomic block 处先插入占位，下载后替换为本地路径）。
const IMAGE_PLACEHOLDER_PREFIX: &str = "{{tweet-image-";

/// 生成第 `idx`（0-based）张图的占位标记。
/// 前缀 `{{` 与后缀 `}}` 在 format! 里都需双写转义。
pub(crate) fn image_placeholder(idx: usize) -> String {
    format!("{IMAGE_PLACEHOLDER_PREFIX}{idx}}}}}")
}

/// 从 x.com / twitter.com 链接中提取推文 id（形如 /status/{纯数字}）。
/// 容忍查询参数、hash、结尾斜杠、大小写（Status）与非 x.com 域名前缀。
pub fn extract_tweet_id(url: &str) -> Option<String> {
    let base = url.split(['?', '#']).next()?;
    let segments: Vec<&str> = base.split('/').collect();
    let pos = segments
        .iter()
        .position(|s| s.eq_ignore_ascii_case("status"))?;
    let id = segments.get(pos + 1)?;
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(id.to_string())
}

/// 是否本地文件 URL（`file://` 前缀，目录导入的项目页用它把路径记进 `source_url`）。
/// 目录也是 URL（RFC 8089），复用来源字段即可，不必为路径另加列。
pub fn is_file_url(url: &str) -> bool {
    url.starts_with("file://")
}

/// 本地绝对路径 → 规范 `file://` URL（`file:///home/u/a b` 中空格等按字节百分号编码）。
/// 存 `source_url` 前统一走这里，保证同一目录永远得到同一字符串（URL 去重免费生效）。
pub fn path_to_file_url(path: &Path) -> String {
    #[cfg(unix)]
    let bytes: &[u8] = std::os::unix::ffi::OsStrExt::as_bytes(path.as_os_str());
    #[cfg(not(unix))]
    let bytes: &[u8] = &path.to_string_lossy().as_bytes().to_vec();
    let mut out = String::from("file://");
    for &b in bytes {
        // 不编码：unreserved + `/`（分隔符）+ `:`（盘符 `C:` 宽容）。其余按字节编码。
        if b.is_ascii_alphanumeric() || b"-._~/:".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `file://` URL → 本地路径；非 file  scheme 返回 None（调用方据此区分网页来源与本地来源）。
/// 只接受空 host 或 `localhost`，拒绝 `file://other-host/...` 这类远端写法。
pub fn file_url_to_path(url: &str) -> Option<PathBuf> {
    let rest = url.strip_prefix("file://")?;
    // 去掉 host 段（`` 或 `localhost` 才合法）
    let path_part = if let Some(after) = rest.strip_prefix('/') {
        format!("/{after}")
    } else if let Some(path) = rest.strip_prefix("localhost/") {
        format!("/{path}")
    } else {
        return None;
    };
    let mut bytes = Vec::with_capacity(path_part.len());
    let raw = path_part.as_bytes();
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'%' {
            let hex = std::str::from_utf8(raw.get(i + 1..i + 3)?).ok()?;
            bytes.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            bytes.push(raw[i]);
            i += 1;
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        Some(PathBuf::from(std::ffi::OsString::from_vec(bytes)))
    }
    #[cfg(not(unix))]
    {
        Some(PathBuf::from(String::from_utf8_lossy(&bytes).into_owned()))
    }
}

/// 从 fxtwitter 响应构造可读内容（纯解析，可单测）：
/// - 普通推文：正文 = tweet.text
/// - 文章型推文（tweet.text 只是 x.com/i/article/… 链接）：正文 = article.content.blocks 拼接，
///   标题 = article.title；无正文段落时回退到 tweet.text。
fn parse_tweet_content(parsed: FxTwitterResponse) -> Result<FxTweetContent> {
    if parsed.code != Some(200) {
        anyhow::bail!(
            "fxtwitter 返回错误: {}",
            parsed.message.unwrap_or_else(|| "未知错误".to_string())
        );
    }
    let tweet = parsed.tweet.context("fxtwitter 响应缺少 tweet 字段")?;
    let article_title = tweet.article.as_ref().and_then(|a| a.title.clone());
    let mut image_urls: Vec<String> = Vec::new();
    let text = match &tweet.article {
        Some(article) => {
            let paragraphs: Vec<String> = article
                .content
                .as_ref()
                .map(|c| {
                    c.blocks
                        .iter()
                        .filter_map(|b| {
                            // atomic block = 图片占位：记录直链、插入占位标记。
                            if b.block_type.as_deref() == Some("atomic") {
                                let idx1 = b.entity_ranges.first().and_then(|r| r.key)?;
                                let url = article
                                    .media_entities
                                    .get(idx1.saturating_sub(1))
                                    .and_then(|e| e.media_info.as_ref())
                                    .and_then(|m| m.original_img_url.clone())?;
                                let ph = image_placeholder(image_urls.len());
                                image_urls.push(url);
                                Some(ph)
                            } else {
                                b.text.clone()
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            if paragraphs.is_empty() {
                match tweet.text {
                    Some(t) => t,
                    None => anyhow::bail!("推文没有正文"),
                }
            } else {
                paragraphs.join("\n\n")
            }
        }
        None => {
            let mut t = match tweet.text {
                Some(t) => t,
                None => anyhow::bail!("推文没有正文"),
            };
            // 普通推文配图：追加在正文末尾（无段落穿插概念）。
            if let Some(media) = &tweet.media {
                for photo in &media.photos {
                    if let Some(url) = &photo.url {
                        let ph = image_placeholder(image_urls.len());
                        image_urls.push(url.clone());
                        t.push_str("\n\n");
                        t.push_str(&ph);
                    }
                }
            }
            t
        }
    };
    let author = tweet.author;
    Ok(FxTweetContent {
        text,
        title: article_title,
        author_name: author.as_ref().and_then(|a| a.name.clone()),
        screen_name: author.as_ref().and_then(|a| a.screen_name.clone()),
        image_urls,
    })
}

/// 调用 fxtwitter 获取推文长文（long_mode=true 取完整正文）
fn fetch_tweet(tweet_id: &str) -> Result<FxTweetContent> {
    let client = crate::http_proxy::with_outbound_proxy(
        reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(30)),
    )
    .build()
    .context("创建 HTTP 客户端失败")?;
    let url = format!("https://api.fxtwitter.com/status/{tweet_id}?long_mode=true");
    let resp = client
        .get(&url)
        .header(
            "User-Agent",
            "elsewhen/0.1 (local-first personal knowledge base)",
        )
        .send()
        .context("请求 fxtwitter 失败")?;
    let parsed: FxTwitterResponse = resp.json().context("解析 fxtwitter 响应失败")?;
    parse_tweet_content(parsed)
}

/// 从 x.com 推文链接抓取长文（只解析，不写库）
pub fn fetch_tweet_text(url: &str) -> Result<TweetText> {
    let tweet_id = extract_tweet_id(url).context(
        "不是有效的 x.com / twitter.com 推文链接（形如 https://x.com/{用户}/status/{推文id}）",
    )?;
    let mut content = fetch_tweet(&tweet_id)?;
    // 下载配图落盘（数据目录/tweet_images/<tweet_id>/），正文占位替换为本地绝对路径。
    // 失败不阻断正文导入——单张下载失败仅跳过该图、占位换成「图片下载失败」说明。
    if !content.image_urls.is_empty() {
        let data_dir = crate::config::AppConfig::load().map(|c| c.data_dir).ok();
        if let Some(dir) = data_dir {
            download_tweet_images(&tweet_id, &dir, &mut content);
        }
    }
    Ok(TweetText {
        tweet_id,
        text: content.text,
        title: content.title,
        author_name: content.author_name,
        screen_name: content.screen_name,
    })
}

/// 把已抓取的推文内容保存为知识库页面（kind=source）。
/// 同一推文重复保存 = 更新同一页（upsert）。
pub fn save_tweet_page(t: &TweetText, source_url: Option<&str>, store: &Store) -> Result<WikiPage> {
    let author_label = match (&t.author_name, &t.screen_name) {
        (Some(name), _) => name.clone(),
        (None, Some(handle)) => format!("@{handle}"),
        (None, None) => format!("推文 {}", t.tweet_id),
    };
    // 文章型推文优先用文章标题；普通推文用「{作者} 的推文」
    let title = match &t.title {
        Some(t) if !t.trim().is_empty() => t.clone(),
        _ => format!("{author_label} 的推文"),
    };
    // 摘要取正文前 ~120 字（按字符取，避免截断代理对）
    let summary: String = t.text.chars().take(120).collect();

    let draft = WikiPageDraft {
        slug: format!("tweet-{}", t.tweet_id),
        kind: "source".to_string(),
        title,
        summary,
        content_md: t.text.clone(),
        tags: vec!["tweet".to_string()],
        source_event_ids: vec![],
        status: "active".to_string(),
        reason: format!("从 x.com 导入推文 {}", t.tweet_id),
        source_url: source_url.map(|s| s.to_string()),
    };
    let outcome = store.upsert_wiki_page(&draft, ContentPolicy::Always)?;
    Ok(outcome.page)
}

/// 把用户粘贴的纯文本保存为知识库页面（kind=note，随手笔记）。
/// content_md 保留全文、绝不截断，仅 summary（索引摘要）截断。
pub fn save_text_page(
    text: &str,
    title: Option<&str>,
    tags: &[String],
    store: &Store,
) -> Result<WikiPage> {
    save_text_page_with_kind(text, title, tags, "note", store)
}

/// 同 `save_text_page`，但页型由调用方指定（"note"=随手笔记 / "topic"=主题）。
/// 「存为主题」从对话消息保存时传 "topic"；其余导入入口仍走 note。
pub fn save_text_page_with_kind(
    text: &str,
    title: Option<&str>,
    tags: &[String],
    kind: &str,
    store: &Store,
) -> Result<WikiPage> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        anyhow::bail!("文本为空，无法保存");
    }
    // 标题：优先用显式传入的，否则取第一行非空内容的前 60 字
    let title = match title {
        Some(t) if !t.trim().is_empty() => t.trim().to_string(),
        _ => {
            let first = trimmed
                .lines()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("")
                .trim();
            if first.is_empty() {
                "未命名笔记".to_string()
            } else {
                first.chars().take(60).collect()
            }
        }
    };
    // 只允许 note / topic 两种，其余报错（避免 UI 传错生成奇怪的页型）。
    let kind = match kind {
        "note" | "topic" => kind,
        other => anyhow::bail!("不支持的页面类型：{other}（仅支持 note/topic）"),
    };

    // 摘要只用于索引展示，截断无妨
    let summary: String = trimmed.chars().take(120).collect();
    // slug 前缀与页型一致：note-xxx / topic-xxx
    let slug = format!("{}-{}", kind, &uuid::Uuid::new_v4().to_string()[..8]);

    // 去掉空白项并去重；始终保留与页型一致的锚点标签（note / topic）
    let mut all_tags: Vec<String> = tags
        .iter()
        .map(|t| t.trim().trim_start_matches('#').to_string())
        .filter(|t| !t.is_empty())
        .collect();
    all_tags.push(kind.to_string());
    all_tags.sort();
    all_tags.dedup();

    let draft = WikiPageDraft {
        slug,
        kind: kind.to_string(),
        title,
        summary,
        content_md: trimmed.to_string(),
        tags: all_tags,
        source_event_ids: vec![],
        status: "active".to_string(),
        reason: "用户粘贴文本导入".to_string(),
        source_url: None,
    };
    let outcome = store.upsert_wiki_page(&draft, ContentPolicy::Always)?;
    // 随手记生而入册（摘录孵化闭环，提案 2026-10-19 评审澄清）：进孵化
    // 闸门的只有导入流（confirm_knowledge_source）——用户「存下来就是
    // 我的知识」，不属于「握取内容等定论」的心智。缺省 endorse，走
    // set_wiki_opinion 双写通道（wiki_pages + knowledge_sources 同进退）。
    store.set_wiki_opinion(&outcome.page.slug, Some("endorse"))?;
    Ok(outcome.page)
}
