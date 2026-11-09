//! 目标存取（FR-PES-005-01/02 & 2026-11 认知型目标升级）。
//!
//! 活跃目标上限 3 条：新建时若活跃已满，主动以 `paused` 状态落库（保留
//! 完整上下文、可随时读档唤醒），不再把创建挡在门外；`new_migrations.rs`
//! 里的数据库触发器仍是最终防线（复活等 UPDATE 路径越限时报
//! `active_goal_limit_reached`，由本层翻成用户看得懂的话）。
//!
//! `phase` 是标签不是槽位：同一阶段可以有多条活跃目标，也不要求三个阶段都有。
//! 目标认知切片（`why`, `current_focus`, `blocker`, `done_criteria`）支持随时单字段滚动更新；
//! 目标生命周期三态：`active`（活跃 ≤3）、`paused`（暂停搁置，保留上下文可随时读档）、
//! `completed`（已达成里程碑）。

use anyhow::Result;
use rusqlite::{Error as SqliteError, OptionalExtension, params};
use uuid::Uuid;

use super::Store;
use super::new_migrations::ACTIVE_GOAL_LIMIT_REACHED;

/// 活跃目标条数上限，与数据库触发器里的常量保持一致。
pub const MAX_ACTIVE_GOALS: usize = 3;

/// 目标阶段：近期 / 中期 / 长远
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalPhase {
    Near,
    Mid,
    Long,
}

impl GoalPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            GoalPhase::Near => "near",
            GoalPhase::Mid => "mid",
            GoalPhase::Long => "long",
        }
    }

    /// 中文展示名，用于注入 AI 上下文。
    pub fn label(self) -> &'static str {
        match self {
            GoalPhase::Near => "近期",
            GoalPhase::Mid => "中期",
            GoalPhase::Long => "长远",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "near" => Some(GoalPhase::Near),
            "mid" => Some(GoalPhase::Mid),
            "long" => Some(GoalPhase::Long),
            _ => None,
        }
    }
}

/// 目标状态：活跃 / 暂停搁置 / 已达成
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalStatus {
    Active,
    Paused,
    Completed,
}

impl GoalStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            GoalStatus::Active => "active",
            GoalStatus::Paused => "paused",
            GoalStatus::Completed => "completed",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "active" => Some(GoalStatus::Active),
            "paused" => Some(GoalStatus::Paused),
            "completed" => Some(GoalStatus::Completed),
            // 兼容历史存量
            "superseded" => Some(GoalStatus::Paused),
            _ => None,
        }
    }
}

/// 一条目标（活跃、暂停或已达成）
#[derive(Debug, Clone)]
pub struct Goal {
    pub id: String,
    pub content: String,
    pub phase: GoalPhase,
    pub status: GoalStatus,
    pub why: Option<String>,
    pub current_focus: Option<String>,
    pub blocker: Option<String>,
    pub done_criteria: Option<String>,
    /// 所属长程计划（P1 起）。无计划独立目标概念已取消：上层「直接建一条
    /// 目标」会包成单步计划再挂接。
    pub plan_id: Option<String>,
    /// 步骤序号（计划内展示排序；缺省排末尾）。
    pub step_index: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
    pub superseded_at: Option<String>,
}

/// 航向偏离校准提醒（Compass Nudge，FR-PES-005-04）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalCompassNudge {
    pub id: String,
    pub goal_id: String,
    pub goal_content: String,
    /// 所属长程计划（P4：停滞步骤属于计划时，提醒携带计划上下文）
    pub plan_id: Option<String>,
    pub plan_intent: Option<String>,
    pub current_focus: String,
    pub stalled_days: i64,
    pub recent_topics: String,
    pub status: String, // "pending" | "dismissed" | "realigned"
    pub created_at: String,
    pub dismissed_until: Option<String>,
}

/// 触发器用 `RAISE(ABORT, 'active_goal_limit_reached')` 拒绝写入，原样冒泡会把
/// 那串下划线标识送到界面上，这里换成能直接展示给用户的话。
fn map_write_error(error: SqliteError) -> anyhow::Error {
    if error.to_string().contains(ACTIVE_GOAL_LIMIT_REACHED) {
        anyhow::anyhow!("活跃目标已达 {MAX_ACTIVE_GOALS} 条上限，请先归档一条再新增")
    } else {
        error.into()
    }
}

pub(crate) const SELECT_COLUMNS: &str = "id, content, phase, status, why, current_focus, blocker, done_criteria, created_at, updated_at, superseded_at, plan_id, step_index";

pub(crate) fn row_to_goal(row: &rusqlite::Row<'_>) -> rusqlite::Result<Goal> {
    let phase: String = row.get(2)?;
    let status: String = row.get(3)?;
    Ok(Goal {
        id: row.get(0)?,
        content: row.get(1)?,
        phase: GoalPhase::parse(&phase).ok_or_else(|| {
            SqliteError::InvalidColumnType(2, "phase".to_string(), rusqlite::types::Type::Text)
        })?,
        status: GoalStatus::parse(&status).ok_or_else(|| {
            SqliteError::InvalidColumnType(3, "status".to_string(), rusqlite::types::Type::Text)
        })?,
        why: row.get(4)?,
        current_focus: row.get(5)?,
        blocker: row.get(6)?,
        done_criteria: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        superseded_at: row.get(10)?,
        plan_id: row.get(11)?,
        step_index: row.get(12)?,
    })
}

impl Store {
    /// 列出活跃目标，按阶段（近期→中期→长远）再按创建时间排序。
    ///
    /// 条数由触发器封顶，故这里无需再取截断。
    pub fn list_active_goals(&self) -> Result<Vec<Goal>> {
        let sql = format!(
            "SELECT {SELECT_COLUMNS} FROM goals WHERE status='active'
             ORDER BY CASE phase WHEN 'near' THEN 0 WHEN 'mid' THEN 1 ELSE 2 END,
                      created_at"
        );
        let mut statement = self.connection.prepare(&sql)?;
        let rows = statement.query_map([], row_to_goal)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// 列出所有非活跃目标（含暂停搁置与已达成），按归档/完成时间倒序。
    pub fn list_archived_goals(&self) -> Result<Vec<Goal>> {
        let sql = format!(
            "SELECT {SELECT_COLUMNS} FROM goals WHERE status IN ('paused', 'completed')
             ORDER BY superseded_at DESC, created_at DESC"
        );
        let mut statement = self.connection.prepare(&sql)?;
        let rows = statement.query_map([], row_to_goal)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// 列出暂停搁置的目标，按暂停时间倒序。
    pub fn list_paused_goals(&self) -> Result<Vec<Goal>> {
        let sql = format!(
            "SELECT {SELECT_COLUMNS} FROM goals WHERE status='paused'
             ORDER BY superseded_at DESC, created_at DESC"
        );
        let mut statement = self.connection.prepare(&sql)?;
        let rows = statement.query_map([], row_to_goal)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// 列出已达成的目标，按达成时间倒序。
    pub fn list_completed_goals(&self) -> Result<Vec<Goal>> {
        let sql = format!(
            "SELECT {SELECT_COLUMNS} FROM goals WHERE status='completed'
             ORDER BY superseded_at DESC, created_at DESC"
        );
        let mut statement = self.connection.prepare(&sql)?;
        let rows = statement.query_map([], row_to_goal)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// 根据 ID 获取单个目标详情
    pub fn get_goal(&self, id: &str) -> Result<Option<Goal>> {
        let sql = format!("SELECT {SELECT_COLUMNS} FROM goals WHERE id=?1");
        Ok(self
            .connection
            .query_row(&sql, [id], row_to_goal)
            .optional()?)
    }

    pub fn count_active_goals(&self) -> Result<i64> {
        Ok(self.connection.query_row(
            "SELECT COUNT(*) FROM goals WHERE status='active'",
            [],
            |row| row.get(0),
        )?)
    }

    /// 新建一条目标（无认知切片）。活跃名额未满时直接激活；已满 3 条时以
    /// `paused` 状态创建，保留完整上下文供随时读档，不自动顶掉既有目标。
    pub fn create_goal(&self, content: &str, phase: GoalPhase) -> Result<Goal> {
        self.create_goal_with_facets(content, phase, None, None, None, None)
    }

    /// 新建一条带认知切片的目标。活跃名额未满时直接激活；已满 3 条时以
    /// `paused` 状态创建（`superseded_at` 记为创建时刻），不报错、不顶替。
    /// 触发器仍是最终防线（并发竞态下插入 active 仍会越限报错）。
    pub fn create_goal_with_facets(
        &self,
        content: &str,
        phase: GoalPhase,
        why: Option<&str>,
        current_focus: Option<&str>,
        blocker: Option<&str>,
        done_criteria: Option<&str>,
    ) -> Result<Goal> {
        let content = content.trim();
        if content.is_empty() {
            anyhow::bail!("目标内容不能为空");
        }
        let now = chrono::Utc::now().to_rfc3339();
        // 活跃已满 → 落为暂停搁置（不再把创建挡在门外）；触发器兜底并发竞态。
        let status = if self.count_active_goals()? >= MAX_ACTIVE_GOALS as i64 {
            GoalStatus::Paused
        } else {
            GoalStatus::Active
        };
        let superseded_at = if status == GoalStatus::Paused {
            Some(now.clone())
        } else {
            None
        };
        let goal = Goal {
            id: Uuid::new_v4().to_string(),
            content: content.to_string(),
            phase,
            status,
            why: why.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
            current_focus: current_focus
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            blocker: blocker
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            done_criteria: done_criteria
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            plan_id: None,
            step_index: None,
            created_at: now.clone(),
            updated_at: now.clone(),
            superseded_at,
        };
        let tx = self.begin_write_transaction()?;
        self.connection
            .execute(
                "INSERT INTO goals(id, content, phase, status, why, current_focus, blocker, done_criteria, created_at, updated_at, superseded_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9, ?10)",
                params![
                    goal.id,
                    goal.content,
                    phase.as_str(),
                    status.as_str(),
                    goal.why,
                    goal.current_focus,
                    goal.blocker,
                    goal.done_criteria,
                    now,
                    goal.superseded_at
                ],
            )
            .map_err(map_write_error)?;
        self.sync_record_goal(&goal.id)?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(goal)
    }

    /// 编辑一条目标的正文与阶段。状态不变，故不触碰上限触发器。
    pub fn update_goal(&self, id: &str, content: &str, phase: GoalPhase) -> Result<()> {
        let content = content.trim();
        if content.is_empty() {
            anyhow::bail!("目标内容不能为空");
        }
        let now = chrono::Utc::now().to_rfc3339();
        let tx = self.begin_write_transaction()?;
        let changed = self
            .connection
            .execute(
                "UPDATE goals SET content=?2, phase=?3, updated_at=?4 WHERE id=?1",
                params![id, content, phase.as_str(), now],
            )
            .map_err(map_write_error)?;
        if changed == 0 {
            anyhow::bail!("目标不存在: {id}");
        }
        self.sync_record_goal(id)?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    /// 滚动更新目标抓手与卡点（最常用的轻量认知更新操作）
    pub fn update_goal_focus(
        &self,
        id: &str,
        current_focus: Option<&str>,
        blocker: Option<&str>,
    ) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        let tx = self.begin_write_transaction()?;
        let changed = self
            .connection
            .execute(
                "UPDATE goals SET current_focus=?2, blocker=?3, updated_at=?4 WHERE id=?1",
                params![
                    id,
                    current_focus.map(str::trim).filter(|s| !s.is_empty()),
                    blocker.map(str::trim).filter(|s| !s.is_empty()),
                    now
                ],
            )
            .map_err(map_write_error)?;
        if changed == 0 {
            anyhow::bail!("目标不存在: {id}");
        }
        // 抓手已更新 → 该目标的 pending 航向校准卡片即告过时：同事务内
        // 标记 realigned，避免横幅在工作台滚动抓手后仍永久展示旧抓手/
        // 旧停滞天数（P1：evaluate_goal_compass 首步直接返回现存卡片）。
        self.connection.execute(
            "UPDATE goal_compass_nudges SET status='realigned' WHERE goal_id=?1 AND status='pending'",
            params![id],
        )?;
        self.sync_record_goal(id)?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    /// 更新目标的所有认知切片字段
    pub fn update_goal_facets(
        &self,
        id: &str,
        why: Option<&str>,
        current_focus: Option<&str>,
        blocker: Option<&str>,
        done_criteria: Option<&str>,
    ) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        let tx = self.begin_write_transaction()?;
        let changed = self
            .connection
            .execute(
                "UPDATE goals SET why=?2, current_focus=?3, blocker=?4, done_criteria=?5, updated_at=?6 WHERE id=?1",
                params![
                    id,
                    why.map(str::trim).filter(|s| !s.is_empty()),
                    current_focus.map(str::trim).filter(|s| !s.is_empty()),
                    blocker.map(str::trim).filter(|s| !s.is_empty()),
                    done_criteria.map(str::trim).filter(|s| !s.is_empty()),
                    now
                ],
            )
            .map_err(map_write_error)?;
        if changed == 0 {
            anyhow::bail!("目标不存在: {id}");
        }
        // 与 update_goal_focus 同理：认知切片（含 current_focus）更新后，
        // pending 偏离卡片的展示内容即告过时，同事务内校准消除。
        self.connection.execute(
            "UPDATE goal_compass_nudges SET status='realigned' WHERE goal_id=?1 AND status='pending'",
            params![id],
        )?;
        self.sync_record_goal(id)?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    /// 完整更新目标的所有正文与切片字段
    pub fn update_goal_full(
        &self,
        id: &str,
        content: &str,
        phase: GoalPhase,
        why: Option<&str>,
        current_focus: Option<&str>,
        blocker: Option<&str>,
        done_criteria: Option<&str>,
    ) -> Result<()> {
        let content = content.trim();
        if content.is_empty() {
            anyhow::bail!("目标内容不能为空");
        }
        let now = chrono::Utc::now().to_rfc3339();
        let tx = self.begin_write_transaction()?;
        let changed = self
            .connection
            .execute(
                "UPDATE goals SET content=?2, phase=?3, why=?4, current_focus=?5, blocker=?6, done_criteria=?7, updated_at=?8 WHERE id=?1",
                params![
                    id,
                    content,
                    phase.as_str(),
                    why.map(str::trim).filter(|s| !s.is_empty()),
                    current_focus.map(str::trim).filter(|s| !s.is_empty()),
                    blocker.map(str::trim).filter(|s| !s.is_empty()),
                    done_criteria.map(str::trim).filter(|s| !s.is_empty()),
                    now
                ],
            )
            .map_err(map_write_error)?;
        if changed == 0 {
            anyhow::bail!("目标不存在: {id}");
        }
        // 同上：full 更新也可能改写 current_focus，pending 卡片随之过时。
        self.connection.execute(
            "UPDATE goal_compass_nudges SET status='realigned' WHERE goal_id=?1 AND status='pending'",
            params![id],
        )?;
        self.sync_record_goal(id)?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    /// 暂停搁置目标。让出活跃名额，但保留所有上下文作为切片历史以备随时复活。
    pub fn pause_goal(&self, id: &str) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        let tx = self.begin_write_transaction()?;
        let changed = self
            .connection
            .execute(
                "UPDATE goals SET status='paused', superseded_at=?2, updated_at=?2 WHERE id=?1",
                params![id, now],
            )
            .map_err(map_write_error)?;
        if changed == 0 {
            anyhow::bail!("目标不存在: {id}");
        }
        self.sync_record_goal(id)?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    /// 完成达成目标。让出活跃名额，记录成就勋章。
    pub fn complete_goal(&self, id: &str) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        let tx = self.begin_write_transaction()?;
        let changed = self
            .connection
            .execute(
                "UPDATE goals SET status='completed', superseded_at=?2, updated_at=?2 WHERE id=?1",
                params![id, now],
            )
            .map_err(map_write_error)?;
        if changed == 0 {
            anyhow::bail!("目标不存在: {id}");
        }
        self.sync_record_goal(id)?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    /// 归档目标（向后兼容接口，等价于 pause_goal）。
    pub fn archive_goal(&self, id: &str) -> Result<()> {
        self.pause_goal(id)
    }

    /// 复活一条非活跃目标。达上限时由触发器拒绝。
    pub fn reactivate_goal(&self, id: &str) -> Result<()> {
        self.reactivate_goal_with_focus(id, None, None)
    }

    /// 带新抓手/卡点复活目标（无缝读档体验）。
    pub fn reactivate_goal_with_focus(
        &self,
        id: &str,
        current_focus: Option<&str>,
        blocker: Option<&str>,
    ) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        let tx = self.begin_write_transaction()?;
        let changed = if current_focus.is_some() || blocker.is_some() {
            self.connection
                .execute(
                    "UPDATE goals SET status='active', superseded_at=NULL,
                            current_focus=COALESCE(?2, current_focus),
                            blocker=COALESCE(?3, blocker),
                            updated_at=?4 WHERE id=?1",
                    params![
                        id,
                        current_focus.map(str::trim).filter(|s| !s.is_empty()),
                        blocker.map(str::trim).filter(|s| !s.is_empty()),
                        now
                    ],
                )
                .map_err(map_write_error)?
        } else {
            self.connection
                .execute(
                    "UPDATE goals SET status='active', superseded_at=NULL, updated_at=?2 WHERE id=?1",
                    params![id, now],
                )
                .map_err(map_write_error)?
        };
        if changed == 0 {
            anyhow::bail!("目标不存在: {id}");
        }
        self.sync_record_goal(id)?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    /// 插入一条航向校准提醒（纯本地衍生衍生提醒，不走 CRDT 同步）。
    pub fn insert_compass_nudge(&self, nudge: &GoalCompassNudge) -> Result<()> {
        self.connection.execute(
            "INSERT INTO goal_compass_nudges(
                id, goal_id, goal_content, plan_id, plan_intent, current_focus, stalled_days,
                recent_topics, status, created_at, dismissed_until
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                nudge.id,
                nudge.goal_id,
                nudge.goal_content,
                nudge.plan_id,
                nudge.plan_intent,
                nudge.current_focus,
                nudge.stalled_days,
                nudge.recent_topics,
                nudge.status,
                nudge.created_at,
                nudge.dismissed_until,
            ],
        )?;
        Ok(())
    }

    /// 获取当前生效中的最新一条航向校准提醒。
    /// 若关联目标已不再处于 active 状态，或当前仍在退避期内，返回 None。
    pub fn get_active_compass_nudge(&self) -> Result<Option<GoalCompassNudge>> {
        let now = chrono::Utc::now().to_rfc3339();
        let mut stmt = self.connection.prepare(
            "SELECT n.id, n.goal_id, n.goal_content, n.plan_id, n.plan_intent, n.current_focus, n.stalled_days,
                    n.recent_topics, n.status, n.created_at, n.dismissed_until
             FROM goal_compass_nudges n
             JOIN goals g ON n.goal_id = g.id
             WHERE n.status = 'pending'
               AND g.status = 'active'
               AND (n.assessment_id IS NULL OR julianday(n.created_at) > julianday(?1,'-7 days'))
               AND (n.dismissed_until IS NULL OR n.dismissed_until <= ?1)
             ORDER BY n.created_at DESC
             LIMIT 1",
        )?;
        let mut rows = stmt.query_map(params![now], |row| {
            Ok(GoalCompassNudge {
                id: row.get(0)?,
                goal_id: row.get(1)?,
                goal_content: row.get(2)?,
                plan_id: row.get(3)?,
                plan_intent: row.get(4)?,
                current_focus: row.get(5)?,
                stalled_days: row.get(6)?,
                recent_topics: row.get(7)?,
                status: row.get(8)?,
                created_at: row.get(9)?,
                dismissed_until: row.get(10)?,
            })
        })?;
        if let Some(res) = rows.next() {
            Ok(Some(res?))
        } else {
            Ok(None)
        }
    }

    /// 标记暂不打扰，设定静默退避天数（通常为 7 天）。
    pub fn dismiss_compass_nudge(&self, id: &str, dismiss_days: i64) -> Result<()> {
        let now = chrono::Utc::now();
        let until = (now + chrono::Duration::days(dismiss_days)).to_rfc3339();
        let changed = self.connection.execute(
            "UPDATE goal_compass_nudges SET status='dismissed', dismissed_until=?2 WHERE id=?1",
            params![id, until],
        )?;
        if changed == 0 {
            anyhow::bail!("航向校准提醒不存在或已处理: {id}");
        }
        Ok(())
    }

    /// 标记为已完成航向校准（例如用户更新了抓手或聚焦攻克卡点）。
    pub fn realign_compass_nudge(&self, id: &str) -> Result<()> {
        self.connection.execute(
            "UPDATE goal_compass_nudges SET status='realigned' WHERE id=?1",
            params![id],
        )?;
        Ok(())
    }

    /// 检查指定目标在最近 within_days 内是否已有提醒，或者是否处于暂不打扰退避期。
    pub fn has_recent_compass_nudge_for_goal(
        &self,
        goal_id: &str,
        within_days: i64,
    ) -> Result<bool> {
        let now = chrono::Utc::now();
        let threshold = (now - chrono::Duration::days(within_days)).to_rfc3339();
        let now_str = now.to_rfc3339();
        let count: i64 = self.connection.query_row(
            "SELECT count(*) FROM goal_compass_nudges
             WHERE goal_id = ?1
               AND (
                 created_at >= ?2
                 OR (dismissed_until IS NOT NULL AND dismissed_until > ?3)
               )",
            params![goal_id, threshold, now_str],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// P4 计划层降频：指定计划在最近 within_days 内是否已有提醒
    /// （含退避期）——同计划换一步也不连环打扰。
    pub fn has_recent_compass_nudge_for_plan(
        &self,
        plan_id: &str,
        within_days: i64,
    ) -> Result<bool> {
        let now = chrono::Utc::now();
        let threshold = (now - chrono::Duration::days(within_days)).to_rfc3339();
        let now_str = now.to_rfc3339();
        let count: i64 = self.connection.query_row(
            "SELECT count(*) FROM goal_compass_nudges
             WHERE plan_id = ?1
               AND (
                 created_at >= ?2
                 OR (dismissed_until IS NOT NULL AND dismissed_until > ?3)
               )",
            params![plan_id, threshold, now_str],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// 与 `storage::tests` 同惯例：临时文件建真库，测完删掉。
    /// 不用 `open_in_memory`，因为迁移与触发器要在真实连接上验证。
    struct TempStore {
        store: Store,
        path: std::path::PathBuf,
    }

    impl Drop for TempStore {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    fn store() -> TempStore {
        let path = std::env::temp_dir().join(format!(
            "elsewhen-goals-test-{}.db",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = Store::open(&path).unwrap();
        TempStore { store, path }
    }

    #[test]
    fn creates_and_lists_active_goals() {
        let store = store();
        store.store.create_goal("上线 v1", GoalPhase::Near).unwrap();
        store
            .store
            .create_goal("做出决策系统", GoalPhase::Long)
            .unwrap();
        let goals = store.store.list_active_goals().unwrap();
        assert_eq!(goals.len(), 2);
        // 排序：近期在长远之前
        assert_eq!(goals[0].phase, GoalPhase::Near);
        assert_eq!(goals[1].phase, GoalPhase::Long);
        assert_eq!(store.store.count_active_goals().unwrap(), 2);
    }

    #[test]
    fn creates_and_updates_goal_with_facets() {
        let store = store();
        let g = store
            .store
            .create_goal_with_facets(
                "全量升级认知目标",
                GoalPhase::Near,
                Some("核心系统演进"),
                Some("落地 Phase G1 存储层"),
                Some("无已知卡点"),
                Some("所有单测全绿且迁移平滑"),
            )
            .unwrap();

        let loaded = store.store.get_goal(&g.id).unwrap().unwrap();
        assert_eq!(loaded.content, "全量升级认知目标");
        assert_eq!(loaded.why.as_deref(), Some("核心系统演进"));
        assert_eq!(
            loaded.current_focus.as_deref(),
            Some("落地 Phase G1 存储层")
        );
        assert_eq!(loaded.blocker.as_deref(), Some("无已知卡点"));
        assert_eq!(
            loaded.done_criteria.as_deref(),
            Some("所有单测全绿且迁移平滑")
        );

        // 滚动更新抓手
        store
            .store
            .update_goal_focus(&g.id, Some("推进 Phase G2 AI 上下文"), Some("等待 G1 落地"))
            .unwrap();
        let updated = store.store.get_goal(&g.id).unwrap().unwrap();
        assert_eq!(
            updated.current_focus.as_deref(),
            Some("推进 Phase G2 AI 上下文")
        );
        assert_eq!(updated.blocker.as_deref(), Some("等待 G1 落地"));
        // why 和 criteria 不受影响
        assert_eq!(updated.why.as_deref(), Some("核心系统演进"));
    }

    #[test]
    fn same_phase_may_hold_several_active_goals() {
        let store = store();
        store.store.create_goal("a", GoalPhase::Near).unwrap();
        store.store.create_goal("b", GoalPhase::Near).unwrap();
        store.store.create_goal("c", GoalPhase::Near).unwrap();
        assert_eq!(store.store.list_active_goals().unwrap().len(), 3);
        // 第四条不再被拒：活跃已满 → 以暂停搁置落库，保留上下文可随时唤醒
        let fourth = store.store.create_goal("d", GoalPhase::Near).unwrap();
        assert_eq!(fourth.status, GoalStatus::Paused);
        assert!(fourth.superseded_at.is_some());
        assert_eq!(store.store.count_active_goals().unwrap(), 3);
        let paused = store.store.list_paused_goals().unwrap();
        assert_eq!(paused.len(), 1);
        assert_eq!(paused[0].content, "d");
    }

    /// 触发器拒绝信息不得把下划线标识原样透给用户。新建路径满额时已改为
    /// 落为 paused 不再报错，这里走「满额下唤醒暂停目标」的 UPDATE 路径覆盖翻译。
    #[test]
    fn cap_error_is_translated_for_display() {
        let store = store();
        for c in ["a", "b", "c"] {
            store.store.create_goal(c, GoalPhase::Near).unwrap();
        }
        let fourth = store.store.create_goal("d", GoalPhase::Near).unwrap();
        assert_eq!(fourth.status, GoalStatus::Paused);
        let err = store
            .store
            .reactivate_goal(&fourth.id)
            .unwrap_err()
            .to_string();
        assert!(err.contains("请先归档"), "实际: {err}");
        assert!(
            !err.contains(ACTIVE_GOAL_LIMIT_REACHED),
            "泄漏了内部标识: {err}"
        );
    }

    /// 按 id 取目标，不按列表下标：列表按阶段排序，改了阶段位置就变了。
    fn active_by_id(store: &TempStore, id: &str) -> Goal {
        store
            .store
            .list_active_goals()
            .unwrap()
            .into_iter()
            .find(|g| g.id == id)
            .unwrap_or_else(|| panic!("活跃目标里找不到 {id}"))
    }

    fn id_of_created(store: &TempStore, content: &str) -> String {
        store
            .store
            .list_active_goals()
            .unwrap()
            .into_iter()
            .find(|g| g.content == content)
            .unwrap_or_else(|| panic!("刚创建的目标 {content} 不在活跃列表里"))
            .id
    }

    #[test]
    fn editing_a_live_goal_is_allowed_at_the_cap() {
        let store = store();
        for c in ["a", "b", "c"] {
            store.store.create_goal(c, GoalPhase::Near).unwrap();
        }
        let target = id_of_created(&store, "a");
        store
            .store
            .update_goal(&target, "改过的目标", GoalPhase::Mid)
            .unwrap();
        let updated = active_by_id(&store, &target);
        assert_eq!(updated.content, "改过的目标");
        assert_eq!(updated.phase, GoalPhase::Mid);
        assert_eq!(store.store.count_active_goals().unwrap(), 3);
    }

    #[test]
    fn pause_and_complete_frees_slot_and_distinguishes_status() {
        let store = store();
        for c in ["a", "b", "c"] {
            store.store.create_goal(c, GoalPhase::Near).unwrap();
        }
        let id_a = id_of_created(&store, "a");
        let id_b = id_of_created(&store, "b");

        // 暂停 a，完成 b
        store.store.pause_goal(&id_a).unwrap();
        assert_eq!(store.store.count_active_goals().unwrap(), 2);
        store.store.complete_goal(&id_b).unwrap();
        assert_eq!(store.store.count_active_goals().unwrap(), 1);

        let paused = store.store.list_paused_goals().unwrap();
        assert_eq!(paused.len(), 1);
        assert_eq!(paused[0].id, id_a);
        assert_eq!(paused[0].status, GoalStatus::Paused);

        let completed = store.store.list_completed_goals().unwrap();
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].id, id_b);
        assert_eq!(completed[0].status, GoalStatus::Completed);

        // list_archived_goals 包含两者
        let archived = store.store.list_archived_goals().unwrap();
        assert_eq!(archived.len(), 2);

        // 腾出名额后可新增
        store.store.create_goal("d", GoalPhase::Mid).unwrap();
        store.store.create_goal("e", GoalPhase::Long).unwrap();
        assert_eq!(store.store.count_active_goals().unwrap(), 3);
    }

    /// 构造「3 条活跃 + 1 条已暂停」，验证复活走 UPDATE 路径时也受上限约束。
    #[test]
    fn reactivate_is_blocked_at_the_cap() {
        let store = store();
        let old = store.store.create_goal("old", GoalPhase::Long).unwrap();
        store.store.pause_goal(&old.id).unwrap();
        for c in ["x", "y", "z"] {
            store.store.create_goal(c, GoalPhase::Near).unwrap();
        }
        assert_eq!(store.store.count_active_goals().unwrap(), 3);
        assert_eq!(store.store.list_archived_goals().unwrap().len(), 1);

        let err = store.store.reactivate_goal(&old.id).unwrap_err();
        assert!(err.to_string().contains("上限"), "实际: {err}");
        assert_eq!(store.store.count_active_goals().unwrap(), 3);
        // 被拒后历史行不得被改动
        assert_eq!(store.store.list_archived_goals().unwrap()[0].id, old.id);
    }

    #[test]
    fn reactivate_with_new_focus_succeeds_once_slot_is_free() {
        let store = store();
        let old = store
            .store
            .create_goal_with_facets(
                "待唤醒目标",
                GoalPhase::Long,
                Some("最初愿景"),
                Some("旧抓手"),
                Some("旧卡点"),
                None,
            )
            .unwrap();
        store.store.pause_goal(&old.id).unwrap();

        for c in ["a", "b", "c"] {
            store.store.create_goal(c, GoalPhase::Near).unwrap();
        }
        let freed = id_of_created(&store, "a");
        store.store.pause_goal(&freed).unwrap();
        assert_eq!(store.store.count_active_goals().unwrap(), 2);

        // 带新抓手读档复活
        store
            .store
            .reactivate_goal_with_focus(&old.id, Some("全新抓手启动"), None)
            .unwrap();
        assert_eq!(store.store.count_active_goals().unwrap(), 3);

        let reactivated = active_by_id(&store, &old.id);
        assert_eq!(reactivated.status, GoalStatus::Active);
        assert_eq!(reactivated.current_focus.as_deref(), Some("全新抓手启动"));
        assert_eq!(reactivated.blocker.as_deref(), Some("旧卡点")); // 保留旧卡点
        assert!(reactivated.superseded_at.is_none());
    }

    #[test]
    fn empty_content_is_rejected_before_touching_the_trigger() {
        let store = store();
        assert!(store.store.create_goal("   ", GoalPhase::Near).is_err());
        assert!(store.store.create_goal("", GoalPhase::Near).is_err());
        assert_eq!(store.store.count_active_goals().unwrap(), 0);
    }

    #[test]
    fn unknown_id_reports_not_found() {
        let store = store();
        assert!(
            store
                .store
                .update_goal("nope", "x", GoalPhase::Near)
                .is_err()
        );
        assert!(store.store.archive_goal("nope").is_err());
        assert!(store.store.pause_goal("nope").is_err());
        assert!(store.store.complete_goal("nope").is_err());
        assert!(store.store.reactivate_goal("nope").is_err());
    }

    #[test]
    fn compass_nudge_lifecycle_and_dismiss_backoff() {
        let store = store();
        let g = store
            .store
            .create_goal_with_facets(
                "攻克多端同步",
                GoalPhase::Near,
                Some("保障数据安全"),
                Some("排查快照分片停滞"),
                None,
                None,
            )
            .unwrap();

        // 初始无 nudge
        assert!(store.store.get_active_compass_nudge().unwrap().is_none());
        assert!(
            !store
                .store
                .has_recent_compass_nudge_for_goal(&g.id, 7)
                .unwrap()
        );

        let nudge = GoalCompassNudge {
            id: "nudge-1".to_string(),
            goal_id: g.id.clone(),
            goal_content: g.content.clone(),
            plan_id: None,
            plan_intent: None,
            current_focus: g.current_focus.clone().unwrap(),
            stalled_days: 4,
            recent_topics: "报销, 闲聊, 整理书架".to_string(),
            status: "pending".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            dismissed_until: None,
        };
        store.store.insert_compass_nudge(&nudge).unwrap();

        // 查询到待对齐卡片
        let active_nudge = store.store.get_active_compass_nudge().unwrap().unwrap();
        assert_eq!(active_nudge.id, "nudge-1");
        assert_eq!(active_nudge.goal_id, g.id);
        assert_eq!(active_nudge.stalled_days, 4);
        assert!(
            store
                .store
                .has_recent_compass_nudge_for_goal(&g.id, 7)
                .unwrap()
        );

        // 用户点击暂不打扰（静默 7 天）
        store
            .store
            .dismiss_compass_nudge(&active_nudge.id, 7)
            .unwrap();
        assert!(store.store.get_active_compass_nudge().unwrap().is_none());
        assert!(
            store
                .store
                .has_recent_compass_nudge_for_goal(&g.id, 7)
                .unwrap()
        );

        // 插入第二条 pending nudge 验证已完成/已暂停的目标会被关联过滤
        let g2 = store
            .store
            .create_goal_with_facets("目标2", GoalPhase::Mid, None, Some("抓手2"), None, None)
            .unwrap();
        let nudge2 = GoalCompassNudge {
            id: "nudge-2".to_string(),
            goal_id: g2.id.clone(),
            goal_content: g2.content.clone(),
            plan_id: None,
            plan_intent: None,
            current_focus: "抓手2".to_string(),
            stalled_days: 5,
            recent_topics: "修车".to_string(),
            status: "pending".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            dismissed_until: None,
        };
        store.store.insert_compass_nudge(&nudge2).unwrap();
        assert!(store.store.get_active_compass_nudge().unwrap().is_some());

        // 当 g2 达成后，其 nudge 自动不再作为 active 展示
        store.store.complete_goal(&g2.id).unwrap();
        assert!(store.store.get_active_compass_nudge().unwrap().is_none());
    }
}
