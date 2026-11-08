//! 知识库消化洞见浮现（提案 2026-11-05-surfacing-digest-insights）：P0/P1。
//!
//! 纪律：
//! - 候选登记（pending）在本机库内排队：仅 insight（带行动建议）/ 新成立的
//!   habit 页 / 识别出的 recurring_cost 页入池；同一页面终身最多浮现 1 次；
//! - 每日最多浮现 1 条（本地自然日），07:00–22:00 之外静默、候选保留；
//! - 仅 owner 主设备插入消息；副设备经同步收到 `digest_insight` 消息后直接
//!   展示，不重复写；动作产物（规则/待办）作为一等公民各自同步；
//! - 本表对齐 `habit_nudge_fires` 的「本机台账」定位：不进快照、不参与同步；
//!   副设备无本机行时，动作按卡片消息内容就地重建台账行（同 id，前向兼容
//!   将来如需把台账提升为同步对象）。

use chrono::{DateTime, Local, Timelike};
use rusqlite::{OptionalExtension, params};
use uuid::Uuid;

use super::Store;
use anyhow::{Context, Result, ensure};

/// 允许入池的高价值类型（提案 §3.1 信噪比红线）。
pub const DIGEST_INSIGHT_KINDS: &[&str] = &["insight", "habit", "recurring_cost"];

const TITLE_MAX_CHARS: usize = 150;
const OBSERVATION_MAX_CHARS: usize = 3000;
/// 卡片产生 72 小时未动作自动过期（提案 §3.4）。
const CARD_EXPIRY_HOURS: i64 = 72;
/// 夜间静默：本地时间 22:00–07:00 不浮现新卡片。
const QUIET_START_HOUR: u32 = 22;
const QUIET_END_HOUR: u32 = 7;

/// 卡片消息的内容前缀（构建与解析共用，改动需同步 round-trip 测试）。
const CARD_PREFIX: &str = "💡 认知洞察｜";
const CARD_ACTION_MARK: &str = "\n\n建议行动：";
const CARD_SOURCE_MARK: &str = "\n\n来源：";

/// 一条浮现台账行。`page_kind`/`page_active` 是读取时的联表计算字段，
/// 不入库（页面删除后仍可读旧卡片）。
#[derive(Clone, Debug)]
pub struct DigestInsightSurfacing {
    pub id: String,
    pub insight_id: Option<String>,
    pub page_slug: String,
    pub kind: String,
    pub title: String,
    pub observation: String,
    pub action_suggestion: Option<String>,
    pub status: String,
    pub surfaced_at: Option<String>,
    pub surfaced_date: Option<String>,
    pub message_id: Option<String>,
    pub action_type: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// 计算字段：页面仍存在时的 kind（删除后为 None）。
    pub page_kind: Option<String>,
    /// 计算字段：页面仍在生效（active）。
    pub page_active: bool,
}

const COLS: &str = "s.id,s.insight_id,s.page_slug,s.kind,s.title,s.observation,\
                    s.action_suggestion,s.status,s.surfaced_at,s.surfaced_date,\
                    s.message_id,s.action_type,s.created_at,s.updated_at,\
                    p.kind,COALESCE(p.status,'')";

fn row_to_surfacing(row: &rusqlite::Row<'_>) -> rusqlite::Result<DigestInsightSurfacing> {
    let page_status: String = row.get(15)?;
    Ok(DigestInsightSurfacing {
        id: row.get(0)?,
        insight_id: row.get(1)?,
        page_slug: row.get(2)?,
        kind: row.get(3)?,
        title: row.get(4)?,
        observation: row.get(5)?,
        action_suggestion: row.get(6)?,
        status: row.get(7)?,
        surfaced_at: row.get(8)?,
        surfaced_date: row.get(9)?,
        message_id: row.get(10)?,
        action_type: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
        page_kind: row.get(14)?,
        page_active: page_status == "active",
    })
}

/// 卡片消息正文（自含渲染所需文本：标题/观察/建议/来源页引用）。
/// 副设备仅凭同步到的这条消息即可完整展示卡片。
pub(crate) fn digest_insight_card_text(
    title: &str,
    observation: &str,
    action_suggestion: Option<&str>,
    page_slug: &str,
) -> String {
    let mut text = format!("{CARD_PREFIX}{title}\n\n{observation}");
    if let Some(action) = action_suggestion.map(str::trim).filter(|a| !a.is_empty()) {
        text.push_str(CARD_ACTION_MARK);
        text.push_str(action);
    }
    text.push_str(CARD_SOURCE_MARK);
    text.push_str(&format!("[[{page_slug}]]"));
    text
}

/// 从卡片消息正文解析回 (title, observation, action, page_slug)。
/// 只用于副设备重建本机台账；格式由 [`digest_insight_card_text`] 保证，
/// 解析失败即报错（宁可拒绝动作，也不凭猜测造规则/待办）。
pub(crate) fn parse_digest_insight_card_text(
    content: &str,
) -> Option<(String, String, Option<String>, String)> {
    let rest = content.trim().strip_prefix(CARD_PREFIX)?;
    let (title, rest) = rest.split_once("\n\n")?;
    let (body, source) = rest.rsplit_once(CARD_SOURCE_MARK)?;
    let slug = source.trim().strip_prefix("[[")?.strip_suffix("]]")?;
    let (observation, action) = match body.rsplit_once(CARD_ACTION_MARK) {
        Some((observation, action)) => (observation, Some(action.trim())),
        None => (body, None),
    };
    let action = action.map(str::to_string).filter(|value| !value.is_empty());
    Some((
        title.trim().to_string(),
        observation.trim().to_string(),
        action,
        slug.trim().to_string(),
    ))
}

fn local_date(now: DateTime<Local>) -> String {
    now.format("%Y-%m-%d").to_string()
}

impl Store {
    /// 登记一条浮现候选（调用方事务内调用，不自行提交）。
    ///
    /// 同一页面已有任何台账行（任意状态）时跳过——「同一 Wiki 页面终身
    /// 最多浮现 1 次」由这里兜底。返回是否新登记。
    pub fn record_digest_insight_candidate(
        &self,
        kind: &str,
        page_slug: &str,
        title: &str,
        observation: &str,
        action_suggestion: Option<&str>,
        insight_id: Option<&str>,
    ) -> Result<bool> {
        ensure!(
            DIGEST_INSIGHT_KINDS.contains(&kind),
            "洞见浮现候选 kind 不合法：{kind}"
        );
        let page_slug = page_slug.trim();
        let title = title.trim();
        let observation = observation.trim();
        ensure!(!page_slug.is_empty(), "洞见浮现候选缺少页面 slug");
        ensure!(
            !title.is_empty() && !observation.is_empty(),
            "洞见浮现候选缺少标题或观察内容"
        );
        let exists: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM digest_insight_surfacings WHERE page_slug=?1)",
            [page_slug],
            |row| row.get(0),
        )?;
        if exists {
            return Ok(false);
        }
        let now = chrono::Utc::now().to_rfc3339();
        self.connection.execute(
            "INSERT INTO digest_insight_surfacings
             (id,insight_id,page_slug,kind,title,observation,action_suggestion,status,created_at,updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,'pending',?8,?8)",
            params![
                Uuid::new_v4().to_string(),
                insight_id,
                page_slug,
                kind,
                title.chars().take(TITLE_MAX_CHARS).collect::<String>(),
                observation
                    .chars()
                    .take(OBSERVATION_MAX_CHARS)
                    .collect::<String>(),
                action_suggestion
                    .map(str::trim)
                    .filter(|value| !value.is_empty()),
                now
            ],
        )?;
        Ok(true)
    }

    pub fn list_digest_insight_surfacings(&self) -> Result<Vec<DigestInsightSurfacing>> {
        let mut statement = self.connection.prepare(&format!(
            "SELECT {COLS} FROM digest_insight_surfacings s
             LEFT JOIN wiki_pages p ON p.slug=s.page_slug
             ORDER BY s.created_at DESC, s.id DESC LIMIT 200"
        ))?;
        let rows = statement.query_map([], row_to_surfacing)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn get_digest_insight_surfacing(&self, id: &str) -> Result<Option<DigestInsightSurfacing>> {
        self.connection
            .query_row(
                &format!(
                    "SELECT {COLS} FROM digest_insight_surfacings s
                     LEFT JOIN wiki_pages p ON p.slug=s.page_slug WHERE s.id=?1"
                ),
                [id],
                row_to_surfacing,
            )
            .optional()
            .map_err(Into::into)
    }

    /// 未动作的已浮现卡片数（UI 角标/侧栏未读口径；expired/acted/dismissed 不算）。
    pub fn pending_digest_insight_count(&self) -> Result<i32> {
        self.connection
            .query_row(
                "SELECT COUNT(*) FROM digest_insight_surfacings WHERE status='surfaced'",
                [],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }

    /// 5s tick 调用：过期超时卡片，然后至多把一条候选浮现进主对话。
    pub fn tick_digest_insights(&self) -> Result<Vec<String>> {
        self.tick_digest_insights_at(Local::now())
    }

    pub(crate) fn tick_digest_insights_at(&self, now: DateTime<Local>) -> Result<Vec<String>> {
        self.expire_due_digest_insight_cards(now)?;
        // 多设备（提案 §5.3）：只有 owner（主设备）写入卡片消息，副设备只
        // 同步展示；单机没有同步权威时本机是唯一设备，照常触发。
        if !self.is_sync_owner_device()? {
            return Ok(Vec::new());
        }
        // 夜间静默：22:00–07:00 不浮现（候选保留到白天）。
        let hour = now.hour();
        if hour < QUIET_END_HOUR || hour >= QUIET_START_HOUR {
            return Ok(Vec::new());
        }
        let Some(conversation_id) = self.digest_insight_main_conversation()? else {
            return Ok(Vec::new());
        };
        let date = local_date(now);
        // 全局每日最多 1 条（含已动作/已忽略/已过期的当天卡片）。
        let surfaced_today: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM digest_insight_surfacings WHERE surfaced_date=?1",
            [&date],
            |row| row.get(0),
        )?;
        if surfaced_today >= 1 {
            return Ok(Vec::new());
        }
        let next: Option<String> = self
            .connection
            .query_row(
                "SELECT id FROM digest_insight_surfacings
                 WHERE status='pending' ORDER BY created_at,id LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        let Some(surfacing) = next
            .as_deref()
            .map(|id| self.get_digest_insight_surfacing(id))
            .transpose()?
            .flatten()
        else {
            return Ok(Vec::new());
        };
        let reference = format!("surfacing:{}", surfacing.id);
        // 竞态/重放：同 ref 消息已在（例如本机重启后或另一端先写）时只补台账。
        let existing: Option<String> = self
            .connection
            .query_row(
                "SELECT id FROM messages WHERE kind='digest_insight' AND ref=?1
                 ORDER BY created_at,id LIMIT 1",
                [&reference],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(existing) = existing {
            let transaction = self.begin_write_transaction()?;
            self.mark_digest_insight_surfaced(&surfacing.id, &date, &existing, now)?;
            if let Some(transaction) = transaction {
                transaction.commit()?;
            }
            return Ok(vec![existing]);
        }
        let content = digest_insight_card_text(
            &surfacing.title,
            &surfacing.observation,
            surfacing.action_suggestion.as_deref(),
            &surfacing.page_slug,
        );
        let transaction = self.begin_write_transaction()?;
        let message_id = self.insert_message_with_kind(
            &conversation_id,
            "assistant",
            &content,
            None,
            None,
            "digest_insight",
            Some(&reference),
        )?;
        self.mark_digest_insight_surfaced(&surfacing.id, &date, &message_id, now)?;
        if let Some(transaction) = transaction {
            transaction.commit()?;
        }
        Ok(vec![message_id])
    }

    /// 四重闭环动作（提案 §4）：rule / habit_nudge / todo / dismiss。
    ///
    /// 幂等：已 acted/dismissed 的卡片重复点击直接返回现状，不产生第二次
    /// 副作用（规则/待办只建一次）。副设备无本机台账行时，按卡片消息内容
    /// 就地重建（同 id），保证「任何设备上都能点击闭环」。
    pub fn act_on_digest_insight(
        &self,
        message_id: &str,
        action: &str,
    ) -> Result<DigestInsightSurfacing> {
        ensure!(
            matches!(action, "rule" | "habit_nudge" | "todo" | "dismiss"),
            "洞见动作不合法：{action}"
        );
        let (reference, content): (Option<String>, String) = self
            .connection
            .query_row(
                "SELECT ref,content FROM messages WHERE id=?1 AND kind='digest_insight'",
                [message_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
            .context("洞见卡片消息不存在")?;
        let reference = reference.context("洞见卡片缺少 ref")?;
        let surfacing_id = reference
            .strip_prefix("surfacing:")
            .context("洞见卡片 ref 不合法")?
            .to_string();
        let mut surfacing = match self.get_digest_insight_surfacing(&surfacing_id)? {
            Some(surfacing) => surfacing,
            None => {
                let (title, observation, action_suggestion, page_slug) =
                    parse_digest_insight_card_text(&content)
                        .context("无法从卡片内容重建洞见台账")?;
                self.insert_reconstructed_digest_surfacing(
                    &surfacing_id,
                    message_id,
                    &title,
                    &observation,
                    action_suggestion.as_deref(),
                    &page_slug,
                )?;
                self.get_digest_insight_surfacing(&surfacing_id)?
                    .context("洞见台账重建后不存在")?
            }
        };
        if matches!(surfacing.status.as_str(), "acted" | "dismissed") {
            return Ok(surfacing);
        }
        match action {
            "rule" => {
                let text = surfacing
                    .action_suggestion
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or(surfacing.observation.trim())
                    .to_string();
                self.add_rule(&text, super::RuleStatus::Active, None)?;
            }
            "todo" => {
                let title = surfacing
                    .action_suggestion
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or(surfacing.title.trim())
                    .to_string();
                self.create_todo(
                    &title,
                    "normal",
                    None,
                    None,
                    Some(&surfacing.page_slug),
                    Some("来自知识库洞见浮现"),
                    false,
                )?;
            }
            "habit_nudge" => {
                // 问候计划由 UI 走既有推断/确认链路先行保存；这里只记录闭环。
            }
            _ => {} // dismiss：只改状态
        }
        let status = if action == "dismiss" {
            "dismissed"
        } else {
            "acted"
        };
        let now = chrono::Utc::now().to_rfc3339();
        self.connection.execute(
            "UPDATE digest_insight_surfacings
             SET status=?1, action_type=?2, updated_at=?3 WHERE id=?4",
            params![status, action, now, surfacing.id],
        )?;
        surfacing.status = status.to_string();
        surfacing.action_type = Some(action.to_string());
        surfacing.updated_at = now;
        Ok(surfacing)
    }

    fn mark_digest_insight_surfaced(
        &self,
        id: &str,
        date: &str,
        message_id: &str,
        now: DateTime<Local>,
    ) -> Result<()> {
        let now = now.with_timezone(&chrono::Utc).to_rfc3339();
        self.connection.execute(
            "UPDATE digest_insight_surfacings
             SET status='surfaced', surfaced_at=?1, surfaced_date=?2, message_id=?3, updated_at=?1
             WHERE id=?4 AND status='pending'",
            params![now, date, message_id, id],
        )?;
        Ok(())
    }

    /// 72 小时未动作的卡片自动转 expired（不再作为未读置顶；acted/dismissed
    /// 是用户已表态的终态，不受影响）。
    fn expire_due_digest_insight_cards(&self, now: DateTime<Local>) -> Result<usize> {
        let now_utc = now.with_timezone(&chrono::Utc);
        let cutoff = (now_utc - chrono::Duration::hours(CARD_EXPIRY_HOURS)).to_rfc3339();
        let now = now_utc.to_rfc3339();
        Ok(self.connection.execute(
            "UPDATE digest_insight_surfacings SET status='expired', updated_at=?1
             WHERE status='surfaced' AND surfaced_at IS NOT NULL AND surfaced_at < ?2",
            params![now, cutoff],
        )?)
    }

    /// 主对话流（与 habit_nudges 同口径，并显式排除目标工作室/目标讨论：
    /// 洞见卡片只出现在用户的主记录流里）。
    fn digest_insight_main_conversation(&self) -> Result<Option<String>> {
        self.connection
            .query_row(
                "SELECT id FROM conversations
                 WHERE archived=0 AND wiki_page_slug IS NULL
                   AND assistant_mode NOT IN ('knowledge_mentor','goal_mentor')
                 ORDER BY CASE WHEN title='主对话流' THEN 0 ELSE 1 END, created_at, id
                 LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    fn insert_reconstructed_digest_surfacing(
        &self,
        id: &str,
        message_id: &str,
        title: &str,
        observation: &str,
        action_suggestion: Option<&str>,
        page_slug: &str,
    ) -> Result<()> {
        // kind 无法从卡片文本还原，统一记 'insight'；习惯出口的可用性由
        // 联表计算字段 page_kind 判定，不依赖这里。
        let now = chrono::Utc::now().to_rfc3339();
        let date = local_date(Local::now());
        self.connection.execute(
            "INSERT OR IGNORE INTO digest_insight_surfacings
             (id,insight_id,page_slug,kind,title,observation,action_suggestion,status,
              surfaced_at,surfaced_date,message_id,created_at,updated_at)
             VALUES (?1,NULL,?2,'insight',?3,?4,?5,'surfaced',?6,?7,?8,?6,?6)",
            params![
                id,
                page_slug,
                title,
                observation,
                action_suggestion,
                now,
                date,
                message_id
            ],
        )?;
        Ok(())
    }
}
