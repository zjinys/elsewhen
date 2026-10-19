//! ConversationSnapshot 与 Store 连接载体（open/事务说明/Clone）。

use super::*;

#[derive(Clone)]
pub(crate) struct ConversationSnapshot {
    pub conversation_id: String,
    pub input_message_id: String,
    pub messages: std::sync::Arc<[MessageSummary]>,
}

pub struct Store {
    // pub(crate) 而非私有：storage.rs 拆出的子模块（migrations / provider / …）
    // 在同一 crate 的 `impl Store` 块里需要直接访问连接与路径。
    pub(crate) connection: Connection,
    pub(crate) path: std::path::PathBuf,
    pub(crate) provider_master: Option<[u8; 32]>,
    pub(crate) conversation_snapshot: Option<ConversationSnapshot>,
    pub(crate) chat_run_identity: Option<crate::ai::generation::RunIdentity>,
    pub(crate) captured_page_task: std::cell::RefCell<Option<(String, chat_page_tasks::PageTask)>>,
}

impl Clone for Store {
    fn clone(&self) -> Self {
        let mut store = Self::open(&self.path).expect("reopen database");
        store.provider_master = self.provider_master;
        store.conversation_snapshot = self.conversation_snapshot.clone();
        store.chat_run_identity = self.chat_run_identity.clone();
        store.captured_page_task = self.captured_page_task.clone();
        store
    }
}

impl Store {
    /// Local compound writes may join a caller-owned transaction. The caller
    /// must propagate errors and own rollback/commit for the entire operation.
    pub(crate) fn begin_write_transaction(&self) -> Result<Option<rusqlite::Transaction<'_>>> {
        if self.connection.is_autocommit() {
            Ok(Some(rusqlite::Transaction::new_unchecked(
                &self.connection,
                rusqlite::TransactionBehavior::Immediate,
            )?))
        } else {
            Ok(None)
        }
    }

    pub fn open(path: &Path) -> Result<Self> {
        let mut connection = Connection::open(path)
            .with_context(|| format!("open SQLite database {}", path.display()))?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.pragma_update(None, "foreign_keys", true)?;
        let initialized = INITIALIZED_DATABASES
            .get_or_init(|| Mutex::new(HashSet::new()))
            .lock()
            .map_err(|_| anyhow::anyhow!("database initialization lock poisoned"))?;
        let first_open = !initialized.contains(path);
        if first_open {
            // All databases use the embedded, transactional migration set.
            new_migrations::initialize(&mut connection)?;
            knowledge::backfill_sources_once(&connection)?;
            #[cfg(unix)]
            crate::config::set_unix_permissions(path, 0o600)?;
        }
        let store = Self {
            connection,
            path: path.to_path_buf(),
            provider_master: None,
            conversation_snapshot: None,
            chat_run_identity: None,
            captured_page_task: Default::default(),
        };
        if first_open {
            // 启动期一次性完成存量回填和 WAL 收敛；普通 API 打开连接不再写库。
            store.backfill_project_source_urls()?;
            store.backfill_wiki_image_placeholders()?;
            crate::knowledge::dependencies::backfill_epochs(&store)?;
            let _ = store
                .connection
                .execute_batch("PRAGMA wal_checkpoint(PASSIVE);");
        }
        Ok(store)
    }

    /// 后台 worker 路径专用打开方式：不做迁移/回填/checkpoint 等启动写入。
    /// 库版本落后于代码预期（init 尚未跑完或库待升级）时返回 `Ok(None)`，
    /// 调用方按「本轮无事可做」处理；绝不能由后台补跑启动写入。
    pub fn open_background(path: &Path) -> Result<Option<Self>> {
        let expected = new_migrations::latest_version()?;
        let current: i64 = {
            let conn = Connection::open(path)
                .with_context(|| format!("open SQLite database {}", path.display()))?;
            conn.query_row("PRAGMA main.user_version", [], |row| row.get(0))?
        };
        if current < expected {
            return Ok(None);
        }
        // 版本已就绪：标记为已初始化，让 open 走快路径（不写库）。
        Self::mark_initialized(path)?;
        Self::open(path).map(Some)
    }

    /// Mark a database as initialized after the bridge startup sequence.
    pub fn mark_initialized(path: &Path) -> Result<()> {
        INITIALIZED_DATABASES
            .get_or_init(|| Mutex::new(HashSet::new()))
            .lock()
            .map_err(|_| anyhow::anyhow!("database initialization lock poisoned"))?
            .insert(path.to_path_buf());
        Ok(())
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
