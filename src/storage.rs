mod adapter;
mod migrations;
mod provider;
mod entities;
mod wiki;
mod conversations;
mod records;
mod events;

use crate::event::{EventSummary, NewEvent};
use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use std::path::Path;

pub use adapter::{
    AiProviderConfig, AiProviderConfigRow, AnalysisJob, AnalysisSummary, StorageAdapter,
};

// Conversation and Message summary structs
#[derive(Debug, Clone)]
pub struct ConversationSummary {
    pub id: String,
    pub title: Option<String>,
    pub tag: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: i32,
    pub last_message_preview: Option<String>,
    pub archived: bool,
    /// 关联的知识页 slug（页内 AI 处理会话）；None 为普通对话
    pub wiki_page_slug: Option<String>,
    /// AI 对话角色：personal_secretary 或 knowledge_mentor
    pub assistant_mode: String,
}

#[derive(Debug, Clone)]
pub struct MessageSummary {
    pub id: String,
    pub conversation_id: String,
    pub parent_message_id: Option<String>,
    pub role: String,
    pub content: String,
    pub created_at: String,
}

/// 用首条用户消息自动生成对话标题：取第一行 → 折叠连续空白 → 截断到 24 字（超出加省略号）。
pub(crate) fn derive_conversation_title(content: &str) -> Option<String> {
    const MAX_CHARS: usize = 24;
    let first_line = content.lines().next().unwrap_or("").trim();
    if first_line.is_empty() {
        return None;
    }
    let mut cleaned = String::with_capacity(first_line.len());
    let mut prev_space = false;
    for ch in first_line.chars() {
        if ch.is_whitespace() {
            if !prev_space {
                cleaned.push(' ');
            }
            prev_space = true;
        } else {
            cleaned.push(ch);
            prev_space = false;
        }
    }
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        return None;
    }
    let mut chars: Vec<char> = cleaned.chars().collect();
    let truncated = chars.len() > MAX_CHARS;
    if truncated {
        chars.truncate(MAX_CHARS);
    }
    let mut title: String = chars.into_iter().collect();
    if truncated {
        title.push('…');
    }
    Some(title)
}

/// 个人经验规则的状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleStatus {
    /// 已生效，注入到 AI 回复的上下文中
    Active,
    /// AI 刚从对话中提议、等待用户确认
    Pending,
}

impl RuleStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            RuleStatus::Active => "active",
            RuleStatus::Pending => "pending",
        }
    }
}

/// 个人经验规则库的一条规则
#[derive(Debug, Clone)]
pub struct RuleSummary {
    pub id: String,
    pub content: String,
    pub status: RuleStatus,
    pub created_at: String,
}

/// 待办状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TodoStatus {
    Open,
    Done,
    Archived,
}

impl TodoStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TodoStatus::Open => "open",
            TodoStatus::Done => "done",
            TodoStatus::Archived => "archived",
        }
    }

    pub fn parse(s: &str) -> TodoStatus {
        match s {
            "done" => TodoStatus::Done,
            "archived" => TodoStatus::Archived,
            _ => TodoStatus::Open,
        }
    }
}

/// 一条个人待办（AI 提议确认后创建，或手动创建）
#[derive(Debug, Clone)]
pub struct Todo {
    pub id: String,
    pub title: String,
    pub status: TodoStatus,
    pub priority: String, // high / normal / low
    pub due_at: Option<String>,
    pub related_event_id: Option<String>,
    pub related_wiki_slug: Option<String>,
    pub note: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 一条最近的历史用户消息（跨对话，供 AI 回忆近期事件）
#[derive(Debug, Clone)]
pub struct RecentUserMessage {
    pub conversation_id: String,
    pub content: String,
}

/// 一条待确认动作（AI 写类工具草拟，用户确认后才执行）
#[derive(Debug, Clone)]
pub struct PendingAction {
    pub id: String,
    pub conversation_id: String,
    pub action: String,
    pub args_json: String,
    pub created_at: String,
}

/// 一条知识库/事件搜索命中
#[derive(Debug, Clone)]
pub struct KnowledgeHit {
    pub kind: String,
    pub title: String,
    pub snippet: String,
}

/// 按天聚合的 token 用量统计
#[derive(Debug, Clone)]
pub struct DailyTokenUsage {
    pub date: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub call_count: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AnalysisJobStats {
    pub pending: i64,
    pub running: i64,
    pub retry: i64,
    pub succeeded: i64,
    pub failed: i64,
}

/// 一条已生成的认知洞察（derived data）
#[derive(Debug, Clone)]
pub struct InsightSummary {
    pub id: String,
    pub created_at: String,
    pub window_days: i64,
    pub prompt_version: String,
    pub lens: String,
    pub title: String,
    pub observation: String,
    pub related_events: Vec<String>,
    pub action: Option<String>,
    pub status: String,
}

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

/// 带 id 的事件记录（digest 需要把事件 id 写进 wiki 页作为溯源）
#[derive(Debug, Clone)]
pub struct EventRecord {
    pub id: String,
    pub recorded_at: String,
    pub raw_text: String,
}

/// 一次用户原始提交及其路由结果。它只负责关联，不取代 event/message/wiki/todo
/// 各自的权威数据；raw_text 保存提交时的原文，后续路由不得覆盖。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputRecord {
    pub id: String,
    pub raw_text: String,
    pub source: String,
    pub route_status: String,
    pub idempotency_key: Option<String>,
    pub event_id: Option<String>,
    pub message_id: Option<String>,
    pub wiki_page_slug: Option<String>,
    pub todo_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyEntry {
    pub event_id: String,
    pub input_id: Option<String>,
    pub message_id: Option<String>,
    pub raw_text: String,
    pub source: String,
    pub event_status: String,
    pub recorded_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyReviewRecord {
    pub id: String,
    pub date: String,
    pub prompt_version: String,
    pub result_json: String,
    pub source_event_ids: Vec<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityFact {
    pub id: String,
    pub entity_kind: String,
    pub entity_slug: String,
    pub fact_text: String,
    pub occurred_at: String,
    pub confidence: i64,
    pub source_event_id: String,
    pub created_at: String,
    pub last_seen_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityMergeStatus {
    pub source_slug: String,
    pub target_slug: String,
    pub entity_kind: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct EventAnalysisDetail {
    pub event_id: String,
    pub raw_text: String,
    pub source: String,
    pub recorded_at: String,
    pub event_status: String,
    pub job_status: String,
    pub attempts: i64,
    pub last_error: Option<String>,
    pub available_at: String,
    pub prompt_version: Option<String>,
    pub result_json: Option<String>,
    pub analysis_created_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventRecordabilityDecision {
    pub event_id: String,
    pub recordable: bool,
    pub kind: String,
    pub reason: String,
    pub created_at: String,
}

pub(crate) fn map_input_record(row: &rusqlite::Row) -> rusqlite::Result<InputRecord> {
    Ok(InputRecord {
        id: row.get(0)?,
        raw_text: row.get(1)?,
        source: row.get(2)?,
        route_status: row.get(3)?,
        idempotency_key: row.get(4)?,
        event_id: row.get(5)?,
        message_id: row.get(6)?,
        wiki_page_slug: row.get(7)?,
        todo_id: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

pub(crate) fn map_wiki_page(row: &rusqlite::Row) -> rusqlite::Result<WikiPage> {
    let tags_raw: String = row.get(6)?;
    let sources_raw: String = row.get(7)?;
    Ok(WikiPage {
        id: row.get(0)?,
        slug: row.get(1)?,
        kind: row.get(2)?,
        title: row.get(3)?,
        summary: row.get(4)?,
        content_md: row.get(5)?,
        tags: serde_json::from_str(&tags_raw).unwrap_or_default(),
        source_event_ids: serde_json::from_str(&sources_raw).unwrap_or_default(),
        evidence_count: row.get(8)?,
        first_seen_at: row.get(9)?,
        last_seen_at: row.get(10)?,
        status: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
        source_url: row.get(14)?,
        area: row.get(15)?,
        based_on: row.get(16)?,
        content_type: row.get(17)?,
        human_edited_at: row.get(18)?,
        opinion: row.get(19)?,
    })
}

/// wiki_pages 行 → WikiPage 的公共列清单。
/// 顺序必须与 `map_wiki_page` 的按位取值（0..=19）严格一致。
pub(crate) const WIKI_PAGE_COLS: &str = "id, slug, kind, title, summary, content_md, tags, source_event_ids, \
     evidence_count, first_seen_at, last_seen_at, status, created_at, updated_at, source_url, \
     COALESCE(area, 'insight'), based_on, content_type, human_edited_at, opinion";

pub struct Store {
    // pub(crate) 而非私有：storage.rs 拆出的子模块（migrations / provider / …）
    // 在同一 crate 的 `impl Store` 块里需要直接访问连接与路径。
    pub(crate) connection: Connection,
    pub(crate) path: std::path::PathBuf,
}

impl Clone for Store {
    fn clone(&self) -> Self {
        Self::open(&self.path).expect("reopen database")
    }
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let connection = Connection::open(path)
            .with_context(|| format!("open SQLite database {}", path.display()))?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        // schema 与 30 个版本迁移抽到 storage::migrations（幂等、防中途崩溃）。
        migrations::ensure_schema(&connection)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        }
        let store = Self {
            connection,
            path: path.to_path_buf(),
        };
        // 幂等存量回填：老项目页的 file:// 来源（只填空缺，无副作用）。
        store.backfill_project_source_urls()?;
        // WAL checkpoint：迁移与回填完成后立即把 WAL 收进主库文件，
        // 避免长驻 GUI + spawn-per-call 写入导致 WAL 无限增长（P2-8）。
        // PASSIVE 不阻塞其他连接；TRUNCATE 只能在无并发读者时收尾，这里用 PASSIVE 最安全。
        let _ = store
            .connection
            .execute_batch("PRAGMA wal_checkpoint(PASSIVE);");
        Ok(store)
    }


    // ── LLM wiki：页面 / 修订 / 日志 / meta ────────────────────────────────



    /// 在知识库页面与个人事件记录中按关键词搜索
    pub fn search_knowledge_base(&self, query: &str, limit: usize) -> Result<Vec<KnowledgeHit>> {
        let like = format!("%{}%", query);
        let mut hits: Vec<KnowledgeHit> = Vec::new();

        let mut wiki_stmt = self.connection.prepare(
            "SELECT kind, title, substr(content_md, 1, 160)
             FROM wiki_pages
             WHERE status = 'active' AND COALESCE(area, 'insight') != 'derivative'
               AND (title LIKE ?1 OR content_md LIKE ?1 OR tags LIKE ?1)
             ORDER BY updated_at DESC
             LIMIT ?2",
        )?;
        let wiki_rows = wiki_stmt.query_map(params![like, limit as i64], |row| {
            Ok(KnowledgeHit {
                kind: format!("知识页:{}", row.get::<_, String>(0)?),
                title: row.get(1)?,
                snippet: row.get(2)?,
            })
        })?;
        for hit in wiki_rows {
            hits.push(hit?);
        }

        if hits.len() < limit {
            let remaining = (limit - hits.len()) as i64;
            let mut event_stmt = self.connection.prepare(
                "SELECT substr(raw_text, 1, 160)
                 FROM events
                 WHERE raw_text LIKE ?1
                 ORDER BY recorded_at DESC
                 LIMIT ?2",
            )?;
            let event_rows = event_stmt.query_map(params![like, remaining], |row| {
                Ok(KnowledgeHit {
                    kind: "事件".to_string(),
                    title: row.get::<_, String>(0)?.chars().take(24).collect(),
                    snippet: row.get(0)?,
                })
            })?;
            for hit in event_rows {
                hits.push(hit?);
            }
        }

        Ok(hits)
    }
}

impl StorageAdapter for Store {
    fn insert_event(&self, event: NewEvent) -> Result<String> {
        Store::insert_event(self, event)
    }

    fn list_events(&self) -> Result<Vec<EventSummary>> {
        Store::list_events(self)
    }

    fn list_analyses(&self) -> Result<Vec<AnalysisSummary>> {
        Store::list_analyses(self)
    }

    fn claim_analysis_job(&self) -> Result<Option<AnalysisJob>> {
        Store::claim_analysis_job(self)
    }

    fn complete_analysis(
        &self,
        job: &AnalysisJob,
        prompt_version: &str,
        result_json: &str,
    ) -> Result<()> {
        Store::complete_analysis(self, job, prompt_version, result_json)
    }

    fn fail_analysis(&self, job: &AnalysisJob, error: &str) -> Result<()> {
        Store::fail_analysis(self, job, error)
    }

    fn active_ai_provider_config(&self) -> Result<Option<AiProviderConfig>> {
        Store::active_ai_provider_config(self)
    }

    fn upsert_ai_provider_config(&self, base_url: &str, model: &str, api_key: &str) -> Result<()> {
        Store::upsert_ai_provider_config(self, base_url, model, api_key)
    }

    fn list_ai_provider_configs(&self) -> Result<Vec<AiProviderConfigRow>> {
        Store::list_ai_provider_configs(self)
    }

    fn save_ai_provider_config(
        &self,
        id: Option<&str>,
        name: &str,
        provider_type: &str,
        base_url: &str,
        model: &str,
        api_key: &str,
        temperature: f64,
        max_tokens: Option<i64>,
    ) -> Result<String> {
        Store::save_ai_provider_config(
            self,
            id,
            name,
            provider_type,
            base_url,
            model,
            api_key,
            temperature,
            max_tokens,
        )
    }

    fn set_active_ai_provider_config(&self, id: &str) -> Result<()> {
        Store::set_active_ai_provider_config(self, id)
    }

    fn delete_ai_provider_config(&self, id: &str) -> Result<()> {
        Store::delete_ai_provider_config(self, id)
    }
}

impl Drop for Store {
    fn drop(&mut self) {
        // 关闭时再 checkpoint 一次：进程退出前把未合并的 WAL 页写回主库，
        // 让 -wal 文件保持接近空的状态（P2-8）。
        let _ = self
            .connection
            .execute_batch("PRAGMA wal_checkpoint(PASSIVE);");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{OptionalExtension, Transaction, TransactionBehavior};
    use std::time::{SystemTime, UNIX_EPOCH};
    use uuid::Uuid;

    fn temporary_database() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "elsewhen-storage-test-{}.db",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn persists_and_lists_raw_events() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        store.insert_event(NewEvent::now("完成最小 MVP")).unwrap();
        let events = store.list_events().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].raw_text, "完成最小 MVP");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn analysis_job_stats_cover_every_queue_status() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        assert_eq!(
            store.analysis_job_stats().unwrap(),
            AnalysisJobStats::default()
        );

        for (index, status) in ["pending", "running", "retry", "succeeded", "failed"]
            .into_iter()
            .enumerate()
        {
            let event_id = store
                .insert_event(NewEvent::now(&format!("queue status {index}")))
                .unwrap();
            store
                .connection
                .execute(
                    "UPDATE analysis_jobs SET status = ?1 WHERE event_id = ?2",
                    params![status, event_id],
                )
                .unwrap();
        }

        assert_eq!(
            store.analysis_job_stats().unwrap(),
            AnalysisJobStats {
                pending: 1,
                running: 1,
                retry: 1,
                succeeded: 1,
                failed: 1,
            }
        );
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn input_record_is_idempotent_and_links_routed_objects() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        let first = store
            .create_input_record(" 今天完成了支付模块 ", "main_input", Some("request-1"))
            .unwrap();
        let repeated = store
            .create_input_record("不同文本也不能重复创建", "main_input", Some("request-1"))
            .unwrap();
        assert_eq!(first.id, repeated.id);
        assert_eq!(repeated.raw_text, "今天完成了支付模块");
        assert_eq!(repeated.route_status, "pending");

        let event_id = store.insert_event(NewEvent::now(&first.raw_text)).unwrap();
        let routed = store
            .update_input_route(&first.id, "routed", Some(&event_id), None, None, None)
            .unwrap();
        assert_eq!(routed.event_id.as_deref(), Some(event_id.as_str()));
        assert_eq!(routed.route_status, "routed");
        assert!(store
            .update_input_route(&first.id, "unknown", None, None, None, None)
            .is_err());

        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn submit_input_as_event_is_atomic_and_idempotent() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        let first = store
            .submit_input_as_event("今天完成统一输入", "main_input", Some("submit-1"))
            .unwrap();
        let repeated = store
            .submit_input_as_event("不会产生第二条", "main_input", Some("submit-1"))
            .unwrap();
        assert_eq!(first.id, repeated.id);
        assert_eq!(first.route_status, "routed");
        assert!(first.event_id.is_some());
        assert_eq!(store.list_events().unwrap().len(), 1);
        assert_eq!(store.analysis_job_stats().unwrap().pending, 1);

        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn submit_conversation_input_links_one_message_and_one_event() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        let conversation_id = store.create_conversation(None, None).unwrap();
        let first = store
            .submit_conversation_input(
                &conversation_id,
                "今天完成主循环接线",
                Some("conv-submit-1"),
            )
            .unwrap();
        let repeated = store
            .submit_conversation_input(&conversation_id, "重复", Some("conv-submit-1"))
            .unwrap();
        assert_eq!(first.id, repeated.id);
        assert!(first.event_id.is_some());
        assert!(first.message_id.is_some());
        assert_eq!(store.list_events().unwrap().len(), 1);
        assert_eq!(store.list_messages(&conversation_id).unwrap().len(), 1);
        assert_eq!(store.analysis_job_stats().unwrap().pending, 1);

        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn idempotency_conflict_error_message_matches_column() {
        // P2-3 兜底分支依赖错误串识别幂等冲突；用裸 SQL 触发唯一冲突验证
        // SQLite 实际报「UNIQUE constraint failed: input_records.idempotency_key」
        // （纯列索引不报索引名），catch 分支的匹配串必须与此一致才有机会命中。
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        store
            .connection
            .execute(
                "INSERT INTO input_records
                 (id,raw_text,source,route_status,idempotency_key,created_at,updated_at)
                 VALUES (?1,'第一次','main_input','pending',?2,?3,?3)",
                params![Uuid::new_v4().to_string(), "dup-key", now],
            )
            .unwrap();
        let err = store
            .connection
            .execute(
                "INSERT INTO input_records
                 (id,raw_text,source,route_status,idempotency_key,created_at,updated_at)
                 VALUES (?1,'第二次','main_input','pending',?2,?3,?3)",
                params![Uuid::new_v4().to_string(), "dup-key", now],
            )
            .unwrap_err();
        assert!(
            err.to_string().contains("input_records.idempotency_key"),
            "错误串应含幂等键列：{err}"
        );
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn single_active_config_conflict_error_message_matches_column() {
        // P2-2 兜底分支依赖识别「同一时刻两条 is_active=1」冲突；裸 SQL 触发
        // 部分唯一索引冲突，确认错误串按列报（同 index 的名字不出现）。
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        let insert = |name: &str| {
            store
                .connection
                .execute(
                    "INSERT INTO ai_provider_configs
                     (id,name,provider_type,base_url,model,api_key_source,is_active,temperature,max_tokens,created_at,updated_at)
                     VALUES (?1,?2,'openai','http://x','gpt','env',1,0.7,NULL,?3,?3)",
                    params![Uuid::new_v4().to_string(), name, now],
                )
                .map(|_| ())
        };
        insert("cfg-a").unwrap();
        let err = insert("cfg-b").unwrap_err();
        assert!(
            err.to_string().contains("ai_provider_configs.is_active"),
            "错误串应含激活标记列：{err}"
        );
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn daily_entries_unify_legacy_capture_and_conversation_events() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        store.insert_event(NewEvent::now("历史事件")).unwrap();
        store
            .submit_input_as_event("Capture 事件", "capture", None)
            .unwrap();
        let conversation_id = store.create_conversation(None, None).unwrap();
        store
            .submit_conversation_input(&conversation_id, "对话事件", None)
            .unwrap();

        let entries = store
            .daily_entries(chrono::Local::now().date_naive())
            .unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.input_id.is_some())
                .count(),
            2
        );
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.message_id.is_some())
                .count(),
            1
        );
        assert!(entries.iter().any(|entry| entry.raw_text == "历史事件"));

        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn daily_reviews_are_versioned_and_require_same_day_sources() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        let event_id = store
            .insert_event(NewEvent::now("今天完成总结契约"))
            .unwrap();
        let today = chrono::Local::now().date_naive();

        assert!(store
            .save_daily_review(today, "daily-review-v1", "{}", &[])
            .is_err());
        assert!(store
            .save_daily_review(
                today - chrono::Duration::days(1),
                "daily-review-v1",
                "{}",
                std::slice::from_ref(&event_id),
            )
            .is_err());

        let first = store
            .save_daily_review(
                today,
                "daily-review-v1",
                r#"{"summary":"第一版"}"#,
                std::slice::from_ref(&event_id),
            )
            .unwrap();
        let second = store
            .save_daily_review(
                today,
                "daily-review-v2",
                r#"{"summary":"第二版"}"#,
                std::slice::from_ref(&event_id),
            )
            .unwrap();
        assert_ne!(first, second);

        let latest = store.latest_daily_review(today).unwrap().unwrap();
        assert_eq!(latest.id, second);
        assert_eq!(latest.prompt_version, "daily-review-v2");
        assert_eq!(latest.source_event_ids, vec![event_id]);
        assert_eq!(store.list_events().unwrap().len(), 1);

        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn entity_facts_are_idempotent_and_raise_confidence() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        let event_id = store
            .insert_event(NewEvent::now("项目进入测试阶段"))
            .unwrap();
        let first = store
            .upsert_entity_fact(
                "project",
                "elsewhen",
                "进入测试阶段",
                chrono::Utc::now().to_rfc3339().as_str(),
                2,
                &event_id,
            )
            .unwrap();
        let second = store
            .upsert_entity_fact(
                "project",
                "elsewhen",
                "进入测试阶段",
                &first.occurred_at,
                4,
                &event_id,
            )
            .unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(second.confidence, 4);
        assert_eq!(
            store
                .list_entity_facts("project", "elsewhen")
                .unwrap()
                .len(),
            1
        );
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn deleting_entity_fact_keeps_source_event() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        let event_id = store
            .insert_event(crate::event::NewEvent::now("原始事实"))
            .unwrap();
        let fact = store
            .upsert_entity_fact(
                "person",
                "person/张三",
                "负责项目",
                "2026-01-01T00:00:00Z",
                3,
                &event_id,
            )
            .unwrap();
        assert!(store.delete_entity_fact(&fact.id).unwrap());
        assert!(store
            .list_entity_facts("person", "person/张三")
            .unwrap()
            .is_empty());
        assert_eq!(store.list_events().unwrap().len(), 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn entity_aliases_are_idempotent_and_scoped() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        store
            .add_entity_alias("project", "project/acme", "ACME")
            .unwrap();
        store
            .add_entity_alias("project", "project/acme", "ACME")
            .unwrap();
        assert_eq!(
            store
                .list_entity_aliases("project", "project/acme")
                .unwrap(),
            vec!["ACME"]
        );
        assert!(store
            .list_entity_aliases("project", "project/other")
            .unwrap()
            .is_empty());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn events_on_date_uses_local_day_boundaries() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        let today = chrono::Local::now().date_naive();
        let yesterday = today - chrono::Duration::days(1);
        let now = chrono::Utc::now();
        store
            .insert_event(NewEvent {
                raw_text: "今天的事件",
                occurred_at: now,
                recorded_at: now,
                source: "test",
            })
            .unwrap();
        store
            .insert_event(NewEvent {
                raw_text: "昨天的事件",
                occurred_at: now - chrono::Duration::days(1),
                recorded_at: now - chrono::Duration::days(1),
                source: "test",
            })
            .unwrap();
        let today_events = store.events_on_date(today).unwrap();
        assert_eq!(today_events.len(), 1);
        assert_eq!(today_events[0].raw_text, "今天的事件");
        let yesterday_events = store.events_on_date(yesterday).unwrap();
        assert_eq!(yesterday_events.len(), 1);
        assert_eq!(yesterday_events[0].raw_text, "昨天的事件");
        assert!(store
            .events_on_date(today - chrono::Duration::days(10))
            .unwrap()
            .is_empty());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn raw_text_cannot_be_changed() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        let id = store.insert_event(NewEvent::now("原始事实")).unwrap();
        let result = store
            .connection
            .execute("UPDATE events SET raw_text = '被覆盖' WHERE id = ?1", [id]);
        assert!(result.is_err());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn event_creation_enqueues_one_analysis_job() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        let event_id = store.insert_event(NewEvent::now("等待后台理解")).unwrap();
        let count: i64 = store
            .connection
            .query_row(
                "SELECT count(*) FROM analysis_jobs WHERE event_id = ?1 AND status = 'pending'",
                [event_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn failed_analysis_keeps_raw_event_and_schedules_retry() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        store
            .insert_event(NewEvent::now("AI 失败也不能丢"))
            .unwrap();
        let job = store.claim_analysis_job().unwrap().unwrap();
        store.fail_analysis(&job, "provider unavailable").unwrap();
        let status: String = store
            .connection
            .query_row(
                "SELECT status FROM analysis_jobs WHERE id = ?1",
                [&job.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "retry");
        assert_eq!(store.list_events().unwrap()[0].raw_text, "AI 失败也不能丢");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn recover_interrupted_analysis_jobs_requeues_running_work() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        store.insert_event(NewEvent::now("恢复中的任务")).unwrap();
        let job = store.claim_analysis_job().unwrap().unwrap();
        assert_eq!(store.analysis_job_stats().unwrap().running, 1);
        assert_eq!(store.recover_interrupted_analysis_jobs().unwrap(), 1);
        assert_eq!(store.analysis_job_stats().unwrap().retry, 1);
        assert!(store.claim_analysis_job().unwrap().is_some());
        let _ = std::fs::remove_file(path);
        drop(job);
    }

    #[test]
    fn event_analysis_detail_exposes_queue_result_and_error() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        let event_id = store.insert_event(NewEvent::now("查看分析详情")).unwrap();
        let pending = store.event_analysis_detail(&event_id).unwrap().unwrap();
        assert_eq!(pending.job_status, "pending");
        assert_eq!(pending.attempts, 0);
        assert!(pending.result_json.is_none());

        let job = store.claim_analysis_job().unwrap().unwrap();
        store.fail_analysis(&job, "invalid schema").unwrap();
        let retry = store.event_analysis_detail(&event_id).unwrap().unwrap();
        assert_eq!(retry.job_status, "retry");
        assert_eq!(retry.last_error.as_deref(), Some("invalid schema"));

        store
            .complete_analysis(
                &job,
                "event-analysis-v1",
                r#"{"schema_version":"event-analysis-v1"}"#,
            )
            .unwrap();
        let succeeded = store.event_analysis_detail(&event_id).unwrap().unwrap();
        assert_eq!(succeeded.job_status, "succeeded");
        assert_eq!(
            succeeded.prompt_version.as_deref(),
            Some("event-analysis-v1")
        );
        assert!(succeeded.result_json.is_some());
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn manual_recordability_is_append_only_and_preserves_raw_event_and_message() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        let conversation_id = store.create_conversation(None, None).unwrap();
        let input = store
            .submit_conversation_input(
                &conversation_id,
                "你这个回答情绪价值不够",
                Some("recordability-1"),
            )
            .unwrap();
        let event_id = input.event_id.clone().unwrap();
        let message_id = input.message_id.clone().unwrap();
        store
            .upsert_entity_fact(
                "topic",
                "topic/回答",
                "评价：不满意",
                "2026-09-21",
                2,
                &event_id,
            )
            .unwrap();
        store
            .create_pending_action(
                &conversation_id,
                "propose_people_relations",
                &format!(r#"{{"source_event_id":"{event_id}"}}"#),
            )
            .unwrap();

        let first = store
            .set_event_recordability(&event_id, false, "manual-ui")
            .unwrap();
        assert!(!first.recordable);
        assert_eq!(first.kind, "discussion");
        assert!(store
            .list_entity_facts("topic", "topic/回答")
            .unwrap()
            .is_empty());
        assert!(store
            .pending_actions_for_conversation(&conversation_id)
            .unwrap()
            .is_empty());
        assert_eq!(
            store
                .get_conversation(&conversation_id)
                .unwrap()
                .unwrap()
                .tag
                .as_deref(),
            Some("discussion")
        );
        assert_eq!(
            store.list_events().unwrap()[0].raw_text,
            "你这个回答情绪价值不够"
        );
        assert_eq!(
            store.list_messages(&conversation_id).unwrap()[0].id,
            message_id
        );

        let second = store
            .set_event_recordability(&event_id, true, "manual-ui")
            .unwrap();
        assert!(second.recordable);
        let decision_count: i64 = store
            .connection
            .query_row(
                "SELECT count(*) FROM event_recordability_decisions WHERE event_id=?1",
                [&event_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(decision_count, 2);
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn reanalysis_requeues_without_deleting_previous_analysis() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        let event_id = store.insert_event(NewEvent::now("存量重新分析")).unwrap();
        let job = store.claim_analysis_job().unwrap().unwrap();
        store.complete_analysis(&job, "event-analysis", r#"{"schema_version":"event-analysis","recordable":true,"kind":"event","event_type":"note","confidence":0.8,"summary":"旧结果","clarifications":[],"people":[],"projects":[],"activities":[],"follow_ups":[]}"#).unwrap();
        assert!(store.requeue_event_analysis(&event_id).unwrap());
        let detail = store.event_analysis_detail(&event_id).unwrap().unwrap();
        assert_eq!(detail.job_status, "pending");
        assert!(detail.result_json.unwrap().contains("旧结果"));
        assert_eq!(
            store
                .connection
                .query_row(
                    "SELECT count(*) FROM event_analyses WHERE event_id=?1",
                    [&event_id],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn provider_config_is_fully_persisted() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        store
            .upsert_ai_provider_config("https://example.test/v1", "test-model", "secret-value")
            .unwrap();
        let provider = store.active_ai_provider_config().unwrap().unwrap();
        assert_eq!(provider.base_url, "https://example.test/v1");
        assert_eq!(provider.model, "test-model");
        assert_eq!(provider.api_key_source, "database");
        assert_eq!(provider.api_key, "secret-value");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn concurrent_connections_can_insert_events() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        let handles = (0..8)
            .map(|index| {
                let connection = store.clone();
                std::thread::spawn(move || {
                    connection
                        .insert_event(NewEvent::now(&format!("并发事件 {index}")))
                        .unwrap();
                })
            })
            .collect::<Vec<_>>();
        for handle in handles {
            handle.join().unwrap();
        }
        assert_eq!(store.list_events().unwrap().len(), 8);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn update_todo_edits_fields_and_clears_optionals() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        let todo = store
            .create_todo(
                "跟进付款",
                "normal",
                Some("2026-09-20"),
                None,
                None,
                Some("原始说明"),
            )
            .unwrap();

        // 编辑：标题/说明/优先级/截止都改
        store
            .update_todo(
                &todo.id,
                "跟进双链路付款",
                Some("已和张玮对齐时间"),
                Some("high"),
                Some("2026-09-18"),
            )
            .unwrap();
        let updated = store.list_todos(None).unwrap();
        assert_eq!(updated.len(), 1);
        assert_eq!(updated[0].title, "跟进双链路付款");
        assert_eq!(updated[0].note.as_deref(), Some("已和张玮对齐时间"));
        assert_eq!(updated[0].priority, "high");
        assert_eq!(updated[0].due_at.as_deref(), Some("2026-09-18"));
        assert_eq!(updated[0].status, TodoStatus::Open, "编辑不改状态");

        // 清除可选字段：传 None
        store
            .update_todo(&todo.id, "跟进双链路付款", None, None, None)
            .unwrap();
        let cleared = store.list_todos(None).unwrap();
        assert!(cleared[0].note.is_none());
        assert!(cleared[0].due_at.is_none());
        assert_eq!(cleared[0].priority, "normal");

        // 空标题拒绝
        assert!(store
            .update_todo(&todo.id, "   ", None, None, None)
            .is_err());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn work_item_link_is_lazy_idempotent_and_keeps_completed_history() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        let todo = store
            .create_todo(
                "讨论发布策略",
                "high",
                Some("2026-09-30"),
                None,
                None,
                Some("先收集约束"),
            )
            .unwrap();

        // 创建待办不会隐式制造知识页；页面升级后关联可回溯。
        assert!(todo.related_wiki_slug.is_none());
        store
            .set_todo_related_wiki_slug(&todo.id, "topic/discuss-release")
            .unwrap();
        let linked = store.list_todos(None).unwrap();
        assert_eq!(
            linked[0].related_wiki_slug.as_deref(),
            Some("topic/discuss-release")
        );

        // 重复升级只覆盖同一关联，不产生第二条待办或改变其字段。
        store
            .set_todo_related_wiki_slug(&todo.id, "topic/discuss-release")
            .unwrap();
        assert_eq!(store.list_todos(None).unwrap().len(), 1);
        store
            .update_todo_status(&todo.id, TodoStatus::Done)
            .unwrap();
        let completed = store.list_todos(None).unwrap();
        assert_eq!(completed[0].status, TodoStatus::Done);
        assert_eq!(
            completed[0].related_wiki_slug.as_deref(),
            Some("topic/discuss-release")
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn get_todo_includes_archived_history_for_on_demand_migration() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        let todo = store
            .create_todo("归档后仍可讨论", "normal", None, None, None, None)
            .unwrap();
        store
            .update_todo_status(&todo.id, TodoStatus::Archived)
            .unwrap();
        let found = store.get_todo(&todo.id).unwrap().unwrap();
        assert_eq!(found.status, TodoStatus::Archived);
        assert!(store.list_todos(None).unwrap().is_empty());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn find_wiki_page_by_tag_exact_matches_json_array_member() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        // tag 精确落在 JSON 数组的一个成员里（P2-5 直查替代全表扫描）
        store
            .upsert_wiki_page(
                &wiki_draft("topic/target", "topic", "命中页"),
                ContentPolicy::Always,
            )
            .unwrap();
        let target = store.get_wiki_page("topic/target").unwrap().unwrap();
        store
            .update_wiki_tags(&target.slug, &["work-item-id:abc".to_string()])
            .unwrap();
        let found = store
            .find_wiki_page_by_tag("work-item-id:abc")
            .unwrap()
            .unwrap();
        assert_eq!(found.slug, "topic/target");

        // 不存在的 tag 返回 None
        assert!(store
            .find_wiki_page_by_tag("work-item-id:nope")
            .unwrap()
            .is_none());

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn relations_upsert_dedupe_and_list_both_directions() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        let rel = RelationDraft {
            from_slug: "person/张三".to_string(),
            from_kind: "person".to_string(),
            to_slug: "kb-双链路付款".to_string(),
            to_kind: "project".to_string(),
            relation: "负责".to_string(),
            note: Some("主导该项目".to_string()),
            confidence: 3,
            source_conversation_id: Some("conv-1".to_string()),
            source_event_id: None,
        };
        store.upsert_relation(&rel).unwrap();
        // 同一条重复写入：去重为 1 条，指向同一页双向都能查到
        store.upsert_relation(&rel).unwrap();
        let from_person = store.list_relations_for_page("person/张三").unwrap();
        let from_project = store.list_relations_for_page("kb-双链路付款").unwrap();
        assert_eq!(from_person.len(), 1);
        assert_eq!(from_project.len(), 1);
        assert_eq!(from_person[0].relation, "负责");
        assert_eq!(from_person[0].to_slug, "kb-双链路付款");
        assert_eq!(store.list_relations().unwrap().len(), 1);

        // 删除
        assert!(store.delete_relation(&from_person[0].id).unwrap());
        assert!(store.list_relations().unwrap().is_empty());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn rename_wiki_page_moves_slug_relations_and_chat() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        let draft = WikiPageDraft {
            slug: "topic/付款流程".to_string(),
            kind: "topic".to_string(),
            title: "付款流程".to_string(),
            summary: "付款流程（由人物关系确认时自动建档）".to_string(),
            content_md: "# 付款流程\n\n跟进付款相关事宜。".to_string(),
            tags: vec!["付款".to_string()],
            source_event_ids: vec![],
            status: "active".to_string(),
            reason: "test".to_string(),
            source_url: None,
        };
        store
            .upsert_wiki_page(&draft, ContentPolicy::Always)
            .unwrap();
        store
            .upsert_relation(&RelationDraft {
                from_slug: "person/谭俊".to_string(),
                from_kind: "person".to_string(),
                to_slug: "topic/付款流程".to_string(),
                to_kind: "topic".to_string(),
                relation: "跟进".to_string(),
                note: Some("处理付款事宜".to_string()),
                confidence: 3,
                source_conversation_id: Some("conv-1".to_string()),
                source_event_id: None,
            })
            .unwrap();
        store
            .create_wiki_chat_conversation("topic/付款流程", "[知识页] 付款流程")
            .unwrap();
        let before_revisions = store.list_wiki_revisions("topic/付款流程").unwrap().len();

        // 改名：标题 + slug 一起换，关系与会话迁移
        let outcome = store
            .rename_wiki_page("topic/付款流程", "fpso111 尾款", "项目真名更正")
            .unwrap();
        assert!(outcome.changed);
        assert_eq!(outcome.new_slug, "topic/fpso111-尾款");
        assert_eq!(outcome.relations_moved, 1);
        assert_eq!(outcome.chats_moved, 1);
        assert!(store.get_wiki_page("topic/付款流程").unwrap().is_none());
        let renamed = store.get_wiki_page("topic/fpso111-尾款").unwrap().unwrap();
        assert_eq!(renamed.title, "fpso111 尾款");
        assert!(
            renamed.summary.contains("fpso111 尾款"),
            "summary 里的旧名应被替换: {}",
            renamed.summary
        );
        // 关系引用已指向新 slug
        let rels = store.list_relations().unwrap();
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0].to_slug, "topic/fpso111-尾款");
        // 页内聊天会话已迁移
        assert!(store
            .find_wiki_chat_conversation("topic/fpso111-尾款")
            .unwrap()
            .is_some());
        assert!(store
            .find_wiki_chat_conversation("topic/付款流程")
            .unwrap()
            .is_none());
        // 修订历史多了一条重命名记录
        assert_eq!(
            store
                .list_wiki_revisions("topic/fpso111-尾款")
                .unwrap()
                .len(),
            before_revisions + 1
        );

        // 标题没变：changed=false，零写操作
        let outcome = store
            .rename_wiki_page("topic/fpso111-尾款", "fpso111 尾款", "x")
            .unwrap();
        assert!(!outcome.changed);

        // 页面不存在：报错并提示可能已改名
        let err = store
            .rename_wiki_page("topic/付款流程", "x", "y")
            .unwrap_err();
        assert!(err.to_string().contains("知识页不存在"), "{err}");

        // 无前缀页（如来源页）：只改标题，slug 不动
        let src_draft = WikiPageDraft {
            slug: "tweet-123".to_string(),
            kind: "source".to_string(),
            title: "旧标题".to_string(),
            summary: "s".to_string(),
            content_md: "c".to_string(),
            tags: vec![],
            source_event_ids: vec![],
            status: "active".to_string(),
            reason: "test".to_string(),
            source_url: None,
        };
        store
            .upsert_wiki_page(&src_draft, ContentPolicy::Always)
            .unwrap();
        let outcome = store
            .rename_wiki_page("tweet-123", "新标题", "更正")
            .unwrap();
        assert!(outcome.changed);
        assert_eq!(outcome.new_slug, "tweet-123", "来源页 slug 应保持不变");
        assert_eq!(
            store.get_wiki_page("tweet-123").unwrap().unwrap().title,
            "新标题"
        );
        let _ = std::fs::remove_file(path);
    }

    fn entity_page(store: &Store, slug: &str, kind: &str, title: &str) {
        store
            .upsert_wiki_page(
                &WikiPageDraft {
                    slug: slug.to_string(),
                    kind: kind.to_string(),
                    title: title.to_string(),
                    summary: String::new(),
                    content_md: format!("# {title}"),
                    tags: vec![],
                    source_event_ids: vec![],
                    status: "active".to_string(),
                    reason: "test".to_string(),
                    source_url: None,
                },
                ContentPolicy::Always,
            )
            .unwrap();
    }

    #[test]
    fn entity_merge_and_undo_preserve_sources_and_restore_rows() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        entity_page(&store, "person/张伟-a", "person", "张伟（设计）");
        entity_page(&store, "person/张伟", "person", "张伟");
        entity_page(&store, "project/付款", "project", "付款项目");
        let event_id = store
            .insert_event(NewEvent::now("张伟负责付款联调"))
            .unwrap();
        let fact = store
            .upsert_entity_fact(
                "person",
                "person/张伟-a",
                "职责：付款联调",
                "2026-09-21",
                4,
                &event_id,
            )
            .unwrap();
        store
            .add_entity_alias("person", "person/张伟-a", "设计张伟")
            .unwrap();
        let relation = store
            .upsert_relation(&RelationDraft {
                from_slug: "person/张伟-a".into(),
                from_kind: "person".into(),
                to_slug: "project/付款".into(),
                to_kind: "project".into(),
                relation: "负责".into(),
                note: None,
                confidence: 4,
                source_conversation_id: None,
                source_event_id: Some(event_id.clone()),
            })
            .unwrap();
        let todo = store
            .create_todo(
                "确认付款联调",
                "high",
                None,
                Some(&event_id),
                Some("person/张伟-a"),
                None,
            )
            .unwrap();

        assert!(store
            .merge_entity("person", "person/张伟-a", "person/张伟")
            .unwrap());
        assert_eq!(
            store
                .get_wiki_page("person/张伟-a")
                .unwrap()
                .unwrap()
                .status,
            "merged"
        );
        assert_eq!(
            store.list_entity_facts("person", "person/张伟").unwrap()[0].id,
            fact.id
        );
        assert_eq!(
            store.list_relations_for_page("person/张伟").unwrap()[0].id,
            relation.id
        );
        assert_eq!(store.list_events().unwrap().len(), 1);
        assert_eq!(
            store.list_todos(None).unwrap()[0]
                .related_wiki_slug
                .as_deref(),
            Some("person/张伟")
        );

        assert!(store.undo_entity_merge("person/张伟-a").unwrap());
        assert_eq!(
            store
                .get_wiki_page("person/张伟-a")
                .unwrap()
                .unwrap()
                .status,
            "active"
        );
        assert_eq!(
            store.list_entity_facts("person", "person/张伟-a").unwrap()[0].id,
            fact.id
        );
        assert_eq!(
            store
                .list_entity_aliases("person", "person/张伟-a")
                .unwrap(),
            vec!["设计张伟"]
        );
        assert_eq!(
            store.list_relations_for_page("person/张伟-a").unwrap()[0].id,
            relation.id
        );
        assert_eq!(store.list_events().unwrap()[0].raw_text, "张伟负责付款联调");
        let restored_todo = store
            .list_todos(None)
            .unwrap()
            .into_iter()
            .find(|item| item.id == todo.id)
            .unwrap();
        assert_eq!(
            restored_todo.related_wiki_slug.as_deref(),
            Some("person/张伟-a")
        );
        assert!(store
            .merge_entity("person", "person/张伟-a", "person/张伟")
            .unwrap());
        assert!(store.undo_entity_merge("person/张伟-a").unwrap());
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn entity_merge_deduplicates_and_undo_restores_source_copies() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        entity_page(&store, "topic/旧", "topic", "旧主题");
        entity_page(&store, "topic/新", "topic", "新主题");
        let event_id = store.insert_event(NewEvent::now("共同事实")).unwrap();
        store
            .upsert_entity_fact(
                "topic",
                "topic/旧",
                "状态：进行中",
                "2026-09-21",
                3,
                &event_id,
            )
            .unwrap();
        store
            .upsert_entity_fact(
                "topic",
                "topic/新",
                "状态：进行中",
                "2026-09-21",
                5,
                &event_id,
            )
            .unwrap();
        store
            .add_entity_alias("topic", "topic/旧", "共同别名")
            .unwrap();
        store
            .add_entity_alias("topic", "topic/新", "共同别名")
            .unwrap();
        store.merge_entity("topic", "topic/旧", "topic/新").unwrap();
        assert_eq!(
            store.list_entity_facts("topic", "topic/新").unwrap().len(),
            1
        );
        assert_eq!(
            store
                .list_entity_aliases("topic", "topic/新")
                .unwrap()
                .len(),
            1
        );
        store.undo_entity_merge("topic/旧").unwrap();
        assert_eq!(
            store.list_entity_facts("topic", "topic/旧").unwrap().len(),
            1
        );
        assert_eq!(
            store.list_entity_facts("topic", "topic/新").unwrap().len(),
            1
        );
        assert_eq!(
            store.list_entity_aliases("topic", "topic/旧").unwrap(),
            vec!["共同别名"]
        );
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn entity_merge_rejects_invalid_targets_and_undo_is_atomic_after_changes() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        entity_page(&store, "person/a", "person", "A");
        entity_page(&store, "person/b", "person", "B");
        entity_page(&store, "project/b", "project", "B项目");
        assert!(store.merge_entity("person", "person/a", "missing").is_err());
        assert!(store
            .merge_entity("person", "person/a", "project/b")
            .is_err());
        let event_id = store.insert_event(NewEvent::now("A事实")).unwrap();
        let fact = store
            .upsert_entity_fact(
                "person",
                "person/a",
                "团队：支付",
                "2026-09-21",
                3,
                &event_id,
            )
            .unwrap();
        store
            .merge_entity("person", "person/a", "person/b")
            .unwrap();
        store
            .connection
            .execute(
                "UPDATE entity_facts SET confidence=5 WHERE id=?1",
                [&fact.id],
            )
            .unwrap();
        assert!(store.undo_entity_merge("person/a").is_err());
        assert_eq!(
            store.get_wiki_page("person/a").unwrap().unwrap().status,
            "merged"
        );
        assert!(store
            .list_entity_facts("person", "person/a")
            .unwrap()
            .is_empty());
        assert_eq!(
            store.list_entity_facts("person", "person/b").unwrap()[0].confidence,
            5
        );
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn legacy_entity_merge_unique_constraint_is_migrated_without_losing_audit() {
        let path = temporary_database();
        let connection = Connection::open(&path).unwrap();
        connection.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE entity_merges (id TEXT PRIMARY KEY,entity_kind TEXT NOT NULL,source_slug TEXT NOT NULL,target_slug TEXT NOT NULL,created_at TEXT NOT NULL,UNIQUE(entity_kind,source_slug)); CREATE TABLE entity_merge_snapshots (merge_id TEXT NOT NULL,table_name TEXT NOT NULL,row_id TEXT NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(merge_id,table_name,row_id),FOREIGN KEY(merge_id) REFERENCES entity_merges(id) ON DELETE CASCADE); INSERT INTO entity_merges VALUES ('m1','person','person/a','person/b','2026-09-21'); INSERT INTO entity_merge_snapshots VALUES ('m1','entity_aliases','a1','{}');").unwrap();
        drop(connection);
        let store = Store::open(&path).unwrap();
        let schema: String = store
            .connection
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name='entity_merges'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(!schema
            .replace(' ', "")
            .contains("UNIQUE(entity_kind,source_slug)"));
        assert_eq!(
            store
                .connection
                .query_row("SELECT count(*) FROM entity_merges", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            store
                .connection
                .query_row("SELECT count(*) FROM entity_merge_snapshots", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn find_wiki_page_by_title_matches_exact_ignoring_case_and_trim() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        store
            .upsert_wiki_page(
                &WikiPageDraft {
                    slug: "person/张伟".to_string(),
                    kind: "person".to_string(),
                    title: "张伟".to_string(),
                    summary: "简介".to_string(),
                    content_md: "内容".to_string(),
                    tags: vec![],
                    source_event_ids: vec![],
                    status: "active".to_string(),
                    reason: "test".to_string(),
                    source_url: None,
                },
                ContentPolicy::Always,
            )
            .unwrap();
        assert!(store.find_wiki_page_by_title(" 张伟 ").unwrap().is_some());
        assert!(store
            .find_wiki_page_by_title("不存在的人")
            .unwrap()
            .is_none());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn list_conversations_excludes_wiki_page_chats() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        let normal = store.create_conversation(Some("普通对话"), None).unwrap();
        let wiki_chat = store
            .create_wiki_chat_conversation("person/张伟", "处理本页")
            .unwrap();
        // 知识页聊天有 wiki_page_slug 关联
        let conv = store.get_conversation(&wiki_chat).unwrap().unwrap();
        assert_eq!(conv.wiki_page_slug.as_deref(), Some("person/张伟"));

        let listed = store.list_conversations().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, normal);
        assert!(listed.iter().all(|c| c.wiki_page_slug.is_none()));

        // 归档列表同样不出现知识页聊天
        store.set_conversation_archived(&normal, true).unwrap();
        let archived = store.list_archived_conversations().unwrap();
        assert!(archived.iter().all(|c| c.wiki_page_slug.is_none()));
        assert_eq!(archived.len(), 1);
        let _ = std::fs::remove_file(path);
    }

    // ── M1：人工编辑保护（human_edited_at + ContentPolicy + opinion）──────────

    fn wiki_draft(slug: &str, kind: &str, content_md: &str) -> WikiPageDraft {
        WikiPageDraft {
            slug: slug.to_string(),
            kind: kind.to_string(),
            title: slug.to_string(),
            summary: "s".to_string(),
            content_md: content_md.to_string(),
            tags: vec![],
            source_event_ids: vec![],
            status: "active".to_string(),
            reason: "test".to_string(),
            source_url: None,
        }
    }

    #[test]
    fn migration_v29_splits_note_kind_and_adds_columns() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        // 预置一条旧语义数据：note- 前缀但 kind=topic（升级前属于「用户粘贴笔记」）
        store
            .upsert_wiki_page(
                &WikiPageDraft {
                    slug: "note-abc".to_string(),
                    kind: "topic".to_string(),
                    title: "旧笔记".to_string(),
                    summary: "s".to_string(),
                    content_md: "旧内容".to_string(),
                    tags: vec![],
                    source_event_ids: vec![],
                    status: "active".to_string(),
                    reason: "test".to_string(),
                    source_url: None,
                },
                ContentPolicy::Always,
            )
            .unwrap();
        drop(store);

        // 重新打开（触发 v29 迁移），存量数据应被修正为 kind='note'
        let store = Store::open(&path).unwrap();
        let page = store.get_wiki_page("note-abc").unwrap().unwrap();
        assert_eq!(page.kind, "note", "note- 前缀旧页应被迁移为 note kind");
        assert_eq!(page.human_edited_at, None);
        assert_eq!(page.opinion, None);
        // 非 note- 前缀的 topic 页不受影响
        store
            .upsert_wiki_page(
                &wiki_draft("topic/subject", "topic", "x"),
                ContentPolicy::Always,
            )
            .unwrap();
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn save_wiki_page_content_sets_human_edited_at_and_rejects_material() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        store
            .upsert_wiki_page(
                &wiki_draft("person/张三", "person", "AI 原始内容"),
                ContentPolicy::Always,
            )
            .unwrap();

        // 正常路径：保存正文 → human_edited_at 置位 + revision 原因带 [human]
        let saved = store
            .save_wiki_page_content("person/张三", "人类修改后的正文", "修正职位", None)
            .unwrap();
        assert!(saved.human_edited_at.is_some());
        assert_eq!(saved.content_md, "人类修改后的正文");
        let (_, revised_content, reason) = store
            .list_wiki_revisions("person/张三")
            .unwrap()
            .first()
            .cloned()
            .unwrap();
        assert_eq!(revised_content, "人类修改后的正文");
        assert!(
            reason.starts_with("[human]"),
            "reason 应带 [human] 前缀: {reason}"
        );
        let log = store.list_wiki_log(5).unwrap();
        assert!(log.iter().any(|(_, e)| e.contains("人工编辑正文")));

        // 空正文拒绝
        assert!(store
            .save_wiki_page_content("person/张三", "   ", "清空", None)
            .is_err());

        // 素材页（source/note）只读拒绝
        store
            .upsert_wiki_page(
                &wiki_draft("tweet-1", "source", "素材内容"),
                ContentPolicy::Always,
            )
            .unwrap();
        assert!(store
            .save_wiki_page_content("tweet-1", "改素材", "不该允许", None)
            .is_err());
        store
            .upsert_wiki_page(
                &wiki_draft("note-x", "note", "笔记内容"),
                ContentPolicy::Always,
            )
            .unwrap();
        assert!(store
            .save_wiki_page_content("note-x", "改笔记", "不该允许", None)
            .is_err());

        // 超过 64k 字符拒绝
        let big = "长".repeat(65537);
        assert!(store
            .save_wiki_page_content("person/张三", &big, "超大", None)
            .is_err());

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn save_wiki_page_content_optimistic_lock_rejects_stale_write() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        store
            .upsert_wiki_page(
                &wiki_draft("person/王五", "person", "AI 原始内容"),
                ContentPolicy::Always,
            )
            .unwrap();

        // 加载时刻的快照 updated_at
        let loaded_at = store
            .get_wiki_page("person/王五")
            .unwrap()
            .unwrap()
            .updated_at;

        // 模拟编辑期间后台 digest 写回：updated_at 变化（直接改库绕过守卫）
        store
            .connection
            .execute(
                "UPDATE wiki_pages SET content_md='digest 新内容', updated_at='2099-01-01T00:00:00Z' WHERE slug='person/王五'",
                [],
            )
            .unwrap();

        // 持旧快照保存 → 冲突拒绝，且不落库
        let err = store
            .save_wiki_page_content("person/王五", "人工修改", "修正", Some(&loaded_at))
            .unwrap_err();
        assert!(err.to_string().contains("编辑冲突"), "应报编辑冲突: {err}");
        assert_eq!(
            store
                .get_wiki_page("person/王五")
                .unwrap()
                .unwrap()
                .content_md,
            "digest 新内容",
            "冲突时不得覆盖后台写入"
        );

        // 持当前快照保存 → 正常通过
        let current_at = store
            .get_wiki_page("person/王五")
            .unwrap()
            .unwrap()
            .updated_at;
        store
            .save_wiki_page_content("person/王五", "人工修改", "修正", Some(&current_at))
            .unwrap();
        assert_eq!(
            store
                .get_wiki_page("person/王五")
                .unwrap()
                .unwrap()
                .content_md,
            "人工修改"
        );

        // 模拟 Dart 往返格式漂移（毫秒精度 + "Z" 后缀）：同一时刻应判定一致。
        // 注意：上面保存成功后 updated_at 已变，需重读当前值再做格式变换。
        let fresh_at = store
            .get_wiki_page("person/王五")
            .unwrap()
            .unwrap()
            .updated_at;
        let reformatted = format!(
            "{}Z",
            chrono::DateTime::parse_from_rfc3339(&fresh_at)
                .unwrap()
                .to_utc()
                .format("%Y-%m-%dT%H:%M:%S%.3f")
        );
        store
            .save_wiki_page_content(
                "person/王五",
                "格式漂移仍应通过",
                "修正",
                Some(&reformatted),
            )
            .unwrap();

        // 不传 expected（None）→ 跳过校验（兼容旧调用方）
        store
            .save_wiki_page_content("person/王五", "无锁保存", "修正", None)
            .unwrap();

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn digest_preserve_respects_human_edited_and_material_pages() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();

        // ① 人工编辑过的档案页：PreserveHumanEdits → 正文不动、证据照累、protected=true
        store
            .upsert_wiki_page(
                &wiki_draft("person/李四", "person", "AI 初稿"),
                ContentPolicy::Always,
            )
            .unwrap();
        store
            .save_wiki_page_content("person/李四", "人工定稿", "人工修正", None)
            .unwrap();
        let outcome = store
            .upsert_wiki_page(
                &WikiPageDraft {
                    slug: "person/李四".to_string(),
                    kind: "person".to_string(),
                    title: "李四（AI 想改名）".to_string(),
                    summary: "AI 摘要".to_string(),
                    content_md: "AI 想覆盖的新内容".to_string(),
                    tags: vec!["ai".to_string()],
                    source_event_ids: vec!["evt-1".to_string(), "evt-2".to_string()],
                    status: "active".to_string(),
                    reason: "digest".to_string(),
                    source_url: None,
                },
                ContentPolicy::PreserveHumanEdits,
            )
            .unwrap();
        assert!(outcome.protected, "人工编辑页应被保护");
        let page = outcome.page;
        assert_eq!(page.content_md, "人工定稿", "正文不可被 digest 覆盖");
        assert_eq!(page.title, "person/李四", "标题不可被 digest 覆盖");
        assert_eq!(page.tags, Vec::<String>::new(), "tags 不可被 digest 覆盖");
        assert_eq!(
            page.source_event_ids,
            vec!["evt-1".to_string(), "evt-2".to_string()],
            "但证据应并集"
        );
        assert_eq!(page.evidence_count, 2);

        // ② 未人工编辑的档案页：PreserveHumanEdits → 正常整篇覆盖
        store
            .upsert_wiki_page(
                &wiki_draft("topic/新主题", "topic", "AI 第一版"),
                ContentPolicy::Always,
            )
            .unwrap();
        let outcome2 = store
            .upsert_wiki_page(
                &wiki_draft("topic/新主题", "topic", "AI 第二版"),
                ContentPolicy::PreserveHumanEdits,
            )
            .unwrap();
        assert!(!outcome2.protected);
        assert_eq!(outcome2.page.content_md, "AI 第二版");

        // ③ 素材页：PreserveHumanEdits → 永不覆盖（采集快照只读）
        store
            .upsert_wiki_page(
                &wiki_draft("tweet-9", "source", "原始素材"),
                ContentPolicy::Always,
            )
            .unwrap();
        let outcome3 = store
            .upsert_wiki_page(
                &wiki_draft("tweet-9", "source", "新素材内容"),
                ContentPolicy::PreserveHumanEdits,
            )
            .unwrap();
        assert!(outcome3.protected);
        assert_eq!(outcome3.page.content_md, "原始素材", "素材正文对 AI 只读");

        // ④ Always 策略 = 素材导入流程：允许刷新素材内容（仅所有者可写）
        let outcome4 = store
            .upsert_wiki_page(
                &wiki_draft("tweet-9", "source", "导入流程刷新"),
                ContentPolicy::Always,
            )
            .unwrap();
        assert!(!outcome4.protected);
        assert_eq!(outcome4.page.content_md, "导入流程刷新");

        // ⑤ Always 策略 = 确认制修订（save_wiki_revision：AI 草拟 → 用户确认后才落库）：
        //    人工编辑页也可被覆盖——这是文档承诺的修订通道，区别于 digest 的静默覆盖
        let outcome5 = store
            .upsert_wiki_page(
                &wiki_draft("person/李四", "person", "用户确认后的修订版本"),
                ContentPolicy::Always,
            )
            .unwrap();
        assert!(!outcome5.protected);
        assert_eq!(outcome5.page.content_md, "用户确认后的修订版本");
        assert!(
            outcome5.page.human_edited_at.is_some(),
            "确认制修订不触碰 human_edited_at 列：曾被人动过的事实开关永久保留"
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn set_wiki_opinion_only_on_material_and_audits() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        store
            .upsert_wiki_page(
                &wiki_draft("tweet-8", "source", "素材"),
                ContentPolicy::Always,
            )
            .unwrap();

        // 认可
        let p = store.set_wiki_opinion("tweet-8", Some("endorse")).unwrap();
        assert_eq!(p.opinion.as_deref(), Some("endorse"));

        // 不认可
        let p = store.set_wiki_opinion("tweet-8", Some("reject")).unwrap();
        assert_eq!(p.opinion.as_deref(), Some("reject"));

        // 清空（未表态）
        let p = store.set_wiki_opinion("tweet-8", None).unwrap();
        assert_eq!(p.opinion, None);

        // 非法值拒绝
        assert!(store.set_wiki_opinion("tweet-8", Some("meh")).is_err());

        // 非素材页拒绝（即使人工编辑过也一样）
        store
            .upsert_wiki_page(
                &wiki_draft("person/王五", "person", "x"),
                ContentPolicy::Always,
            )
            .unwrap();
        assert!(store
            .set_wiki_opinion("person/王五", Some("endorse"))
            .is_err());

        // 审计日志
        let log = store.list_wiki_log(10).unwrap();
        assert!(log.iter().any(|(_, e)| e.contains("素材评价")));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn update_project_path_validates_and_backfills_legacy_pages() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        // 存量页：只有正文快照、没有 source_url
        let dir_a = std::env::temp_dir().join("elsewhen-proj-a");
        let dir_b = std::env::temp_dir().join("elsewhen-proj-b");
        std::fs::create_dir_all(&dir_a).unwrap();
        std::fs::create_dir_all(&dir_b).unwrap();
        let content = format!(
            "# 资产档案：A\n\n## 资产边界\n- 项目目录：`{}`\n",
            dir_a.display()
        );
        store
            .upsert_wiki_page(&wiki_draft("project/a", "project", &content), ContentPolicy::Always)
            .unwrap();
        // 新 open 触发幂等回填
        drop(store);
        let store = Store::open(&path).unwrap();
        let page = store.get_wiki_page("project/a").unwrap().unwrap();
        let url = page.source_url.clone().unwrap();
        assert!(url.starts_with("file://"));
        // 回填按正文原样记录（不做 canonicalize，目录可能已搬走）
        assert_eq!(crate::wiki::file_url_to_path(&url).unwrap(), dir_a);
        // 二次 open 不再重复写 updated_at（幂等无副作用）
        let updated_before = page.updated_at.clone();
        drop(store);
        let store = Store::open(&path).unwrap();
        let page2 = store.get_wiki_page("project/a").unwrap().unwrap();
        assert_eq!(page2.updated_at, updated_before);
        // 改路径：新目录必须存在
        assert!(store
            .update_project_path("project/a", "/definitely/not/here")
            .is_err());
        let moved = store
            .update_project_path("project/a", &dir_b.display().to_string())
            .unwrap();
        let new_url = moved.source_url.clone().unwrap();
        assert!(new_url.starts_with("file://"));
        assert_eq!(
            crate::wiki::file_url_to_path(&new_url).unwrap(),
            dir_b.canonicalize().unwrap()
        );
        // 正文快照行同步更新
        assert!(moved.content_md.contains(&dir_b.display().to_string()));
        assert!(!moved.content_md.contains(&dir_a.display().to_string()));
        // 非项目页拒绝
        store
            .upsert_wiki_page(&wiki_draft("person/王五", "person", "x"), ContentPolicy::Always)
            .unwrap();
        assert!(store
            .update_project_path("person/王五", &dir_b.display().to_string())
            .is_err());
        std::fs::remove_dir_all(&dir_a).ok();
        std::fs::remove_dir_all(&dir_b).ok();
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn digest_protection_does_not_record_revisions_or_opinion_changes() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        store
            .upsert_wiki_page(
                &wiki_draft("person/赵六", "person", "v1 AI"),
                ContentPolicy::Always,
            )
            .unwrap();
        store
            .save_wiki_page_content("person/赵六", "v1 人工", "修正", None)
            .unwrap();
        let revs_before = store.list_wiki_revisions("person/赵六").unwrap().len();

        // 受保护写回：不追加 revision（内容没变，纯证据累加）
        let outcome = store
            .upsert_wiki_page(
                &WikiPageDraft {
                    slug: "person/赵六".to_string(),
                    kind: "person".to_string(),
                    title: "赵六".to_string(),
                    summary: "s".to_string(),
                    content_md: "AI 想覆盖".to_string(),
                    tags: vec![],
                    source_event_ids: vec!["evt-x".to_string()],
                    status: "active".to_string(),
                    reason: "digest".to_string(),
                    source_url: None,
                },
                ContentPolicy::PreserveHumanEdits,
            )
            .unwrap();
        assert!(outcome.protected);
        let revs_after = store.list_wiki_revisions("person/赵六").unwrap().len();
        assert_eq!(revs_before, revs_after, "保护降级写回不应追加 revision");

        // opinion / human_edited_at 不受 upsert 影响
        store
            .upsert_wiki_page(
                &wiki_draft("tweet-7", "source", "素材"),
                ContentPolicy::Always,
            )
            .unwrap();
        store.set_wiki_opinion("tweet-7", Some("endorse")).unwrap();
        let before = store.get_wiki_page("tweet-7").unwrap().unwrap();
        assert!(before.opinion.is_some());
        store
            .upsert_wiki_page(
                &wiki_draft("tweet-7", "source", "改不了"),
                ContentPolicy::PreserveHumanEdits,
            )
            .unwrap();
        let after = store.get_wiki_page("tweet-7").unwrap().unwrap();
        assert_eq!(
            after.opinion,
            Some("endorse".to_string()),
            "评价不被 digest 清掉"
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn message_ordering_is_deterministic_on_same_timestamp() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        let conversation_id = store.create_conversation(None, None).unwrap();
        // 两条消息刻意用完全相同的 created_at（旧实现没有 id tiebreak，
        // 同毫秒时 SQLite 返回顺序不确定，跨调用会翻转）。
        let same_ts = "2026-09-25T10:00:00.000000000Z";
        for (index, (id, content)) in [("msg-tie-2", "b"), ("msg-tie-1", "a")]
            .into_iter()
            .enumerate()
        {
            store
                .connection
                .execute(
                    "INSERT INTO messages (id, conversation_id, parent_message_id, role, content, created_at)
                     VALUES (?1, ?2, NULL, 'user', ?3, ?4)",
                    params![id, conversation_id, content, same_ts],
                )
                .unwrap();
        }
        // list_messages / get_child_messages 都应按 id ASC 破平，顺序稳定。
        let messages = store.list_messages(&conversation_id).unwrap();
        let ids: Vec<&str> = messages.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["msg-tie-1", "msg-tie-2"],
            "同时间戳消息按 id ASC 稳定排序"
        );
        let child = store.get_child_messages("msg-tie-1").unwrap();
        assert!(child.is_empty());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn migration_v29_recovers_when_only_second_column_is_missing() {
        let path = temporary_database();
        // 完整迁移建库（human_edited_at 与 opinion 两列都在）
        {
            let store = Store::open(&path).unwrap();
            store
                .upsert_wiki_page(&wiki_draft("note-abc", "topic", "x"), ContentPolicy::Always)
                .unwrap();
        }
        // 模拟 v29 中断：human_edited_at 已加、opinion 未加（旧实现用 execute_batch
        // 一次性 ALTER 两列，中途崩溃会留下只加了第一列的死状态；且守卫只看第一列，
        // 下次 open 永久跳过第二列 → 所有 wiki SELECT 报 no such column）。
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute("ALTER TABLE wiki_pages DROP COLUMN opinion", [])
            .unwrap();
        // 模拟中断：v29 尚未落记录（旧实现中 ALTER 与版本记录在同一条
        // execute_batch 里，两列间崩溃则两者都未提交）。
        conn.execute("DELETE FROM schema_migrations WHERE version = 29", [])
            .unwrap();
        drop(conn);

        // 重新打开：第二列应被单独补上，不再被第一列的存在性挡住。
        let store = Store::open(&path).unwrap();
        let has_opinion = {
            let mut statement = store
                .connection
                .prepare("PRAGMA table_info(wiki_pages)")
                .unwrap();
            statement
                .query_map([], |row| row.get::<_, String>(1))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap()
                .iter()
                .any(|name| name == "opinion")
        };
        assert!(has_opinion, "中断后重开应补齐 opinion 列");
        // 补齐后正常读取 wiki 页不再报错
        let page = store.get_wiki_page("note-abc").unwrap().unwrap();
        assert_eq!(page.opinion, None);
        let _ = std::fs::remove_file(path);
    }
}
