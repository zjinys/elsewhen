//! Wiki 页面 DTO：元信息、正文页、草稿与重命名结果、关系。

/// wiki_pages 的列表元数据视图：刻意**不含 content_md**。
/// 左侧栏/列表场景只需要元数据——全量正文随 FFI 传输 + 每页图片引用解析
/// （WikiPageDto::serve 的 N+1）是页数增长后的首要成本。明细一律按 slug
/// 经 wiki_pageProvider 单页读取。
#[derive(Debug, Clone)]
pub struct WikiPageMeta {
    pub id: String,
    pub slug: String,
    pub kind: String,
    pub title: String,
    pub summary: String,
    pub tags: Vec<String>,
    pub evidence_count: i64,
    pub first_seen_at: String,
    pub last_seen_at: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub source_url: Option<String>,
    pub area: String,
    /// 孵化定论（摘录闭环 2026-10-20）：NULL=孵化中（source/note 待定
    /// 论），endorse/reject=已定论。非 source/note kind 恒 NULL——侧栏
    /// 按钮只对素材/人工 kind 出现。
    pub opinion: Option<String>,
}

pub(crate) fn map_wiki_page_meta(row: &rusqlite::Row) -> rusqlite::Result<WikiPageMeta> {
    let tags_raw: String = row.get(5)?;
    Ok(WikiPageMeta {
        id: row.get(0)?,
        slug: row.get(1)?,
        kind: row.get(2)?,
        title: row.get(3)?,
        summary: row.get(4)?,
        tags: serde_json::from_str(&tags_raw).unwrap_or_default(),
        evidence_count: row.get(6)?,
        first_seen_at: row.get(7)?,
        last_seen_at: row.get(8)?,
        status: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
        source_url: row.get(12)?,
        area: row.get(13)?,
        opinion: row.get(14)?,
    })
}

/// wiki_pages 行 → WikiPageMeta 的公共列清单（无 content_md）。
/// 顺序必须与 `map_wiki_page_meta` 的按位取值（0..=13）严格一致。
pub(crate) const WIKI_PAGE_META_COLS: &str = "id, slug, kind, title, summary, tags, \
     evidence_count, first_seen_at, last_seen_at, status, created_at, updated_at, source_url, \
     COALESCE(area, 'insight'), opinion";

/// wiki 页面（LLM wiki 知识库的一页，markdown 正文，derived data）
#[derive(Debug, Clone)]
pub struct WikiPage {
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
    /// 来源 URL（URL 导入页记录出处）；None 表示匿名导入/本地生成
    pub source_url: Option<String>,
    /// 来源/用途分区：imported（素材库）/ network（人物项目）/ insight（知识沉淀）/ derivative（派生产物）
    pub area: String,
    /// 派生产物指向的原页面 slug（仅 area=derivative 有值）
    pub based_on: Option<String>,
    /// 派生产物的加工类型（总结/提炼观点/抖音文案…自由字符串，仅 area=derivative 有值）
    pub content_type: Option<String>,
    /// 最近一次人工编辑该页正文的时间；非空 ⇔ 该页由人工持有，digest 不再整篇覆盖
    pub human_edited_at: Option<String>,
    /// 素材页观点评价：Some("endorse")=认可 / Some("reject")=不认可 / None=未表态（缺省认可）
    pub opinion: Option<String>,
}

/// 一次写回（创建或更新）的输入草案
#[derive(Clone)]
pub struct WikiPageDraft {
    pub slug: String,
    pub kind: String,
    pub title: String,
    pub summary: String,
    pub content_md: String,
    pub tags: Vec<String>,
    pub source_event_ids: Vec<String>,
    pub status: String,
    pub reason: String,
    /// 来源 URL（URL 导入页记录出处）
    pub source_url: Option<String>,
}

/// 写回策略：digest 等 AI 覆盖路径对「人工持有页」的保护档位。
/// 保护只作用于内容列（content_md/title/summary/tags）；证据列（source_event_ids/
/// evidence_count）与时间戳永远由系统合并，不受任何策略影响。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentPolicy {
    /// 不保护：整篇覆盖内容列。仅用于「有显式授权」的写回——素材导入流程（素材页的
    /// 唯一合法写入方，可刷新采集快照）；AI 草拟 → 用户确认制（save_wiki_revision 修订、
    /// save_knowledge_draft 建档，用户确认即显式授权，区别于 digest 的静默覆盖）。
    Always,
    /// 保护人工持有页：目标行 human_edited_at 非空（人工编辑过的档案/派生页），
    /// 或 kind∈{source,note}（采集素材，对人和 AI 都只读正文）时，内容列不动、
    /// 只并集证据 + 刷新 last_seen_at。AI 静默生成路径（digest/洞察归档/关系建档）一律走这一档。
    PreserveHumanEdits,
}

/// upsert 结果
#[derive(Debug, Clone)]
pub struct WikiUpsertOutcome {
    pub created: bool,
    pub page: WikiPage,
    /// 本次是否因「人工持有」保护被降级：只累加证据，内容列未动（对应 DigestResult.skipped）
    pub protected: bool,
}

/// 重命名知识页的结果
#[derive(Debug, Clone)]
pub struct RenameWikiOutcome {
    pub old_slug: String,
    pub new_slug: String,
    pub old_title: String,
    pub new_title: String,
    /// 是否真正改名（标题没变时为 false，什么都不做）
    pub changed: bool,
    /// 随名迁移的关系条数（from_slug / to_slug 命中旧 slug 的）
    pub relations_moved: usize,
    /// 随名迁移的页内聊天会话数
    pub chats_moved: usize,
}

/// 一条人物关系：`from`（一般是人物页）↔ `to`（事情/项目页等），带关系类型
#[derive(Debug, Clone)]
pub struct Relation {
    pub id: String,
    pub from_slug: String,
    pub from_kind: String,
    pub to_slug: String,
    pub to_kind: String,
    pub relation: String,
    pub note: Option<String>,
    pub confidence: i64,
    pub created_at: String,
    pub last_seen_at: String,
}

/// 新建/更新一条人物关系的输入
pub struct RelationDraft {
    pub from_slug: String,
    pub from_kind: String,
    pub to_slug: String,
    pub to_kind: String,
    pub relation: String,
    pub note: Option<String>,
    pub confidence: i64,
    pub source_conversation_id: Option<String>,
    pub source_event_id: Option<String>,
}
