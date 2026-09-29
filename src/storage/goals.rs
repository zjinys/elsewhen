//! 目标存取（FR-PES-005-01/02）。
//!
//! 活跃目标上限 3 条由 `new_migrations.rs` 里的数据库触发器强制，本层不自己数，
//! 只负责把触发器抛出的 `active_goal_limit_reached` 翻成用户看得懂的话。
//!
//! `phase` 是标签不是槽位：同一阶段可以有多条活跃目标，也不要求三个阶段都有。
//! 目标改内容走 `update_goal`（同一目标），换阶段或换方向走归档加新增，旧目标保留。

use anyhow::Result;
use rusqlite::{params, Error as SqliteError};
use uuid::Uuid;

use super::new_migrations::ACTIVE_GOAL_LIMIT_REACHED;
use super::Store;

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

/// 目标状态：活跃 / 已归档
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalStatus {
    Active,
    Superseded,
}

impl GoalStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            GoalStatus::Active => "active",
            GoalStatus::Superseded => "superseded",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "active" => Some(GoalStatus::Active),
            "superseded" => Some(GoalStatus::Superseded),
            _ => None,
        }
    }
}

/// 一条目标（活跃或已归档）
#[derive(Debug, Clone)]
pub struct Goal {
    pub id: String,
    pub content: String,
    pub phase: GoalPhase,
    pub status: GoalStatus,
    pub created_at: String,
    pub updated_at: String,
    pub superseded_at: Option<String>,
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

const SELECT_COLUMNS: &str = "id, content, phase, status, created_at, updated_at, superseded_at";

fn row_to_goal(row: &rusqlite::Row<'_>) -> rusqlite::Result<Goal> {
    let phase: String = row.get(2)?;
    let status: String = row.get(3)?;
    Ok(Goal {
        id: row.get(0)?,
        content: row.get(1)?,
        // 两列都有 CHECK 约束，解析失败说明数据被绕过写入，此时让上层报错而非静默降级。
        phase: GoalPhase::parse(&phase).ok_or_else(|| {
            SqliteError::InvalidColumnType(2, "phase".to_string(), rusqlite::types::Type::Text)
        })?,
        status: GoalStatus::parse(&status).ok_or_else(|| {
            SqliteError::InvalidColumnType(3, "status".to_string(), rusqlite::types::Type::Text)
        })?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        superseded_at: row.get(6)?,
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

    /// 列出已归档目标，按归档时间倒序。
    pub fn list_archived_goals(&self) -> Result<Vec<Goal>> {
        let sql = format!(
            "SELECT {SELECT_COLUMNS} FROM goals WHERE status='superseded'
             ORDER BY superseded_at DESC, created_at DESC"
        );
        let mut statement = self.connection.prepare(&sql)?;
        let rows = statement.query_map([], row_to_goal)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn count_active_goals(&self) -> Result<i64> {
        Ok(self.connection.query_row(
            "SELECT COUNT(*) FROM goals WHERE status='active'",
            [],
            |row| row.get(0),
        )?)
    }

    /// 新建一条活跃目标。达上限时由触发器拒绝，不自动顶掉既有目标。
    pub fn create_goal(&self, content: &str, phase: GoalPhase) -> Result<Goal> {
        let content = content.trim();
        if content.is_empty() {
            anyhow::bail!("目标内容不能为空");
        }
        let now = chrono::Utc::now().to_rfc3339();
        let goal = Goal {
            id: Uuid::new_v4().to_string(),
            content: content.to_string(),
            phase,
            status: GoalStatus::Active,
            created_at: now.clone(),
            updated_at: now.clone(),
            superseded_at: None,
        };
        self.connection
            .execute(
                "INSERT INTO goals(id, content, phase, status, created_at, updated_at, superseded_at)
                 VALUES (?1, ?2, ?3, 'active', ?4, ?4, NULL)",
                params![goal.id, goal.content, phase.as_str(), now],
            )
            .map_err(map_write_error)?;
        Ok(goal)
    }

    /// 编辑一条目标的正文与阶段。状态不变，故不触碰上限触发器。
    pub fn update_goal(&self, id: &str, content: &str, phase: GoalPhase) -> Result<()> {
        let content = content.trim();
        if content.is_empty() {
            anyhow::bail!("目标内容不能为空");
        }
        let now = chrono::Utc::now().to_rfc3339();
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
        Ok(())
    }

    /// 归档一条目标。归档即让出活跃名额，但保留内容与阶段作为历史。
    pub fn archive_goal(&self, id: &str) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        let changed = self
            .connection
            .execute(
                "UPDATE goals SET status='superseded', superseded_at=?2, updated_at=?2 WHERE id=?1",
                params![id, now],
            )
            .map_err(map_write_error)?;
        if changed == 0 {
            anyhow::bail!("目标不存在: {id}");
        }
        Ok(())
    }

    /// 复活一条已归档目标。达上限时由触发器拒绝。
    pub fn reactivate_goal(&self, id: &str) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        let changed = self
            .connection
            .execute(
                "UPDATE goals SET status='active', superseded_at=NULL, updated_at=?2 WHERE id=?1",
                params![id, now],
            )
            .map_err(map_write_error)?;
        if changed == 0 {
            anyhow::bail!("目标不存在: {id}");
        }
        Ok(())
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
    fn same_phase_may_hold_several_active_goals() {
        let store = store();
        store.store.create_goal("a", GoalPhase::Near).unwrap();
        store.store.create_goal("b", GoalPhase::Near).unwrap();
        store.store.create_goal("c", GoalPhase::Near).unwrap();
        assert_eq!(store.store.list_active_goals().unwrap().len(), 3);
        // 第四条才被拒
        let err = store.store.create_goal("d", GoalPhase::Near).unwrap_err();
        assert!(err.to_string().contains("上限"), "实际: {err}");
    }

    /// 拒绝信息不得把触发器的下划线标识原样透给用户。
    #[test]
    fn cap_error_is_translated_for_display() {
        let store = store();
        for c in ["a", "b", "c"] {
            store.store.create_goal(c, GoalPhase::Near).unwrap();
        }
        let err = store
            .store
            .create_goal("d", GoalPhase::Near)
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
    fn archive_frees_a_slot_and_keeps_history() {
        let store = store();
        for c in ["a", "b", "c"] {
            store.store.create_goal(c, GoalPhase::Near).unwrap();
        }
        let target = id_of_created(&store, "a");
        store.store.archive_goal(&target).unwrap();
        assert_eq!(store.store.count_active_goals().unwrap(), 2);

        let archived = store.store.list_archived_goals().unwrap();
        assert_eq!(archived.len(), 1);
        assert_eq!(archived[0].id, target);
        assert_eq!(archived[0].content, "a");
        assert!(archived[0].superseded_at.is_some());

        store.store.create_goal("d", GoalPhase::Mid).unwrap();
        assert_eq!(store.store.count_active_goals().unwrap(), 3);
    }

    /// 构造「3 条活跃 + 1 条已归档」，验证复活走 UPDATE 路径时也受上限约束。
    #[test]
    fn reactivate_is_blocked_at_the_cap() {
        let store = store();
        let old = store.store.create_goal("old", GoalPhase::Long).unwrap();
        store.store.archive_goal(&old.id).unwrap();
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
    fn reactivate_succeeds_once_a_slot_is_free() {
        let store = store();
        let old = store.store.create_goal("old", GoalPhase::Long).unwrap();
        store.store.archive_goal(&old.id).unwrap();
        for c in ["a", "b", "c"] {
            store.store.create_goal(c, GoalPhase::Near).unwrap();
        }
        let freed = id_of_created(&store, "a");
        store.store.archive_goal(&freed).unwrap();
        assert_eq!(store.store.count_active_goals().unwrap(), 2);

        store.store.reactivate_goal(&old.id).unwrap();
        assert_eq!(store.store.count_active_goals().unwrap(), 3);
        // 复活后归档列表里只剩先前让位的那条
        let archived = store.store.list_archived_goals().unwrap();
        assert_eq!(archived.len(), 1);
        assert_eq!(archived[0].id, freed);
        assert!(active_by_id(&store, &old.id).superseded_at.is_none());
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
        assert!(store
            .store
            .update_goal("nope", "x", GoalPhase::Near)
            .is_err());
        assert!(store.store.archive_goal("nope").is_err());
        assert!(store.store.reactivate_goal("nope").is_err());
    }
}
