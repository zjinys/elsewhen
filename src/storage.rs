mod adapter;

use crate::event::{EventSummary, NewEvent};
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use std::path::Path;
use uuid::Uuid;

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

/// upsert 结果
#[derive(Debug, Clone)]
pub struct WikiUpsertOutcome {
    pub created: bool,
    pub page: WikiPage,
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

fn map_input_record(row: &rusqlite::Row) -> rusqlite::Result<InputRecord> {
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

fn map_wiki_page(row: &rusqlite::Row) -> rusqlite::Result<WikiPage> {
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
    })
}

/// wiki_pages 行 → WikiPage 的公共列清单。
/// 顺序必须与 `map_wiki_page` 的按位取值（0..=17）严格一致。
const WIKI_PAGE_COLS: &str = "id, slug, kind, title, summary, content_md, tags, source_event_ids, \
     evidence_count, first_seen_at, last_seen_at, status, created_at, updated_at, source_url, \
     COALESCE(area, 'insight'), based_on, content_type";

pub struct Store {
    connection: Connection,
    path: std::path::PathBuf,
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
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             CREATE TABLE IF NOT EXISTS schema_migrations (
               version INTEGER PRIMARY KEY,
               applied_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS events (
               id TEXT PRIMARY KEY,
               occurred_at TEXT NOT NULL,
               recorded_at TEXT NOT NULL,
               processed_at TEXT,
               raw_text TEXT NOT NULL CHECK (length(trim(raw_text)) > 0),
               source TEXT NOT NULL,
               status TEXT NOT NULL DEFAULT 'pending',
               created_at TEXT NOT NULL,
               updated_at TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_events_recorded_at ON events(recorded_at);
             CREATE INDEX IF NOT EXISTS idx_events_status ON events(status);
             CREATE TRIGGER IF NOT EXISTS prevent_raw_event_mutation
             BEFORE UPDATE OF raw_text, recorded_at, source ON events
             BEGIN
               SELECT RAISE(ABORT, 'raw_event_is_immutable');
             END;
             INSERT OR IGNORE INTO schema_migrations(version, applied_at)
             VALUES (1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
             CREATE TABLE IF NOT EXISTS analysis_jobs (
               id TEXT PRIMARY KEY, event_id TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'pending',
               attempts INTEGER NOT NULL DEFAULT 0, last_error TEXT, available_at TEXT NOT NULL,
               created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
               FOREIGN KEY(event_id) REFERENCES events(id), UNIQUE(event_id)
             );
             CREATE INDEX IF NOT EXISTS idx_analysis_jobs_ready ON analysis_jobs(status, available_at);
             CREATE TABLE IF NOT EXISTS event_analyses (
               id TEXT PRIMARY KEY, event_id TEXT NOT NULL, prompt_version TEXT NOT NULL,
               result_json TEXT NOT NULL, created_at TEXT NOT NULL,
               FOREIGN KEY(event_id) REFERENCES events(id)
             );
             INSERT OR IGNORE INTO schema_migrations(version, applied_at)
             VALUES (2, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
             CREATE TABLE IF NOT EXISTS ai_provider_configs (
               id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE,
               provider_type TEXT NOT NULL, base_url TEXT NOT NULL, model TEXT NOT NULL,
               api_key_source TEXT NOT NULL, api_key TEXT,
               enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0,1)),
               created_at TEXT NOT NULL, updated_at TEXT NOT NULL
             );
             INSERT OR IGNORE INTO schema_migrations(version, applied_at)
             VALUES (3, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
             CREATE TABLE IF NOT EXISTS conversations (
               id TEXT PRIMARY KEY,
               title TEXT,
               tag TEXT CHECK(tag IN ('diary', 'idea', 'discussion', 'general')),
               created_at TEXT NOT NULL,
               updated_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS messages (
               id TEXT PRIMARY KEY,
               conversation_id TEXT NOT NULL,
               parent_message_id TEXT,
               role TEXT NOT NULL CHECK(role IN ('user', 'assistant')),
               content TEXT NOT NULL,
               created_at TEXT NOT NULL,
               FOREIGN KEY(conversation_id) REFERENCES conversations(id) ON DELETE CASCADE,
               FOREIGN KEY(parent_message_id) REFERENCES messages(id) ON DELETE SET NULL
             );
             CREATE INDEX IF NOT EXISTS idx_messages_conversation ON messages(conversation_id, created_at);
             INSERT OR IGNORE INTO schema_migrations(version, applied_at)
             VALUES (4, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
             CREATE TABLE IF NOT EXISTS insights (
               id TEXT PRIMARY KEY,
               created_at TEXT NOT NULL,
               window_days INTEGER NOT NULL,
               prompt_version TEXT NOT NULL,
               lens TEXT NOT NULL,
               title TEXT NOT NULL,
               observation TEXT NOT NULL,
               related_raw TEXT NOT NULL DEFAULT '[]',
               action TEXT,
               status TEXT NOT NULL DEFAULT 'new'
             );
             CREATE INDEX IF NOT EXISTS idx_insights_created_at ON insights(created_at);
             INSERT OR IGNORE INTO schema_migrations(version, applied_at)
             VALUES (5, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
             CREATE TABLE IF NOT EXISTS app_meta (
               key TEXT PRIMARY KEY,
               value TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS wiki_pages (
               id TEXT PRIMARY KEY,
               slug TEXT NOT NULL UNIQUE,
               kind TEXT NOT NULL,
               title TEXT NOT NULL,
               summary TEXT NOT NULL DEFAULT '',
               content_md TEXT NOT NULL,
               tags TEXT NOT NULL DEFAULT '[]',
               source_event_ids TEXT NOT NULL DEFAULT '[]',
               evidence_count INTEGER NOT NULL DEFAULT 1,
               first_seen_at TEXT NOT NULL,
               last_seen_at TEXT NOT NULL,
               status TEXT NOT NULL DEFAULT 'active',
               created_at TEXT NOT NULL,
               updated_at TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_wiki_pages_kind ON wiki_pages(kind);
             CREATE TABLE IF NOT EXISTS wiki_revisions (
               id TEXT PRIMARY KEY,
               page_id TEXT NOT NULL,
               content_md TEXT NOT NULL,
               reason TEXT NOT NULL,
               source_event_id TEXT,
               created_at TEXT NOT NULL,
               FOREIGN KEY(page_id) REFERENCES wiki_pages(id) ON DELETE CASCADE
             );
             CREATE INDEX IF NOT EXISTS idx_wiki_revisions_page ON wiki_revisions(page_id, created_at);
             CREATE TABLE IF NOT EXISTS wiki_log (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               ts TEXT NOT NULL,
               entry TEXT NOT NULL
             );
             INSERT OR IGNORE INTO schema_migrations(version, applied_at)
             VALUES (6, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
              CREATE TABLE IF NOT EXISTS token_usage (
                id TEXT PRIMARY KEY,
                conversation_id TEXT,
                prompt_tokens INTEGER NOT NULL DEFAULT 0,
                completion_tokens INTEGER NOT NULL DEFAULT 0,
                total_tokens INTEGER NOT NULL DEFAULT 0,
                model TEXT,
                created_at TEXT NOT NULL,
                FOREIGN KEY(conversation_id) REFERENCES conversations(id) ON DELETE SET NULL
              );
              CREATE INDEX IF NOT EXISTS idx_token_usage_created ON token_usage(created_at);
              INSERT OR IGNORE INTO schema_migrations(version, applied_at)
              VALUES (7, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));",
        )?;
        // 版本 21：人物 / 项目 / 主题的最小结构化事实层，来源事件不可省略。
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS entity_facts (
               id TEXT PRIMARY KEY,
               entity_kind TEXT NOT NULL CHECK(entity_kind IN ('person','project','topic')),
               entity_slug TEXT NOT NULL,
               fact_text TEXT NOT NULL CHECK(length(trim(fact_text)) > 0),
               occurred_at TEXT NOT NULL,
               confidence INTEGER NOT NULL CHECK(confidence BETWEEN 0 AND 5),
               source_event_id TEXT NOT NULL,
               created_at TEXT NOT NULL,
               last_seen_at TEXT NOT NULL,
               FOREIGN KEY(source_event_id) REFERENCES events(id),
               UNIQUE(entity_kind, entity_slug, fact_text, source_event_id)
             );
             CREATE INDEX IF NOT EXISTS idx_entity_facts_entity
               ON entity_facts(entity_kind, entity_slug, occurred_at DESC);
             CREATE INDEX IF NOT EXISTS idx_entity_facts_source
               ON entity_facts(source_event_id);
             INSERT OR IGNORE INTO schema_migrations(version, applied_at)
             VALUES (21, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));",
        )?;
        // 版本 22：关系可选关联真实事件，避免把会话本身伪装成事实来源。
        let relations_table_exists: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='relations')",
            [],
            |row| row.get(0),
        )?;
        let has_relation_event = relations_table_exists && {
            let mut statement = connection.prepare("PRAGMA table_info(relations)")?;
            let columns = statement
                .query_map([], |row| row.get::<_, String>(1))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            columns.iter().any(|name| name == "source_event_id")
        };
        if relations_table_exists && !has_relation_event {
            connection.execute_batch(
                "ALTER TABLE relations ADD COLUMN source_event_id TEXT;
                 INSERT OR IGNORE INTO schema_migrations(version, applied_at)
                 VALUES (22, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));",
            )?;
        }
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS entity_aliases (
               id TEXT PRIMARY KEY,
               entity_kind TEXT NOT NULL CHECK(entity_kind IN ('person','project','topic')),
               entity_slug TEXT NOT NULL,
               alias TEXT NOT NULL CHECK(length(trim(alias)) > 0),
               created_at TEXT NOT NULL,
               UNIQUE(entity_kind, entity_slug, alias)
             );
             CREATE INDEX IF NOT EXISTS idx_entity_aliases_lookup ON entity_aliases(alias);
             INSERT OR IGNORE INTO schema_migrations(version, applied_at)
             VALUES (23, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));",
        )?;
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS entity_merges (
               id TEXT PRIMARY KEY,
               entity_kind TEXT NOT NULL,
               source_slug TEXT NOT NULL,
               target_slug TEXT NOT NULL,
               created_at TEXT NOT NULL,
               undone_at TEXT
             );
             INSERT OR IGNORE INTO schema_migrations(version, applied_at)
             VALUES (24, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));",
        )?;
        let has_merge_undone_at = {
            let mut statement = connection.prepare("PRAGMA table_info(entity_merges)")?;
            let columns = statement
                .query_map([], |row| row.get::<_, String>(1))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            columns.iter().any(|name| name == "undone_at")
        };
        if !has_merge_undone_at {
            connection.execute("ALTER TABLE entity_merges ADD COLUMN undone_at TEXT", [])?;
        }
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS entity_merge_snapshots (
               merge_id TEXT NOT NULL,
               table_name TEXT NOT NULL,
               row_id TEXT NOT NULL,
               disposition TEXT NOT NULL DEFAULT 'moved' CHECK(disposition IN ('moved','deduplicated')),
               payload TEXT NOT NULL,
               PRIMARY KEY(merge_id, table_name, row_id),
               FOREIGN KEY(merge_id) REFERENCES entity_merges(id) ON DELETE CASCADE
             );
             INSERT OR IGNORE INTO schema_migrations(version, applied_at)
             VALUES (25, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));",
        )?;
        let has_merge_disposition = {
            let mut statement = connection.prepare("PRAGMA table_info(entity_merge_snapshots)")?;
            let columns = statement
                .query_map([], |row| row.get::<_, String>(1))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            columns.iter().any(|name| name == "disposition")
        };
        if !has_merge_disposition {
            connection.execute_batch(
                "ALTER TABLE entity_merge_snapshots ADD COLUMN disposition TEXT NOT NULL DEFAULT 'moved';",
            )?;
        }
        connection.execute(
            "INSERT OR IGNORE INTO schema_migrations(version, applied_at) VALUES (26, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
            [],
        )?;
        let merge_schema: String = connection.query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='entity_merges'",
            [],
            |row| row.get(0),
        )?;
        if merge_schema
            .replace(' ', "")
            .contains("UNIQUE(entity_kind,source_slug)")
        {
            connection.execute_batch(
                "PRAGMA foreign_keys=OFF;
                 BEGIN IMMEDIATE;
                 CREATE TABLE entity_merges_new (
                   id TEXT PRIMARY KEY, entity_kind TEXT NOT NULL, source_slug TEXT NOT NULL,
                   target_slug TEXT NOT NULL, created_at TEXT NOT NULL, undone_at TEXT
                 );
                 INSERT INTO entity_merges_new SELECT id,entity_kind,source_slug,target_slug,created_at,undone_at FROM entity_merges;
                 CREATE TABLE entity_merge_snapshots_new (
                   merge_id TEXT NOT NULL, table_name TEXT NOT NULL, row_id TEXT NOT NULL,
                   disposition TEXT NOT NULL DEFAULT 'moved', payload TEXT NOT NULL,
                   PRIMARY KEY(merge_id,table_name,row_id),
                   FOREIGN KEY(merge_id) REFERENCES entity_merges_new(id) ON DELETE CASCADE
                 );
                 INSERT INTO entity_merge_snapshots_new SELECT merge_id,table_name,row_id,disposition,payload FROM entity_merge_snapshots;
                 DROP TABLE entity_merge_snapshots;
                 DROP TABLE entity_merges;
                 ALTER TABLE entity_merges_new RENAME TO entity_merges;
                 ALTER TABLE entity_merge_snapshots_new RENAME TO entity_merge_snapshots;
                 COMMIT;
                 PRAGMA foreign_keys=ON;",
            )?;
        }
        connection.execute(
            "INSERT OR IGNORE INTO schema_migrations(version, applied_at) VALUES (27, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
            [],
        )?;
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS event_recordability_decisions (
               id TEXT PRIMARY KEY,
               event_id TEXT NOT NULL,
               recordable INTEGER NOT NULL CHECK(recordable IN (0,1)),
               kind TEXT NOT NULL CHECK(kind IN ('event','discussion','chitchat','meta')),
               reason TEXT NOT NULL,
               created_at TEXT NOT NULL,
               FOREIGN KEY(event_id) REFERENCES events(id)
             );
             CREATE INDEX IF NOT EXISTS idx_event_recordability_latest
               ON event_recordability_decisions(event_id, created_at DESC, id DESC);
             INSERT OR IGNORE INTO schema_migrations(version, applied_at)
             VALUES (28, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));",
        )?;
        let has_api_key = {
            let mut statement = connection.prepare("PRAGMA table_info(ai_provider_configs)")?;
            let columns = statement
                .query_map([], |row| row.get::<_, String>(1))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            columns.iter().any(|name| name == "api_key")
        };
        if !has_api_key {
            connection.execute(
                "ALTER TABLE ai_provider_configs ADD COLUMN api_key TEXT",
                [],
            )?;
        }
        // Add tag column to conversations if it doesn't exist
        let has_tag = {
            let mut statement = connection.prepare("PRAGMA table_info(conversations)")?;
            let columns = statement
                .query_map([], |row| row.get::<_, String>(1))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            columns.iter().any(|name| name == "tag")
        };
        if !has_tag {
            connection.execute(
                "ALTER TABLE conversations ADD COLUMN tag TEXT CHECK(tag IN ('diary', 'idea', 'discussion', 'general'))",
                [],
            )?;
        }
        // Add parent_message_id to messages if it doesn't exist
        let has_parent = {
            let mut statement = connection.prepare("PRAGMA table_info(messages)")?;
            let columns = statement
                .query_map([], |row| row.get::<_, String>(1))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            columns.iter().any(|name| name == "parent_message_id")
        };
        if !has_parent {
            connection.execute(
                "ALTER TABLE messages ADD COLUMN parent_message_id TEXT REFERENCES messages(id) ON DELETE SET NULL",
                [],
            )?;
        }
        // Always (re)create the index: for fresh DBs the column exists after
        // CREATE TABLE, for old DBs after the ALTER TABLE above.
        connection.execute(
            "CREATE INDEX IF NOT EXISTS idx_messages_parent ON messages(parent_message_id)",
            [],
        )?;
        // Add archived column to conversations if it doesn't exist
        let has_archived = {
            let mut statement = connection.prepare("PRAGMA table_info(conversations)")?;
            let columns = statement
                .query_map([], |row| row.get::<_, String>(1))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            columns.iter().any(|name| name == "archived")
        };
        if !has_archived {
            connection.execute(
                "ALTER TABLE conversations ADD COLUMN archived INTEGER NOT NULL DEFAULT 0 CHECK(archived IN (0,1))",
                [],
            )?;
        }
        // 版本 9：为历史无标题对话回填「首条用户消息」生成的标题
        let v9_pending = {
            let mut statement =
                connection.prepare("SELECT COUNT(*) FROM schema_migrations WHERE version = 9")?;
            statement.query_row([], |row| row.get::<_, i64>(0))?
        };
        if v9_pending == 0 {
            let untitled_ids = {
                let mut statement = connection.prepare(
                    "SELECT c.id FROM conversations c
                     WHERE (c.title IS NULL OR trim(c.title) = '')
                       AND EXISTS (SELECT 1 FROM messages m
                                   WHERE m.conversation_id = c.id AND m.role = 'user')",
                )?;
                let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
                rows.collect::<rusqlite::Result<Vec<_>>>()?
            };
            for id in &untitled_ids {
                let first_user: Option<String> = connection
                    .query_row(
                        "SELECT m.content FROM messages m
                         WHERE m.conversation_id = ?1 AND m.role = 'user'
                         ORDER BY m.created_at ASC LIMIT 1",
                        [id],
                        |row| row.get(0),
                    )
                    .optional()?;
                if let Some(content) = first_user {
                    if let Some(title) = derive_conversation_title(&content) {
                        connection.execute(
                            "UPDATE conversations SET title = ?1 WHERE id = ?2",
                            params![title, id],
                        )?;
                    }
                }
            }
            connection.execute(
                "INSERT OR IGNORE INTO schema_migrations(version, applied_at)
                 VALUES (9, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                [],
            )?;
        }
        // 版本 10：个人经验规则库（rules 表）
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS rules (
               id TEXT PRIMARY KEY,
               content TEXT NOT NULL CHECK (length(trim(content)) > 0),
               status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','pending')),
               created_at TEXT NOT NULL,
               updated_at TEXT NOT NULL
             );
             INSERT OR IGNORE INTO schema_migrations(version, applied_at)
             VALUES (10, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));",
        )?;
        // 版本 11：待确认动作（AI 写类工具的确认门：草拟 → 用户确认 → 执行）
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS pending_actions (
               id TEXT PRIMARY KEY,
               conversation_id TEXT NOT NULL,
               action TEXT NOT NULL,
               args_json TEXT NOT NULL,
               status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','done','declined')),
               created_at TEXT NOT NULL,
               FOREIGN KEY(conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
             );
             CREATE INDEX IF NOT EXISTS idx_pending_actions_conv
               ON pending_actions(conversation_id, status, created_at);
             INSERT OR IGNORE INTO schema_migrations(version, applied_at)
             VALUES (11, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));",
        )?;
        // 版本 12：AI provider 多配置 + 单激活。
        // is_active 为唯一激活标记（partial unique index 强制最多一条=1）；
        // temperature / max_tokens 随配置保存，对话生成时读取。
        {
            let has_active = {
                let mut statement = connection.prepare("PRAGMA table_info(ai_provider_configs)")?;
                let columns = statement
                    .query_map([], |row| row.get::<_, String>(1))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                columns.iter().any(|name| name == "is_active")
            };
            if !has_active {
                connection.execute_batch(
                    "ALTER TABLE ai_provider_configs ADD COLUMN is_active INTEGER NOT NULL DEFAULT 0;
                     ALTER TABLE ai_provider_configs ADD COLUMN temperature REAL NOT NULL DEFAULT 0.7;
                     ALTER TABLE ai_provider_configs ADD COLUMN max_tokens INTEGER;
                     CREATE UNIQUE INDEX IF NOT EXISTS idx_ai_provider_configs_single_active
                       ON ai_provider_configs(is_active) WHERE is_active=1;
                     UPDATE ai_provider_configs SET is_active=1
                       WHERE id=(SELECT id FROM ai_provider_configs
                                 ORDER BY updated_at DESC, created_at DESC LIMIT 1)
                         AND NOT EXISTS (SELECT 1 FROM ai_provider_configs WHERE is_active=1);
                     INSERT OR IGNORE INTO schema_migrations(version, applied_at)
                     VALUES (12, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));",
                )?;
            }
        }
        // 版本 13：规则关联提出它的会话。
        // 确认门按会话隔离——「好」只转正本会话的待确认规则，
        // 中间穿插其他消息也可能误删，后续确认逻辑据此按会话处理。
        {
            let has_conv = {
                let mut statement = connection.prepare("PRAGMA table_info(rules)")?;
                let columns = statement
                    .query_map([], |row| row.get::<_, String>(1))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                columns.iter().any(|name| name == "conversation_id")
            };
            if !has_conv {
                connection.execute_batch(
                    "ALTER TABLE rules ADD COLUMN conversation_id TEXT;
                     INSERT OR IGNORE INTO schema_migrations(version, applied_at)
                     VALUES (13, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));",
                )?;
            }
        }
        // 版本 14：个人待办（AI 提议 + 用户确认后创建，也可手动建）。
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS todos (
               id TEXT PRIMARY KEY,
               title TEXT NOT NULL CHECK(length(trim(title)) > 0),
               status TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open','done','archived')),
               priority TEXT NOT NULL DEFAULT 'normal' CHECK(priority IN ('high','normal','low')),
               due_at TEXT,
               related_event_id TEXT,
               related_wiki_slug TEXT,
               note TEXT,
               created_at TEXT NOT NULL,
               updated_at TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_todos_status ON todos(status, created_at);
             INSERT OR IGNORE INTO schema_migrations(version, applied_at)
             VALUES (14, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));",
        )?;
        // 版本 15：知识页来源链接（URL 导入页记录出处）
        {
            let has_src = {
                let mut statement = connection.prepare("PRAGMA table_info(wiki_pages)")?;
                let columns = statement
                    .query_map([], |row| row.get::<_, String>(1))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                columns.iter().any(|name| name == "source_url")
            };
            if !has_src {
                connection.execute_batch(
                    "ALTER TABLE wiki_pages ADD COLUMN source_url TEXT;
                     INSERT OR IGNORE INTO schema_migrations(version, applied_at)
                     VALUES (15, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));",
                )?;
            }
        }
        // 版本 16：对话可选关联一个知识页（页内 AI 处理会话）
        {
            let has_wiki = {
                let mut statement = connection.prepare("PRAGMA table_info(conversations)")?;
                let columns = statement
                    .query_map([], |row| row.get::<_, String>(1))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                columns.iter().any(|name| name == "wiki_page_slug")
            };
            if !has_wiki {
                connection.execute_batch(
                    "ALTER TABLE conversations ADD COLUMN wiki_page_slug TEXT;
                     INSERT OR IGNORE INTO schema_migrations(version, applied_at)
                     VALUES (16, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));",
                )?;
            }
        }
        // 版本 17：人物关系（AI 从对话识别「人 ↔ 事情/项目」，用户确认后保存）
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS relations (
               id TEXT PRIMARY KEY,
               from_slug TEXT NOT NULL,
               from_kind TEXT NOT NULL,
               to_slug TEXT NOT NULL,
               to_kind TEXT NOT NULL,
               relation TEXT NOT NULL,
               note TEXT,
               confidence INTEGER NOT NULL DEFAULT 3,
               source_conversation_id TEXT,
               source_event_id TEXT,
               created_at TEXT NOT NULL,
               last_seen_at TEXT NOT NULL,
               UNIQUE(from_slug, to_slug, relation)
             );
             CREATE INDEX IF NOT EXISTS idx_relations_from ON relations(from_slug);
             CREATE INDEX IF NOT EXISTS idx_relations_to ON relations(to_slug);
             INSERT OR IGNORE INTO schema_migrations(version, applied_at)
             VALUES (17, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));",
        )?;
        // 版本 18：知识页按「来源/用途」分区（area）。
        // imported=素材库（推文/网页/粘贴文本，原文锁定）、network=人物/项目关系网、
        // insight=知识沉淀（对话提炼的结论/规则）、derivative=对某页加工出的派生产物（不进主列表）。
        // 历史数据按 slug 前缀 / kind / 来源回填。
        {
            let has_area = {
                let mut statement = connection.prepare("PRAGMA table_info(wiki_pages)")?;
                let columns = statement
                    .query_map([], |row| row.get::<_, String>(1))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                columns.iter().any(|name| name == "area")
            };
            if !has_area {
                let tx = connection.unchecked_transaction()?;
                tx.execute_batch(
                    "ALTER TABLE wiki_pages ADD COLUMN area TEXT;
                     ALTER TABLE wiki_pages ADD COLUMN based_on TEXT;
                     ALTER TABLE wiki_pages ADD COLUMN content_type TEXT;
                     UPDATE wiki_pages SET area = CASE
                       WHEN slug LIKE 'person/%' OR slug LIKE 'topic/%' THEN 'network'
                       WHEN kind = 'source' OR slug LIKE 'tweet-%' OR slug LIKE 'note-%'
                            OR slug LIKE 'import-%' OR source_url IS NOT NULL THEN 'imported'
                       ELSE 'insight' END
                     WHERE area IS NULL;
                     INSERT OR IGNORE INTO schema_migrations(version, applied_at)
                     VALUES (18, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));",
                )?;
                tx.commit()?;
            }
        }
        // 版本 19：统一输入关联层。原始提交先落盘，再异步路由到已有权威对象。
        // 表只记录关联，不复制这些对象的业务状态。
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS input_records (
               id TEXT PRIMARY KEY,
               raw_text TEXT NOT NULL CHECK(length(trim(raw_text)) > 0),
               source TEXT NOT NULL,
               route_status TEXT NOT NULL DEFAULT 'pending'
                 CHECK(route_status IN ('pending','routed','needs_confirmation','failed')),
               idempotency_key TEXT,
               event_id TEXT,
               message_id TEXT,
               wiki_page_slug TEXT,
               todo_id TEXT,
               created_at TEXT NOT NULL,
               updated_at TEXT NOT NULL,
               FOREIGN KEY(event_id) REFERENCES events(id),
               FOREIGN KEY(message_id) REFERENCES messages(id),
               FOREIGN KEY(todo_id) REFERENCES todos(id)
             );
             CREATE UNIQUE INDEX IF NOT EXISTS idx_input_records_idempotency
               ON input_records(idempotency_key) WHERE idempotency_key IS NOT NULL;
             CREATE INDEX IF NOT EXISTS idx_input_records_created_at
               ON input_records(created_at);
             INSERT OR IGNORE INTO schema_migrations(version, applied_at)
             VALUES (19, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));",
        )?;
        // 版本 20：每日总结按版本追加，来源通过独立关联表显式引用原始事件。
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS daily_reviews (
               id TEXT PRIMARY KEY,
               review_date TEXT NOT NULL,
               prompt_version TEXT NOT NULL,
               result_json TEXT NOT NULL,
               created_at TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_daily_reviews_date
               ON daily_reviews(review_date, created_at);
             CREATE TABLE IF NOT EXISTS daily_review_sources (
               review_id TEXT NOT NULL,
               event_id TEXT NOT NULL,
               PRIMARY KEY(review_id, event_id),
               FOREIGN KEY(review_id) REFERENCES daily_reviews(id) ON DELETE CASCADE,
               FOREIGN KEY(event_id) REFERENCES events(id)
             );
             INSERT OR IGNORE INTO schema_migrations(version, applied_at)
             VALUES (20, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));",
        )?;
        connection.execute(
            "INSERT OR IGNORE INTO schema_migrations(version, applied_at)
             VALUES (8, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
            [],
        )?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(Self {
            connection,
            path: path.to_path_buf(),
        })
    }

    pub fn insert_event(&self, event: NewEvent<'_>) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute(
            "INSERT INTO events
             (id, occurred_at, recorded_at, raw_text, source, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6, ?6)",
            params![
                id,
                event.occurred_at.to_rfc3339(),
                event.recorded_at.to_rfc3339(),
                event.raw_text,
                event.source,
                now,
            ],
        )?;
        transaction.execute(
            "INSERT INTO analysis_jobs (id, event_id, status, attempts, available_at, created_at, updated_at)
             VALUES (?1, ?2, 'pending', 0, ?3, ?3, ?3)",
            params![Uuid::new_v4().to_string(), id, now],
        )?;
        transaction.commit()?;
        Ok(id)
    }

    pub fn create_input_record(
        &self,
        raw_text: &str,
        source: &str,
        idempotency_key: Option<&str>,
    ) -> Result<InputRecord> {
        let raw_text = raw_text.trim();
        let source = source.trim();
        if raw_text.is_empty() {
            anyhow::bail!("input raw_text 不能为空");
        }
        if source.is_empty() {
            anyhow::bail!("input source 不能为空");
        }
        let key = idempotency_key.map(str::trim).filter(|key| !key.is_empty());
        if let Some(key) = key {
            if let Some(existing) = self.get_input_record_by_idempotency_key(key)? {
                return Ok(existing);
            }
        }

        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        self.connection.execute(
            "INSERT INTO input_records
             (id,raw_text,source,route_status,idempotency_key,created_at,updated_at)
             VALUES (?1,?2,?3,'pending',?4,?5,?5)",
            params![id, raw_text, source, key, now],
        )?;
        self.get_input_record(&id)?
            .context("input record 创建后读取失败")
    }

    /// 普通个人输入的最小统一提交路径：input record、不可变 event 与分析任务
    /// 在同一事务内提交。网络和 AI 均不参与此路径。
    pub fn submit_input_as_event(
        &self,
        raw_text: &str,
        source: &str,
        idempotency_key: Option<&str>,
    ) -> Result<InputRecord> {
        let raw_text = raw_text.trim();
        let source = source.trim();
        if raw_text.is_empty() || source.is_empty() {
            anyhow::bail!("input raw_text 和 source 不能为空");
        }
        let key = idempotency_key.map(str::trim).filter(|key| !key.is_empty());
        if let Some(key) = key {
            if let Some(existing) = self.get_input_record_by_idempotency_key(key)? {
                return Ok(existing);
            }
        }

        let input_id = Uuid::new_v4().to_string();
        let event_id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now();
        let now_text = now.to_rfc3339();
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute(
            "INSERT INTO events
             (id,occurred_at,recorded_at,raw_text,source,status,created_at,updated_at)
             VALUES (?1,?2,?2,?3,?4,'pending',?2,?2)",
            params![event_id, now_text, raw_text, source],
        )?;
        transaction.execute(
            "INSERT INTO analysis_jobs
             (id,event_id,status,attempts,available_at,created_at,updated_at)
             VALUES (?1,?2,'pending',0,?3,?3,?3)",
            params![Uuid::new_v4().to_string(), event_id, now_text],
        )?;
        transaction.execute(
            "INSERT INTO input_records
             (id,raw_text,source,route_status,idempotency_key,event_id,created_at,updated_at)
             VALUES (?1,?2,?3,'routed',?4,?5,?6,?6)",
            params![input_id, raw_text, source, key, event_id, now_text],
        )?;
        transaction.commit()?;
        self.get_input_record(&input_id)?
            .context("统一输入提交后读取失败")
    }

    /// 主对话输入：同一事务保存用户消息与个人事件，并用 input record 关联。
    pub fn submit_conversation_input(
        &self,
        conversation_id: &str,
        raw_text: &str,
        idempotency_key: Option<&str>,
    ) -> Result<InputRecord> {
        let raw_text = raw_text.trim();
        if raw_text.is_empty() {
            anyhow::bail!("input raw_text 不能为空");
        }
        let key = idempotency_key.map(str::trim).filter(|key| !key.is_empty());
        if let Some(key) = key {
            if let Some(existing) = self.get_input_record_by_idempotency_key(key)? {
                return Ok(existing);
            }
        }
        if self.get_conversation(conversation_id)?.is_none() {
            anyhow::bail!("Conversation not found: {conversation_id}");
        }

        let input_id = Uuid::new_v4().to_string();
        let event_id = Uuid::new_v4().to_string();
        let message_id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute(
            "INSERT INTO events
             (id,occurred_at,recorded_at,raw_text,source,status,created_at,updated_at)
             VALUES (?1,?2,?2,?3,'conversation','pending',?2,?2)",
            params![event_id, now, raw_text],
        )?;
        transaction.execute(
            "INSERT INTO analysis_jobs
             (id,event_id,status,attempts,available_at,created_at,updated_at)
             VALUES (?1,?2,'pending',0,?3,?3,?3)",
            params![Uuid::new_v4().to_string(), event_id, now],
        )?;
        transaction.execute(
            "INSERT INTO messages (id,conversation_id,role,content,created_at)
             VALUES (?1,?2,'user',?3,?4)",
            params![message_id, conversation_id, raw_text, now],
        )?;
        transaction.execute(
            "UPDATE conversations SET updated_at=?1 WHERE id=?2",
            params![now, conversation_id],
        )?;
        transaction.execute(
            "INSERT INTO input_records
             (id,raw_text,source,route_status,idempotency_key,event_id,message_id,created_at,updated_at)
             VALUES (?1,?2,'conversation','routed',?3,?4,?5,?6,?6)",
            params![input_id, raw_text, key, event_id, message_id, now],
        )?;
        transaction.commit()?;
        self.get_input_record(&input_id)?
            .context("对话统一输入提交后读取失败")
    }

    pub fn get_input_record(&self, id: &str) -> Result<Option<InputRecord>> {
        self.connection
            .query_row(
                "SELECT id,raw_text,source,route_status,idempotency_key,event_id,message_id,
                        wiki_page_slug,todo_id,created_at,updated_at
                 FROM input_records WHERE id=?1",
                [id],
                map_input_record,
            )
            .optional()
            .context("read input record")
    }

    pub fn latest_event_id_for_conversation(
        &self,
        conversation_id: &str,
    ) -> Result<Option<String>> {
        self.connection
            .query_row(
                "SELECT i.event_id FROM input_records i
             JOIN messages m ON m.id=i.message_id
             WHERE m.conversation_id=?1 AND i.event_id IS NOT NULL
             ORDER BY m.created_at DESC, i.created_at DESC LIMIT 1",
                [conversation_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn conversation_id_for_event(&self, event_id: &str) -> Result<Option<String>> {
        self.connection
            .query_row(
                "SELECT m.conversation_id FROM input_records i
                 JOIN messages m ON m.id=i.message_id
                 WHERE i.event_id=?1 LIMIT 1",
                [event_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    fn get_input_record_by_idempotency_key(&self, key: &str) -> Result<Option<InputRecord>> {
        self.connection
            .query_row(
                "SELECT id,raw_text,source,route_status,idempotency_key,event_id,message_id,
                        wiki_page_slug,todo_id,created_at,updated_at
                 FROM input_records WHERE idempotency_key=?1",
                [key],
                map_input_record,
            )
            .optional()
            .context("read input record by idempotency key")
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_input_route(
        &self,
        id: &str,
        route_status: &str,
        event_id: Option<&str>,
        message_id: Option<&str>,
        wiki_page_slug: Option<&str>,
        todo_id: Option<&str>,
    ) -> Result<InputRecord> {
        if !matches!(
            route_status,
            "pending" | "routed" | "needs_confirmation" | "failed"
        ) {
            anyhow::bail!("非法 input route_status: {route_status}");
        }
        let changed = self.connection.execute(
            "UPDATE input_records
             SET route_status=?2,event_id=COALESCE(?3,event_id),message_id=COALESCE(?4,message_id),
                 wiki_page_slug=COALESCE(?5,wiki_page_slug),todo_id=COALESCE(?6,todo_id),
                 updated_at=?7 WHERE id=?1",
            params![
                id,
                route_status,
                event_id,
                message_id,
                wiki_page_slug,
                todo_id,
                chrono::Utc::now().to_rfc3339()
            ],
        )?;
        if changed == 0 {
            anyhow::bail!("input record 不存在: {id}");
        }
        self.get_input_record(id)?
            .context("input route 更新后读取失败")
    }

    pub fn claim_analysis_job(&self) -> Result<Option<AnalysisJob>> {
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        let job = transaction.query_row(
            "SELECT j.id, j.event_id, e.raw_text, j.attempts FROM analysis_jobs j JOIN events e ON e.id=j.event_id
             WHERE j.status IN ('pending','retry') AND j.available_at <= ?1 ORDER BY j.created_at LIMIT 1",
            [chrono::Utc::now().to_rfc3339()],
            |row| Ok(AnalysisJob { id: row.get(0)?, event_id: row.get(1)?, raw_text: row.get(2)?, attempts: row.get(3)? }),
        ).optional()?;
        if let Some(ref job) = job {
            transaction.execute("UPDATE analysis_jobs SET status='running', attempts=attempts+1, updated_at=?2 WHERE id=?1", params![job.id, chrono::Utc::now().to_rfc3339()])?;
        }
        transaction.commit()?;
        Ok(job)
    }

    /// Return jobs left running by a previous process to the durable queue.
    /// This is called once at bridge startup, never from `Store::open`, so it
    /// cannot steal work from a live worker in the current process.
    pub fn recover_interrupted_analysis_jobs(&self) -> Result<usize> {
        let now = chrono::Utc::now().to_rfc3339();
        Ok(self.connection.execute(
            "UPDATE analysis_jobs
             SET status='retry', last_error='应用退出时分析尚未完成',
                 available_at=?1, updated_at=?1
             WHERE status='running'",
            [now],
        )?)
    }

    pub fn complete_analysis(
        &self,
        job: &AnalysisJob,
        prompt_version: &str,
        result_json: &str,
    ) -> Result<()> {
        let transaction = self.connection.unchecked_transaction()?;
        let now = chrono::Utc::now().to_rfc3339();
        transaction.execute("INSERT INTO event_analyses (id,event_id,prompt_version,result_json,created_at) VALUES (?1,?2,?3,?4,?5)", params![Uuid::new_v4().to_string(), job.event_id, prompt_version, result_json, now])?;
        transaction.execute(
            "UPDATE analysis_jobs SET status='succeeded', updated_at=?2 WHERE id=?1",
            params![job.id, now],
        )?;
        transaction.execute(
            "UPDATE events SET status='processed', processed_at=?2, updated_at=?2 WHERE id=?1",
            params![job.event_id, now],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn fail_analysis(&self, job: &AnalysisJob, error: &str) -> Result<()> {
        let delay = 2_i64.pow((job.attempts as u32).min(8));
        let available = chrono::Utc::now() + chrono::Duration::seconds(delay);
        self.connection.execute("UPDATE analysis_jobs SET status=CASE WHEN attempts >= 5 THEN 'failed' ELSE 'retry' END, last_error=?2, available_at=?3, updated_at=?4 WHERE id=?1", params![job.id, error, available.to_rfc3339(), chrono::Utc::now().to_rfc3339()])?;
        Ok(())
    }

    /// Aggregate the durable analysis queue by its complete status vocabulary.
    /// Missing statuses are returned as zero so callers can render a stable UI.
    pub fn analysis_job_stats(&self) -> Result<AnalysisJobStats> {
        self.connection
            .query_row(
                "SELECT
                    COALESCE(SUM(status = 'pending'), 0),
                    COALESCE(SUM(status = 'running'), 0),
                    COALESCE(SUM(status = 'retry'), 0),
                    COALESCE(SUM(status = 'succeeded'), 0),
                    COALESCE(SUM(status = 'failed'), 0)
                 FROM analysis_jobs",
                [],
                |row| {
                    Ok(AnalysisJobStats {
                        pending: row.get(0)?,
                        running: row.get(1)?,
                        retry: row.get(2)?,
                        succeeded: row.get(3)?,
                        failed: row.get(4)?,
                    })
                },
            )
            .context("aggregate analysis job stats")
    }

    pub fn event_analysis_detail(&self, event_id: &str) -> Result<Option<EventAnalysisDetail>> {
        self.connection
            .query_row(
                "SELECT e.id,e.raw_text,e.source,e.recorded_at,e.status,
                        j.status,j.attempts,j.last_error,j.available_at,
                        a.prompt_version,a.result_json,a.created_at
                 FROM events e
                 JOIN analysis_jobs j ON j.event_id=e.id
                 LEFT JOIN event_analyses a ON a.id=(
                   SELECT latest.id FROM event_analyses latest
                   WHERE latest.event_id=e.id
                   ORDER BY latest.created_at DESC LIMIT 1
                 )
                 WHERE e.id=?1",
                [event_id],
                |row| {
                    Ok(EventAnalysisDetail {
                        event_id: row.get(0)?,
                        raw_text: row.get(1)?,
                        source: row.get(2)?,
                        recorded_at: row.get(3)?,
                        event_status: row.get(4)?,
                        job_status: row.get(5)?,
                        attempts: row.get(6)?,
                        last_error: row.get(7)?,
                        available_at: row.get(8)?,
                        prompt_version: row.get(9)?,
                        result_json: row.get(10)?,
                        analysis_created_at: row.get(11)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn latest_event_recordability_decision(
        &self,
        event_id: &str,
    ) -> Result<Option<EventRecordabilityDecision>> {
        self.connection
            .query_row(
                "SELECT event_id,recordable,kind,reason,created_at
                 FROM event_recordability_decisions
                 WHERE event_id=?1 ORDER BY created_at DESC,id DESC LIMIT 1",
                [event_id],
                |row| {
                    Ok(EventRecordabilityDecision {
                        event_id: row.get(0)?,
                        recordable: row.get(1)?,
                        kind: row.get(2)?,
                        reason: row.get(3)?,
                        created_at: row.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn event_id_for_message(&self, message_id: &str) -> Result<Option<String>> {
        self.connection
            .query_row(
                "SELECT event_id FROM input_records WHERE message_id=?1 AND event_id IS NOT NULL LIMIT 1",
                [message_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn set_event_recordability(
        &self,
        event_id: &str,
        recordable: bool,
        reason: &str,
    ) -> Result<EventRecordabilityDecision> {
        if self.event_analysis_detail(event_id)?.is_none() {
            anyhow::bail!("事件不存在");
        }
        let kind = if recordable { "event" } else { "discussion" };
        let now = chrono::Utc::now().to_rfc3339();
        let tx = self.connection.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO event_recordability_decisions (id,event_id,recordable,kind,reason,created_at)
             VALUES (?1,?2,?3,?4,?5,?6)",
            params![Uuid::new_v4().to_string(), event_id, recordable, kind, reason.trim(), now],
        )?;
        if !recordable {
            tx.execute(
                "DELETE FROM entity_facts WHERE source_event_id=?1",
                [event_id],
            )?;
            tx.execute("DELETE FROM relations WHERE source_event_id=?1", [event_id])?;
            tx.execute(
                "UPDATE pending_actions SET status='declined'
                 WHERE status='pending' AND args_json LIKE '%' || ?1 || '%'",
                [event_id],
            )?;
            tx.execute(
                "UPDATE conversations SET tag='discussion',updated_at=?2
                 WHERE id IN (
                   SELECT m.conversation_id FROM input_records i
                   JOIN messages m ON m.id=i.message_id WHERE i.event_id=?1
                 ) AND (tag IS NULL OR tag='general')",
                params![event_id, now],
            )?;
        }
        tx.commit()?;
        self.latest_event_recordability_decision(event_id)?
            .context("记录人工分类后读取失败")
    }

    pub fn requeue_event_analysis(&self, event_id: &str) -> Result<bool> {
        let now = chrono::Utc::now().to_rfc3339();
        let affected = self.connection.execute(
            "UPDATE analysis_jobs SET status='pending',attempts=0,last_error=NULL,available_at=?2,updated_at=?2
             WHERE event_id=?1 AND status<>'running'",
            params![event_id, now],
        )?;
        if affected > 0 {
            self.connection.execute(
                "UPDATE events SET status='pending',processed_at=NULL,updated_at=?2 WHERE id=?1",
                params![event_id, now],
            )?;
        }
        Ok(affected > 0)
    }

    pub fn save_daily_review(
        &self,
        date: chrono::NaiveDate,
        prompt_version: &str,
        result_json: &str,
        source_event_ids: &[String],
    ) -> Result<String> {
        let prompt_version = prompt_version.trim();
        if prompt_version.is_empty() {
            anyhow::bail!("daily review prompt_version 不能为空");
        }
        if source_event_ids.is_empty() {
            anyhow::bail!("daily review 必须引用至少一条来源事件");
        }
        let daily_event_ids = self
            .daily_entries(date)?
            .into_iter()
            .map(|entry| entry.event_id)
            .collect::<Vec<_>>();
        for event_id in source_event_ids {
            if !daily_event_ids
                .iter()
                .any(|candidate| candidate == event_id)
            {
                anyhow::bail!("daily review 来源不属于目标日期: {event_id}");
            }
        }

        let transaction = self.connection.unchecked_transaction()?;
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        transaction.execute(
            "INSERT INTO daily_reviews
             (id,review_date,prompt_version,result_json,created_at)
             VALUES (?1,?2,?3,?4,?5)",
            params![
                id,
                date.format("%Y-%m-%d").to_string(),
                prompt_version,
                result_json,
                now
            ],
        )?;
        for event_id in source_event_ids {
            transaction.execute(
                "INSERT OR IGNORE INTO daily_review_sources (review_id,event_id)
                 VALUES (?1,?2)",
                params![id, event_id],
            )?;
        }
        transaction.commit()?;
        Ok(id)
    }

    pub fn latest_daily_review(
        &self,
        date: chrono::NaiveDate,
    ) -> Result<Option<DailyReviewRecord>> {
        let review = self
            .connection
            .query_row(
                "SELECT id,review_date,prompt_version,result_json,created_at
                 FROM daily_reviews WHERE review_date=?1
                 ORDER BY created_at DESC,rowid DESC LIMIT 1",
                [date.format("%Y-%m-%d").to_string()],
                |row| {
                    Ok(DailyReviewRecord {
                        id: row.get(0)?,
                        date: row.get(1)?,
                        prompt_version: row.get(2)?,
                        result_json: row.get(3)?,
                        source_event_ids: Vec::new(),
                        created_at: row.get(4)?,
                    })
                },
            )
            .optional()?;
        let Some(mut review) = review else {
            return Ok(None);
        };
        let mut statement = self.connection.prepare(
            "SELECT event_id FROM daily_review_sources
             WHERE review_id=?1 ORDER BY rowid",
        )?;
        review.source_event_ids = statement
            .query_map([&review.id], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(Some(review))
    }

    pub fn upsert_entity_fact(
        &self,
        entity_kind: &str,
        entity_slug: &str,
        fact_text: &str,
        occurred_at: &str,
        confidence: i64,
        source_event_id: &str,
    ) -> Result<EntityFact> {
        if !matches!(entity_kind, "person" | "project" | "topic") {
            anyhow::bail!("非法实体类型: {entity_kind}");
        }
        if entity_slug.trim().is_empty() || fact_text.trim().is_empty() {
            anyhow::bail!("实体标识和事实内容不能为空");
        }
        if !(0..=5).contains(&confidence) {
            anyhow::bail!("事实置信度必须在 0..5");
        }
        let now = chrono::Utc::now().to_rfc3339();
        self.connection.execute(
            "INSERT INTO entity_facts
             (id,entity_kind,entity_slug,fact_text,occurred_at,confidence,source_event_id,created_at,last_seen_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?8)
             ON CONFLICT(entity_kind,entity_slug,fact_text,source_event_id) DO UPDATE SET
               confidence=MAX(entity_facts.confidence, excluded.confidence),
               last_seen_at=excluded.last_seen_at",
            params![Uuid::new_v4().to_string(), entity_kind, entity_slug.trim(), fact_text.trim(), occurred_at, confidence, source_event_id, now],
        )?;
        self.connection
            .query_row(
                "SELECT id,entity_kind,entity_slug,fact_text,occurred_at,confidence,source_event_id,created_at,last_seen_at
                 FROM entity_facts WHERE entity_kind=?1 AND entity_slug=?2 AND fact_text=?3 AND source_event_id=?4",
                params![entity_kind, entity_slug.trim(), fact_text.trim(), source_event_id],
                |row| Ok(EntityFact { id: row.get(0)?, entity_kind: row.get(1)?, entity_slug: row.get(2)?, fact_text: row.get(3)?, occurred_at: row.get(4)?, confidence: row.get(5)?, source_event_id: row.get(6)?, created_at: row.get(7)?, last_seen_at: row.get(8)? }),
            )
            .map_err(Into::into)
    }

    pub fn list_entity_facts(
        &self,
        entity_kind: &str,
        entity_slug: &str,
    ) -> Result<Vec<EntityFact>> {
        let mut statement = self.connection.prepare(
            "SELECT id,entity_kind,entity_slug,fact_text,occurred_at,confidence,source_event_id,created_at,last_seen_at
             FROM entity_facts WHERE entity_kind=?1 AND entity_slug=?2 ORDER BY occurred_at DESC, id DESC",
        )?;
        let rows = statement.query_map(params![entity_kind, entity_slug], |row| {
            Ok(EntityFact {
                id: row.get(0)?,
                entity_kind: row.get(1)?,
                entity_slug: row.get(2)?,
                fact_text: row.get(3)?,
                occurred_at: row.get(4)?,
                confidence: row.get(5)?,
                source_event_id: row.get(6)?,
                created_at: row.get(7)?,
                last_seen_at: row.get(8)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn delete_entity_fact(&self, id: &str) -> Result<bool> {
        Ok(self
            .connection
            .execute("DELETE FROM entity_facts WHERE id=?1", [id])?
            > 0)
    }

    pub fn list_entity_aliases(&self, entity_kind: &str, entity_slug: &str) -> Result<Vec<String>> {
        let mut statement = self.connection.prepare("SELECT alias FROM entity_aliases WHERE entity_kind=?1 AND entity_slug=?2 ORDER BY alias")?;
        let rows = statement.query_map(params![entity_kind, entity_slug], |row| row.get(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn add_entity_alias(
        &self,
        entity_kind: &str,
        entity_slug: &str,
        alias: &str,
    ) -> Result<()> {
        let alias = alias.trim();
        if alias.is_empty() {
            anyhow::bail!("别名不能为空");
        }
        self.connection.execute("INSERT OR IGNORE INTO entity_aliases (id,entity_kind,entity_slug,alias,created_at) VALUES (?1,?2,?3,?4,?5)", params![Uuid::new_v4().to_string(), entity_kind, entity_slug, alias, chrono::Utc::now().to_rfc3339()])?;
        Ok(())
    }

    /// 合并两个已确认的实体。原始事件不变，来源实体保留合并记录。
    pub fn merge_entity(
        &self,
        entity_kind: &str,
        source_slug: &str,
        target_slug: &str,
    ) -> Result<bool> {
        if !matches!(entity_kind, "person" | "project" | "topic")
            || source_slug.trim().is_empty()
            || target_slug.trim().is_empty()
            || source_slug == target_slug
        {
            anyhow::bail!("实体合并参数无效");
        }
        let source = self.get_wiki_page(source_slug)?.context("来源实体不存在")?;
        let target = self.get_wiki_page(target_slug)?.context("目标实体不存在")?;
        if source.kind != entity_kind || target.kind != entity_kind {
            anyhow::bail!("来源和目标必须是同一种实体类型");
        }
        if source.status == "merged" || target.status == "merged" {
            anyhow::bail!("已合并实体不能再次合并");
        }
        if self.connection.query_row("SELECT EXISTS(SELECT 1 FROM entity_merges WHERE entity_kind=?1 AND source_slug=?2 AND undone_at IS NULL)", params![entity_kind, source_slug], |r| r.get::<_, bool>(0))? {
            anyhow::bail!("该实体已经有合并记录");
        }
        let tx = self.connection.unchecked_transaction()?;
        let now = chrono::Utc::now().to_rfc3339();
        let merge_id = Uuid::new_v4().to_string();
        tx.execute("INSERT INTO entity_merges (id,entity_kind,source_slug,target_slug,created_at,undone_at) VALUES (?1,?2,?3,?4,?5,NULL)", params![merge_id, entity_kind, source_slug, target_slug, now])?;
        let mut facts = tx.prepare("SELECT id,fact_text,occurred_at,confidence,source_event_id,created_at,last_seen_at FROM entity_facts WHERE entity_kind=?1 AND entity_slug=?2")?;
        for row in facts.query_map(params![entity_kind, source_slug], |r| Ok((r.get::<_, String>(0)?, serde_json::json!({"id":r.get::<_,String>(0)?,"fact_text":r.get::<_,String>(1)?,"occurred_at":r.get::<_,String>(2)?,"confidence":r.get::<_,i64>(3)?,"source_event_id":r.get::<_,String>(4)?,"created_at":r.get::<_,String>(5)?,"last_seen_at":r.get::<_,String>(6)?}))))? {
            let (id, payload) = row?; tx.execute("INSERT INTO entity_merge_snapshots (merge_id,table_name,row_id,payload) VALUES (?1,'entity_facts',?2,?3)", params![merge_id, id, payload.to_string()])?;
        }
        let mut aliases = tx.prepare("SELECT id,alias,created_at FROM entity_aliases WHERE entity_kind=?1 AND entity_slug=?2")?;
        for row in aliases.query_map(params![entity_kind, source_slug], |r| Ok((r.get::<_,String>(0)?, serde_json::json!({"id":r.get::<_,String>(0)?,"alias":r.get::<_,String>(1)?,"created_at":r.get::<_,String>(2)?}))))? {
            let (id, payload) = row?; tx.execute("INSERT INTO entity_merge_snapshots (merge_id,table_name,row_id,payload) VALUES (?1,'entity_aliases',?2,?3)", params![merge_id, id, payload.to_string()])?;
        }
        let mut relations = tx.prepare("SELECT id,from_slug,from_kind,to_slug,to_kind,relation,note,confidence,source_conversation_id,source_event_id,created_at,last_seen_at FROM relations WHERE (from_kind=?1 AND from_slug=?2) OR (to_kind=?1 AND to_slug=?2)")?;
        for row in relations.query_map(params![entity_kind, source_slug], |r| Ok((r.get::<_,String>(0)?, serde_json::json!({"id":r.get::<_,String>(0)?,"from_slug":r.get::<_,String>(1)?,"from_kind":r.get::<_,String>(2)?,"to_slug":r.get::<_,String>(3)?,"to_kind":r.get::<_,String>(4)?,"relation":r.get::<_,String>(5)?,"note":r.get::<_,Option<String>>(6)?,"confidence":r.get::<_,i64>(7)?,"source_conversation_id":r.get::<_,Option<String>>(8)?,"source_event_id":r.get::<_,Option<String>>(9)?,"created_at":r.get::<_,String>(10)?,"last_seen_at":r.get::<_,String>(11)?}))))? {
            let (id, payload) = row?; tx.execute("INSERT INTO entity_merge_snapshots (merge_id,table_name,row_id,payload) VALUES (?1,'relations',?2,?3)", params![merge_id, id, payload.to_string()])?;
        }
        let mut todos = tx.prepare("SELECT id,title,status,priority,due_at,related_event_id,note,created_at,updated_at FROM todos WHERE related_wiki_slug=?1")?;
        for row in todos.query_map([source_slug], |r| Ok((r.get::<_,String>(0)?, serde_json::json!({"id":r.get::<_,String>(0)?,"title":r.get::<_,String>(1)?,"status":r.get::<_,String>(2)?,"priority":r.get::<_,String>(3)?,"due_at":r.get::<_,Option<String>>(4)?,"related_event_id":r.get::<_,Option<String>>(5)?,"note":r.get::<_,Option<String>>(6)?,"created_at":r.get::<_,String>(7)?,"updated_at":r.get::<_,String>(8)?}))))? {
            let (id, payload) = row?; tx.execute("INSERT INTO entity_merge_snapshots (merge_id,table_name,row_id,payload) VALUES (?1,'todos',?2,?3)", params![merge_id,id,payload.to_string()])?;
        }
        drop(facts);
        drop(aliases);
        drop(relations);
        drop(todos);
        tx.execute("UPDATE entity_merge_snapshots SET disposition='deduplicated' WHERE merge_id=?1 AND table_name='entity_facts' AND row_id IN (SELECT s.id FROM entity_facts s WHERE s.entity_kind=?2 AND s.entity_slug=?3 AND EXISTS (SELECT 1 FROM entity_facts t WHERE t.entity_kind=?2 AND t.entity_slug=?4 AND t.fact_text=s.fact_text AND t.source_event_id=s.source_event_id))", params![merge_id, entity_kind, source_slug, target_slug])?;
        tx.execute("DELETE FROM entity_facts WHERE entity_kind=?1 AND entity_slug=?2 AND EXISTS (SELECT 1 FROM entity_facts t WHERE t.entity_kind=?1 AND t.entity_slug=?3 AND t.fact_text=entity_facts.fact_text AND t.source_event_id=entity_facts.source_event_id)", params![entity_kind, source_slug, target_slug])?;
        tx.execute(
            "UPDATE entity_facts SET entity_slug=?1 WHERE entity_kind=?2 AND entity_slug=?3",
            params![target_slug, entity_kind, source_slug],
        )?;
        tx.execute("UPDATE entity_merge_snapshots SET disposition='deduplicated' WHERE merge_id=?1 AND table_name='entity_aliases' AND row_id IN (SELECT s.id FROM entity_aliases s WHERE s.entity_kind=?2 AND s.entity_slug=?3 AND EXISTS (SELECT 1 FROM entity_aliases t WHERE t.entity_kind=?2 AND t.entity_slug=?4 AND t.alias=s.alias))", params![merge_id, entity_kind, source_slug, target_slug])?;
        tx.execute("DELETE FROM entity_aliases WHERE entity_kind=?1 AND entity_slug=?2 AND EXISTS (SELECT 1 FROM entity_aliases t WHERE t.entity_kind=?1 AND t.entity_slug=?3 AND t.alias=entity_aliases.alias)", params![entity_kind, source_slug, target_slug])?;
        tx.execute(
            "UPDATE entity_aliases SET entity_slug=?1 WHERE entity_kind=?2 AND entity_slug=?3",
            params![target_slug, entity_kind, source_slug],
        )?;
        tx.execute("UPDATE entity_merge_snapshots SET disposition='deduplicated' WHERE merge_id=?1 AND table_name='relations' AND row_id IN (SELECT s.id FROM relations s WHERE ((s.from_kind=?2 AND s.from_slug=?3) OR (s.to_kind=?2 AND s.to_slug=?3)) AND EXISTS (SELECT 1 FROM relations t WHERE t.id<>s.id AND t.from_slug=CASE WHEN s.from_kind=?2 AND s.from_slug=?3 THEN ?4 ELSE s.from_slug END AND t.to_slug=CASE WHEN s.to_kind=?2 AND s.to_slug=?3 THEN ?4 ELSE s.to_slug END AND t.relation=s.relation))", params![merge_id, entity_kind, source_slug, target_slug])?;
        tx.execute("DELETE FROM relations WHERE id IN (SELECT row_id FROM entity_merge_snapshots WHERE merge_id=?1 AND table_name='relations' AND disposition='deduplicated')", [&merge_id])?;
        tx.execute("UPDATE relations SET from_slug=CASE WHEN from_kind=?1 AND from_slug=?2 THEN ?3 ELSE from_slug END, to_slug=CASE WHEN to_kind=?1 AND to_slug=?2 THEN ?3 ELSE to_slug END WHERE (from_kind=?1 AND from_slug=?2) OR (to_kind=?1 AND to_slug=?2)", params![entity_kind, source_slug, target_slug])?;
        tx.execute(
            "UPDATE todos SET related_wiki_slug=?1 WHERE related_wiki_slug=?2",
            params![target_slug, source_slug],
        )?;
        tx.execute(
            "UPDATE wiki_pages SET status='merged', updated_at=?1 WHERE slug=?2",
            params![now, source_slug],
        )?;
        tx.commit()?;
        Ok(true)
    }

    pub fn undo_entity_merge(&self, source_slug: &str) -> Result<bool> {
        let (merge_id, kind, target): (String, String, String) = self.connection.query_row(
            "SELECT id,entity_kind,target_slug FROM entity_merges WHERE source_slug=?1 AND undone_at IS NULL ORDER BY created_at DESC LIMIT 1",
            [source_slug],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        ).optional()?.context("找不到可撤销的合并记录")?;
        let tx = self.connection.unchecked_transaction()?;
        let status: String = tx.query_row(
            "SELECT status FROM wiki_pages WHERE slug=?1",
            [source_slug],
            |r| r.get(0),
        )?;
        if status != "merged" {
            anyhow::bail!("来源实体状态已变化，不能安全撤销");
        }
        let snapshots = {
            let mut statement = tx.prepare("SELECT table_name,row_id,disposition,payload FROM entity_merge_snapshots WHERE merge_id=?1 ORDER BY table_name,row_id")?;
            let rows = statement
                .query_map([&merge_id], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows
        };
        for (table, row_id, disposition, payload) in snapshots {
            let value: serde_json::Value = serde_json::from_str(&payload)?;
            let text = |key: &str| -> Result<String> {
                value
                    .get(key)
                    .and_then(|v| v.as_str())
                    .map(str::to_owned)
                    .with_context(|| format!("合并快照缺少 {key}"))
            };
            let number = |key: &str| -> Result<i64> {
                value
                    .get(key)
                    .and_then(|v| v.as_i64())
                    .with_context(|| format!("合并快照缺少 {key}"))
            };
            match table.as_str() {
                "entity_facts" => {
                    let fact_text = text("fact_text")?;
                    let occurred_at = text("occurred_at")?;
                    let confidence = number("confidence")?;
                    let event = text("source_event_id")?;
                    let created = text("created_at")?;
                    let seen = text("last_seen_at")?;
                    if disposition == "moved" {
                        let matches: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM entity_facts WHERE id=?1 AND entity_kind=?2 AND entity_slug=?3 AND fact_text=?4 AND occurred_at=?5 AND confidence=?6 AND source_event_id=?7 AND created_at=?8 AND last_seen_at=?9)", params![row_id,kind,target,fact_text,occurred_at,confidence,event,created,seen], |r| r.get(0))?;
                        if !matches {
                            anyhow::bail!("合并后的事实已被修改或删除，不能安全撤销");
                        }
                        tx.execute(
                            "UPDATE entity_facts SET entity_slug=?1 WHERE id=?2",
                            params![source_slug, row_id],
                        )?;
                    } else {
                        tx.execute("INSERT INTO entity_facts (id,entity_kind,entity_slug,fact_text,occurred_at,confidence,source_event_id,created_at,last_seen_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)", params![row_id,kind,source_slug,fact_text,occurred_at,confidence,event,created,seen])?;
                    }
                }
                "entity_aliases" => {
                    let alias = text("alias")?;
                    let created = text("created_at")?;
                    if disposition == "moved" {
                        let matches: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM entity_aliases WHERE id=?1 AND entity_kind=?2 AND entity_slug=?3 AND alias=?4 AND created_at=?5)", params![row_id,kind,target,alias,created], |r| r.get(0))?;
                        if !matches {
                            anyhow::bail!("合并后的别名已被修改或删除，不能安全撤销");
                        }
                        tx.execute(
                            "UPDATE entity_aliases SET entity_slug=?1 WHERE id=?2",
                            params![source_slug, row_id],
                        )?;
                    } else {
                        tx.execute("INSERT INTO entity_aliases (id,entity_kind,entity_slug,alias,created_at) VALUES (?1,?2,?3,?4,?5)", params![row_id,kind,source_slug,alias,created])?;
                    }
                }
                "relations" => {
                    let from_slug = text("from_slug")?;
                    let from_kind = text("from_kind")?;
                    let to_slug = text("to_slug")?;
                    let to_kind = text("to_kind")?;
                    let relation = text("relation")?;
                    let confidence = number("confidence")?;
                    let created = text("created_at")?;
                    let seen = text("last_seen_at")?;
                    let note = value
                        .get("note")
                        .and_then(|v| v.as_str())
                        .map(str::to_owned);
                    let conversation = value
                        .get("source_conversation_id")
                        .and_then(|v| v.as_str())
                        .map(str::to_owned);
                    let event = value
                        .get("source_event_id")
                        .and_then(|v| v.as_str())
                        .map(str::to_owned);
                    let merged_from = if from_kind == kind && from_slug == source_slug {
                        target.clone()
                    } else {
                        from_slug.clone()
                    };
                    let merged_to = if to_kind == kind && to_slug == source_slug {
                        target.clone()
                    } else {
                        to_slug.clone()
                    };
                    if disposition == "moved" {
                        let matches: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM relations WHERE id=?1 AND from_slug=?2 AND from_kind=?3 AND to_slug=?4 AND to_kind=?5 AND relation=?6 AND note IS ?7 AND confidence=?8 AND source_conversation_id IS ?9 AND source_event_id IS ?10 AND created_at=?11 AND last_seen_at=?12)", params![row_id,merged_from,from_kind,merged_to,to_kind,relation,note,confidence,conversation,event,created,seen], |r| r.get(0))?;
                        if !matches {
                            anyhow::bail!("合并后的关系已被修改或删除，不能安全撤销");
                        }
                        tx.execute(
                            "UPDATE relations SET from_slug=?1,to_slug=?2 WHERE id=?3",
                            params![from_slug, to_slug, row_id],
                        )?;
                    } else {
                        tx.execute("INSERT INTO relations (id,from_slug,from_kind,to_slug,to_kind,relation,note,confidence,source_conversation_id,source_event_id,created_at,last_seen_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)", params![row_id,from_slug,from_kind,to_slug,to_kind,relation,note,confidence,conversation,event,created,seen])?;
                    }
                }
                "todos" => {
                    let title = text("title")?;
                    let todo_status = text("status")?;
                    let priority = text("priority")?;
                    let created = text("created_at")?;
                    let updated = text("updated_at")?;
                    let due = value
                        .get("due_at")
                        .and_then(|v| v.as_str())
                        .map(str::to_owned);
                    let event = value
                        .get("related_event_id")
                        .and_then(|v| v.as_str())
                        .map(str::to_owned);
                    let note = value
                        .get("note")
                        .and_then(|v| v.as_str())
                        .map(str::to_owned);
                    let matches: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM todos WHERE id=?1 AND title=?2 AND status=?3 AND priority=?4 AND due_at IS ?5 AND related_event_id IS ?6 AND related_wiki_slug=?7 AND note IS ?8 AND created_at=?9 AND updated_at=?10)", params![row_id,title,todo_status,priority,due,event,target,note,created,updated], |r| r.get(0))?;
                    if !matches {
                        anyhow::bail!("合并后的关联待办已被修改，不能安全撤销");
                    }
                    tx.execute(
                        "UPDATE todos SET related_wiki_slug=?1 WHERE id=?2",
                        params![source_slug, row_id],
                    )?;
                }
                _ => anyhow::bail!("未知的合并快照类型"),
            }
        }
        let now = chrono::Utc::now().to_rfc3339();
        tx.execute(
            "UPDATE wiki_pages SET status='active',updated_at=?1 WHERE slug=?2",
            params![now, source_slug],
        )?;
        tx.execute(
            "UPDATE entity_merges SET undone_at=?1 WHERE id=?2",
            params![now, merge_id],
        )?;
        tx.commit()?;
        Ok(true)
    }

    pub fn entity_merge_status(&self, source_slug: &str) -> Result<Option<EntityMergeStatus>> {
        self.connection.query_row(
            "SELECT source_slug,target_slug,entity_kind,created_at FROM entity_merges WHERE source_slug=?1 AND undone_at IS NULL ORDER BY created_at DESC LIMIT 1",
            [source_slug],
            |r| Ok(EntityMergeStatus { source_slug:r.get(0)?, target_slug:r.get(1)?, entity_kind:r.get(2)?, created_at:r.get(3)? }),
        ).optional().map_err(Into::into)
    }

    pub fn upsert_ai_provider_config(
        &self,
        base_url: &str,
        model: &str,
        api_key: &str,
    ) -> Result<()> {
        // 兼容旧接口：以固定名 'default' 保存（若库中无激活配置则该条会自动激活）
        self.save_ai_provider_config(
            None,
            "default",
            "openai-compatible",
            base_url,
            model,
            api_key,
            0.7,
            None,
        )?;
        Ok(())
    }

    /// 列出全部 AI provider 配置（支持多配置，仅一个 is_active=1）
    pub fn list_ai_provider_configs(&self) -> Result<Vec<AiProviderConfigRow>> {
        let mut statement = self.connection.prepare(
            "SELECT id,name,provider_type,base_url,model,api_key_source,is_active,temperature,max_tokens
             FROM ai_provider_configs ORDER BY created_at ASC, id ASC",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok(AiProviderConfigRow {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    provider_type: row.get(2)?,
                    base_url: row.get(3)?,
                    model: row.get(4)?,
                    api_key_source: row.get(5)?,
                    is_active: row.get::<_, i64>(6)? != 0,
                    temperature: row.get(7)?,
                    max_tokens: row.get(8)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Full provider configurations for runtime failover, including secrets.
    /// The active provider is returned first, followed by creation order.
    pub fn list_ai_provider_configs_for_runtime(&self) -> Result<Vec<AiProviderConfig>> {
        let mut statement = self.connection.prepare(
            "SELECT id,name,provider_type,base_url,model,api_key_source,
                    COALESCE(api_key,''),is_active,temperature,max_tokens
             FROM ai_provider_configs
             ORDER BY is_active DESC, created_at ASC, id ASC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(AiProviderConfig {
                id: row.get(0)?,
                name: row.get(1)?,
                provider_type: row.get(2)?,
                base_url: row.get(3)?,
                model: row.get(4)?,
                api_key_source: row.get(5)?,
                api_key: row.get(6)?,
                is_active: row.get::<_, i64>(7)? != 0,
                temperature: row.get(8)?,
                max_tokens: row.get(9)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// 新增 / 编辑 AI provider 配置。id 为空则新建；
    /// 新建且当前无激活配置时自动激活。api_key 传空串表示保留已有 key 不变。
    pub fn save_ai_provider_config(
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
        let now = chrono::Utc::now().to_rfc3339();
        if let Some(pid) = id {
            if !pid.is_empty() {
                let changed = self.connection.execute(
                    "UPDATE ai_provider_configs SET
                       name=?1, provider_type=?2, base_url=?3, model=?4,
                       api_key_source='database',
                       api_key=CASE WHEN ?5='' THEN api_key ELSE ?5 END,
                       temperature=?6, max_tokens=?7, updated_at=?8
                     WHERE id=?9",
                    params![
                        name,
                        provider_type,
                        base_url,
                        model,
                        api_key,
                        temperature,
                        max_tokens,
                        now,
                        pid
                    ],
                )?;
                if changed == 0 {
                    anyhow::bail!("未找到要更新的配置（id={pid}）");
                }
                return Ok(pid.to_string());
            }
        }
        // 新建：若库中尚无激活配置，则自动激活（保证始终存在激活项）
        let has_active: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM ai_provider_configs WHERE is_active=1",
            [],
            |r| r.get(0),
        )?;
        let new_id = Uuid::new_v4().to_string();
        let new_active = if has_active == 0 { 1 } else { 0 };
        self.connection
            .execute(
                "INSERT INTO ai_provider_configs
                 (id,name,provider_type,base_url,model,api_key_source,api_key,is_active,temperature,max_tokens,created_at,updated_at)
                 VALUES (?1,?2,?3,?4,?5,'database',?6,?7,?8,?9,?10,?10)",
                params![new_id, name, provider_type, base_url, model, api_key, new_active, temperature, max_tokens, now],
            )
            .map_err(|e| {
                if e.to_string()
                    .contains("UNIQUE constraint failed: ai_provider_configs.name")
                {
                    anyhow::anyhow!("配置名「{name}」已存在，请换一个名称")
                } else {
                    anyhow::anyhow!("{e}")
                }
            })?;
        Ok(new_id)
    }

    /// 把指定配置设为激活（其余全部取消激活），保证有且仅有一个激活项
    pub fn set_active_ai_provider_config(&self, id: &str) -> Result<()> {
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute(
            "UPDATE ai_provider_configs SET is_active=0, updated_at=?1",
            params![chrono::Utc::now().to_rfc3339()],
        )?;
        let changed = transaction.execute(
            "UPDATE ai_provider_configs SET is_active=1, updated_at=?1 WHERE id=?2",
            params![chrono::Utc::now().to_rfc3339(), id],
        )?;
        if changed == 0 {
            transaction.rollback()?;
            anyhow::bail!("未找到要激活的配置（id={id}）");
        }
        transaction.commit()?;
        Ok(())
    }

    /// 删除一条配置；若删除的恰是激活项，则自动把剩余第一条配置激活
    pub fn delete_ai_provider_config(&self, id: &str) -> Result<()> {
        let transaction = self.connection.unchecked_transaction()?;
        let was_active: i64 = transaction.query_row(
            "SELECT COUNT(*) FROM ai_provider_configs WHERE id=?1 AND is_active=1",
            params![id],
            |r| r.get(0),
        )?;
        let deleted =
            transaction.execute("DELETE FROM ai_provider_configs WHERE id=?1", params![id])?;
        if deleted == 0 {
            transaction.rollback()?;
            anyhow::bail!("未找到要删除的配置（id={id}）");
        }
        if was_active > 0 {
            transaction.execute(
                "UPDATE ai_provider_configs SET is_active=1, updated_at=?1
                 WHERE id=(SELECT id FROM ai_provider_configs ORDER BY created_at ASC, id ASC LIMIT 1)",
                params![chrono::Utc::now().to_rfc3339()],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn active_ai_provider_config(&self) -> Result<Option<AiProviderConfig>> {
        self.connection
            .query_row(
                "SELECT id,name,provider_type,base_url,model,api_key_source,api_key,is_active,temperature,max_tokens
                 FROM ai_provider_configs
                 WHERE is_active=1 AND api_key IS NOT NULL LIMIT 1",
                [],
                |row| {
                    Ok(AiProviderConfig {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        provider_type: row.get(2)?,
                        base_url: row.get(3)?,
                        model: row.get(4)?,
                        api_key_source: row.get(5)?,
                        api_key: row.get(6)?,
                        is_active: row.get::<_, i64>(7)? != 0,
                        temperature: row.get(8)?,
                        max_tokens: row.get(9)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn list_events(&self) -> Result<Vec<EventSummary>> {
        let mut statement = self
            .connection
            .prepare("SELECT recorded_at, raw_text FROM events ORDER BY recorded_at DESC")?;
        let rows = statement.query_map([], |row| {
            Ok(EventSummary {
                recorded_at: row.get(0)?,
                raw_text: row.get(1)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// 查某个「本地日历日」记录的事件（按 recorded_at，本地时区日界 → UTC 区间，倒序）。
    /// 用户说「6月20日有哪些事件」→ 用本地日界解释，跨时区也正确。
    pub fn events_on_date(&self, date: chrono::NaiveDate) -> Result<Vec<EventSummary>> {
        use chrono::{Local, TimeZone};
        let day_edges = |d: chrono::NaiveDate| {
            let naive_local = d.and_hms_opt(0, 0, 0).expect("midnight is valid");
            match Local
                .from_local_datetime(&naive_local)
                .single()
                .or_else(|| Local.from_local_datetime(&naive_local).earliest())
            {
                Some(dt) => dt.with_timezone(&chrono::Utc).to_rfc3339(),
                // DST 空洞等罕见情形：按 UTC 同名时刻兜底，避免 panic
                None => naive_local.and_utc().to_rfc3339(),
            }
        };
        let start_utc = day_edges(date);
        let end_utc = day_edges(date + chrono::Duration::days(1));
        let mut statement = self.connection.prepare(
            "SELECT recorded_at, raw_text FROM events
             WHERE recorded_at >= ?1 AND recorded_at < ?2
             ORDER BY recorded_at DESC",
        )?;
        let rows = statement.query_map(params![start_utc, end_utc], |row| {
            Ok(EventSummary {
                recorded_at: row.get(0)?,
                raw_text: row.get(1)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// 统一日流：events 是权威全集；input_records 仅补充新流程的关联信息。
    /// 因此历史事件与新 Capture/对话输入都会出现，且每个 event 只返回一次。
    pub fn daily_entries(&self, date: chrono::NaiveDate) -> Result<Vec<DailyEntry>> {
        use chrono::{Local, TimeZone};
        let edge = |day: chrono::NaiveDate| {
            let local_midnight = day.and_hms_opt(0, 0, 0).expect("midnight is valid");
            Local
                .from_local_datetime(&local_midnight)
                .single()
                .or_else(|| Local.from_local_datetime(&local_midnight).earliest())
                .map(|value| value.with_timezone(&chrono::Utc).to_rfc3339())
                .unwrap_or_else(|| local_midnight.and_utc().to_rfc3339())
        };
        let start = edge(date);
        let end = edge(date + chrono::Duration::days(1));
        let mut statement = self.connection.prepare(
            "SELECT e.id,i.id,i.message_id,e.raw_text,e.source,e.status,e.recorded_at
             FROM events e LEFT JOIN input_records i ON i.event_id=e.id
             WHERE e.recorded_at>=?1 AND e.recorded_at<?2
             ORDER BY e.recorded_at DESC,e.id DESC",
        )?;
        let rows = statement.query_map(params![start, end], |row| {
            Ok(DailyEntry {
                event_id: row.get(0)?,
                input_id: row.get(1)?,
                message_id: row.get(2)?,
                raw_text: row.get(3)?,
                source: row.get(4)?,
                event_status: row.get(5)?,
                recorded_at: row.get(6)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// 最近 N 天内的最多 limit 条事件（按记录时间倒序）
    pub fn recent_events(&self, days: i64, limit: usize) -> Result<Vec<EventSummary>> {
        let since = (chrono::Utc::now() - chrono::Duration::days(days)).to_rfc3339();
        let mut statement = self.connection.prepare(
            "SELECT recorded_at, raw_text FROM events
             WHERE recorded_at >= ?1 ORDER BY recorded_at DESC LIMIT ?2",
        )?;
        let rows = statement.query_map(params![since, limit as i64], |row| {
            Ok(EventSummary {
                recorded_at: row.get(0)?,
                raw_text: row.get(1)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// 保存一条 AI 生成的认知洞察（derived data，不影响原始事件）
    pub fn insert_insight(
        &self,
        window_days: i64,
        prompt_version: &str,
        lens: &str,
        title: &str,
        observation: &str,
        related_events: &[String],
        action: Option<&str>,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let related_raw = serde_json::to_string(related_events)?;
        self.connection.execute(
            "INSERT INTO insights
             (id, created_at, window_days, prompt_version, lens, title, observation, related_raw, action, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'new')",
            params![
                id,
                now,
                window_days,
                prompt_version,
                lens,
                title,
                observation,
                related_raw,
                action,
            ],
        )?;
        Ok(id)
    }

    /// 列出全部已存洞察（新的在前）
    pub fn list_insights(&self) -> Result<Vec<InsightSummary>> {
        let mut statement = self.connection.prepare(
            "SELECT id, created_at, window_days, prompt_version, lens, title, observation,
                    related_raw, action, status
             FROM insights ORDER BY created_at DESC",
        )?;
        let rows = statement.query_map([], |row| {
            let related_raw: String = row.get(7)?;
            let related_events: Vec<String> =
                serde_json::from_str(&related_raw).unwrap_or_default();
            Ok(InsightSummary {
                id: row.get(0)?,
                created_at: row.get(1)?,
                window_days: row.get(2)?,
                prompt_version: row.get(3)?,
                lens: row.get(4)?,
                title: row.get(5)?,
                observation: row.get(6)?,
                related_events,
                action: row.get(8)?,
                status: row.get(9)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// 最近 N 天内最多 limit 条事件（含 id，用于 wiki 溯源）
    pub fn recent_event_records(&self, days: i64, limit: usize) -> Result<Vec<EventRecord>> {
        let since = (chrono::Utc::now() - chrono::Duration::days(days)).to_rfc3339();
        let mut statement = self.connection.prepare(
            "SELECT id, recorded_at, raw_text FROM events
             WHERE recorded_at >= ?1 ORDER BY recorded_at ASC LIMIT ?2",
        )?;
        let rows = statement.query_map(params![since, limit as i64], |row| {
            Ok(EventRecord {
                id: row.get(0)?,
                recorded_at: row.get(1)?,
                raw_text: row.get(2)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    // ── LLM wiki：页面 / 修订 / 日志 / meta ────────────────────────────────

    pub fn get_wiki_page(&self, slug: &str) -> Result<Option<WikiPage>> {
        let mut statement = self.connection.prepare(
            "SELECT id, slug, kind, title, summary, content_md, tags, source_event_ids,
                    evidence_count, first_seen_at, last_seen_at, status, created_at, updated_at,
                    source_url, COALESCE(area, 'insight'), based_on, content_type
             FROM wiki_pages WHERE slug = ?1",
        )?;
        let page = statement
            .query_row(params![slug], |row| map_wiki_page(row))
            .optional()?;
        Ok(page)
    }

    /// 按来源 URL 查已导入页面（URL 去重用）
    pub fn find_wiki_page_by_source_url(&self, source_url: &str) -> Result<Option<WikiPage>> {
        let mut statement = self.connection.prepare(
            "SELECT id, slug, kind, title, summary, content_md, tags, source_event_ids,
                    evidence_count, first_seen_at, last_seen_at, status, created_at, updated_at,
                    source_url, COALESCE(area, 'insight'), based_on, content_type
             FROM wiki_pages WHERE source_url = ?1
             ORDER BY updated_at DESC LIMIT 1",
        )?;
        let page = statement
            .query_row(params![source_url], |row| map_wiki_page(row))
            .optional()?;
        Ok(page)
    }

    /// 按标题精确查已存在页面（标题去重用；先于确定性 slug 判断，避免同名页重复建档）
    pub fn find_wiki_page_by_title(&self, title: &str) -> Result<Option<WikiPage>> {
        let mut statement = self.connection.prepare(
            "SELECT id, slug, kind, title, summary, content_md, tags, source_event_ids,
                    evidence_count, first_seen_at, last_seen_at, status, created_at, updated_at,
                    source_url, COALESCE(area, 'insight'), based_on, content_type
             FROM wiki_pages WHERE lower(trim(title)) = lower(trim(?1))
             ORDER BY updated_at DESC LIMIT 1",
        )?;
        let page = statement
            .query_row(params![title], |row| map_wiki_page(row))
            .optional()?;
        Ok(page)
    }

    pub fn find_wiki_pages_by_title(&self, title: &str) -> Result<Vec<WikiPage>> {
        let mut statement = self.connection.prepare(
            "SELECT id, slug, kind, title, summary, content_md, tags, source_event_ids,
                    evidence_count, first_seen_at, last_seen_at, status, created_at, updated_at,
                    source_url, COALESCE(area, 'insight'), based_on, content_type
             FROM wiki_pages WHERE lower(trim(title)) = lower(trim(?1))
             ORDER BY updated_at DESC",
        )?;
        let rows = statement.query_map(params![title], map_wiki_page)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn find_wiki_pages_by_title_or_alias(&self, name: &str) -> Result<Vec<WikiPage>> {
        let mut statement = self.connection.prepare(
            "SELECT DISTINCT p.id, p.slug, p.kind, p.title, p.summary, p.content_md, p.tags, p.source_event_ids,
                    p.evidence_count, p.first_seen_at, p.last_seen_at, p.status, p.created_at, p.updated_at,
                    p.source_url, COALESCE(p.area, 'insight'), p.based_on, p.content_type
             FROM wiki_pages p LEFT JOIN entity_aliases a ON a.entity_slug=p.slug AND a.entity_kind=p.kind
             WHERE lower(trim(p.title))=lower(trim(?1)) OR lower(trim(a.alias))=lower(trim(?1))
             ORDER BY p.updated_at DESC",
        )?;
        let rows = statement.query_map(params![name], map_wiki_page)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// 列出知识库页面（主列表）。
    /// - `kind`：按内容类型过滤；`area`：按来源/用途分区过滤（imported/network/insight）。
    /// - 默认排除派生产物（area=derivative，它们只经 `list_derivatives` 按原文展开读取）。
    /// - 顺序：有来源 URL 的（素材）在前，其余按最近更新时间倒序。
    pub fn list_wiki_pages(&self, kind: Option<&str>, area: Option<&str>) -> Result<Vec<WikiPage>> {
        let mut sql = String::from("SELECT ");
        sql.push_str(WIKI_PAGE_COLS);
        sql.push_str(" FROM wiki_pages WHERE 1=1");
        let mut owned: Vec<String> = Vec::new();
        match area {
            Some(a) => {
                owned.push(a.to_string());
                sql.push_str(" AND COALESCE(area, 'insight') = ?");
            }
            None => sql.push_str(" AND COALESCE(area, 'insight') != 'derivative'"),
        }
        if let Some(k) = kind {
            owned.push(k.to_string());
            sql.push_str(" AND kind = ?");
        }
        sql.push_str(" ORDER BY source_url IS NULL, last_seen_at DESC");
        let arg_refs: Vec<&dyn rusqlite::ToSql> =
            owned.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
        let mut statement = self.connection.prepare(&sql)?;
        let rows = statement.query_map(arg_refs.as_slice(), map_wiki_page)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// 某页的派生产物列表（AI 加工成果：总结/提炼/文案…），按创建时间倒序。
    pub fn list_derivatives(&self, based_on: &str) -> Result<Vec<WikiPage>> {
        let mut statement = self.connection.prepare(
            "SELECT id, slug, kind, title, summary, content_md, tags, source_event_ids,
                    evidence_count, first_seen_at, last_seen_at, status, created_at, updated_at,
                    source_url, COALESCE(area, 'insight'), based_on, content_type
             FROM wiki_pages WHERE based_on = ?1 AND area = 'derivative'
             ORDER BY created_at DESC",
        )?;
        let rows = statement.query_map(params![based_on], map_wiki_page)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// 创建一个「派生产物」页：对某页加工（总结/提炼观点/文案…）的成果。
    /// 挂靠原文（based_on + content_type），area=derivative，不进主列表；
    /// 不修改原文页的任何内容。
    pub fn create_derivative(
        &self,
        based_on_slug: &str,
        content_type: &str,
        title: &str,
        content_md: &str,
        reason: &str,
    ) -> Result<WikiPage> {
        let base = self.get_wiki_page(based_on_slug)?.with_context(|| {
            format!("知识页不存在：{based_on_slug}（不能对不存在的页面创建派生产物）")
        })?;
        let content_md = content_md.trim().to_string();
        if content_md.is_empty() {
            anyhow::bail!("派生产物正文为空，无法保存");
        }
        let id = Uuid::new_v4().to_string();
        let slug = format!("der-{}", &id[..8]);
        let summary: String = content_md
            .chars()
            .take(120)
            .collect::<String>()
            .trim_end()
            .to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let tags_raw = serde_json::to_string(&vec!["派生产物".to_string()])?;
        self.connection.execute(
            "INSERT INTO wiki_pages
             (id, slug, kind, title, summary, content_md, tags, source_event_ids,
              evidence_count, first_seen_at, last_seen_at, status, created_at, updated_at,
              source_url, area, based_on, content_type)
             VALUES (?1, ?2, 'derivative', ?3, ?4, ?5, ?6, '[]', 1,
                     ?7, ?7, 'active', ?7, ?7, NULL, 'derivative', ?8, ?9)",
            params![
                id,
                slug,
                title.trim(),
                summary,
                content_md,
                tags_raw,
                now,
                base.slug,
                content_type.trim(),
            ],
        )?;
        self.record_wiki_revision(&id, &content_md, reason, None)?;
        self.get_wiki_page(&slug)?.context("派生产物创建后读取失败")
    }

    /// 新建 wiki 页时按 kind / slug / 来源自动推导「来源/用途」分区。
    /// - `network`：人物（person）、事情/项目（topic/ 前缀）——关系网实体；
    /// - `imported`：外部素材（kind=source 或有来源 URL，以及 tweet-/note-/import- 前缀的导入页）；
    /// - `derivative`：派生产物（不经过 draft 新建，但兜底）；
    /// - 其余（AI 对话沉淀的知识、规则等）→ `insight`。
    fn derive_wiki_area(kind: &str, slug: &str, source_url: Option<&str>) -> String {
        if kind == "derivative" {
            return "derivative".to_string();
        }
        if kind == "person" || slug.starts_with("person/") || slug.starts_with("topic/") {
            return "network".to_string();
        }
        if kind == "source"
            || source_url.is_some()
            || slug.starts_with("tweet-")
            || slug.starts_with("note-")
            || slug.starts_with("import-")
        {
            return "imported".to_string();
        }
        "insight".to_string()
    }

    /// 创建或更新一个 wiki 页面。核心做确定性合并：
    /// 已存在 → 更新内容 + 事件 id 并集 + evidence_count = 并集长度；不存在 → 新建。
    /// 每次写回都记录一条 revision。
    pub fn upsert_wiki_page(&self, draft: &WikiPageDraft) -> Result<WikiUpsertOutcome> {
        let now = chrono::Utc::now().to_rfc3339();
        let tags_raw = serde_json::to_string(&draft.tags)?;

        let existing = self.get_wiki_page(&draft.slug)?;
        if let Some(page) = existing {
            // 合并（确定性，不允许 LLM 直接改数字）
            let mut all_ids = page.source_event_ids.clone();
            for id in &draft.source_event_ids {
                if !all_ids.contains(id) {
                    all_ids.push(id.clone());
                }
            }
            let evidence_count = all_ids.len() as i64;
            let sources_raw = serde_json::to_string(&all_ids)?;
            self.connection.execute(
                "UPDATE wiki_pages
                 SET title=?1, summary=?2, content_md=?3, tags=?4, source_event_ids=?5,
                     evidence_count=?6, last_seen_at=?7, status=?8, updated_at=?7,
                     source_url=COALESCE(?10, source_url)
                 WHERE id=?9",
                params![
                    draft.title,
                    draft.summary,
                    draft.content_md,
                    tags_raw,
                    sources_raw,
                    evidence_count,
                    now,
                    draft.status,
                    page.id,
                    draft.source_url,
                ],
            )?;
            self.record_wiki_revision(&page.id, &draft.content_md, &draft.reason, None)?;
            let updated = self.get_wiki_page(&draft.slug)?.unwrap();
            Ok(WikiUpsertOutcome {
                created: false,
                page: updated,
            })
        } else {
            let id = Uuid::new_v4().to_string();
            let sources_raw = serde_json::to_string(&draft.source_event_ids)?;
            let evidence_count = draft.source_event_ids.len().max(1) as i64;
            // 新建页自动推导分区（旧页保留原分区，见上方 UPDATE 分支）
            let area =
                Self::derive_wiki_area(&draft.kind, &draft.slug, draft.source_url.as_deref());
            self.connection.execute(
                "INSERT INTO wiki_pages
                 (id, slug, kind, title, summary, content_md, tags, source_event_ids,
                  evidence_count, first_seen_at, last_seen_at, status, created_at, updated_at,
                  source_url, area)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10, ?11, ?10, ?10, ?12, ?13)",
                params![
                    id,
                    draft.slug,
                    draft.kind,
                    draft.title,
                    draft.summary,
                    draft.content_md,
                    tags_raw,
                    sources_raw,
                    evidence_count,
                    now,
                    draft.status,
                    draft.source_url,
                    area,
                ],
            )?;
            self.record_wiki_revision(&id, &draft.content_md, &draft.reason, None)?;
            Ok(WikiUpsertOutcome {
                created: true,
                page: self.get_wiki_page(&draft.slug)?.unwrap(),
            })
        }
    }

    /// 用户手动重写一页的标签（元数据组织用）。
    /// 规范化：去 `#` 前缀、去首尾空白、去重、保序，最多保留 24 个。
    /// 标签变更会更新 `updated_at` 并追加一条 revision（正文不变，便于审计）。
    pub fn update_wiki_tags(&self, slug: &str, tags: &[String]) -> Result<WikiPage> {
        let page = self
            .get_wiki_page(slug)?
            .with_context(|| format!("知识页不存在: {slug}"))?;
        let mut cleaned: Vec<String> = Vec::new();
        for raw in tags {
            let t = raw.trim().trim_start_matches('#').trim().to_string();
            if t.is_empty() || cleaned.contains(&t) {
                continue;
            }
            cleaned.push(t);
            if cleaned.len() >= 24 {
                break;
            }
        }
        let now = chrono::Utc::now().to_rfc3339();
        let tags_raw = serde_json::to_string(&cleaned)?;
        self.connection.execute(
            "UPDATE wiki_pages SET tags = ?1, updated_at = ?2 WHERE id = ?3",
            params![tags_raw, now, page.id],
        )?;
        // 审计：标签变更也留一条 revision（正文沿用当前内容，reason 记录本次动作）
        let reason = if cleaned.is_empty() {
            "标签更新：（清空）".to_string()
        } else {
            format!("标签更新：{}", cleaned.join(", "))
        };
        self.record_wiki_revision(&page.id, &page.content_md, &reason, None)?;
        self.get_wiki_page(slug)?.context("标签更新后读取失败")
    }

    /// 重命名知识页：标题 + slug 一起换，事务内原子迁移关系引用与页内聊天会话，并追加一条修订记录。
    ///
    /// - 带前缀的 slug（`person/…`、`topic/…` 等）按前缀 + 新标题重算新 slug（撞名自动避让）；
    /// - 无前缀的 slug（`tweet-…`、`kb-…` 等）保留原 slug，只改标题（源资料引用不能断）；
    /// - 标题未变时直接返回 `changed=false`，不做任何写操作。
    pub fn rename_wiki_page(
        &self,
        slug: &str,
        new_title: &str,
        reason: &str,
    ) -> Result<RenameWikiOutcome> {
        let page = self.get_wiki_page(slug)?.with_context(|| {
            format!(
                "知识页不存在：{slug}（可能已被改名或删除——如果刚改过名，请用新名字操作，可在对话里列出知识库确认当前名称）"
            )
        })?;
        let new_title = new_title.trim().to_string();
        if new_title.is_empty() {
            anyhow::bail!("新标题不能为空");
        }
        let old_title = page.title.clone();
        let changed = new_title != old_title;
        if !changed {
            return Ok(RenameWikiOutcome {
                old_slug: slug.to_string(),
                new_slug: slug.to_string(),
                old_title,
                new_title,
                changed: false,
                relations_moved: 0,
                chats_moved: 0,
            });
        }
        // 计算新 slug：带前缀的页面重算（撞名避让），无前缀的保留原 slug
        let new_slug = match slug.rsplit_once('/') {
            Some((prefix, _)) => crate::wiki::unique_slug(
                self,
                &format!("{prefix}/{}", crate::wiki::slugify(&new_title)),
                &new_title,
            )?,
            None => slug.to_string(),
        };
        let now = chrono::Utc::now().to_rfc3339();
        let tx = self.connection.unchecked_transaction()?;
        // 1) 页面本体：换 slug + 标题，summary 里的旧名同步替换
        let new_summary = if old_title.is_empty() {
            page.summary.clone()
        } else {
            page.summary.replace(&old_title, &new_title)
        };
        tx.execute(
            "UPDATE wiki_pages SET slug=?1, title=?2, summary=?3, updated_at=?4 WHERE id=?5",
            params![new_slug, new_title, new_summary, now, page.id],
        )?;
        // 2) 关系引用迁移
        let mut relations_moved = 0usize;
        if new_slug != slug {
            relations_moved += tx.execute(
                "UPDATE relations SET from_slug=?1 WHERE from_slug=?2",
                params![new_slug, slug],
            )? as usize;
            relations_moved += tx.execute(
                "UPDATE relations SET to_slug=?1 WHERE to_slug=?2",
                params![new_slug, slug],
            )? as usize;
        }
        // 3) 页内聊天会话迁移
        let chats_moved = if new_slug != slug {
            tx.execute(
                "UPDATE conversations SET wiki_page_slug=?1 WHERE wiki_page_slug=?2",
                params![new_slug, slug],
            )? as usize
        } else {
            0
        };
        // 4) 审计修订记录
        tx.execute(
            "INSERT INTO wiki_revisions (id, page_id, content_md, reason, source_event_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                Uuid::new_v4().to_string(),
                page.id,
                page.content_md,
                reason,
                Option::<String>::None,
                now,
            ],
        )?;
        tx.commit()?;
        self.append_wiki_log(&format!(
            "知识页重命名：{old_title}（{slug}）→ {new_title}（{new_slug}）"
        ))?;
        Ok(RenameWikiOutcome {
            old_slug: slug.to_string(),
            new_slug,
            old_title,
            new_title,
            changed: true,
            relations_moved,
            chats_moved,
        })
    }

    fn record_wiki_revision(
        &self,
        page_id: &str,
        content_md: &str,
        reason: &str,
        source_event_id: Option<&str>,
    ) -> Result<()> {
        self.connection.execute(
            "INSERT INTO wiki_revisions
             (id, page_id, content_md, reason, source_event_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                Uuid::new_v4().to_string(),
                page_id,
                content_md,
                reason,
                source_event_id,
                chrono::Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn list_wiki_revisions(&self, slug: &str) -> Result<Vec<(String, String, String)>> {
        // (created_at, content_md, reason)
        let mut statement = self.connection.prepare(
            "SELECT r.created_at, r.content_md, r.reason
             FROM wiki_revisions r JOIN wiki_pages p ON p.id = r.page_id
             WHERE p.slug = ?1 ORDER BY r.created_at DESC",
        )?;
        let rows = statement.query_map(params![slug], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn append_wiki_log(&self, entry: &str) -> Result<()> {
        self.connection.execute(
            "INSERT INTO wiki_log (ts, entry) VALUES (?1, ?2)",
            params![chrono::Utc::now().to_rfc3339(), entry],
        )?;
        Ok(())
    }

    pub fn list_wiki_log(&self, limit: i64) -> Result<Vec<(String, String)>> {
        let mut statement = self
            .connection
            .prepare("SELECT ts, entry FROM wiki_log ORDER BY id DESC LIMIT ?1")?;
        let rows = statement.query_map(params![limit], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn get_meta(&self, key: &str) -> Result<Option<String>> {
        let mut statement = self
            .connection
            .prepare("SELECT value FROM app_meta WHERE key = ?1")?;
        statement
            .query_row(params![key], |row| row.get(0))
            .optional()
            .map_err(Into::into)
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.connection.execute(
            "INSERT INTO app_meta(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn list_analyses(&self) -> Result<Vec<AnalysisSummary>> {
        let mut statement = self.connection.prepare(
            "SELECT e.raw_text,
                    coalesce(json_extract(a.result_json,'$.event_type'),'unknown'),
                    coalesce(json_extract(a.result_json,'$.confidence'),0),
                    coalesce(json_extract(a.result_json,'$.clarifications'),'[]')
             FROM event_analyses a JOIN events e ON e.id=a.event_id
             ORDER BY a.created_at DESC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(AnalysisSummary {
                raw_text: row.get(0)?,
                event_type: row.get(1)?,
                confidence: row.get(2)?,
                clarifications: row.get(3)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    // Rules（个人经验规则库）

    /// 列出规则；status 为 None 时返回全部（active + pending）。
    /// conversation_id 为 None 时列出全局；为 Some 时只列该会话提出的 pending
    /// （v13 前遗留 conversation_id IS NULL 的规则始终包含，保证兼容）。
    pub fn list_rules(
        &self,
        status: Option<RuleStatus>,
        conversation_id: Option<&str>,
    ) -> Result<Vec<RuleSummary>> {
        let mut sql = "SELECT id, content, status, created_at FROM rules".to_string();
        let mut conditions: Vec<String> = Vec::new();
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if let Some(s) = status {
            conditions.push("status = ?".to_string());
            params.push(Box::new(s.as_str().to_string()));
        }
        if let Some(cid) = conversation_id {
            conditions.push("(conversation_id = ? OR conversation_id IS NULL)".to_string());
            params.push(Box::new(cid.to_string()));
        }
        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }
        sql.push_str(" ORDER BY created_at ASC");
        let mut statement = self.connection.prepare(&sql)?;
        let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();
        let rows = statement.query_map(rusqlite::params_from_iter(param_refs), |row| {
            let status_str: String = row.get(2)?;
            Ok(RuleSummary {
                id: row.get(0)?,
                content: row.get(1)?,
                status: if status_str == "pending" {
                    RuleStatus::Pending
                } else {
                    RuleStatus::Active
                },
                created_at: row.get(3)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// 已生效的规则（注入 AI 回复上下文用）
    pub fn list_active_rules(&self) -> Result<Vec<RuleSummary>> {
        self.list_rules(Some(RuleStatus::Active), None)
    }

    /// 新增一条规则。
    /// conversation_id：来源于哪个会话（pending 确认门按会话隔离用）；None 表示不归属
    pub fn add_rule(
        &self,
        content: &str,
        status: RuleStatus,
        conversation_id: Option<&str>,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        self.connection.execute(
            "INSERT INTO rules (id, content, status, conversation_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params![id, content, status.as_str(), conversation_id, now],
        )?;
        Ok(id)
    }

    /// 把本会话的待确认规则升级为生效（含 v13 前遗留的无归属规则）
    pub fn promote_pending_rules(&self, conversation_id: &str) -> Result<usize> {
        let now = chrono::Utc::now().to_rfc3339();
        let affected = self.connection.execute(
            "UPDATE rules SET status = 'active', updated_at = ?1
             WHERE status = 'pending'
               AND (conversation_id = ?2 OR conversation_id IS NULL)",
            params![now, conversation_id],
        )?;
        Ok(affected)
    }

    /// 丢弃本会话的待确认规则（用户明确拒绝；含 v13 前遗留的无归属规则）
    pub fn discard_pending_rules(&self, conversation_id: &str) -> Result<usize> {
        let affected = self.connection.execute(
            "DELETE FROM rules
             WHERE status = 'pending'
               AND (conversation_id = ?1 OR conversation_id IS NULL)",
            params![conversation_id],
        )?;
        Ok(affected)
    }

    /// 删除一条规则；返回是否存在并删除。
    pub fn delete_rule(&self, id: &str) -> Result<bool> {
        let affected = self
            .connection
            .execute("DELETE FROM rules WHERE id = ?1", [id])?;
        Ok(affected > 0)
    }

    /// 新增一条待确认动作（写类工具的草拟阶段）
    pub fn create_pending_action(
        &self,
        conversation_id: &str,
        action: &str,
        args_json: &str,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        self.connection.execute(
            "INSERT INTO pending_actions (id, conversation_id, action, args_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, conversation_id, action, args_json, now],
        )?;
        Ok(id)
    }

    // ── 个人待办 ────────────────────────────────────────────────────────

    pub fn create_todo(
        &self,
        title: &str,
        priority: &str,
        due_at: Option<&str>,
        related_event_id: Option<&str>,
        related_wiki_slug: Option<&str>,
        note: Option<&str>,
    ) -> Result<Todo> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        self.connection.execute(
            "INSERT INTO todos (id, title, status, priority, due_at, related_event_id, related_wiki_slug, note, created_at, updated_at)
             VALUES (?1, ?2, 'open', ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            params![id, title, priority, due_at, related_event_id, related_wiki_slug, note, now],
        )?;
        Ok(Todo {
            id,
            title: title.to_string(),
            status: TodoStatus::Open,
            priority: priority.to_string(),
            due_at: due_at.map(|s| s.to_string()),
            related_event_id: related_event_id.map(|s| s.to_string()),
            related_wiki_slug: related_wiki_slug.map(|s| s.to_string()),
            note: note.map(|s| s.to_string()),
            created_at: now.clone(),
            updated_at: now,
        })
    }

    pub fn list_todos(&self, status_filter: Option<&str>) -> Result<Vec<Todo>> {
        let sql = match status_filter {
            Some(_) => "SELECT id, title, status, priority, due_at, related_event_id, related_wiki_slug, note, created_at, updated_at
                        FROM todos WHERE status = ?1 ORDER BY created_at DESC",
            None => "SELECT id, title, status, priority, due_at, related_event_id, related_wiki_slug, note, created_at, updated_at
                    FROM todos WHERE status != 'archived' ORDER BY status, created_at DESC",
        };
        let mut statement = self.connection.prepare(sql)?;
        let mapper = |row: &rusqlite::Row| -> rusqlite::Result<Todo> {
            Ok(Todo {
                id: row.get(0)?,
                title: row.get(1)?,
                status: TodoStatus::parse(&row.get::<_, String>(2)?),
                priority: row.get(3)?,
                due_at: row.get(4)?,
                related_event_id: row.get(5)?,
                related_wiki_slug: row.get(6)?,
                note: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        };
        let rows = match status_filter {
            Some(s) => statement.query_map(params![s], mapper)?,
            None => statement.query_map([], mapper)?,
        };
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// Fetch a todo by id, including archived history. Used by on-demand
    /// work-item migration so archived todos are never silently lost.
    pub fn get_todo(&self, id: &str) -> Result<Option<Todo>> {
        let mut todos = self.list_todos(Some("archived"))?;
        todos.extend(self.list_todos(None)?);
        Ok(todos.into_iter().find(|todo| todo.id == id))
    }

    pub fn update_todo_status(&self, id: &str, status: TodoStatus) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.connection.execute(
            "UPDATE todos SET status=?1, updated_at=?2 WHERE id=?3",
            params![status.as_str(), now, id],
        )?;
        Ok(())
    }

    pub fn set_todo_related_wiki_slug(&self, id: &str, slug: &str) -> Result<()> {
        let changed = self.connection.execute(
            "UPDATE todos SET related_wiki_slug=?1, updated_at=?2 WHERE id=?3",
            params![slug, chrono::Utc::now().to_rfc3339(), id],
        )?;
        if changed == 0 {
            anyhow::bail!("未找到待办（id={id}）");
        }
        Ok(())
    }

    /// 更新待办的可编辑字段（标题 / 补充 / 优先级 / 截止时间）。
    /// 状态与关联字段保持不变；note/due_at 传 None 表示清除；
    /// priority 传 None 时回落到默认值 "normal"（列 NOT NULL）。
    pub fn update_todo(
        &self,
        id: &str,
        title: &str,
        note: Option<&str>,
        priority: Option<&str>,
        due_at: Option<&str>,
    ) -> Result<()> {
        let title = title.trim();
        if title.is_empty() {
            anyhow::bail!("待办内容不能为空");
        }
        let priority = priority.unwrap_or("normal");
        let now = chrono::Utc::now().to_rfc3339();
        self.connection.execute(
            "UPDATE todos SET title=?1, note=?2, priority=?3, due_at=?4, updated_at=?5 WHERE id=?6",
            params![title, note, priority, due_at, now, id],
        )?;
        Ok(())
    }

    pub fn delete_todo(&self, id: &str) -> Result<bool> {
        let n = self
            .connection
            .execute("DELETE FROM todos WHERE id=?1", [id])?;
        Ok(n > 0)
    }

    // ── 人物关系 ────────────────────────────────────────────────────────

    /// 新建或刷新一条人物关系（(from, to, relation) 唯一，重复则更新 note / 时间戳）。
    pub fn upsert_relation(&self, draft: &RelationDraft) -> Result<Relation> {
        let now = chrono::Utc::now().to_rfc3339();
        self.connection.execute(
            "INSERT INTO relations
               (id, from_slug, from_kind, to_slug, to_kind, relation, note, confidence,
                source_conversation_id, source_event_id, created_at, last_seen_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)
             ON CONFLICT(from_slug, to_slug, relation) DO UPDATE SET
               note = ?7, confidence = ?8, source_event_id = COALESCE(?10, relations.source_event_id), last_seen_at = ?11",
            params![
                Uuid::new_v4().to_string(),
                draft.from_slug,
                draft.from_kind,
                draft.to_slug,
                draft.to_kind,
                draft.relation,
                draft.note,
                draft.confidence,
                draft.source_conversation_id,
                draft.source_event_id,
                now.clone(),
            ],
        )?;
        // 读回真实行（拿到真实 id）
        let mut statement = self.connection.prepare(
            "SELECT id, from_slug, from_kind, to_slug, to_kind, relation, note, confidence,
                    created_at, last_seen_at
             FROM relations
             WHERE from_slug = ?1 AND to_slug = ?2 AND relation = ?3",
        )?;
        let rel = statement
            .query_row(
                params![draft.from_slug, draft.to_slug, draft.relation],
                Self::map_relation,
            )
            .optional()?
            .unwrap_or(Relation {
                id: String::new(),
                from_slug: draft.from_slug.clone(),
                from_kind: draft.from_kind.clone(),
                to_slug: draft.to_slug.clone(),
                to_kind: draft.to_kind.clone(),
                relation: draft.relation.clone(),
                note: draft.note.clone(),
                confidence: draft.confidence,
                created_at: now.clone(),
                last_seen_at: now,
            });
        Ok(rel)
    }

    fn map_relation(row: &rusqlite::Row) -> rusqlite::Result<Relation> {
        Ok(Relation {
            id: row.get(0)?,
            from_slug: row.get(1)?,
            from_kind: row.get(2)?,
            to_slug: row.get(3)?,
            to_kind: row.get(4)?,
            relation: row.get(5)?,
            note: row.get(6)?,
            confidence: row.get(7)?,
            created_at: row.get(8)?,
            last_seen_at: row.get(9)?,
        })
    }

    /// 与某页相关的关系（双向：作为人物方或作为事情/项目方）
    pub fn list_relations_for_page(&self, slug: &str) -> Result<Vec<Relation>> {
        let mut statement = self.connection.prepare(
            "SELECT id, from_slug, from_kind, to_slug, to_kind, relation, note, confidence,
                    created_at, last_seen_at
             FROM relations WHERE from_slug = ?1 OR to_slug = ?1
             ORDER BY last_seen_at DESC",
        )?;
        let rows = statement
            .query_map(params![slug], Self::map_relation)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// 全部人物关系（备用：未来人物视图用）
    pub fn list_relations(&self) -> Result<Vec<Relation>> {
        let mut statement = self.connection.prepare(
            "SELECT id, from_slug, from_kind, to_slug, to_kind, relation, note, confidence,
                    created_at, last_seen_at
             FROM relations ORDER BY last_seen_at DESC",
        )?;
        let rows = statement
            .query_map([], Self::map_relation)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn delete_relation(&self, id: &str) -> Result<bool> {
        let n = self
            .connection
            .execute("DELETE FROM relations WHERE id=?1", [id])?;
        Ok(n > 0)
    }

    // ── pending actions (continued) ─────────────────────────────────────
    /// 列出某个对话里待确认的动作（按时间先后）
    pub fn pending_actions_for_conversation(
        &self,
        conversation_id: &str,
    ) -> Result<Vec<PendingAction>> {
        let mut statement = self.connection.prepare(
            "SELECT id, conversation_id, action, args_json, created_at
             FROM pending_actions
             WHERE conversation_id = ?1 AND status = 'pending'
             ORDER BY created_at ASC",
        )?;
        let rows = statement
            .query_map(params![conversation_id], |row| {
                Ok(PendingAction {
                    id: row.get(0)?,
                    conversation_id: row.get(1)?,
                    action: row.get(2)?,
                    args_json: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn action_exists_for_event(
        &self,
        conversation_id: &str,
        action: &str,
        event_id: &str,
    ) -> Result<bool> {
        let needle = format!("%{}%", event_id);
        Ok(self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM pending_actions WHERE conversation_id=?1 AND action=?2 AND args_json LIKE ?3)",
            params![conversation_id, action, needle], |row| row.get(0))?)
    }

    /// 删除一条待确认动作（执行完或用户拒绝后清理）
    pub fn delete_pending_action(&self, id: &str) -> Result<()> {
        self.connection
            .execute("DELETE FROM pending_actions WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn update_pending_action_args(&self, id: &str, args_json: &str) -> Result<bool> {
        Ok(self.connection.execute(
            "UPDATE pending_actions SET args_json=?1 WHERE id=?2 AND status='pending'",
            params![args_json, id],
        )? > 0)
    }

    pub fn decline_pending_action(&self, id: &str) -> Result<bool> {
        Ok(self.connection.execute(
            "UPDATE pending_actions SET status='declined' WHERE id=?1 AND status='pending'",
            [id],
        )? > 0)
    }

    /// 清空某个对话的全部待确认动作，返回删除条数
    pub fn delete_pending_actions_for_conversation(&self, conversation_id: &str) -> Result<usize> {
        let affected = self.connection.execute(
            "DELETE FROM pending_actions WHERE conversation_id = ?1",
            params![conversation_id],
        )?;
        Ok(affected)
    }

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

    /// 最近的历史用户消息（跨对话，按时间倒序取 limit 条，每条截断 max_chars）。
    /// 用于给 AI 注入「近期发生过的事」，弥补单对话上下文的记忆断层。
    pub fn recent_user_messages(
        &self,
        limit: usize,
        max_chars: usize,
    ) -> Result<Vec<RecentUserMessage>> {
        let mut statement = self.connection.prepare(
            "SELECT conversation_id, substr(content, 1, ?2) FROM messages
             WHERE role = 'user'
             ORDER BY created_at DESC
             LIMIT ?1",
        )?;
        let rows = statement.query_map(params![limit as i64, max_chars as i64], |row| {
            Ok(RecentUserMessage {
                conversation_id: row.get(0)?,
                content: row.get(1)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    // Conversation management

    pub fn create_conversation(&self, title: Option<&str>, tag: Option<&str>) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        self.connection.execute(
            "INSERT INTO conversations (id, title, tag, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)",
            params![id, title, tag, now],
        )?;
        Ok(id)
    }

    /// 查询某个知识页是否已有处理会话（页内 AI 聊天），返回会话 id
    pub fn find_wiki_chat_conversation(&self, wiki_page_slug: &str) -> Result<Option<String>> {
        let mut statement = self.connection.prepare(
            "SELECT id FROM conversations
             WHERE wiki_page_slug = ?1 AND archived = 0
             ORDER BY updated_at DESC LIMIT 1",
        )?;
        let id = statement
            .query_row(params![wiki_page_slug], |row| row.get::<_, String>(0))
            .optional()?;
        Ok(id)
    }

    /// 为知识页创建处理会话（带 wiki_page_slug 关联）
    pub fn create_wiki_chat_conversation(
        &self,
        wiki_page_slug: &str,
        title: &str,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        self.connection.execute(
            "INSERT INTO conversations (id, title, tag, wiki_page_slug, created_at, updated_at)
             VALUES (?1, ?2, 'idea', ?3, ?4, ?4)",
            params![id, title, wiki_page_slug, now],
        )?;
        Ok(id)
    }

    pub fn list_conversations(&self) -> Result<Vec<ConversationSummary>> {
        let mut statement = self.connection.prepare(
            "SELECT c.id, c.title, c.tag, c.created_at, c.updated_at, c.archived,
                    c.wiki_page_slug,
                    COUNT(m.id) as message_count,
                    (SELECT m2.content FROM messages m2
                     WHERE m2.conversation_id = c.id
                     ORDER BY m2.created_at DESC LIMIT 1) as last_message
             FROM conversations c
             LEFT JOIN messages m ON m.conversation_id = c.id
             WHERE c.archived = 0 AND c.wiki_page_slug IS NULL
             GROUP BY c.id
             ORDER BY c.updated_at DESC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(ConversationSummary {
                id: row.get(0)?,
                title: row.get(1)?,
                tag: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
                message_count: row.get(7)?,
                last_message_preview: row.get(8)?,
                archived: row.get(5)?,
                wiki_page_slug: row.get(6)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// Archived conversations, newest first.
    pub fn list_archived_conversations(&self) -> Result<Vec<ConversationSummary>> {
        let mut statement = self.connection.prepare(
            "SELECT c.id, c.title, c.tag, c.created_at, c.updated_at, c.archived,
                    c.wiki_page_slug,
                    COUNT(m.id) as message_count,
                    (SELECT m2.content FROM messages m2
                     WHERE m2.conversation_id = c.id
                     ORDER BY m2.created_at DESC LIMIT 1) as last_message
             FROM conversations c
             LEFT JOIN messages m ON m.conversation_id = c.id
             WHERE c.archived = 1 AND c.wiki_page_slug IS NULL
             GROUP BY c.id
             ORDER BY c.updated_at DESC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(ConversationSummary {
                id: row.get(0)?,
                title: row.get(1)?,
                tag: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
                message_count: row.get(7)?,
                last_message_preview: row.get(8)?,
                archived: row.get(5)?,
                wiki_page_slug: row.get(6)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn rename_conversation(&self, conversation_id: &str, title: &str) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.connection.execute(
            "UPDATE conversations SET title = ?1, updated_at = ?2 WHERE id = ?3",
            params![title, now, conversation_id],
        )?;
        Ok(())
    }

    pub fn set_conversation_archived(&self, conversation_id: &str, archived: bool) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.connection.execute(
            "UPDATE conversations SET archived = ?1, updated_at = ?2 WHERE id = ?3",
            params![archived as i64, now, conversation_id],
        )?;
        Ok(())
    }

    /// 删除已归档的普通对话；知识页专用会话和未归档对话均拒绝删除。
    pub fn delete_archived_conversation(&self, conversation_id: &str) -> Result<bool> {
        let eligible: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM conversations WHERE id=?1 AND archived=1 AND wiki_page_slug IS NULL)",
            [conversation_id], |row| row.get(0))?;
        if !eligible {
            return Ok(false);
        }
        Ok(self
            .connection
            .execute("DELETE FROM conversations WHERE id=?1", [conversation_id])?
            > 0)
    }

    pub fn get_conversation(&self, conversation_id: &str) -> Result<Option<ConversationSummary>> {
        self.connection
            .query_row(
                "SELECT c.id, c.title, c.tag, c.created_at, c.updated_at, c.archived,
                        c.wiki_page_slug,
                        COUNT(m.id) as message_count,
                        (SELECT m2.content FROM messages m2
                         WHERE m2.conversation_id = c.id
                         ORDER BY m2.created_at DESC LIMIT 1) as last_message
                 FROM conversations c
                 LEFT JOIN messages m ON m.conversation_id = c.id
                 WHERE c.id = ?1
                 GROUP BY c.id",
                [conversation_id],
                |row| {
                    Ok(ConversationSummary {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        tag: row.get(2)?,
                        created_at: row.get(3)?,
                        updated_at: row.get(4)?,
                        message_count: row.get(7)?,
                        last_message_preview: row.get(8)?,
                        archived: row.get(5)?,
                        wiki_page_slug: row.get(6)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    // Message management
    pub fn send_message(
        &self,
        conversation_id: &str,
        role: &str,
        content: &str,
        parent_message_id: Option<&str>,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        let transaction = self.connection.unchecked_transaction()?;

        // Insert message
        transaction.execute(
            "INSERT INTO messages (id, conversation_id, parent_message_id, role, content, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, conversation_id, parent_message_id, role, content, now],
        )?;

        // Update conversation timestamp
        transaction.execute(
            "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
            params![now, conversation_id],
        )?;

        transaction.commit()?;
        Ok(id)
    }

    // Token usage tracking
    /// 记录一次 AI 调用的 token 用量（provider 返回的 usage；缺失时由调用方本地估算兜底）
    pub fn record_token_usage(
        &self,
        conversation_id: Option<&str>,
        prompt_tokens: i64,
        completion_tokens: i64,
        total_tokens: i64,
        model: Option<&str>,
    ) -> Result<()> {
        self.connection.execute(
            "INSERT INTO token_usage (id, conversation_id, prompt_tokens, completion_tokens, total_tokens, model, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                Uuid::new_v4().to_string(),
                conversation_id,
                prompt_tokens,
                completion_tokens,
                total_tokens,
                model,
                chrono::Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    /// 按天聚合最近 N 天的 token 用量（含当天），按日期倒序
    pub fn daily_token_usage(&self, days: u32) -> Result<Vec<DailyTokenUsage>> {
        let offset = format!("-{} days", days);
        let mut statement = self.connection.prepare(
            "SELECT substr(created_at, 1, 10) AS day,
                    SUM(prompt_tokens), SUM(completion_tokens), SUM(total_tokens), COUNT(*)
             FROM token_usage
             WHERE created_at >= datetime('now', ?1)
             GROUP BY day
             ORDER BY day DESC",
        )?;
        let rows = statement.query_map(params![offset], |row| {
            Ok(DailyTokenUsage {
                date: row.get(0)?,
                prompt_tokens: row.get(1)?,
                completion_tokens: row.get(2)?,
                total_tokens: row.get(3)?,
                call_count: row.get(4)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn list_messages(&self, conversation_id: &str) -> Result<Vec<MessageSummary>> {
        let mut statement = self.connection.prepare(
            "SELECT id, conversation_id, parent_message_id, role, content, created_at
             FROM messages
             WHERE conversation_id = ?1
             ORDER BY created_at ASC",
        )?;
        let rows = statement.query_map([conversation_id], |row| {
            Ok(MessageSummary {
                id: row.get(0)?,
                conversation_id: row.get(1)?,
                parent_message_id: row.get(2)?,
                role: row.get(3)?,
                content: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    // Get child messages of a specific message (for branching conversations)
    pub fn get_child_messages(&self, parent_id: &str) -> Result<Vec<MessageSummary>> {
        let mut statement = self.connection.prepare(
            "SELECT id, conversation_id, parent_message_id, role, content, created_at
             FROM messages
             WHERE parent_message_id = ?1
             ORDER BY created_at ASC",
        )?;
        let rows = statement.query_map([parent_id], |row| {
            Ok(MessageSummary {
                id: row.get(0)?,
                conversation_id: row.get(1)?,
                parent_message_id: row.get(2)?,
                role: row.get(3)?,
                content: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    // Get the message chain from root to a specific message
    pub fn get_message_chain(&self, message_id: &str) -> Result<Vec<MessageSummary>> {
        let mut chain = Vec::new();
        let mut current_id = Some(message_id.to_string());

        while let Some(id) = current_id {
            let message: MessageSummary = self.connection.query_row(
                "SELECT id, conversation_id, parent_message_id, role, content, created_at
                 FROM messages WHERE id = ?1",
                [&id],
                |row| {
                    Ok(MessageSummary {
                        id: row.get(0)?,
                        conversation_id: row.get(1)?,
                        parent_message_id: row.get(2)?,
                        role: row.get(3)?,
                        content: row.get(4)?,
                        created_at: row.get(5)?,
                    })
                },
            )?;

            current_id = message.parent_message_id.clone();
            chain.push(message);
        }

        chain.reverse(); // Root to leaf order
        Ok(chain)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

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
        store.upsert_wiki_page(&draft).unwrap();
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
        store.upsert_wiki_page(&src_draft).unwrap();
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
            .upsert_wiki_page(&WikiPageDraft {
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
            })
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
            .upsert_wiki_page(&WikiPageDraft {
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
            })
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
}
