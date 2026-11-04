//! 长程计划（Plan）：AI 为中心的意图拆解（roadmap 2026-11-04，P1 存储与协议）。
//!
//! 计划本身不占活跃名额（「目标不设上限、注意力保护移驻并发步骤」）；
//! 步骤就是 `goals` 行，经 `plan_id` / `step_index` 挂接。假设清单（软时间窗、
//! 现状假设、路径假设、风险）为 JSON 数组文本，语义由上层解释，存储层负责
//! 整行一致与同步捕获（`sync_record_plan`）。

use anyhow::Result;
use rusqlite::{Error as SqliteError, OptionalExtension, params};
use uuid::Uuid;

use super::{Goal, GoalPhase, GoalStatus, MAX_ACTIVE_GOALS};
use super::Store;
use super::goals::{SELECT_COLUMNS, row_to_goal};

/// 计划状态机。`proposed` 由审阅卡（确认制）承载，落行即从 `active` 起步。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanStatus {
    Proposed,
    Active,
    Paused,
    Completed,
}

impl PlanStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            PlanStatus::Proposed => "proposed",
            PlanStatus::Active => "active",
            PlanStatus::Paused => "paused",
            PlanStatus::Completed => "completed",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PlanStatus::Proposed => "待定",
            PlanStatus::Active => "推进中",
            PlanStatus::Paused => "暂停搁置",
            PlanStatus::Completed => "已达成",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "proposed" => Some(PlanStatus::Proposed),
            "active" => Some(PlanStatus::Active),
            "paused" => Some(PlanStatus::Paused),
            "completed" => Some(PlanStatus::Completed),
            _ => None,
        }
    }
}

/// 一条长程计划
#[derive(Debug, Clone)]
pub struct GoalPlan {
    pub id: String,
    pub intent_text: String,
    pub vision: String,
    /// 假设清单 JSON 数组文本（软时间窗等；语义由上层解释）
    pub assumptions: String,
    pub status: PlanStatus,
    pub created_at: String,
    pub updated_at: String,
}

/// 计划步骤草稿：P2 AI 拆解提案落库时的单步载荷（与提案 args 同形）。
#[derive(Debug, Clone)]
pub struct PlanStepDraft {
    pub content: String,
    pub phase: GoalPhase,
    pub why: Option<String>,
    pub current_focus: Option<String>,
    pub blocker: Option<String>,
    pub done_criteria: Option<String>,
}

const PLAN_COLUMNS: &str =
    "id, intent_text, vision, assumptions, status, created_at, updated_at";

fn row_to_plan(row: &rusqlite::Row<'_>) -> rusqlite::Result<GoalPlan> {
    let status: String = row.get(4)?;
    Ok(GoalPlan {
        id: row.get(0)?,
        intent_text: row.get(1)?,
        vision: row.get(2)?,
        assumptions: row.get(3)?,
        status: PlanStatus::parse(&status).ok_or_else(|| {
            SqliteError::InvalidColumnType(4, "status".to_string(), rusqlite::types::Type::Text)
        })?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

impl Store {
    /// 新建计划。意图必填；假设清单必须是 JSON 数组（坏载荷不入库）。
    pub fn create_plan(
        &self,
        intent_text: &str,
        vision: &str,
        assumptions_json: &str,
    ) -> Result<GoalPlan> {
        let intent_text = intent_text.trim();
        if intent_text.is_empty() {
            anyhow::bail!("计划意图不能为空");
        }
        let parsed: serde_json::Value = serde_json::from_str(assumptions_json)
            .map_err(|_| anyhow::anyhow!("假设清单必须是合法 JSON"))?;
        if !parsed.is_array() {
            anyhow::bail!("假设清单必须是 JSON 数组");
        }
        let now = chrono::Utc::now().to_rfc3339();
        let plan = GoalPlan {
            id: Uuid::new_v4().to_string(),
            intent_text: intent_text.to_string(),
            vision: vision.trim().to_string(),
            assumptions: assumptions_json.to_string(),
            status: PlanStatus::Active,
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        let tx = self.begin_write_transaction()?;
        self.connection.execute(
            "INSERT INTO goal_plans(id, intent_text, vision, assumptions, status, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            params![
                plan.id,
                plan.intent_text,
                plan.vision,
                plan.assumptions,
                plan.status.as_str(),
                now
            ],
        )?;
        self.sync_record_plan(&plan.id, false)?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(plan)
    }

    pub fn get_plan(&self, id: &str) -> Result<Option<GoalPlan>> {
        let sql = format!("SELECT {PLAN_COLUMNS} FROM goal_plans WHERE id=?1");
        let plan = self
            .connection
            .query_row(&sql, [id], row_to_plan)
            .optional()?;
        Ok(plan)
    }

    /// 列出全部计划（创建时间升序；数量由用户意图而来，不设上限）。
    pub fn list_plans(&self) -> Result<Vec<GoalPlan>> {
        let sql = format!("SELECT {PLAN_COLUMNS} FROM goal_plans ORDER BY created_at, id");
        let mut statement = self.connection.prepare(&sql)?;
        let rows = statement.query_map([], row_to_plan)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// 更新计划的愿景与假设清单（拆解纠错路径：用户改假设即改这里）。
    pub fn update_plan_meta(
        &self,
        id: &str,
        vision: &str,
        assumptions_json: &str,
    ) -> Result<()> {
        let parsed: serde_json::Value = serde_json::from_str(assumptions_json)
            .map_err(|_| anyhow::anyhow!("假设清单必须是合法 JSON"))?;
        if !parsed.is_array() {
            anyhow::bail!("假设清单必须是 JSON 数组");
        }
        let now = chrono::Utc::now().to_rfc3339();
        let tx = self.begin_write_transaction()?;
        let changed = self.connection.execute(
            "UPDATE goal_plans SET vision=?2, assumptions=?3, updated_at=?4 WHERE id=?1",
            params![id, vision.trim(), assumptions_json, now],
        )?;
        if changed == 0 {
            anyhow::bail!("计划不存在");
        }
        self.sync_record_plan(id, false)?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    /// 计划状态流转（暂停搁置 / 读档复活 / 标记达成）。
    pub fn set_plan_status(&self, id: &str, status: PlanStatus) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        let tx = self.begin_write_transaction()?;
        let changed = self.connection.execute(
            "UPDATE goal_plans SET status=?2, updated_at=?3 WHERE id=?1",
            params![id, status.as_str(), now],
        )?;
        if changed == 0 {
            anyhow::bail!("计划不存在");
        }
        self.sync_record_plan(id, false)?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    /// 把一条目标挂接为某计划的步骤（P2/P3 的拆解落库与 AI 调度入口）。
    /// `step_index` 为展示排序键；缺省排在末尾。
    pub fn attach_goal_to_plan(
        &self,
        goal_id: &str,
        plan_id: &str,
        step_index: Option<i64>,
    ) -> Result<()> {
        let plan_exists: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM goal_plans WHERE id=?1)",
            [plan_id],
            |r| r.get(0),
        )?;
        if !plan_exists {
            anyhow::bail!("计划不存在");
        }
        let now = chrono::Utc::now().to_rfc3339();
        let tx = self.begin_write_transaction()?;
        let changed = self.connection.execute(
            "UPDATE goals SET plan_id=?2, step_index=?3, updated_at=?4 WHERE id=?1",
            params![goal_id, plan_id, step_index, now],
        )?;
        if changed == 0 {
            anyhow::bail!("目标不存在");
        }
        self.sync_record_goal(goal_id)?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    /// 按步骤序列出计划的全部步骤（step_index 升序，无序号排末尾）。
    pub fn list_plan_steps(&self, plan_id: &str) -> Result<Vec<Goal>> {
        let sql = format!(
            "SELECT {SELECT_COLUMNS} FROM goals WHERE plan_id=?1 \
             ORDER BY COALESCE(step_index, 9223372036854775807), created_at"
        );
        let mut statement = self.connection.prepare(&sql)?;
        let rows = statement.query_map([plan_id], row_to_goal)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// 并发闸计数：计划内当前处于活跃的步骤数（P2 AI 调度据此决定
    /// 唤醒下一个等待步骤；全局活跃上限仍由 goals 触发器兜底）。
    pub fn count_active_steps(&self, plan_id: &str) -> Result<i64> {
        let count = self.connection.query_row(
            "SELECT COUNT(*) FROM goals WHERE plan_id=?1 AND status='active'",
            [plan_id],
            |r| r.get(0),
        )?;
        Ok(count)
    }

    /// 等待队列：计划内已挂接但落暂停的步骤（按 step_index 升序）。
    /// 唤醒 = 对单条步骤调用既有 `reactivate_goal_with_focus`（名额未满时）。
    pub fn list_waiting_steps(&self, plan_id: &str) -> Result<Vec<Goal>> {
        let sql = format!(
            "SELECT {SELECT_COLUMNS} FROM goals WHERE plan_id=?1 AND status='paused' \
             ORDER BY COALESCE(step_index, 9223372036854775807), created_at"
        );
        let mut statement = self.connection.prepare(&sql)?;
        let rows = statement.query_map([plan_id], row_to_goal)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// 删除计划：先解绑其全部步骤（目标保留为未挂计划状态），再广播删除
    /// 并物理删行。步骤解绑各自走 `sync_record_goal`（对端看到的是目标行
    /// 更新，而非目标删除）。
    pub fn delete_plan(&self, id: &str) -> Result<()> {
        let exists: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM goal_plans WHERE id=?1)",
            [id],
            |r| r.get(0),
        )?;
        if !exists {
            anyhow::bail!("计划不存在");
        }
        let now = chrono::Utc::now().to_rfc3339();
        let tx = self.begin_write_transaction()?;
        let step_ids: Vec<String> = {
            let mut statement = self.connection.prepare(
                "SELECT id FROM goals WHERE plan_id=?1 \
                 ORDER BY COALESCE(step_index, 9223372036854775807), created_at",
            )?;
            statement
                .query_map([id], |r| r.get(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        for step_id in step_ids {
            self.connection.execute(
                "UPDATE goals SET plan_id=NULL, step_index=NULL, updated_at=?2 WHERE id=?1",
                params![step_id, now],
            )?;
            self.sync_record_goal(&step_id)?;
        }
        self.sync_record_plan(id, true)?;
        self.connection
            .execute("DELETE FROM goal_plans WHERE id=?1", [id])?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    /// P2：确认落库一条完整计划——计划整行 + 全部步骤目标，单事务提交。
    /// 第一步按活跃名额激活；其余一律落暂停排队（「同计划默认 1 步推进」，
    /// 注意力保护由 AI 调度，触发器仍兜底全局上限）。
    pub fn create_plan_with_steps(
        &self,
        intent_text: &str,
        vision: &str,
        assumptions_json: &str,
        steps: &[PlanStepDraft],
    ) -> Result<GoalPlan> {
        if steps.is_empty() {
            anyhow::bail!("计划至少需要一个步骤");
        }
        if steps.len() > 12 {
            anyhow::bail!("单个计划最多 12 个步骤");
        }
        let intent_text = intent_text.trim();
        if intent_text.is_empty() {
            anyhow::bail!("计划意图不能为空");
        }
        let parsed: serde_json::Value = serde_json::from_str(assumptions_json)
            .map_err(|_| anyhow::anyhow!("假设清单必须是合法 JSON"))?;
        if !parsed.is_array() {
            anyhow::bail!("假设清单必须是 JSON 数组");
        }
        let now = chrono::Utc::now().to_rfc3339();
        let plan = GoalPlan {
            id: Uuid::new_v4().to_string(),
            intent_text: intent_text.to_string(),
            vision: vision.trim().to_string(),
            assumptions: assumptions_json.to_string(),
            status: PlanStatus::Active,
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        let tx = self.begin_write_transaction()?;
        self.connection.execute(
            "INSERT INTO goal_plans(id, intent_text, vision, assumptions, status, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            params![
                plan.id,
                plan.intent_text,
                plan.vision,
                plan.assumptions,
                plan.status.as_str(),
                now
            ],
        )?;
        self.sync_record_plan(&plan.id, false)?;
        let activate_first = self.count_active_goals()? < MAX_ACTIVE_GOALS as i64;
        for (i, step) in steps.iter().enumerate() {
            let status = if i == 0 && activate_first {
                GoalStatus::Active
            } else {
                GoalStatus::Paused
            };
            let _ = self.insert_plan_step(&plan.id, (i as i64) + 1, step, status)?;
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(plan)
    }

    /// P3：「直接建一条目标」= 自动包成单步计划（无独立目标概念）。
    /// 计划意图 = 目标正文（原话不改写）；步骤 1 = 该目标本身。
    /// 活跃名额未满时步骤直接激活；已满时落暂停排队（既有满额语义）。
    pub fn create_goal_as_single_step_plan(
        &self,
        content: &str,
        phase: GoalPhase,
        why: Option<&str>,
        current_focus: Option<&str>,
        blocker: Option<&str>,
        done_criteria: Option<&str>,
    ) -> Result<(GoalPlan, Goal)> {
        let steps = [PlanStepDraft {
            content: content.to_string(),
            phase,
            why: why.map(str::to_string),
            current_focus: current_focus.map(str::to_string),
            blocker: blocker.map(str::to_string),
            done_criteria: done_criteria.map(str::to_string),
        }];
        let plan = self.create_plan_with_steps(content, "", "[]", &steps)?;
        let goal = self
            .list_plan_steps(&plan.id)?
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("单步计划缺少步骤"))?;
        Ok((plan, goal))
    }

    /// P2：为既有计划追加步骤（追加拆解）。序号接续现有最大值；新步骤全部
    /// 落暂停排队，活跃名额空出时再由推进循环唤醒。
    pub fn append_plan_steps(&self, plan_id: &str, steps: &[PlanStepDraft]) -> Result<Vec<String>> {
        if steps.is_empty() {
            anyhow::bail!("至少要追加一个步骤");
        }
        if steps.len() > 12 {
            anyhow::bail!("单次最多追加 12 个步骤");
        }
        let exists: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM goal_plans WHERE id=?1)",
            [plan_id],
            |r| r.get(0),
        )?;
        if !exists {
            anyhow::bail!("计划不存在");
        }
        let base: i64 = self.connection.query_row(
            "SELECT COALESCE(MAX(step_index), 0) FROM goals WHERE plan_id=?1",
            [plan_id],
            |r| r.get(0),
        )?;
        let tx = self.begin_write_transaction()?;
        let mut ids = Vec::with_capacity(steps.len());
        for (i, step) in steps.iter().enumerate() {
            ids.push(self.insert_plan_step(
                plan_id,
                base + (i as i64) + 1,
                step,
                GoalStatus::Paused,
            )?);
        }
        // 计划本体 updated_at 随之同步（列表排序与对端感知）。
        self.connection.execute(
            "UPDATE goal_plans SET updated_at=?2 WHERE id=?1",
            params![plan_id, chrono::Utc::now().to_rfc3339()],
        )?;
        self.sync_record_plan(plan_id, false)?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(ids)
    }

    /// 插入一条已挂接的步骤目标（计划列随出生写入；同步在同一事务内捕获）。
    fn insert_plan_step(
        &self,
        plan_id: &str,
        step_index: i64,
        step: &PlanStepDraft,
        status: GoalStatus,
    ) -> Result<String> {
        let content = step.content.trim();
        if content.is_empty() {
            anyhow::bail!("步骤内容不能为空");
        }
        let now = chrono::Utc::now().to_rfc3339();
        let superseded_at = if status == GoalStatus::Paused {
            Some(now.clone())
        } else {
            None
        };
        let id = Uuid::new_v4().to_string();
        self.connection.execute(
            "INSERT INTO goals(id, content, phase, status, why, current_focus, blocker, done_criteria, created_at, updated_at, superseded_at, plan_id, step_index) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9, ?10, ?11, ?12)",
            params![
                id,
                content,
                step.phase.as_str(),
                status.as_str(),
                step.why.as_deref().map(str::trim).filter(|s| !s.is_empty()),
                step.current_focus
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty()),
                step.blocker
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty()),
                step.done_criteria
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty()),
                now,
                superseded_at,
                plan_id,
                step_index
            ],
        )?;
        self.sync_record_goal(&id)?;
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::GoalPhase;
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
            "elsewhen-plans-test-{}.db",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = Store::open(&path).unwrap();
        TempStore { store, path }
    }

    #[test]
    fn creates_lists_and_updates_plans() {
        let store = store();
        let a = store
            .store
            .create_plan(
                "3 年内月入 10 万",
                "可持续的独立收入",
                r#"[{"kind":"现状假设","text":"当前主业占用 60% 时间"}]"#,
            )
            .unwrap();
        let b = store.store.create_plan("跑完一次半马", "", "[]").unwrap();
        assert_eq!(a.status, PlanStatus::Active);
        let plans = store.store.list_plans().unwrap();
        assert_eq!(plans.len(), 2);
        assert!(plans.iter().any(|p| p.id == a.id));
        assert!(plans.iter().any(|p| p.id == b.id));

        store
            .store
            .update_plan_meta(
                &a.id,
                "改后的愿景",
                r#"[{"kind":"路径假设","text":"先验证付费意愿"}]"#,
            )
            .unwrap();
        let a2 = store.store.get_plan(&a.id).unwrap().unwrap();
        assert_eq!(a2.vision, "改后的愿景");
        assert!(a2.assumptions.contains("路径假设"));

        store
            .store
            .set_plan_status(&b.id, PlanStatus::Paused)
            .unwrap();
        let b2 = store.store.get_plan(&b.id).unwrap().unwrap();
        assert_eq!(b2.status, PlanStatus::Paused);
        assert!(store
            .store
            .set_plan_status("nope", PlanStatus::Completed)
            .is_err());
        assert!(store.store.get_plan("nope").unwrap().is_none());
    }

    #[test]
    fn rejects_empty_intent_and_bad_assumptions() {
        let store = store();
        assert!(store.store.create_plan("   ", "", "[]").is_err());
        assert!(store.store.create_plan("意图", "", "not json").is_err());
        assert!(
            store.store.create_plan("意图", "", "{}").is_err(),
            "假设清单必须是 JSON 数组"
        );
        let plan = store.store.create_plan("意图", "", "[]").unwrap();
        assert!(store
            .store
            .update_plan_meta(&plan.id, "v", r#"{"a":1}"#)
            .is_err());
    }

    #[test]
    fn attaches_goals_and_lists_steps_in_order() {
        let store = store();
        let plan = store.store.create_plan("发布 1.0", "", "[]").unwrap();
        let g1 = store
            .store
            .create_goal_with_facets(
                "第一步：跑通快照",
                GoalPhase::Near,
                None,
                Some("收敛分片"),
                None,
                None,
            )
            .unwrap();
        let g2 = store
            .store
            .create_goal("第二步：多端冒烟", GoalPhase::Mid)
            .unwrap();
        store
            .store
            .attach_goal_to_plan(&g2.id, &plan.id, Some(2))
            .unwrap();
        store
            .store
            .attach_goal_to_plan(&g1.id, &plan.id, Some(1))
            .unwrap();

        let steps = store.store.list_plan_steps(&plan.id).unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].id, g1.id);
        assert_eq!(steps[0].plan_id.as_deref(), Some(plan.id.as_str()));
        assert_eq!(steps[0].step_index, Some(1));
        assert_eq!(steps[1].id, g2.id);

        // 未挂计划的目标不出现在步骤里。
        let lone = store
            .store
            .create_goal("散装目标", GoalPhase::Long)
            .unwrap();
        assert!(!store
            .store
            .list_plan_steps(&plan.id)
            .unwrap()
            .iter()
            .any(|g| g.id == lone.id));
    }

    #[test]
    fn attach_rejects_unknown_targets() {
        let store = store();
        let plan = store.store.create_plan("计划", "", "[]").unwrap();
        let goal = store.store.create_goal("目标", GoalPhase::Near).unwrap();
        assert!(store
            .store
            .attach_goal_to_plan(&goal.id, "no-plan", Some(1))
            .is_err());
        assert!(store
            .store
            .attach_goal_to_plan("no-goal", &plan.id, Some(1))
            .is_err());
    }

    #[test]
    fn gate_counts_active_steps_and_lists_waiting_in_order() {
        let store = store();
        let plan = store.store.create_plan("闸门计划", "", "[]").unwrap();
        let g1 = store.store.create_goal("步骤一", GoalPhase::Near).unwrap();
        let g2 = store.store.create_goal("步骤二", GoalPhase::Mid).unwrap();
        store
            .store
            .attach_goal_to_plan(&g1.id, &plan.id, Some(1))
            .unwrap();
        store
            .store
            .attach_goal_to_plan(&g2.id, &plan.id, Some(2))
            .unwrap();
        assert_eq!(store.store.count_active_steps(&plan.id).unwrap(), 2);

        // 落暂停即进入等待队列，活跃计数让位。
        store.store.pause_goal(&g2.id).unwrap();
        assert_eq!(store.store.count_active_steps(&plan.id).unwrap(), 1);
        let waiting = store.store.list_waiting_steps(&plan.id).unwrap();
        assert_eq!(waiting.len(), 1);
        assert_eq!(waiting[0].id, g2.id);

        // 唤回复活：回到活跃计数，等待队列清空。
        store
            .store
            .reactivate_goal_with_focus(&g2.id, None, None)
            .unwrap();
        assert_eq!(store.store.count_active_steps(&plan.id).unwrap(), 2);
        assert!(store.store.list_waiting_steps(&plan.id).unwrap().is_empty());
    }

    #[test]
    fn delete_plan_detaches_steps_and_removes_row() {
        let store = store();
        let plan = store.store.create_plan("将被删除", "", "[]").unwrap();
        let g1 = store.store.create_goal("步骤", GoalPhase::Near).unwrap();
        store
            .store
            .attach_goal_to_plan(&g1.id, &plan.id, Some(1))
            .unwrap();
        store.store.delete_plan(&plan.id).unwrap();
        assert!(store.store.get_plan(&plan.id).unwrap().is_none());
        // 步骤目标保留，但已解绑（对端看到的是目标行更新而非删除）。
        let step = store.store.get_goal(&g1.id).unwrap().unwrap();
        assert!(step.plan_id.is_none() && step.step_index.is_none());
        assert!(store.store.delete_plan("nope").is_err());
    }

    #[test]
    fn create_plan_with_steps_activates_first_and_queues_rest() {
        let store = store();
        let steps = vec![
            PlanStepDraft {
                content: "第一步：验证付费意愿".into(),
                phase: GoalPhase::Near,
                why: None,
                current_focus: Some("做 10 次访谈".into()),
                blocker: None,
                done_criteria: Some("10 次访谈完成".into()),
            },
            PlanStepDraft {
                content: "第二步：最小可售卖方案".into(),
                phase: GoalPhase::Near,
                why: None,
                current_focus: None,
                blocker: None,
                done_criteria: None,
            },
            PlanStepDraft {
                content: "第三步：首笔收入".into(),
                phase: GoalPhase::Mid,
                why: None,
                current_focus: None,
                blocker: None,
                done_criteria: None,
            },
        ];
        let plan = store
            .store
            .create_plan_with_steps("3 年内月入 10 万", "可持续的独立收入", "[]", &steps)
            .unwrap();
        let listed = store.store.list_plan_steps(&plan.id).unwrap();
        assert_eq!(listed.len(), 3);
        assert_eq!(listed[0].status, crate::storage::GoalStatus::Active);
        assert_eq!(listed[0].step_index, Some(1));
        assert_eq!(listed[0].plan_id.as_deref(), Some(plan.id.as_str()));
        assert_eq!(listed[1].status, crate::storage::GoalStatus::Paused);
        assert_eq!(listed[2].status, crate::storage::GoalStatus::Paused);
        assert_eq!(store.store.count_active_steps(&plan.id).unwrap(), 1);
        assert_eq!(store.store.list_waiting_steps(&plan.id).unwrap().len(), 2);

        // 追加：序号接续、全部排队。
        let more = vec![PlanStepDraft {
            content: "第四步：规模化".into(),
            phase: GoalPhase::Long,
            why: None,
            current_focus: None,
            blocker: None,
            done_criteria: None,
        }];
        assert_eq!(store.store.append_plan_steps(&plan.id, &more).unwrap().len(), 1);
        let listed = store.store.list_plan_steps(&plan.id).unwrap();
        assert_eq!(listed.len(), 4);
        assert_eq!(listed[3].step_index, Some(4));
        assert_eq!(listed[3].status, crate::storage::GoalStatus::Paused);
    }

    #[test]
    fn create_plan_with_steps_respects_global_active_cap() {
        let store = store();
        for i in 0..3 {
            store
                .store
                .create_goal(&format!("满额占位 {i}"), GoalPhase::Near)
                .unwrap();
        }
        let steps = vec![PlanStepDraft {
            content: "第一步".into(),
            phase: GoalPhase::Near,
            why: None,
            current_focus: None,
            blocker: None,
            done_criteria: None,
        }];
        let plan = store
            .store
            .create_plan_with_steps("满额时的计划", "", "[]", &steps)
            .unwrap();
        let listed = store.store.list_plan_steps(&plan.id).unwrap();
        assert_eq!(
            listed[0].status,
            crate::storage::GoalStatus::Paused,
            "全局活跃已满：第一步也先排队"
        );
        assert_eq!(store.store.list_waiting_steps(&plan.id).unwrap().len(), 1);
    }

    #[test]
    fn create_goal_as_single_step_plan_wraps_active_and_queues_when_full() {
        let store = store();
        let (plan, goal) = store
            .store
            .create_goal_as_single_step_plan(
                "发布 Elsewhen 1.0",
                GoalPhase::Near,
                Some("验证闭环"),
                Some("收敛快照"),
                None,
                Some("全绿"),
            )
            .unwrap();
        assert_eq!(plan.intent_text, "发布 Elsewhen 1.0");
        assert_eq!(goal.plan_id.as_deref(), Some(plan.id.as_str()));
        assert_eq!(goal.step_index, Some(1));
        assert_eq!(goal.status, crate::storage::GoalStatus::Active);
        assert_eq!(goal.current_focus.as_deref(), Some("收敛快照"));

        // 占满全局名额后，新单步计划的目标落暂停排队。
        store.store.create_goal("占位 A", GoalPhase::Near).unwrap();
        store.store.create_goal("占位 B", GoalPhase::Near).unwrap();
        let (_, queued) = store
            .store
            .create_goal_as_single_step_plan(
                "满额后的目标",
                GoalPhase::Mid,
                None,
                None,
                None,
                None,
            )
            .unwrap();
        assert_eq!(queued.status, crate::storage::GoalStatus::Paused);
    }
}
