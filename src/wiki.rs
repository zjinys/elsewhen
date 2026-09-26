//! LLM wiki：个人知识库层。
//!
//! 思想来自 Karpathy 的 "LLM Wiki" 模式（gist 442a6bf555914893e9891c11519de94f）：
//!
//! - 三层架构：原始事件（不可变，事实来源） / wiki 页（LLM 维护、核心合并的 markdown） / schema（约束 LLM 的规则）
//! - "编译一次，持续更新"：知识不是每次查询时从原始文档重推导，而是持续复利的产物
//! - 三个操作：ingest（写回） / query（导航 + 引用） / lint（健康检查）
//! - 好答案归档回 wiki 成为新页面，探索也复利
//!
//! 本模块：确定性合并、校验、索引与导出。LLM 只提供"提议"，本模块按规则落地。

use crate::ai::memory::ContextMessage;
use crate::ai::provider::{AiProvider, OpenAiCompatibleConfig, OpenAiCompatibleProvider};
use crate::event::{AnnotationSet, EventSummary};
use crate::storage::{ContentPolicy, EventRecord, RelationDraft, Store, WikiPage, WikiPageDraft};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

pub const WIKI_PROMPT_VERSION: &str = "wiki-digest-v1";

/// 允许的页面类型
pub const WIKI_KINDS: &[&str] = &[
    "profile",
    "person",
    "recurring_cost",
    "capability",
    "asset",
    "project",
    "relationship",
    "decision",
    "habit",
    "constraint",
    "insight",
    "topic",
    "source",
    // 用户粘贴笔记（M1 kind 拆分：从 topic 独立为素材档 note）
    "note",
];

/// 认知推微生成时要导航的页面类型（镜像四透镜的取材范围）
pub const INSIGHT_KINDS: &[&str] = &[
    "profile",
    "recurring_cost",
    "capability",
    "asset",
    "habit",
    "constraint",
    "decision",
    "relationship",
];

pub fn validate_kind(kind: &str) -> bool {
    WIKI_KINDS.contains(&kind)
}

pub fn validate_slug(slug: &str) -> bool {
    let seg_ok = |s: &str| {
        !s.is_empty()
            && !s.starts_with('-')
            && !s.ends_with('-')
            && !s.contains(' ')
            && s.chars().all(|c| c.is_alphanumeric() || c == '-')
    };
    let parts: Vec<&str> = slug.split('/').collect();
    parts.len() <= 2 && parts.iter().all(|p| seg_ok(p))
}

/// 任意字符串 → 小写 kebab-case（用于 insight 归档页 slug）
pub fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for c in s.chars() {
        let keep = if c.is_alphanumeric() {
            Some(c.to_ascii_lowercase())
        } else {
            None
        };
        if let Some(k) = keep {
            out.push(k);
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        out = "page".to_string();
    }
    out
}

/// 生成不冲突的 slug：若 `{base}` 已被「不同标题」的页面占用（同名 slug 撞车，
/// 如历史页标题不同但 slugify 后相同），则追加 -2、-3… 后缀避让。
/// 同名同人走 find_wiki_page_by_title 合并，这里只兜底冲突。
pub(crate) fn unique_slug(store: &Store, base: &str, title: &str) -> Result<String> {
    let mut slug = base.to_string();
    let mut n = 2usize;
    loop {
        match store.get_wiki_page(&slug)? {
            None => return Ok(slug),
            Some(page) if page.title.trim() == title => return Ok(slug),
            Some(_) => {
                slug = format!("{base}-{n}");
                n += 1;
            }
        }
    }
}

fn kind_dir(kind: &str) -> String {
    kind.replace('_', "-")
}

/// 生成 wiki 索引 markdown（导航用，对应 LLM wiki 的 index.md）。按 kind 分组，一行摘要 + 证据数。
pub fn build_index_md(pages: &[WikiPage]) -> String {
    let mut lines = vec!["# 知识库索引".to_string()];
    let mut by_kind: Vec<(String, Vec<&WikiPage>)> = Vec::new();
    for page in pages {
        if let Some(entry) = by_kind.iter_mut().find(|(k, _)| *k == page.kind) {
            entry.1.push(page);
        } else {
            by_kind.push((page.kind.clone(), vec![page]));
        }
    }
    by_kind.sort_by(|a, b| a.0.cmp(&b.0));
    for (kind, mut pages) in by_kind {
        pages.sort_by(|a, b| b.evidence_count.cmp(&a.evidence_count));
        lines.push(format!("\n### {}", kind));
        for p in pages {
            let e = if p.evidence_count > 0 {
                format!("（{} 条事件支持）", p.evidence_count)
            } else {
                String::new()
            };
            lines.push(format!("- `{}` — {}{}", p.slug, p.summary, e));
        }
    }
    lines.join("\n")
}

/// 选取用于 prompt 上下文的页面（数量与字符上限）
pub fn select_context_pages(
    pages: Vec<WikiPage>,
    kinds: &[&str],
    max_chars: usize,
) -> Vec<WikiPage> {
    let mut picked: Vec<WikiPage> = pages
        .into_iter()
        .filter(|p| p.status != "archived" && kinds.contains(&p.kind.as_str()))
        .collect();
    picked.sort_by(|a, b| {
        b.evidence_count
            .cmp(&a.evidence_count)
            .then_with(|| b.last_seen_at.cmp(&a.last_seen_at))
    });
    let mut total = 0usize;
    picked.retain(|p| {
        let cost = p.content_md.len().min(4000);
        if total + cost <= max_chars {
            total += cost;
            true
        } else {
            false
        }
    });
    picked
}

#[cfg(not(test))]
fn call_provider(store: &Store, system: &str, user: &str, max_tokens: u32) -> Result<String> {
    let ai_config = store
        .active_ai_provider_config()?
        .context("没有可用的 AI Provider 配置，请先 `elsewhen settings` 配置")?;
    let provider_config = OpenAiCompatibleConfig {
        base_url: ai_config.base_url,
        api_key: ai_config.api_key,
        model: ai_config.model,
        temperature: 0.3,
        max_tokens: Some(max_tokens),
    };
    let provider = OpenAiCompatibleProvider::new(provider_config)?;
    provider
        .generate_reply(vec![
            ContextMessage::new("system", system.to_string()),
            ContextMessage::new("user", user.to_string()),
        ])
        .map(|r| r.content)
}

#[cfg(test)]
fn call_provider(_store: &Store, _system: &str, user: &str, _max_tokens: u32) -> Result<String> {
    Ok(user.to_string())
}

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
}

/// fxtwitter 的 article 字段（note tweet / x 文章）
#[derive(Debug, Deserialize)]
struct FxArticle {
    title: Option<String>,
    content: Option<FxArticleContent>,
}

#[derive(Debug, Deserialize)]
struct FxArticleContent {
    #[serde(default)]
    blocks: Vec<FxArticleBlock>,
}

#[derive(Debug, Deserialize)]
struct FxArticleBlock {
    text: Option<String>,
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
        let client = reqwest::blocking::Client::builder()
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
    if title.is_empty() {
        None
    } else {
        Some(title)
    }
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
struct FxTweetContent {
    text: String,
    title: Option<String>,
    author_name: Option<String>,
    screen_name: Option<String>,
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
    let text = match &tweet.article {
        Some(article) => {
            let paragraphs: Vec<&str> = article
                .content
                .as_ref()
                .map(|c| c.blocks.iter().filter_map(|b| b.text.as_deref()).collect())
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
        None => match tweet.text {
            Some(t) => t,
            None => anyhow::bail!("推文没有正文"),
        },
    };
    let author = tweet.author;
    Ok(FxTweetContent {
        text,
        title: article_title,
        author_name: author.as_ref().and_then(|a| a.name.clone()),
        screen_name: author.as_ref().and_then(|a| a.screen_name.clone()),
    })
}

/// 调用 fxtwitter 获取推文长文（long_mode=true 取完整正文）
fn fetch_tweet(tweet_id: &str) -> Result<FxTweetContent> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
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
    let content = fetch_tweet(&tweet_id)?;
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

/// 把用户粘贴的纯文本保存为知识库页面（kind=topic）。
/// content_md 保留全文、绝不截断，仅 summary（索引摘要）截断。
pub fn save_text_page(
    text: &str,
    title: Option<&str>,
    tags: &[String],
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
    // 摘要只用于索引展示，截断无妨
    let summary: String = trimmed.chars().take(120).collect();
    let slug = format!("note-{}", &uuid::Uuid::new_v4().to_string()[..8]);

    // 去掉空白项并去重；始终保留「note」锚点标签
    let mut all_tags: Vec<String> = tags
        .iter()
        .map(|t| t.trim().trim_start_matches('#').to_string())
        .filter(|t| !t.is_empty())
        .collect();
    all_tags.push("note".to_string());
    all_tags.sort();
    all_tags.dedup();

    let draft = WikiPageDraft {
        slug,
        kind: "note".to_string(),
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
    Ok(outcome.page)
}

// ── 人物关系（AI 从对话识别「人 ↔ 事情/项目」，用户确认后落地） ───────────

/// 执行「人物 + 关系」草拟（用户已确认）。
/// 输入 args：`{ "people": [{"name","role_note"}], "relations": [{"person","target","relation","note"}] }`
/// - 每个新人物建 kind=person 页（slug `person/<名>`，标题查重，不重复建档）
/// - 每个关联目标按标题查重，不存在则建 kind=topic 页（slug `topic/<名>`）
/// - 再写入结构化关系（(from,to,relation) 唯一）
/// 返回执行摘要。
pub fn apply_people_relations(
    args: &serde_json::Value,
    store: &Store,
    conversation_id: &str,
) -> Result<String> {
    #[derive(Deserialize)]
    struct PersonDraft {
        name: String,
        #[serde(default)]
        role_note: String,
    }
    #[derive(Deserialize)]
    struct RelationDraftArg {
        #[serde(default)]
        person: String,
        #[serde(default)]
        target: String,
        #[serde(default)]
        relation: String,
        #[serde(default)]
        note: String,
        #[serde(default)]
        from_slug: Option<String>,
        #[serde(default)]
        to_slug: Option<String>,
    }
    #[derive(Deserialize)]
    struct PeopleRelationsArgs {
        #[serde(default)]
        people: Vec<PersonDraft>,
        #[serde(default)]
        relations: Vec<RelationDraftArg>,
        #[serde(default)]
        source_event_id: Option<String>,
    }

    let parsed: PeopleRelationsArgs = serde_json::from_value(args.clone())?;
    if parsed.people.is_empty() && parsed.relations.is_empty() {
        anyhow::bail!("没有需要保存的人物或关系");
    }

    // 1) 人物建档：标题精确查重 → 不存在才建 person 页
    let mut person_entries: Vec<(String, String, String)> = Vec::new(); // (name, slug, kind)
    for p in &parsed.people {
        let name = p.name.trim().to_string();
        if name.is_empty() {
            continue;
        }
        if person_entries.iter().any(|(n, _, _)| *n == name) {
            continue;
        }
        let matches = store.find_wiki_pages_by_title_or_alias(&name)?;
        let selected_slug = parsed
            .relations
            .iter()
            .find(|relation| relation.person.trim() == name)
            .and_then(|relation| relation.from_slug.clone());
        if matches.len() > 1 && selected_slug.is_none() {
            anyhow::bail!(
                "人物「{name}」存在多个同名页面（{}），请先在知识库中消歧后再确认",
                matches
                    .iter()
                    .map(|page| page.slug.as_str())
                    .collect::<Vec<_>>()
                    .join("、")
            );
        }
        if let Some(page) = selected_slug
            .as_deref()
            .and_then(|slug| matches.iter().find(|page| page.slug == slug))
            .or_else(|| matches.first())
            .cloned()
        {
            person_entries.push((name, page.slug.clone(), page.kind.clone()));
            continue;
        }
        let slug = unique_slug(store, &format!("person/{}", slugify(&name)), &name)?;
        let note = p.role_note.trim().to_string();
        let summary = if note.is_empty() {
            format!("{name}（对话中出现的人物）")
        } else {
            note.clone()
        };
        let content_md = if note.is_empty() {
            format!("{name} 是对话中出现的人物。")
        } else {
            format!("# {name}\n\n{note}\n\n---\n由 AI 从对话中识别，用户确认后建档。")
        };
        let draft = WikiPageDraft {
            slug: slug.clone(),
            kind: "person".to_string(),
            title: name.clone(),
            summary,
            content_md,
            tags: vec![name.clone()],
            source_event_ids: vec![],
            status: "active".to_string(),
            reason: "AI 从对话识别人物，用户确认".to_string(),
            source_url: None,
        };
        store.upsert_wiki_page(&draft, ContentPolicy::PreserveHumanEdits)?;
        person_entries.push((name, slug, "person".to_string()));
    }

    // 2) 关联目标：标题查重 → 不存在建 topic 页
    let mut target_entries: Vec<(String, String, String)> = Vec::new(); // (target, slug, kind)
    for r in &parsed.relations {
        let target = r.target.trim().to_string();
        if target.is_empty() || target_entries.iter().any(|(t, _, _)| *t == target) {
            continue;
        }
        let target_matches = store.find_wiki_pages_by_title_or_alias(&target)?;
        let selected_target_slug = parsed
            .relations
            .iter()
            .find(|relation| relation.target.trim() == target)
            .and_then(|relation| relation.to_slug.clone());
        if target_matches.len() > 1 && selected_target_slug.is_none() {
            anyhow::bail!(
                "事项「{target}」存在多个同名页面（{}），请先在知识库中消歧后再确认",
                target_matches
                    .iter()
                    .map(|page| page.slug.as_str())
                    .collect::<Vec<_>>()
                    .join("、")
            );
        }
        let (slug, kind) = match selected_target_slug
            .as_deref()
            .and_then(|slug| target_matches.iter().find(|page| page.slug == slug))
            .or_else(|| target_matches.first())
            .cloned()
        {
            Some(page) => (page.slug, page.kind),
            None => {
                let slug = unique_slug(store, &format!("topic/{}", slugify(&target)), &target)?;
                let draft = WikiPageDraft {
                    slug: slug.clone(),
                    kind: "topic".to_string(),
                    title: target.clone(),
                    summary: format!("{target}（由人物关系确认时自动建档）"),
                    content_md: format!("# {target}\n\n（由人物关系确认时自动创建，待补充内容。）"),
                    tags: vec![],
                    source_event_ids: vec![],
                    status: "active".to_string(),
                    reason: "AI 人物关系确认时自动建档".to_string(),
                    source_url: None,
                };
                store.upsert_wiki_page(&draft, ContentPolicy::PreserveHumanEdits)?;
                (slug, "topic".to_string())
            }
        };
        target_entries.push((target, slug, kind));
    }

    // 3) 写关系
    let mut saved = 0usize;
    let mut relation_lines: Vec<String> = Vec::new();
    for r in &parsed.relations {
        let person = r.person.trim().to_string();
        let target = r.target.trim().to_string();
        let Some((_, inferred_from_slug, inferred_from_kind)) =
            person_entries.iter().find(|(n, _, _)| *n == person)
        else {
            continue;
        };
        let Some((_, inferred_to_slug, inferred_to_kind)) =
            target_entries.iter().find(|(t, _, _)| *t == target)
        else {
            continue;
        };
        let from_slug = r
            .from_slug
            .as_deref()
            .unwrap_or(inferred_from_slug)
            .to_string();
        let from_kind = inferred_from_kind.clone();
        let to_slug = r.to_slug.as_deref().unwrap_or(inferred_to_slug).to_string();
        let to_kind = inferred_to_kind.clone();
        let relation = if r.relation.trim().is_empty() {
            "参与".to_string()
        } else {
            r.relation.trim().to_string()
        };
        let note = if r.note.trim().is_empty() {
            None
        } else {
            Some(r.note.trim().to_string())
        };
        store.upsert_relation(&RelationDraft {
            from_slug: from_slug.clone(),
            from_kind: from_kind.clone(),
            to_slug: to_slug.clone(),
            to_kind: to_kind.clone(),
            relation: relation.clone(),
            note,
            confidence: 3,
            source_conversation_id: Some(conversation_id.to_string()),
            source_event_id: parsed.source_event_id.clone(),
        })?;
        if let Some(event_id) = parsed.source_event_id.as_deref() {
            let occurred_at = store
                .event_analysis_detail(event_id)?
                .map(|detail| detail.recorded_at)
                .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
            let fact = format!("与{target}的关系：{relation}");
            store.upsert_entity_fact(&from_kind, &from_slug, &fact, &occurred_at, 3, event_id)?;
            let target_fact = format!("{person}：{relation}");
            store.upsert_entity_fact(
                &to_kind,
                &to_slug,
                &target_fact,
                &occurred_at,
                3,
                event_id,
            )?;
        }
        saved += 1;
        relation_lines.push(format!("{person} —— {relation} —— {target}"));
    }

    let mut out = String::new();
    if !person_entries.is_empty() {
        let names = person_entries
            .iter()
            .map(|(n, _, _)| n.clone())
            .collect::<Vec<_>>()
            .join("、");
        out.push_str(&format!("人物 {}：{names}\n", person_entries.len()));
    }
    if saved > 0 {
        out.push_str(&format!(
            "关系 {} 条：\n{}",
            saved,
            relation_lines.join("\n")
        ));
    } else {
        out.push_str("（没有落地的关系）");
    }
    Ok(out)
}

// ── 批量提取：历史事件 → 人物/关系草拟（@/# 标注为权威，AI 补全） ──────────

/// 单次批量提取最多喂给 LLM 的事件条数（提示词体积可控）
const BATCH_EXTRACT_MAX_EVENTS: usize = 120;
/// 单条事件进入提示词的最大字符数
const BATCH_EXTRACT_EVENT_CHARS: usize = 220;

/// 批量提取：扫描事件库，`@人名` / `#事情` 标注视为权威实体，再让 LLM 根据事件上下文
/// 补全人物 role_note 与「人物 ↔ 事情/项目」关系。
/// 只产草拟、不落库，返回可直接进入待确认动作的：
/// `{ "people": [{name,role_note}], "relations": [{person,target,relation,note}] }`
/// （落库仍走 execute_pending_action → apply_people_relations，用户确认后才写）。
pub fn propose_people_relations_from_events(store: &Store) -> Result<serde_json::Value> {
    use crate::event::{parse_annotations_many, AnnotationSet};

    let events = store.list_events()?;
    if events.is_empty() {
        anyhow::bail!("事件库为空，没有可提取的内容");
    }

    // @/# 标注：用户在事件里明确写死的实体，权威且必须全部纳入
    let annotations: AnnotationSet =
        parse_annotations_many(events.iter().map(|e| e.raw_text.as_str()));

    let total = events.len();
    let sampled: Vec<&EventSummary> = events.iter().take(BATCH_EXTRACT_MAX_EVENTS).collect();
    let mut event_lines = String::new();
    for (idx, e) in sampled.iter().enumerate() {
        let text: String = e.raw_text.chars().take(BATCH_EXTRACT_EVENT_CHARS).collect();
        event_lines.push_str(&format!("{}| {}\n", idx + 1, text.trim()));
    }
    let mut annotated = String::new();
    if !annotations.people.is_empty() {
        annotated.push_str(&format!("人物标注：{}\n", annotations.people.join("、")));
    }
    if !annotations.targets.is_empty() {
        annotated.push_str(&format!(
            "事情/项目标注：{}",
            annotations.targets.join("、")
        ));
    }

    let system = r#"你是 elsewhen 个人知识库的「人物关系」批量提取器。任务：阅读用户的事件，输出「人物 + 人物↔事情/项目关系」。

抽取规则：
- 事件里用 @人名 标注的一定是人、#事情 标注的一定是事情/项目（如「@张伟 负责 #双链路付款」）。已标注实体必须全部纳入 people / relations；同名但备注不同（「张伟（市场部）」「张伟（设计）」）是不同的人，不能合并。
- 未标注的人物：只提取信息具体、对用户重要的人（有称呼、有身份或参与的明确事情），不为随口一提的名字建条目。
- relations 的 person/target 必须来自 people 或已标注实体；relation 用 负责/参与/合作/对接/跟进/顾问 等 2~4 字动词。
- role_note 一句话身份/背景，来自事件上下文；没有可写信息就留空字符串。
- 宁缺毋滥：没有把握的关系不要编造。

严格输出 JSON（不要代码块围栏、不要其他任何文字）：
{"people":[{"name":"姓名","role_note":"身份备注"}],"relations":[{"person":"姓名","target":"事情/项目","relation":"负责","note":"补充说明"}]}"#;
    let user = format!(
        "共 {total} 条事件（展示最近 {} 条）：\n\n{event_lines}\n{annotated}",
        sampled.len()
    );

    let reply = call_provider(store, system, &user, 4000)?;
    let parsed = parse_json_value(&reply).map_err(|_| {
        anyhow::anyhow!(
            "批量提取失败：AI 返回无法解析——{}…",
            reply.chars().take(200).collect::<String>()
        )
    })?;
    Ok(normalize_extraction(&parsed, &annotations))
}

/// 把 LLM 返回的提取结果归一化：
/// - 确保所有 @/# 标注实体都出现在 people（LLM 漏掉也兜底补上，role_note 留空）；
/// - relations 里的 person/target 若不在实体集里则跳过（LLM 幻觉过滤）。
pub fn normalize_extraction(parsed: &serde_json::Value, ann: &AnnotationSet) -> serde_json::Value {
    let mut people: Vec<serde_json::Value> = parsed
        .get("people")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    for p in &ann.people {
        let known = people.iter().any(|v| {
            v.get("name")
                .and_then(|n| n.as_str())
                .map(|s| s.trim() == p.as_str())
                .unwrap_or(false)
        });
        if !known {
            people.push(serde_json::json!({ "name": p, "role_note": "" }));
        }
    }
    let mut relations: Vec<serde_json::Value> = parsed
        .get("relations")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    relations.retain(|r| {
        let person = r
            .get("person")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .unwrap_or("");
        let target = r
            .get("target")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .unwrap_or("");
        !person.is_empty()
            && !target.is_empty()
            && people.iter().any(|v| {
                v.get("name")
                    .and_then(|n| n.as_str())
                    .map(|s| s.trim() == person)
                    .unwrap_or(false)
            })
    });
    serde_json::json!({ "people": people, "relations": relations })
}

/// 宽容解析 LLM 返回的 JSON 对象：容忍 ``` 围栏与前后杂质文字。
fn parse_json_value(reply: &str) -> Result<serde_json::Value> {
    let trimmed = reply.trim();
    let inner = if trimmed.starts_with("```") {
        trimmed
            .lines()
            .filter(|l| !l.starts_with("```"))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string()
    } else {
        trimmed.to_string()
    };
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&inner) {
        return Ok(v);
    }
    if let Some(start) = inner.find('{') {
        if let Some(end) = inner.rfind('}') {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&inner[start..=end]) {
                return Ok(v);
            }
        }
    }
    anyhow::bail!("无法解析 AI 返回的 JSON")
}

const DIGEST_SYSTEM_PROMPT: &str = r#"你是 elsewhen 个人知识库的 wiki 维护者。任务：读新事件，把它们承载的"持久事实"提炼并写进 wiki 页面。

页面 kind 枚举（必须严格使用其一）：
- profile：关于用户身份、背景、状态的基本事实
- person：对话/事件中出现的重要人物（姓名、身份、TA 参与或负责的事情/项目），一人一页，跨事件合并，不重复建档
- recurring_cost：反复出现的固定支出或反复动作（通勤、固定费用、例行事务）
- capability：用户掌握的技能/能力（含正在学习的）
- asset：用户拥有但可能闲置/未充分利用的资产（设备、空间、时间块、关系）
- project：用户参与的项目
- relationship：重要人际关系/合作
- decision：重要决策及理由（尤其涉及钱、时间、取舍的）
- habit：规律习惯
- constraint：约束条件
- topic：其他值得长期记住的主题

写页规则：
- 只提炼事件里【有依据】的事实，绝不编造；不确定就在内容里标注（待确认）。
- slug：小写 kebab-case，格式 <kind 去下划线>/<简短名>，例如 recurring-cost/dongguan-huizhou-commute。
- 已有页面需要更新时 op=update，给出【全文新内容】（基于旧内容增量修补，不丢失旧事实）。
- 若事件没有产生任何新事实，返回空数组。
- 宁缺毋滥：一条事件通常 0~1 个页面变更，最多 2 个。"#;

/// LLM 返回的页面变更提议
#[derive(Debug, Clone, Deserialize)]
pub struct DigestProposal {
    #[serde(default)]
    pub op: String, // "create" | "update"（缺失则按 slug 是否已存在推断）
    pub kind: String,
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub summary: String,
    pub content: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub source_event_ids: Vec<String>, // 事件编号（用户消息里的 [N]）
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Default)]
pub struct DigestResult {
    pub created: Vec<String>,
    pub updated: Vec<String>,
    pub skipped: Vec<String>,
}

pub struct DigestOptions {
    pub days: i64,
    pub max_events: usize,
    pub dry_run: bool,
    pub force: bool, // 忽略上次 digest 游标
}

impl Default for DigestOptions {
    fn default() -> Self {
        Self {
            days: 7,
            max_events: 80,
            dry_run: false,
            force: false,
        }
    }
}

fn build_digest_user_prompt(
    events: &[EventRecord],
    store: &Store,
    max_chars: usize,
) -> Result<String> {
    let mut out = String::from("这是等待消化的新事件（[编号] 时间 | 内容）：\n");
    for (i, e) in events.iter().enumerate() {
        out.push_str(&format!("[{}] {} | {}\n", i + 1, e.recorded_at, e.raw_text));
    }

    let pages = store.list_wiki_pages(None, None)?;
    let index = build_index_md(&pages);
    out.push_str("\n当前 wiki 索引：\n");
    out.push_str(&index);

    let context = select_context_pages(pages, WIKI_KINDS, max_chars);
    if !context.is_empty() {
        out.push_str("\n相关现有页面全文（合并/更新时参考）：\n");
        for p in &context {
            let cost = p.content_md.len().min(4000);
            out.push_str(&format!(
                "\n--- {} ---\n{}",
                p.slug,
                p.content_md.chars().take(cost).collect::<String>()
            ));
        }
    }

    out.push_str(
        "\n\n输出 JSON 数组，不要输出任何其他文字、不要代码块围栏：\n\
         [{\"op\":\"create|update\",\"kind\":\"...\",\"slug\":\"...\",\"title\":\"...\",\n\
         \"summary\":\"一行摘要\",\"content\":\"markdown 正文，用 - 要点\",\n\
         \"tags\":[\"...\"],\"source_event_ids\":[\"编号\"],\"reason\":\"为什么建/改\"}]",
    );
    Ok(out)
}

fn parse_proposals(reply: &str) -> Result<Vec<DigestProposal>> {
    let trimmed = reply.trim();
    let inner = if trimmed.starts_with("```") {
        trimmed
            .lines()
            .filter(|l| !l.starts_with("```"))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string()
    } else {
        trimmed.to_string()
    };
    if let Ok(list) = serde_json::from_str::<Vec<DigestProposal>>(&inner) {
        return Ok(list);
    }
    if let Some(start) = inner.find('[') {
        if let Some(end) = inner.rfind(']') {
            if let Ok(list) = serde_json::from_str::<Vec<DigestProposal>>(&inner[start..=end]) {
                return Ok(list);
            }
        }
    }
    anyhow::bail!(
        "无法解析 digest 返回的 JSON。返回内容前 200 字：{}",
        &reply.chars().take(200).collect::<String>()
    )
}

/// 执行一次 digest：把最近事件消化成 wiki 页面变更（LLM 提议 + 核心确定性合并）。
pub fn generate_digest(store: &Store, opts: &DigestOptions) -> Result<DigestResult> {
    let days = opts.days;
    let events = store.recent_event_records(days, opts.max_events)?;

    // 只处理上次 digest 之后的事件（用游标；force 或没有游标时全量）
    let cursor = store.get_meta("last_digest_at")?;
    let fresh: Vec<EventRecord> = if opts.force || cursor.is_none() {
        events
    } else {
        let cursor = cursor.unwrap();
        events
            .into_iter()
            .filter(|e| e.recorded_at.as_str() > cursor.as_str())
            .collect()
    };

    if fresh.is_empty() {
        return Ok(DigestResult {
            created: vec!["__no_new_events__".to_string()],
            updated: Vec::new(),
            skipped: Vec::new(),
        });
    }

    let user = build_digest_user_prompt(&fresh, store, 8000)?;
    let reply = call_provider(store, DIGEST_SYSTEM_PROMPT, &user, 3000)?;
    if std::env::var("ELSEWHEN_DEBUG").is_ok() {
        eprintln!("[debug] digest raw reply:\n{}", reply);
    }
    let proposals = parse_proposals(&reply)?;

    let mut result = DigestResult::default();
    if opts.dry_run {
        // 预览模式：只展示提议，不写库
        for p in &proposals {
            let label = match validate_kind(&p.kind) && validate_slug(&p.slug) {
                true => "OK",
                false => "SKIP(非法 kind/slug)",
            };
            result
                .skipped
                .push(format!("[{}] {} {} {}", label, p.op, p.slug, p.title));
        }
        return Ok(result);
    }

    for p in &proposals {
        if !validate_kind(&p.kind) || !validate_slug(&p.slug) || p.content.trim().is_empty() {
            result
                .skipped
                .push(format!("非法提议: kind={} slug={}", p.kind, p.slug));
            continue;
        }
        // 事件编号 → 真实事件 id（溯源）
        let ids: Vec<String> = p
            .source_event_ids
            .iter()
            .filter_map(|idx| {
                idx.trim()
                    .parse::<usize>()
                    .ok()
                    .and_then(|i| i.checked_sub(1))
            })
            .filter_map(|i| fresh.get(i))
            .map(|e| e.id.clone())
            .collect();

        let draft = WikiPageDraft {
            slug: p.slug.clone(),
            kind: p.kind.clone(),
            title: p.title.clone(),
            summary: p.summary.clone(),
            content_md: p.content.clone(),
            tags: p.tags.clone(),
            source_event_ids: ids.clone(),
            status: "active".to_string(),
            reason: p
                .reason
                .clone()
                .unwrap_or_else(|| "digest 消化新事件".to_string()),
            source_url: None,
        };
        let outcome = store.upsert_wiki_page(&draft, ContentPolicy::PreserveHumanEdits)?;
        if outcome.protected {
            result
                .skipped
                .push(format!("human-edited, 仅累加证据: {}", p.slug));
        } else if outcome.created {
            result.created.push(p.slug.clone());
        } else {
            result.updated.push(p.slug.clone());
        }
    }

    if !result.created.is_empty() || !result.updated.is_empty() {
        let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let created = result.created.join(", ");
        let updated = result.updated.join(", ");
        let mut entry = format!(
            "## [{}] digest | created: {}; updated: {}",
            date, created, updated
        );
        if updated.is_empty() {
            entry = format!("## [{}] digest | created: {}", date, created);
        }
        store.append_wiki_log(&entry)?;
    }
    store.set_meta("last_digest_at", &chrono::Utc::now().to_rfc3339())?;

    Ok(result)
}

// ── export：物化 markdown 树（可接 Obsidian）──────────────────────────────

pub struct ExportReport {
    pub files: Vec<String>,
    pub dir: String,
}

/// 把 wiki 物化为 markdown 目录：index.md + log.md + <kind>/<slug>.md
pub fn export_wiki(store: &Store, dir: &Path) -> Result<ExportReport> {
    std::fs::create_dir_all(dir).with_context(|| format!("创建导出目录 {}", dir.display()))?;

    let pages = store.list_wiki_pages(None, None)?;
    let mut files = Vec::new();

    let index = build_index_md(&pages);
    let index_path = dir.join("index.md");
    std::fs::write(&index_path, index)?;
    files.push(index_path.display().to_string());

    let log_path = dir.join("log.md");
    let mut log_content = String::from("# Wiki Log\n");
    let log = store.list_wiki_log(200)?;
    for (ts, entry) in log.iter().rev() {
        log_content.push_str(&format!("<!-- {} -->\n{}\n", ts, entry));
    }
    std::fs::write(&log_path, log_content)?;
    files.push(log_path.display().to_string());

    for page in pages {
        let sub = dir.join(kind_dir(&page.kind));
        std::fs::create_dir_all(&sub)?;
        // 完整 slug 转文件名（`/` → `-`）：slug 前缀不必等于 kind，
        // 只取最后一段会让 `person/a` 与 `misc/a` 落到同名文件互相覆盖。
        let file_name = page.slug.replace('/', "-");
        let path = sub.join(format!("{}.md", file_name));
        let sources: Vec<String> = page
            .source_event_ids
            .iter()
            .map(|s| format!("  - \"{}\"", s))
            .collect();
        let tags: Vec<String> = page.tags.iter().map(|t| format!("  - \"{}\"", t)).collect();
        let content = format!(
            "---\nkind: {}\nslug: {}\nevidence_count: {}\nstatus: {}\nupdated_at: {}\ntags:\n{}\nsources:\n{}\n---\n\n# {}\n\n{}",
            page.kind,
            page.slug,
            page.evidence_count,
            page.status,
            page.updated_at,
            if tags.is_empty() { "  - []".to_string() } else { tags.join("\n") },
            if sources.is_empty() { "  - []".to_string() } else { sources.join("\n") },
            page.title,
            page.content_md
        );
        std::fs::write(&path, content)?;
        files.push(path.display().to_string());
    }

    Ok(ExportReport {
        files,
        dir: dir.display().to_string(),
    })
}

// ── lint：确定性健康检查 ──────────────────────────────────────────────────

/// 确定性检查：孤儿页、无溯源页。返回问题描述列表。
/// insight 页是综合产物（依据是 wiki 页而非事件），天然无事件溯源，跳过。
pub fn lint_wiki(store: &Store) -> Result<Vec<String>> {
    let pages = store.list_wiki_pages(None, None)?;
    let mut issues = Vec::new();

    for p in &pages {
        if p.status == "archived" || p.kind == "insight" {
            continue;
        }
        if p.source_event_ids.is_empty() {
            issues.push(format!("无事件溯源: {} `{}`", p.kind, p.slug));
        }
        let referenced = pages
            .iter()
            .filter(|o| o.id != p.id)
            .any(|o| o.content_md.contains(&p.slug));
        if !referenced {
            issues.push(format!("无入链页面（orphan）: {}", p.slug));
        }
    }
    Ok(issues)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_validation() {
        assert!(validate_slug("recurring-cost/dongguan-huizhou-commute"));
        assert!(validate_slug("habit/morning-run"));
        assert!(validate_slug("recurring-cost/东莞惠州通勤")); // 中文 slug 允许
        assert!(!validate_slug("Recurring Cost/xx")); // 空格
        assert!(!validate_slug("a//b")); // 多层
        assert!(!validate_slug("-lead"));
        assert!(!validate_slug("a-b-"));
    }

    #[test]
    fn slugify_works() {
        assert_eq!(slugify("副业不是加法是乘法"), "副业不是加法是乘法");
        assert_eq!(
            slugify("Dongguan-Huizhou Commute"),
            "dongguan-huizhou-commute"
        );
        assert_eq!(slugify("  顺风车 2.0  计划  "), "顺风车-2-0-计划");
        assert_eq!(slugify("您/好 世界"), "您-好-世界");
        assert_eq!(slugify("!!!#"), "page"); // 全符号 → fallback
    }

    #[test]
    fn normalize_extraction_backfills_annotations_and_filters_hallucinations() {
        use crate::event::AnnotationSet;

        let parsed = serde_json::json!({
            "people": [
                {"name": "张伟", "role_note": "对接付款流程"},
                {"name": "项目负责人"}
            ],
            "relations": [
                {"person": "张伟", "target": "双链路付款", "relation": "负责", "note": ""},
                {"person": "不存在的人", "target": "幻想的项目", "relation": "负责", "note": ""}
            ]
        });
        let ann = AnnotationSet {
            people: vec!["张伟".into(), "李婷（客户）".into()],
            targets: vec!["双链路付款".into()],
        };
        let out = normalize_extraction(&parsed, &ann);
        let people = out["people"].as_array().unwrap();
        let names: Vec<&str> = people.iter().map(|p| p["name"].as_str().unwrap()).collect();
        // LLM 漏掉的标注实体兜底补进 people
        assert!(names.contains(&"李婷（客户）"));
        assert_eq!(people[people.len() - 1]["role_note"].as_str().unwrap(), "");
        // 幻觉关系被过滤（person 不在实体集里）
        let relations = out["relations"].as_array().unwrap();
        assert_eq!(relations.len(), 1);
        assert_eq!(relations[0]["person"], "张伟");
        assert_eq!(relations[0]["target"], "双链路付款");
    }

    #[test]
    fn unique_slug_avoids_different_title_collision_but_reuses_same_title() {
        let path = std::env::temp_dir().join(format!(
            "elsewhen-wiki-unique-slug-{}.db",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = Store::open(&path).unwrap();
        // 先用不同标题占住 person/zhang-wei 这个 slug
        store
            .upsert_wiki_page(
                &WikiPageDraft {
                    slug: "person/zhang-wei".into(),
                    kind: "person".into(),
                    title: "Zhang Wei".into(),
                    summary: String::new(),
                    content_md: String::new(),
                    tags: vec![],
                    source_event_ids: vec![],
                    status: "active".into(),
                    reason: "test".into(),
                    source_url: None,
                },
                ContentPolicy::Always,
            )
            .unwrap();
        // 不同标题 → 后缀避让
        assert_eq!(
            unique_slug(&store, "person/zhang-wei", "张伟").unwrap(),
            "person/zhang-wei-2"
        );
        // 同名 slug 但标题相同 → 直接复用
        assert_eq!(
            unique_slug(&store, "person/zhang-wei", "Zhang Wei").unwrap(),
            "person/zhang-wei"
        );
        // 空闲 slug 直接用
        assert_eq!(
            unique_slug(&store, "topic/shuang-lian-lu", "双链路").unwrap(),
            "topic/shuang-lian-lu"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn index_md_groups_by_kind() {
        let pages = vec![
            WikiPage {
                id: "1".into(),
                slug: "recurring-cost/dg-huizhou".into(),
                kind: "recurring_cost".into(),
                title: "往返".into(),
                summary: "每周通勤".into(),
                content_md: "x".into(),
                tags: vec![],
                source_event_ids: vec!["a".into(), "b".into(), "c".into()],
                evidence_count: 3,
                first_seen_at: String::new(),
                last_seen_at: String::new(),
                status: "active".into(),
                created_at: String::new(),
                updated_at: String::new(),
                source_url: None,
                area: "insight".into(),
                based_on: None,
                content_type: None,
                human_edited_at: None,
                opinion: None,
            },
            WikiPage {
                id: "2".into(),
                slug: "capability/rust".into(),
                kind: "capability".into(),
                title: "Rust".into(),
                summary: "系统编程".into(),
                content_md: "y".into(),
                tags: vec![],
                source_event_ids: vec![],
                evidence_count: 0,
                first_seen_at: String::new(),
                last_seen_at: String::new(),
                status: "active".into(),
                created_at: String::new(),
                updated_at: String::new(),
                source_url: None,
                area: "insight".into(),
                based_on: None,
                content_type: None,
                human_edited_at: None,
                opinion: None,
            },
        ];
        let md = build_index_md(&pages);
        assert!(md.contains("### recurring_cost"));
        assert!(md.contains("3 条事件支持"));
        assert!(md.contains("### capability"));
    }

    #[test]
    fn update_wiki_tags_normalizes_and_persists() {
        let path = std::env::temp_dir().join(format!(
            "elsewhen-wiki-tags-test-{}.db",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = Store::open(&path).unwrap();
        let draft = WikiPageDraft {
            slug: "topic/tag-test".into(),
            kind: "topic".into(),
            title: "标签测试".into(),
            summary: "s".into(),
            content_md: "正文".into(),
            tags: vec!["旧".into()],
            source_event_ids: vec![],
            status: "active".into(),
            reason: "t".into(),
            source_url: None,
        };
        store
            .upsert_wiki_page(&draft, ContentPolicy::Always)
            .unwrap();

        // 去 #、去空白、去重、忽略空串，保持顺序
        let updated = store
            .update_wiki_tags(
                "topic/tag-test",
                &[" #工作 ".into(), "工作".into(), "  ".into(), "Rust".into()],
            )
            .unwrap();
        assert_eq!(updated.tags, vec!["工作".to_string(), "Rust".to_string()]);
        // 重新读取应持久化
        let reread = store.get_wiki_page("topic/tag-test").unwrap().unwrap();
        assert_eq!(reread.tags, vec!["工作".to_string(), "Rust".to_string()]);

        // 清空标签
        let cleared = store.update_wiki_tags("topic/tag-test", &[]).unwrap();
        assert!(cleared.tags.is_empty());

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn save_text_page_keeps_tags_and_note_anchor() {
        let path = std::env::temp_dir().join(format!(
            "elsewhen-wiki-text-tags-test-{}.db",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = Store::open(&path).unwrap();
        let tags = vec![" #工作 ".to_string(), "Rust".to_string(), "".to_string()];
        let page =
            save_text_page("这是一段要保存的笔记正文", Some("我的笔记"), &tags, &store).unwrap();
        assert_eq!(
            page.kind, "note",
            "用户粘贴笔记归素材档 kind=note（M1 语义拆分）"
        );
        assert_eq!(page.title, "我的笔记");
        assert!(page.tags.contains(&"工作".to_string()), "{:?}", page.tags);
        assert!(page.tags.contains(&"Rust".to_string()), "{:?}", page.tags);
        assert!(
            page.tags.contains(&"note".to_string()),
            "应保留 note 锚点标签: {:?}",
            page.tags
        );
        assert!(
            !page.tags.iter().any(|t| t.is_empty()),
            "不应有空标签: {:?}",
            page.tags
        );
        // 空文本拒绝
        assert!(save_text_page("   ", Some("x"), &[], &store).is_err());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn apply_people_relations_builds_pages_and_relations() {
        let path = std::env::temp_dir().join(format!(
            "elsewhen-wiki-relations-test-{}.db",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = Store::open(&path).unwrap();

        // 预置一个已存在的目标页（模拟 digest 已建档的项目）
        store
            .upsert_wiki_page(
                &WikiPageDraft {
                    slug: "project/shuanglian".into(),
                    kind: "project".into(),
                    title: "双链路付款".into(),
                    summary: "项目简介".into(),
                    content_md: "正文".into(),
                    tags: vec![],
                    source_event_ids: vec!["evt-1".into()],
                    status: "active".into(),
                    reason: "digest".into(),
                    source_url: None,
                },
                ContentPolicy::Always,
            )
            .unwrap();

        let args = serde_json::json!({
            "people": [
                {"name": "张玮", "role_note": "双链路付款项目的产研负责人"},
                {"name": "李婷", "role_note": "客户对接人"}
            ],
            "relations": [
                {"person": "张玮", "target": "双链路付款", "relation": "负责", "note": "主导项目推进"},
                {"person": "李婷", "target": "双链路付款", "relation": "参与", "note": ""}
            ]
        });
        let summary = apply_people_relations(&args, &store, "conv-1").unwrap();
        assert!(summary.contains("张玮"), "{summary}");
        assert!(summary.contains("负责"), "{summary}");

        // 人物页建档且分别是 person / topic 目标复用已有 project 页
        let zhangwei = store.get_wiki_page("person/张玮").unwrap().unwrap();
        assert_eq!(zhangwei.kind, "person");
        assert!(zhangwei.content_md.contains("产研负责人"));
        let liting = store.get_wiki_page("person/李婷").unwrap().unwrap();
        assert_eq!(liting.kind, "person");

        // 目标页：已存在的 project 页被复用，没有自动建 topic 页
        assert!(store.get_wiki_page("topic/双链路付款").unwrap().is_none());
        assert_eq!(
            store
                .find_wiki_page_by_title("双链路付款")
                .unwrap()
                .unwrap()
                .kind,
            "project"
        );

        // 关系双向可见
        let rels = store.list_relations_for_page("person/张玮").unwrap();
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0].relation, "负责");
        assert_eq!(rels[0].to_slug, "project/shuanglian");
        assert_eq!(
            store
                .list_relations_for_page("project/shuanglian")
                .unwrap()
                .len(),
            2
        );

        // 再次提交同样的人物 → 不重复建档
        let again = apply_people_relations(
            &serde_json::json!({"people": [{"name": "张玮", "role_note": "产研负责人"}]}),
            &store,
            "conv-1",
        )
        .unwrap();
        assert!(again.contains("张玮"));
        assert_eq!(
            store.list_wiki_pages(Some("person"), None).unwrap().len(),
            2,
            "不应重复建档"
        );

        // 空草拟拒绝
        assert!(apply_people_relations(&serde_json::json!({}), &store, "conv-1").is_err());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn apply_people_relations_uses_explicit_disambiguation_slugs() {
        let path = std::env::temp_dir().join(format!(
            "elsewhen-wiki-disambiguation-{}.db",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = Store::open(&path).unwrap();
        for slug in ["person/张伟-市场部", "person/张伟-设计"] {
            store
                .upsert_wiki_page(
                    &WikiPageDraft {
                        slug: slug.into(),
                        kind: "person".into(),
                        title: "张伟".into(),
                        summary: slug.into(),
                        content_md: "正文".into(),
                        tags: vec![],
                        source_event_ids: vec![],
                        status: "active".into(),
                        reason: "test".into(),
                        source_url: None,
                    },
                    ContentPolicy::Always,
                )
                .unwrap();
        }
        let args = serde_json::json!({
            "people": [{"name":"张伟"}],
            "relations": [{"person":"张伟","target":"新项目","relation":"负责","from_slug":"person/张伟-设计"}]
        });
        let summary = apply_people_relations(&args, &store, "conv-1").unwrap();
        assert!(summary.contains("张伟"));
        assert_eq!(
            store
                .list_relations_for_page("person/张伟-设计")
                .unwrap()
                .len(),
            1
        );
        assert!(store
            .list_relations_for_page("person/张伟-市场部")
            .unwrap()
            .is_empty());
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn proposals_parse_plain_and_fenced() {
        let reply = r#"[{"op":"create","kind":"recurring_cost","slug":"recurring-cost/test","title":"t","summary":"s","content":"- a","tags":["x"],"source_event_ids":["1"],"reason":"r"}]"#;
        let list = parse_proposals(reply).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].slug, "recurring-cost/test");

        let fenced = format!("```json\n{}\n```", reply);
        assert_eq!(parse_proposals(&fenced).unwrap().len(), 1);
    }

    #[test]
    fn garbage_errors() {
        assert!(parse_proposals("不是 JSON").is_err());
    }

    #[test]
    fn extract_tweet_id_parses_common_urls() {        assert_eq!(
            extract_tweet_id("https://x.com/someone/status/123456789").as_deref(),
            Some("123456789")
        );
        assert_eq!(
            extract_tweet_id("https://twitter.com/someone/status/123456789?s=20").as_deref(),
            Some("123456789")
        );
        assert_eq!(
            extract_tweet_id("https://x.com/i/web/status/123456789/").as_deref(),
            Some("123456789")
        );
        assert_eq!(
            extract_tweet_id("https://mobile.twitter.com/someone/status/123456789#x").as_deref(),
            Some("123456789")
        );
        // 非推文链接 / 空输入 / 非数字 id
        assert_eq!(extract_tweet_id("https://example.com/not-a-tweet"), None);
        assert_eq!(extract_tweet_id(""), None);
        assert_eq!(extract_tweet_id("https://x.com/someone/status/abc"), None);
    }

    #[test]
    fn file_url_roundtrips_local_paths() {
        // 普通路径原样往返
        let p = PathBuf::from("/home/pp/playground/ai/elsewhen");
        let url = path_to_file_url(&p);
        assert_eq!(url, "file:///home/pp/playground/ai/elsewhen");
        assert!(is_file_url(&url));
        assert_eq!(file_url_to_path(&url).as_deref(), Some(p.as_path()));
        // 空格与中文按字节编码后可还原
        let p2 = PathBuf::from("/tmp/我的 项目/a b");
        let url2 = path_to_file_url(&p2);
        assert!(is_file_url(&url2));
        assert!(!url2.contains(' '));
        assert_eq!(file_url_to_path(&url2).as_deref(), Some(p2.as_path()));
        // 非 file scheme 拒绝；远端 host 拒绝
        assert!(!is_file_url("https://example.com/x"));
        assert_eq!(file_url_to_path("https://example.com/x"), None);
        assert_eq!(file_url_to_path("file://other-host/tmp/x"), None);
        // localhost 宽容
        assert_eq!(
            file_url_to_path("file://localhost/tmp/x").as_deref(),
            Some(Path::new("/tmp/x"))
        );
    }

    #[test]
    fn article_tweet_parses_blocks_instead_of_url_text() {
        // text 只是文章链接；长文在 article.content.blocks
        let json = r#"{
            "code": 200,
            "tweet": {
                "text": "https://x.com/i/article/2099410385648717824",
                "author": {"name": "伟大", "screen_name": "Huouo908070"},
                "article": {
                    "title": "为什么你手握 Codex、Claude，依然赚不到钱？",
                    "content": {
                        "blocks": [
                            {"text": "过去，一个人赚不到钱，往往还能找到很多具体的理由。"},
                            {"text": "这些理由过去确实成立，因为技术本身就是门槛。"}
                        ]
                    }
                }
            }
        }"#;
        let parsed: FxTwitterResponse = serde_json::from_str(json).unwrap();
        let out = parse_tweet_content(parsed).unwrap();
        assert_eq!(
            out.title.as_deref(),
            Some("为什么你手握 Codex、Claude，依然赚不到钱？")
        );
        assert_eq!(out.text, "过去，一个人赚不到钱，往往还能找到很多具体的理由。\n\n这些理由过去确实成立，因为技术本身就是门槛。");
        assert_eq!(out.author_name.as_deref(), Some("伟大"));
        assert_eq!(out.screen_name.as_deref(), Some("Huouo908070"));
    }

    #[test]
    fn normal_tweet_parses_text_without_article() {
        let json = r#"{
            "code": 200,
            "tweet": {
                "text": "just setting up my twttr",
                "author": {"name": "jack", "screen_name": "jack"}
            }
        }"#;
        let parsed: FxTwitterResponse = serde_json::from_str(json).unwrap();
        let out = parse_tweet_content(parsed).unwrap();
        assert_eq!(out.text, "just setting up my twttr");
        assert_eq!(out.title, None);
    }

    #[test]
    fn article_without_blocks_falls_back_to_text() {
        let json = r#"{
            "code": 200,
            "tweet": {
                "text": "https://x.com/i/article/1",
                "article": {"title": "只有标题没正文"}
            }
        }"#;
        let parsed: FxTwitterResponse = serde_json::from_str(json).unwrap();
        let out = parse_tweet_content(parsed).unwrap();
        assert_eq!(out.text, "https://x.com/i/article/1");
        assert_eq!(out.title.as_deref(), Some("只有标题没正文"));
    }

    #[test]
    fn non_200_response_is_error() {
        let json = r#"{"code": 404, "message": "Tweet not found"}"#;
        let parsed: FxTwitterResponse = serde_json::from_str(json).unwrap();
        assert!(parse_tweet_content(parsed).is_err());
    }
}
