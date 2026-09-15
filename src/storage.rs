mod adapter;

use crate::event::{EventSummary, NewEvent};
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use std::path::Path;
use uuid::Uuid;

pub use adapter::{AnalysisJob, AnalysisSummary, AiProviderConfig, StorageAdapter};

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
}

/// upsert 结果
#[derive(Debug, Clone)]
pub struct WikiUpsertOutcome {
    pub created: bool,
    pub page: WikiPage,
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
        let now = chrono::Utc::now().to_rfc3339();
        self.connection.execute(
            "INSERT INTO ai_provider_configs
             (id,name,provider_type,base_url,model,api_key_source,api_key,enabled,created_at,updated_at)
             VALUES (?1,'default','openai-compatible',?2,?3,'database',?4,1,?5,?5)
             ON CONFLICT(name) DO UPDATE SET base_url=excluded.base_url, model=excluded.model,
             provider_type=excluded.provider_type, api_key_source=excluded.api_key_source, api_key=excluded.api_key,
             enabled=1, updated_at=excluded.updated_at",
            params![Uuid::new_v4().to_string(), base_url, model, api_key, now],
        )?;
        Ok(())
    }

    pub fn active_ai_provider_config(&self) -> Result<Option<AiProviderConfig>> {
        self.connection
            .query_row(
            "SELECT provider_type,base_url,model,api_key_source,api_key FROM ai_provider_configs
             WHERE enabled=1 AND api_key IS NOT NULL ORDER BY updated_at DESC LIMIT 1",
                [],
                |row| {
                    Ok(AiProviderConfig {
                        provider_type: row.get(0)?,
                        base_url: row.get(1)?,
                        model: row.get(2)?,
                    api_key_source: row.get(3)?,
                    api_key: row.get(4)?,
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
                    evidence_count, first_seen_at, last_seen_at, status, created_at, updated_at
             FROM wiki_pages WHERE slug = ?1",
        )?;
        let page = statement
            .query_row(params![slug], |row| map_wiki_page(row))
            .optional()?;
        Ok(page)
    }

    pub fn list_wiki_pages(&self, kind: Option<&str>) -> Result<Vec<WikiPage>> {
        let mut statement = match kind {
            Some(_) => self.connection.prepare(
                "SELECT id, slug, kind, title, summary, content_md, tags, source_event_ids,
                        evidence_count, first_seen_at, last_seen_at, status, created_at, updated_at
                 FROM wiki_pages WHERE kind = ?1 ORDER BY last_seen_at DESC",
            )?,
            None => self.connection.prepare(
                "SELECT id, slug, kind, title, summary, content_md, tags, source_event_ids,
                        evidence_count, first_seen_at, last_seen_at, status, created_at, updated_at
                 FROM wiki_pages ORDER BY kind ASC, last_seen_at DESC",
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
                     evidence_count=?6, last_seen_at=?7, status=?8, updated_at=?7
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
                  evidence_count, first_seen_at, last_seen_at, status, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10, ?11, ?10, ?10)",
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
                ],
            )?;
            self.record_wiki_revision(&id, &draft.content_md, &draft.reason, None)?;
            Ok(WikiUpsertOutcome {
                created: true,
                page: self.get_wiki_page(&draft.slug)?.unwrap(),
            })
        }
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

    pub fn list_conversations(&self) -> Result<Vec<ConversationSummary>> {
        let mut statement = self.connection.prepare(
            "SELECT c.id, c.title, c.tag, c.created_at, c.updated_at,
                    COUNT(m.id) as message_count,
                    (SELECT m2.content FROM messages m2
                     WHERE m2.conversation_id = c.id
                     ORDER BY m2.created_at DESC LIMIT 1) as last_message
             FROM conversations c
             LEFT JOIN messages m ON m.conversation_id = c.id
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
                message_count: row.get(5)?,
                last_message_preview: row.get(6)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn get_conversation(&self, conversation_id: &str) -> Result<Option<ConversationSummary>> {
        self.connection
            .query_row(
                "SELECT c.id, c.title, c.tag, c.created_at, c.updated_at,
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
                        message_count: row.get(5)?,
                        last_message_preview: row.get(6)?,
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
}
