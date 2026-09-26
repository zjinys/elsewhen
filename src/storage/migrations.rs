//! Schema 与版本迁移（从 `Store::open` 抽出）。
//!
//! 历史迁移按版本号幂等追加到 `schema_migrations`：`CREATE TABLE IF NOT EXISTS` /
//! `PRAGMA table_info` 探测后条件 `ALTER`，任意一步中途崩溃都可在下次 open 重放，
//! 不会产生半迁移状态。新增 schema 变更请继续沿用这一模式（探测 → 条件迁移 →
//! `INSERT OR IGNORE INTO schema_migrations`）。
//!
//! 迁移操作的是已建立的连接（PRAGMA/WAL 由 `Store::open` 在调用前设置）。

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};

use super::derive_conversation_title;

/// 建立全部表结构并把所有未应用的版本迁移推进到最新。幂等，可重复调用。
pub(crate) fn ensure_schema(connection: &Connection) -> Result<()> {
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
// 版本 30：区分主对话秘书角色与知识页导师角色。
let has_assistant_mode = {
    let mut statement = connection.prepare("PRAGMA table_info(conversations)")?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    columns.iter().any(|name| name == "assistant_mode")
};
if !has_assistant_mode {
    connection.execute_batch(
        "ALTER TABLE conversations ADD COLUMN assistant_mode TEXT NOT NULL DEFAULT 'personal_secretary';
         INSERT OR IGNORE INTO schema_migrations(version, applied_at)
         VALUES (30, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));",
    )?;
} else {
    connection.execute(
        "INSERT OR IGNORE INTO schema_migrations(version, applied_at) VALUES (30, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
        [],
    )?;
}
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
// 版本 29：wiki 页「人类直接编辑」基础 —— kind 双语义拆分 + 人工编辑保护位 + 素材观点评价。
//  - kind 拆分：用户粘贴笔记（slug `note-` 前缀）从 kind=topic 拆为新 kind=note（采集素材档），
//    AI 提炼的主题页保持 topic。权限判定不再单看 kind 字段，而是 (kind, slug 前缀) 组合。
//  - human_edited_at：非空 ⇔ 该页曾被人工编辑，digest 不得整篇覆盖正文（内容列只读，证据照累）。
//  - opinion：素材页（source/note）观点评价（'endorse'/'reject'/NULL=未表态，缺省认可）。
// 列结构（ALTER 需判存在）与数据 backfill（幂等规则，每次执行无副作用：只命中
// `kind='topic' AND slug LIKE 'note-%'` 这一条确定性分类，新建页本身就是 note）分开处理。
{
    let has_column = |name: &str| -> rusqlite::Result<bool> {
        let mut statement = connection.prepare("PRAGMA table_info(wiki_pages)")?;
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(columns.iter().any(|c| c == name))
    };
    let has_human_edited_at = has_column("human_edited_at")?;
    let has_opinion = has_column("opinion")?;
    // 两列分开判存在再各自 ALTER：execute_batch 中途崩溃只会丢
    // 未执行的那列，下次 open 不会因第一列已存在而永久跳过第二列。
    if !has_human_edited_at {
        connection.execute("ALTER TABLE wiki_pages ADD COLUMN human_edited_at TEXT", [])?;
    }
    if !has_opinion {
        connection.execute("ALTER TABLE wiki_pages ADD COLUMN opinion TEXT", [])?;
    }
    if !has_human_edited_at || !has_opinion {
        connection.execute(
            "INSERT OR IGNORE INTO schema_migrations(version, applied_at)
             VALUES (29, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
            [],
        )?;
    }
}
// 数据 backfill 与列无关，恒执行：旧库（v29 前已写入的存量）和
// 升级中途落库的 `note-` 前缀 topic 页都会在这一步归位为 kind='note'。
connection.execute(
    "UPDATE wiki_pages SET kind='note' WHERE kind='topic' AND slug LIKE 'note-%'",
    [],
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
    Ok(())
}
