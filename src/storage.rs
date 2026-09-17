mod adapter;

use crate::event::{EventSummary, NewEvent};
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use std::path::Path;
use uuid::Uuid;

pub use adapter::{
    AiProviderConfigRow, AnalysisJob, AnalysisSummary, AiProviderConfig, StorageAdapter,
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
}

/// 带 id 的事件记录（digest 需要把事件 id 写进 wiki 页作为溯源）
#[derive(Debug, Clone)]
pub struct EventRecord {
    pub id: String,
    pub recorded_at: String,
    pub raw_text: String,
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
    })
}

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
                let mut statement =
                    connection.prepare("PRAGMA table_info(ai_provider_configs)")?;
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
               created_at TEXT NOT NULL,
               last_seen_at TEXT NOT NULL,
               UNIQUE(from_slug, to_slug, relation)
             );
             CREATE INDEX IF NOT EXISTS idx_relations_from ON relations(from_slug);
             CREATE INDEX IF NOT EXISTS idx_relations_to ON relations(to_slug);
             INSERT OR IGNORE INTO schema_migrations(version, applied_at)
             VALUES (17, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));",
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
                    params![name, provider_type, base_url, model, api_key, temperature, max_tokens, now, pid],
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
        let deleted = transaction.execute(
            "DELETE FROM ai_provider_configs WHERE id=?1",
            params![id],
        )?;
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
                    source_url
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
                    source_url
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
                    source_url
             FROM wiki_pages WHERE lower(trim(title)) = lower(trim(?1))
             ORDER BY updated_at DESC LIMIT 1",
        )?;
        let page = statement
            .query_row(params![title], |row| map_wiki_page(row))
            .optional()?;
        Ok(page)
    }

    pub fn list_wiki_pages(&self, kind: Option<&str>) -> Result<Vec<WikiPage>> {
        let mut statement = match kind {
            Some(_) => self.connection.prepare(
                "SELECT id, slug, kind, title, summary, content_md, tags, source_event_ids,
                        evidence_count, first_seen_at, last_seen_at, status, created_at, updated_at,
                        source_url
                 FROM wiki_pages WHERE kind = ?1 ORDER BY last_seen_at DESC",
            )?,
            None => self.connection.prepare(
                "SELECT id, slug, kind, title, summary, content_md, tags, source_event_ids,
                        evidence_count, first_seen_at, last_seen_at, status, created_at, updated_at,
                        source_url
                 FROM wiki_pages ORDER BY source_url IS NULL, last_seen_at DESC",
            )?,
        };
        let rows = match kind {
            Some(k) => statement.query_map(params![k], map_wiki_page)?,
            None => statement.query_map([], map_wiki_page)?,
        };
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
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
            self.connection.execute(
                "INSERT INTO wiki_pages
                 (id, slug, kind, title, summary, content_md, tags, source_event_ids,
                  evidence_count, first_seen_at, last_seen_at, status, created_at, updated_at,
                  source_url)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10, ?11, ?10, ?10, ?12)",
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
    pub fn rename_wiki_page(&self, slug: &str, new_title: &str, reason: &str) -> Result<RenameWikiOutcome> {
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
            Some((prefix, _)) => crate::wiki::unique_slug(self, &format!("{prefix}/{}", crate::wiki::slugify(&new_title)), &new_title)?,
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
            relations_moved += tx
                .execute("UPDATE relations SET from_slug=?1 WHERE from_slug=?2", params![new_slug, slug])?
                as usize;
            relations_moved += tx
                .execute("UPDATE relations SET to_slug=?1 WHERE to_slug=?2", params![new_slug, slug])?
                as usize;
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
        let rows = statement
            .query_map(rusqlite::params_from_iter(param_refs), |row| {
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

    pub fn update_todo_status(&self, id: &str, status: TodoStatus) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.connection.execute(
            "UPDATE todos SET status=?1, updated_at=?2 WHERE id=?3",
            params![status.as_str(), now, id],
        )?;
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
                source_conversation_id, created_at, last_seen_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)
             ON CONFLICT(from_slug, to_slug, relation) DO UPDATE SET
               note = ?7, confidence = ?8, last_seen_at = ?10",
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

    /// 删除一条待确认动作（执行完或用户拒绝后清理）
    pub fn delete_pending_action(&self, id: &str) -> Result<()> {
        self.connection
            .execute("DELETE FROM pending_actions WHERE id = ?1", [id])?;
        Ok(())
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
             WHERE status = 'active' AND (title LIKE ?1 OR content_md LIKE ?1 OR tags LIKE ?1)
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
    pub fn send_message(&self, conversation_id: &str, role: &str, content: &str, parent_message_id: Option<&str>) -> Result<String> {
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

    fn complete_analysis(&self, job: &AnalysisJob, prompt_version: &str, result_json: &str) -> Result<()> {
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
        Store::save_ai_provider_config(self, id, name, provider_type, base_url, model, api_key, temperature, max_tokens)
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
        assert!(
            store.events_on_date(today - chrono::Duration::days(10)).unwrap().is_empty()
        );
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
            .create_todo("跟进付款", "normal", Some("2026-09-20"), None, None, Some("原始说明"))
            .unwrap();

        // 编辑：标题/说明/优先级/截止都改
        store
            .update_todo(&todo.id, "跟进双链路付款", Some("已和张玮对齐时间"), Some("high"), Some("2026-09-18"))
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
        assert!(store.update_todo(&todo.id, "   ", None, None, None).is_err());
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
        assert!(
            store.find_wiki_chat_conversation("topic/fpso111-尾款").unwrap().is_some()
        );
        assert!(store.find_wiki_chat_conversation("topic/付款流程").unwrap().is_none());
        // 修订历史多了一条重命名记录
        assert_eq!(
            store.list_wiki_revisions("topic/fpso111-尾款").unwrap().len(),
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
        let outcome = store.rename_wiki_page("tweet-123", "新标题", "更正").unwrap();
        assert!(outcome.changed);
        assert_eq!(outcome.new_slug, "tweet-123", "来源页 slug 应保持不变");
        assert_eq!(
            store.get_wiki_page("tweet-123").unwrap().unwrap().title,
            "新标题"
        );
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
        assert!(store.find_wiki_page_by_title("不存在的人").unwrap().is_none());
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
