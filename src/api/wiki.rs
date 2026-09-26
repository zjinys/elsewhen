//! 知识库页面 FRB 门面。

use super::*;

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
    /// 来源/用途分区：imported（素材库）/ network（人物项目）/ insight（知识沉淀）/ derivative（派生产物）
    pub area: String,
    /// 派生产物指向的原页面 slug（仅 derivative 有值）
    pub based_on: Option<String>,
    /// 派生产物的加工类型（总结/提炼观点/抖音文案…，仅 derivative 有值）
    pub content_type: Option<String>,
    /// 最近一次人工编辑正文的时间（非空 ⇔ 该页由人工持有，digest 不整篇覆盖）
    pub human_edited_at: Option<String>,
    /// 素材页观点评价：Some("endorse")/Some("reject")/None=未表态（缺省认可）
    pub opinion: Option<String>,
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
            area: p.area,
            based_on: p.based_on,
            content_type: p.content_type,
            human_edited_at: p.human_edited_at,
            opinion: p.opinion,
        }
    }
}

/// List wiki pages（主列表）。kind/area 均为 None 时列出全部（不含派生产物）。
/// area：imported（素材库）/ network（人物项目）/ insight（知识沉淀）。
pub fn list_wiki_pages(kind: Option<String>, area: Option<String>) -> Result<Vec<WikiPageDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let pages = store.list_wiki_pages(kind.as_deref(), area.as_deref())?;
    Ok(pages.into_iter().map(WikiPageDto::from).collect())
}

/// 某页的派生产物列表（AI 加工成果，挂在该页详情下，不进主列表）。
pub fn list_wiki_page_derivatives(slug: String) -> Result<Vec<WikiPageDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let pages = store.list_derivatives(&slug)?;
    Ok(pages.into_iter().map(WikiPageDto::from).collect())
}

/// 直接把一段内容保存为某知识页的派生产物（页内 AI 聊天「保存」按钮用，跳过 AI 草拟确认）。
pub fn create_wiki_derivative(
    based_on_slug: String,
    content_type: String,
    title: String,
    content_md: String,
) -> Result<WikiPageDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let page = store.create_derivative(
        &based_on_slug,
        &content_type,
        &title,
        &content_md,
        "由页内 AI 对话直接保存为派生产物",
    )?;
    Ok(WikiPageDto::from(page))
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

/// 修改一张项目页关联的本地目录（目录搬家后在知识页纠正路径）。
/// 路径记进 `source_url`（file:// 规范形式）；新路径必须存在且是目录。返回更新后的页面。
pub fn update_project_path(slug: String, new_path: String) -> Result<WikiPageDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let page = store.update_project_path(&slug, &new_path)?;
    Ok(WikiPageDto::from(page))
}

/// 刷新一张目录导入的项目页：按 `source_url`（file://）重扫目录并整篇更新。
/// 目录不存在会直接报错（提示先改路径）。返回刷新后的页面。
pub fn refresh_project_page(slug: String) -> Result<WikiPageDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let page = crate::local_sources::refresh_project_page(&store, &slug)?;
    Ok(WikiPageDto::from(page))
}

/// 人类编辑保存一页正文（限可编辑 kind；素材页只读拒绝）。
/// 保存后 `human_edited_at` 置位：该页被 AI digest 视为人工持有，不再整篇覆盖。
/// `expected_updated_at`：乐观锁（§11 Q3）——传加载时的 updated_at（rfc3339），
/// 与当前不一致则报「编辑冲突」，拒绝静默覆盖编辑期间的后台写入；None 跳过校验。
pub fn save_wiki_page_content(
    slug: String,
    content_md: String,
    reason: String,
    expected_updated_at: Option<String>,
) -> Result<WikiPageDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let page = store.save_wiki_page_content(
        &slug,
        &content_md,
        &reason,
        expected_updated_at.as_deref(),
    )?;
    Ok(WikiPageDto::from(page))
}

/// 素材页观点评价（仅 source/note）。opinion：Some("endorse")=认可 / Some("reject")=不认可 /
/// None=清空回未表态（读取按缺省认可处理）。不改变正文、不置位人工编辑保护。
pub fn set_wiki_opinion(slug: String, opinion: Option<String>) -> Result<WikiPageDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let page = store.set_wiki_opinion(&slug, opinion.as_deref())?;
    Ok(WikiPageDto::from(page))
}
