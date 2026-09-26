//! 任意 URL 导入 FRB 门面。

use super::*;
use crate::storage::ContentPolicy;

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
            if source_kind == "tweet" {
                "tweet"
            } else {
                "import"
            },
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
    let outcome = store.upsert_wiki_page(&draft, ContentPolicy::Always)?;
    Ok(WikiPageDto::from(outcome.page))
}
