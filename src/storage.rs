mod adapter;
mod migrations;
mod provider;
mod entities;

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
        human_edited_at: row.get(18)?,
        opinion: row.get(19)?,
    })
}

/// wiki_pages 行 → WikiPage 的公共列清单。
/// 顺序必须与 `map_wiki_page` 的按位取值（0..=19）严格一致。
const WIKI_PAGE_COLS: &str = "id, slug, kind, title, summary, content_md, tags, source_event_ids, \
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
            // 幂等写入原子化（P2-3）：不做「先查后插」（check-then-insert 有并发窗口，
            // 两个同键请求会同时判定不存在并双双 INSERT），直接用部分唯一索引兜底——
            // 同名键并发时 INSERT OR IGNORE 让后写者静默跳过，再回读既有记录返回，
            // 且首个请求按 pending 落库，与原来的插入行为一致。
            self.connection.execute(
                "INSERT OR IGNORE INTO input_records
                 (id,raw_text,source,route_status,idempotency_key,created_at,updated_at)
                 VALUES (?1,?2,?3,'pending',?4,?5,?5)",
                params![
                    Uuid::new_v4().to_string(),
                    raw_text,
                    source,
                    key,
                    chrono::Utc::now().to_rfc3339()
                ],
            )?;
            return self
                .get_input_record_by_idempotency_key(key)?
                .context("input record 幂等键写入后读取既有记录失败");
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
        // 幂等键的并发窗口（先查后插存在时间差）由部分唯一索引兜底：后到的
        // 重复键在这里触发 UNIQUE 冲突——必须回滚整个事务（否则 event+job 已成
        // 孤儿行），再按幂等键回读既有记录返回（P2-3）。
        let inserted = transaction.execute(
            "INSERT INTO input_records
             (id,raw_text,source,route_status,idempotency_key,event_id,created_at,updated_at)
             VALUES (?1,?2,?3,'routed',?4,?5,?6,?6)",
            params![input_id, raw_text, source, key, event_id, now_text],
        );
        match inserted {
            Ok(_) => {}
            Err(e)
                if e.to_string().contains("input_records.idempotency_key") && key.is_some() =>
            {
                transaction.rollback()?;
                return self
                    .get_input_record_by_idempotency_key(key.unwrap())?
                    .context("并发重复提交：回滚后读取既有 input record 失败");
            }
            Err(e) => return Err(e.into()),
        }
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
        // 与 submit_input_as_event 相同的幂等并发兜底（P2-3）：重复键冲突时
        // 回滚整个事务（event+job+message 一并撤销），再回读既有记录。
        let inserted = transaction.execute(
            "INSERT INTO input_records
             (id,raw_text,source,route_status,idempotency_key,event_id,message_id,created_at,updated_at)
             VALUES (?1,?2,'conversation','routed',?3,?4,?5,?6,?6)",
            params![input_id, raw_text, key, event_id, message_id, now],
        );
        match inserted {
            Ok(_) => {}
            Err(e)
                if e.to_string().contains("input_records.idempotency_key") && key.is_some() =>
            {
                transaction.rollback()?;
                return self
                    .get_input_record_by_idempotency_key(key.unwrap())?
                    .context("并发重复提交：回滚后读取既有 input record 失败");
            }
            Err(e) => return Err(e.into()),
        }
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
        let event_id = self
            .connection
            .query_row(
                "SELECT i.event_id FROM messages m
             LEFT JOIN input_records i ON i.message_id=m.id
             WHERE m.conversation_id=?1 AND m.role='user'
             ORDER BY m.created_at DESC LIMIT 1",
                [conversation_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(anyhow::Error::from)?;
        Ok(event_id.flatten())
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


    pub fn list_events(&self) -> Result<Vec<EventSummary>> {
        let mut statement = self.connection.prepare(
            "SELECT id, recorded_at, raw_text, source, status
             FROM events ORDER BY recorded_at DESC, id DESC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(EventSummary {
                id: row.get(0)?,
                recorded_at: row.get(1)?,
                raw_text: row.get(2)?,
                source: row.get(3)?,
                status: row.get(4)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// 按 id 删除事件（AI 尝试失败时回滚 WriteDirect 写入用，见 conversation.rs）。
    /// events 的防改触发器只拦 UPDATE 特定列，不拦 DELETE。
    pub fn delete_event(&self, id: &str) -> Result<bool> {
        let n = self
            .connection
            .execute("DELETE FROM events WHERE id=?1", [id])?;
        Ok(n > 0)
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
            "SELECT id, recorded_at, raw_text, source, status FROM events
             WHERE recorded_at >= ?1 AND recorded_at < ?2
             ORDER BY recorded_at DESC, id DESC",
        )?;
        let rows = statement.query_map(params![start_utc, end_utc], |row| {
            Ok(EventSummary {
                id: row.get(0)?,
                recorded_at: row.get(1)?,
                raw_text: row.get(2)?,
                source: row.get(3)?,
                status: row.get(4)?,
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
            "SELECT id, recorded_at, raw_text, source, status FROM events
             WHERE recorded_at >= ?1 ORDER BY recorded_at DESC, id DESC LIMIT ?2",
        )?;
        let rows = statement.query_map(params![since, limit as i64], |row| {
            Ok(EventSummary {
                id: row.get(0)?,
                recorded_at: row.get(1)?,
                raw_text: row.get(2)?,
                source: row.get(3)?,
                status: row.get(4)?,
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
             WHERE recorded_at >= ?1 ORDER BY recorded_at ASC, id ASC LIMIT ?2",
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
                    source_url, COALESCE(area, 'insight'), based_on, content_type, human_edited_at, opinion
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
                    source_url, COALESCE(area, 'insight'), based_on, content_type, human_edited_at, opinion
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
                    source_url, COALESCE(area, 'insight'), based_on, content_type, human_edited_at, opinion
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
                    source_url, COALESCE(area, 'insight'), based_on, content_type, human_edited_at, opinion
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
                    p.source_url, COALESCE(p.area, 'insight'), p.based_on, p.content_type,
                    p.human_edited_at, p.opinion
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
                    source_url, COALESCE(area, 'insight'), based_on, content_type, human_edited_at, opinion
             FROM wiki_pages WHERE based_on = ?1 AND area = 'derivative'
             ORDER BY created_at DESC",
        )?;
        let rows = statement.query_map(params![based_on], map_wiki_page)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// 按标签精确匹配定位知识页（tags 为 JSON 数组字符串，用 instr 做含有匹配，
    /// 避免路径分隔字符演义的 LIKE 转义问题）。上限取最近更新的一页。
    pub fn find_wiki_page_by_tag(&self, tag: &str) -> Result<Option<WikiPage>> {
        let mut statement = self.connection.prepare(
            "SELECT id, slug, kind, title, summary, content_md, tags, source_event_ids,
                    evidence_count, first_seen_at, last_seen_at, status, created_at, updated_at,
                    source_url, COALESCE(area, 'insight'), based_on, content_type, human_edited_at, opinion
             FROM wiki_pages
             WHERE instr(tags, ?1) > 0
             ORDER BY updated_at DESC LIMIT 1",
        )?;
        let rows = statement.query_map(params![tag], map_wiki_page)?;
        let mut pages = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(pages.pop())
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
        if kind == "person"
            || kind == "project"
            || slug.starts_with("person/")
            || slug.starts_with("project/")
            || slug.starts_with("topic/")
        {
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
    pub fn upsert_wiki_page(
        &self,
        draft: &WikiPageDraft,
        policy: ContentPolicy,
    ) -> Result<WikiUpsertOutcome> {
        let now = chrono::Utc::now().to_rfc3339();
        let tags_raw = serde_json::to_string(&draft.tags)?;

        let existing = self.get_wiki_page(&draft.slug)?;
        if let Some(page) = existing {
            // 人工持有判定：仅 PreserveHumanEdits 拦截——human_edited_at 非空
            // （人工编辑过的档案/派生页）或 kind∈{source,note}（采集素材，对人和
            // AI 都只读正文，仅素材导入流用 Always 刷新）。
            // Always 是显式授权路径（素材导入；AI 草拟→用户确认的修订/建档），不拦。
            // 保护时正文（content_md/title/summary/tags）不变，只并集证据 + 刷新 last_seen_at。
            let human_held = match policy {
                ContentPolicy::Always => false,
                ContentPolicy::PreserveHumanEdits => {
                    page.human_edited_at.is_some()
                        || matches!(page.kind.as_str(), "source" | "note")
                }
            };
            let content_changed = page.content_md != draft.content_md;
            if human_held && content_changed {
                // 合并（确定性，不允许 LLM 直接改数字）：只累加证据。
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
                     SET source_event_ids=?1, evidence_count=?2, last_seen_at=?3, updated_at=?3
                     WHERE id=?4",
                    params![sources_raw, evidence_count, now, page.id,],
                )?;
                let updated = self.get_wiki_page(&draft.slug)?.unwrap();
                return Ok(WikiUpsertOutcome {
                    created: false,
                    page: updated,
                    protected: true,
                });
            }
            // 合并（确定性，不允许 LLM 直接改数字）
            let mut all_ids = page.source_event_ids.clone();
            for id in &draft.source_event_ids {
                if !all_ids.contains(id) {
                    all_ids.push(id.clone());
                }
            }
            let evidence_count = all_ids.len() as i64;
            let sources_raw = serde_json::to_string(&all_ids)?;
            // 页面写入与 revision 同事务提交：审计链与内容不脱节
            let tx = self.connection.unchecked_transaction()?;
            tx.execute(
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
            Self::record_wiki_revision_on(&tx, &page.id, &draft.content_md, &draft.reason, None)?;
            tx.commit()?;
            let updated = self.get_wiki_page(&draft.slug)?.unwrap();
            Ok(WikiUpsertOutcome {
                created: false,
                page: updated,
                protected: false,
            })
        } else {
            let id = Uuid::new_v4().to_string();
            let sources_raw = serde_json::to_string(&draft.source_event_ids)?;
            let evidence_count = draft.source_event_ids.len().max(1) as i64;
            // 新建页自动推导分区（旧页保留原分区，见上方 UPDATE 分支）
            let area =
                Self::derive_wiki_area(&draft.kind, &draft.slug, draft.source_url.as_deref());
            // 建页与首条 revision 同事务提交
            let tx = self.connection.unchecked_transaction()?;
            tx.execute(
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
            Self::record_wiki_revision_on(&tx, &id, &draft.content_md, &draft.reason, None)?;
            tx.commit()?;
            Ok(WikiUpsertOutcome {
                created: true,
                page: self.get_wiki_page(&draft.slug)?.unwrap(),
                protected: false,
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

    /// 修改一张项目页关联的本地目录（目录搬家后在这里纠正路径）。
    /// - 仅 kind=project；新路径必须存在且是目录（否则拒绝，避免指到空处）；
    /// - 路径记进 `source_url`（file:// 规范形式），正文里的「项目目录：`...`」快照行同步改掉；
    /// - 留 revision + wiki_log，不置 `human_edited_at`（元数据修正，不是正文创作）。
    pub fn update_project_path(&self, slug: &str, new_path: &str) -> Result<WikiPage> {
        let page = self
            .get_wiki_page(slug)?
            .with_context(|| format!("知识页不存在: {slug}"))?;
        if page.kind != "project" {
            anyhow::bail!(
                "只有项目页（kind=project）可以修改本地路径，当前 kind={}",
                page.kind
            );
        }
        let trimmed = new_path.trim();
        if trimmed.is_empty() {
            anyhow::bail!("新路径不能为空");
        }
        let raw = std::path::PathBuf::from(trimmed);
        if !raw.is_dir() {
            anyhow::bail!("目录不存在或不可访问: {trimmed}");
        }
        // 规范化（解引用符号链接）：同一目录永远得到同一 file://，去重不漂移。
        let canonical = raw.canonicalize().unwrap_or(raw);
        let new_url = crate::wiki::path_to_file_url(&canonical);
        let display = canonical.display().to_string();
        // 正文快照行同步：只换「项目目录：`...`」这一行的反引号内路径。
        let mut content = page.content_md.clone();
        if let Some(pos) = content.find("项目目录：") {
            let rest = &content[pos..];
            if let Some(open) = rest.find('`') {
                let after_open = pos + open + 1;
                if let Some(close_rel) = content[after_open..].find('`') {
                    content.replace_range(after_open..after_open + close_rel, &display);
                }
            }
        }
        let old_url = page.source_url.clone().unwrap_or_default();
        let now = chrono::Utc::now().to_rfc3339();
        self.connection.execute(
            "UPDATE wiki_pages SET source_url = ?1, content_md = ?2, updated_at = ?3 WHERE id = ?4",
            params![new_url, content, now, page.id],
        )?;
        let reason = if old_url.is_empty() {
            format!("项目路径设置：{display}")
        } else {
            format!("项目路径修改：{old_url} → {new_url}")
        };
        self.record_wiki_revision(&page.id, &content, &reason, None)?;
        self.append_wiki_log(&format!("项目路径修改：{slug} → {display}"))?;
        self.get_wiki_page(slug)?.context("项目路径修改后读取失败")
    }

    /// 存量回填（幂等）：目录导入时代久远的项目页只有正文快照、没有 `source_url`，
    /// 从「项目目录：`...`」解析出路径并记进 `source_url`（file://）。
    /// 只命中 `source_url IS NULL` 的 project 页，填过后不再重复执行，无副作用。
    pub fn backfill_project_source_urls(&self) -> Result<usize> {
        let mut statement = self.connection.prepare(
            "SELECT id, slug, content_md FROM wiki_pages
             WHERE kind = 'project' AND source_url IS NULL",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        let rows = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        drop(statement);
        let mut filled = 0;
        for (id, slug, content) in rows {
            let Some(path) = Self::parse_project_dir_snapshot(&content) else {
                continue;
            };
            let url = crate::wiki::path_to_file_url(&path);
            let now = chrono::Utc::now().to_rfc3339();
            self.connection.execute(
                "UPDATE wiki_pages SET source_url = ?1, updated_at = ?2 WHERE id = ?3",
                params![url, now, id],
            )?;
            self.append_wiki_log(&format!("项目路径回填：{slug} → {}", path.display()))?;
            filled += 1;
        }
        Ok(filled)
    }

    /// 从项目页正文快照解析目录（`项目目录：` + 反引号路径，analyze_project 的固定格式）。
    fn parse_project_dir_snapshot(content: &str) -> Option<std::path::PathBuf> {
        let pos = content.find("项目目录：")?;
        let rest = &content[pos..];
        let open = rest.find('`')?;
        let after = &rest[open + 1..];
        let close = after.find('`')?;
        let path = after[..close].trim();
        if path.is_empty() || !path.starts_with('/') {
            return None;
        }
        Some(std::path::PathBuf::from(path))
    }

    /// 人类编辑保存一页正文（「人类直接编辑」主线入口）。
    ///
    /// - 仅允许可编辑 kind（person/project/capability/recurring_cost/topic/…）；
    ///   采集素材 kind（source/note）只读，直接拒绝。
    /// - 非空、长度上限 64k 字符。
    /// - 乐观锁（§11 Q3）：`expected_updated_at` 提供时须与当前 `updated_at` 一致，
    ///   否则报「编辑冲突」——编辑会话期间页面被后台 digest 写回时，拒绝静默覆盖。
    /// - 写 revision（reason 前缀 `[human]`）+ wiki_log 审计；
    /// - 置 `human_edited_at=now`：此后该页被 AI digest 视为「人工持有」，不再整篇覆盖正文。
    pub fn save_wiki_page_content(
        &self,
        slug: &str,
        content_md: &str,
        reason: &str,
        expected_updated_at: Option<&str>,
    ) -> Result<WikiPage> {
        let page = self
            .get_wiki_page(slug)?
            .with_context(|| format!("知识页不存在: {slug}（可能已被改名或删除）"))?;
        if matches!(page.kind.as_str(), "source" | "note") {
            anyhow::bail!(
                "素材页（kind={}）只读，不支持人工编辑正文；只能表态评价（认可/不认可）",
                page.kind
            );
        }
        if let Some(expected) = expected_updated_at {
            // 乐观锁按毫秒精度比较解析后的时间戳，而非字符串相等：
            // Dart 侧 DateTime.parse 会把纳秒截断为微秒并转本地时区，
            // 字符串往返不可能精确还原 rfc3339（"Z" vs "+00:00"、精度位数）。
            // 解析失败（调用方传非 rfc3339）按冲突处理——fail-closed 优于静默覆盖。
            let expected_ts = chrono::DateTime::parse_from_rfc3339(expected);
            let current_ts = chrono::DateTime::parse_from_rfc3339(&page.updated_at);
            let consistent = matches!(
                (expected_ts, current_ts),
                (Ok(e), Ok(c)) if e.timestamp_millis() == c.timestamp_millis()
            );
            if !consistent {
                anyhow::bail!(
                    "编辑冲突：页面在你编辑期间已被更新（加载于 {}，当前 {}）；请重新加载后合并修改，或强制覆盖",
                    expected,
                    page.updated_at
                );
            }
        }
        let content_md = content_md.trim().to_string();
        if content_md.is_empty() {
            anyhow::bail!("正文为空，无法保存");
        }
        let char_count = content_md.chars().count();
        if char_count > 65536 {
            anyhow::bail!("正文超过 64k 字符上限（当前 {char_count} 字符）");
        }
        let now = chrono::Utc::now().to_rfc3339();
        self.connection.execute(
            "UPDATE wiki_pages SET content_md=?1, human_edited_at=?2, updated_at=?2 WHERE id=?3",
            params![content_md, now, page.id],
        )?;
        let reason = reason.trim();
        self.record_wiki_revision(
            &page.id,
            &content_md,
            &format!(
                "[human] {}",
                if reason.is_empty() {
                    "人工编辑正文"
                } else {
                    reason
                }
            ),
            None,
        )?;
        self.append_wiki_log(&format!(
            "人工编辑正文：{slug}（{}）",
            if reason.is_empty() {
                "无备注"
            } else {
                reason
            }
        ))?;
        self.get_wiki_page(slug)?.context("人工编辑保存后读取失败")
    }

    /// 素材页观点评价（素材唯一的交互入口）。
    ///
    /// - 仅允许采集素材 kind（source/note），非素材页拒绝；
    /// - `None` = 清空回未表态（读取时按缺省认可 'endorse' 处理）；
    ///   `Some("endorse")`/`Some("reject")` = 认可 / 不认可；
    /// - 不改变正文、不置位 `human_edited_at`，只写 wiki_log 审计。
    pub fn set_wiki_opinion(&self, slug: &str, opinion: Option<&str>) -> Result<WikiPage> {
        let page = self
            .get_wiki_page(slug)?
            .with_context(|| format!("知识页不存在: {slug}（可能已被改名或删除）"))?;
        if !matches!(page.kind.as_str(), "source" | "note") {
            anyhow::bail!(
                "只有采集素材页（source/note）可以表态评价，当前 kind={}",
                page.kind
            );
        }
        let value: Option<String> = match opinion {
            Some(o) => {
                let o = o.trim();
                if o != "endorse" && o != "reject" {
                    anyhow::bail!("评价只接受 endorse / reject，收到：{o}");
                }
                Some(o.to_string())
            }
            None => None,
        };
        let now = chrono::Utc::now().to_rfc3339();
        self.connection.execute(
            "UPDATE wiki_pages SET opinion=?1, updated_at=?2 WHERE id=?3",
            params![value, now, page.id],
        )?;
        let label = match value.as_deref() {
            Some("endorse") => "认可",
            Some("reject") => "不认可",
            _ => "清空（回归未表态，缺省认可）",
        };
        self.append_wiki_log(&format!("素材评价：{slug} → {label}"))?;
        self.get_wiki_page(slug)?.context("评价保存后读取失败")
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
        Self::record_wiki_revision_on(
            &self.connection,
            page_id,
            content_md,
            reason,
            source_event_id,
        )
    }

    /// 在指定连接上追加一条 wiki revision；事务通过 Deref 传入 `&Transaction` 亦可。
    /// upsert_wiki_page 把「页面写入 + revision」放进同一事务原子提交，
    /// 避免页面改了但 revision 没记（或反之）导致审计断链。
    fn record_wiki_revision_on(
        conn: &rusqlite::Connection,
        page_id: &str,
        content_md: &str,
        reason: &str,
        source_event_id: Option<&str>,
    ) -> Result<()> {
        conn.execute(
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

    /// 删除 app_meta 键（覆盖层「跟随全局」时清除覆盖值，回落到全局层）
    pub fn remove_meta(&self, key: &str) -> Result<()> {
        self.connection
            .execute("DELETE FROM app_meta WHERE key = ?1", params![key])?;
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
        // 单行直查：不做两遍全表 list 再内存 find（P2-5）
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
        self.connection
            .query_row(
                "SELECT id, title, status, priority, due_at, related_event_id,
                        related_wiki_slug, note, created_at, updated_at
                 FROM todos WHERE id = ?1",
                [id],
                mapper,
            )
            .optional()
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

    /// 判断本会话是否已有同一动作、同一标题的待确认草稿。
    /// 标题是知识页创建的业务幂等键；正文允许在确认前继续完善，但不应因此产生第二个页面。
    pub fn pending_action_for_title(
        &self,
        conversation_id: &str,
        action: &str,
        title: &str,
    ) -> Result<Option<PendingAction>> {
        let actions = self.pending_actions_for_conversation(conversation_id)?;
        Ok(actions.into_iter().find(|pa| {
            if pa.action != action {
                return false;
            }
            serde_json::from_str::<serde_json::Value>(&pa.args_json)
                .ok()
                .and_then(|args| args.get("title").and_then(|v| v.as_str()).map(str::trim).map(String::from))
                .is_some_and(|pending_title| pending_title.eq_ignore_ascii_case(title.trim()))
        }))
    }

    pub fn pending_action_by_id(
        &self,
        conversation_id: &str,
        id: &str,
    ) -> Result<Option<PendingAction>> {
        Ok(self.pending_actions_for_conversation(conversation_id)?
            .into_iter()
            .find(|action| action.id == id))
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
            "INSERT INTO conversations (id, title, tag, assistant_mode, created_at, updated_at) VALUES (?1, ?2, ?3, 'personal_secretary', ?4, ?4)",
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
            "INSERT INTO conversations (id, title, tag, wiki_page_slug, assistant_mode, created_at, updated_at)
             VALUES (?1, ?2, 'idea', ?3, 'knowledge_mentor', ?4, ?4)",
            params![id, title, wiki_page_slug, now],
        )?;
        Ok(id)
    }

    pub fn list_conversations(&self) -> Result<Vec<ConversationSummary>> {
        let mut statement = self.connection.prepare(
            "SELECT c.id, c.title, c.tag, c.created_at, c.updated_at, c.archived,
                    c.wiki_page_slug, c.assistant_mode,
                    COUNT(m.id) as message_count,
                    (SELECT m2.content FROM messages m2
                     WHERE m2.conversation_id = c.id
                     ORDER BY m2.created_at DESC, m2.rowid DESC LIMIT 1) as last_message
             FROM conversations c
             LEFT JOIN messages m ON m.conversation_id = c.id
             WHERE c.archived = 0 AND c.wiki_page_slug IS NULL
             GROUP BY c.id
             ORDER BY c.updated_at DESC, c.id DESC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(ConversationSummary {
                id: row.get(0)?,
                title: row.get(1)?,
                tag: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
                message_count: row.get(8)?,
                last_message_preview: row.get(9)?,
                archived: row.get(5)?,
                wiki_page_slug: row.get(6)?,
                assistant_mode: row.get(7)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// Archived conversations, newest first.
    pub fn list_archived_conversations(&self) -> Result<Vec<ConversationSummary>> {
        let mut statement = self.connection.prepare(
            "SELECT c.id, c.title, c.tag, c.created_at, c.updated_at, c.archived,
                    c.wiki_page_slug, c.assistant_mode,
                    COUNT(m.id) as message_count,
                    (SELECT m2.content FROM messages m2
                     WHERE m2.conversation_id = c.id
                     ORDER BY m2.created_at DESC, m2.rowid DESC LIMIT 1) as last_message
             FROM conversations c
             LEFT JOIN messages m ON m.conversation_id = c.id
             WHERE c.archived = 1 AND c.wiki_page_slug IS NULL
             GROUP BY c.id
             ORDER BY c.updated_at DESC, c.id DESC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(ConversationSummary {
                id: row.get(0)?,
                title: row.get(1)?,
                tag: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
                message_count: row.get(8)?,
                last_message_preview: row.get(9)?,
                archived: row.get(5)?,
                wiki_page_slug: row.get(6)?,
                assistant_mode: row.get(7)?,
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
                        c.wiki_page_slug, c.assistant_mode,
                        COUNT(m.id) as message_count,
                        (SELECT m2.content FROM messages m2
                         WHERE m2.conversation_id = c.id
                         ORDER BY m2.created_at DESC, m2.rowid DESC LIMIT 1) as last_message
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
                        message_count: row.get(8)?,
                        last_message_preview: row.get(9)?,
                        archived: row.get(5)?,
                        wiki_page_slug: row.get(6)?,
                        assistant_mode: row.get(7)?,
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
             ORDER BY created_at ASC, id ASC",
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
             ORDER BY created_at ASC, id ASC",
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
