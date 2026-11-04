//! goal_tools.rs：目标与抓手认知工具

use super::Tool;
use super::dispatch::{arg_str, arg_str_opt};
use super::write_tools::store_create_pending;
use super::{ToolContext, ToolPolicy};
use anyhow::Result;
use serde_json::{Value, json};

/// 查询目标清单（支持按状态过滤：active / paused / completed / all）
pub(super) struct ListGoalsTool;
impl Tool for ListGoalsTool {
    fn name(&self) -> &'static str {
        "list_goals"
    }
    fn description(&self) -> &'static str {
        "查询个人目标列表（可过滤状态：active 活跃 / paused 暂停 / completed 已达成 / all 全部）。用户问起历史目标、已暂停或已达成目标时使用。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "status": {
                    "type": "string",
                    "enum": ["active", "paused", "completed", "all"],
                    "description": "可选：状态过滤，缺省为 active"
                }
            },
            "additionalProperties": false
        })
    }
    fn deferred(&self) -> bool {
        true
    }
    fn policy(&self) -> ToolPolicy {
        ToolPolicy::Read
    }
    fn run(&self, args: &Value, ctx: &ToolContext) -> Result<String> {
        let status = args
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("active");
        let goals = match status {
            "active" => ctx.store.list_active_goals()?,
            "paused" => ctx.store.list_paused_goals()?,
            "completed" => ctx.store.list_completed_goals()?,
            "all" => {
                let mut list = ctx.store.list_active_goals()?;
                list.extend(ctx.store.list_paused_goals()?);
                list.extend(ctx.store.list_completed_goals()?);
                list
            }
            _ => ctx.store.list_active_goals()?,
        };
        if goals.is_empty() {
            return Ok(format!("当前暂无状态为 {status} 的目标。"));
        }
        let mut out = format!("目标清单（{status}）：\n");
        for g in goals {
            out.push_str(&format!(
                "- [{}] [ID:{}] {}\n",
                g.phase.label(),
                g.id,
                g.content
            ));
            if let Some(why) = &g.why {
                if !why.trim().is_empty() {
                    out.push_str(&format!("  · 核心动力：{}\n", why.trim()));
                }
            }
            if let Some(focus) = &g.current_focus {
                if !focus.trim().is_empty() {
                    out.push_str(&format!("  · 当前抓手：{}\n", focus.trim()));
                }
            }
            if let Some(blocker) = &g.blocker {
                if !blocker.trim().is_empty() {
                    out.push_str(&format!("  · 已知卡点：{}\n", blocker.trim()));
                }
            }
            if let Some(done) = &g.done_criteria {
                if !done.trim().is_empty() {
                    out.push_str(&format!("  · 完成判据：{}\n", done.trim()));
                }
            }
        }
        Ok(out)
    }
}

/// 提议设立新目标（确认后创建）
pub(super) struct CreateGoalTool;
impl Tool for CreateGoalTool {
    fn name(&self) -> &'static str {
        "create_goal"
    }
    fn description(&self) -> &'static str {
        "为用户设立新的认知型目标（需用户在审阅卡确认）。活跃目标严格上限为 3 个，满额时新目标会以「暂停搁置」状态创建、可随时读档唤醒。请在对话中协助用户明确：阶段、愿景、为什么做(why)、近1~2周第一启动抓手(current_focus)、可能卡点(blocker)以及达成判据(done_criteria)。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "content": { "type": "string", "description": "目标愿景名称，必填" },
                "phase": { "type": "string", "enum": ["near", "mid", "long"], "description": "目标周期：near(近期1~3月)/mid(中期)/long(长远)，缺省 near" },
                "why": { "type": "string", "description": "为什么做 / 核心动力（1~2句话），可选" },
                "current_focus": { "type": "string", "description": "当前 1~2 周最先启动的抓手切片，可选" },
                "blocker": { "type": "string", "description": "已知卡点 / 困难预判，可选" },
                "done_criteria": { "type": "string", "description": "完成判据 / 验收标准，可选" }
            },
            "required": ["content"],
            "additionalProperties": false
        })
    }
    fn deferred(&self) -> bool {
        true
    }
    fn policy(&self) -> ToolPolicy {
        ToolPolicy::WriteConfirm
    }
    fn run(&self, args: &Value, ctx: &ToolContext) -> Result<String> {
        let content = arg_str(args, "content")?;
        let phase = arg_str_opt(args, "phase").unwrap_or_else(|| "near".to_string());
        let why = arg_str_opt(args, "why");
        let current_focus = arg_str_opt(args, "current_focus");
        let blocker = arg_str_opt(args, "blocker");
        let done_criteria = arg_str_opt(args, "done_criteria");

        let action_args = json!({
            "content": content,
            "phase": phase,
            "why": why,
            "current_focus": current_focus,
            "blocker": blocker,
            "done_criteria": done_criteria,
        });

        store_create_pending(ctx.store, ctx.conversation_id, "create_goal", &action_args)?;
        Ok(format!(
            "已为你草拟设立目标提议（待确认，尚未写入）：\n「{content}」\n—— 请在审阅卡片中核对动力、当前抓手与判据；点击接受后创建（活跃已满 3 条时会先放入「暂停搁置」，可随时读档唤醒）。"
        ))
    }
}

/// 提议更新目标的认知切片（抓手、卡点、判据）
pub(super) struct UpdateGoalFacetsTool;
impl Tool for UpdateGoalFacetsTool {
    fn name(&self) -> &'static str {
        "update_goal_facets"
    }
    fn description(&self) -> &'static str {
        "更新既有活跃目标的当前抓手（current_focus）、已知卡点（blocker）或完成判据（需用户确认）。适用于攻克一个切片后推向下一个切片，或卡点发生变化时。无需归档重建目标。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "goal_id": { "type": "string", "description": "要更新的目标 ID，必填" },
                "current_focus": { "type": "string", "description": "最新推进的抓手切片（最近1~2周），可选" },
                "blocker": { "type": "string", "description": "最新识别的卡点或解题卡点，可选" },
                "done_criteria": { "type": "string", "description": "更新完成判据，可选" }
            },
            "required": ["goal_id"],
            "additionalProperties": false
        })
    }
    fn deferred(&self) -> bool {
        true
    }
    fn policy(&self) -> ToolPolicy {
        ToolPolicy::WriteConfirm
    }
    fn run(&self, args: &Value, ctx: &ToolContext) -> Result<String> {
        let goal_id = arg_str(args, "goal_id")?;
        let goal = ctx
            .store
            .get_goal(&goal_id)?
            .ok_or_else(|| anyhow::anyhow!("未找到 ID 为 {goal_id} 的目标"))?;
        let current_focus = arg_str_opt(args, "current_focus");
        let blocker = arg_str_opt(args, "blocker");
        let done_criteria = arg_str_opt(args, "done_criteria");

        let action_args = json!({
            "goal_id": goal_id,
            "goal_content": goal.content,
            "current_focus": current_focus,
            "blocker": blocker,
            "done_criteria": done_criteria,
        });

        store_create_pending(
            ctx.store,
            ctx.conversation_id,
            "update_goal_facets",
            &action_args,
        )?;
        Ok(format!(
            "已为你草拟更新目标「{}」切片的提议（待确认，尚未生效）：\n—— 请在审阅卡片中核对新的当前抓手或卡点，点击接受后生效。",
            goal.content
        ))
    }
}

/// 提议暂停目标（确认后置为 paused，随时可读档复活）
pub(super) struct PauseGoalTool;
impl Tool for PauseGoalTool {
    fn name(&self) -> &'static str {
        "pause_goal"
    }
    fn description(&self) -> &'static str {
        "提议把一条活跃目标暂时搁置（需用户在审阅卡确认）。适用于用户明确表示想先放下/暂停某个目标、或精力需要让位给其他目标时。暂停的目标保留全部切片，随时可读档复活。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "goal_id": { "type": "string", "description": "要暂停的目标 ID，必填" },
                "reason": { "type": "string", "description": "暂停原因（会展示给用户核对），可选" }
            },
            "required": ["goal_id"],
            "additionalProperties": false
        })
    }
    fn deferred(&self) -> bool {
        true
    }
    fn policy(&self) -> ToolPolicy {
        ToolPolicy::WriteConfirm
    }
    fn run(&self, args: &Value, ctx: &ToolContext) -> Result<String> {
        let goal_id = arg_str(args, "goal_id")?;
        let goal = ctx
            .store
            .get_goal(&goal_id)?
            .ok_or_else(|| anyhow::anyhow!("未找到 ID 为 {goal_id} 的目标"))?;
        if goal.status != crate::storage::GoalStatus::Active {
            anyhow::bail!("目标「{}」当前不是活跃状态，无需暂停", goal.content);
        }
        let reason = arg_str_opt(args, "reason");
        let action_args = json!({
            "goal_id": goal_id,
            "goal_content": goal.content,
            "reason": reason,
        });
        store_create_pending(ctx.store, ctx.conversation_id, "pause_goal", &action_args)?;
        Ok(format!(
            "已为你草拟暂停目标「{}」的提议（待确认，尚未生效）：\n—— 请在审阅卡片中核对，点击接受后目标进入暂停搁置（可随时读档复活）。",
            goal.content
        ))
    }
}

/// 提议达成目标（确认后归档为成就）
pub(super) struct CompleteGoalTool;
impl Tool for CompleteGoalTool {
    fn name(&self) -> &'static str {
        "complete_goal"
    }
    fn description(&self) -> &'static str {
        "提议把一条活跃目标标记为达成（需用户在审阅卡确认）。仅在用户明确表示目标已经完成、或达成判据已满足时使用。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "goal_id": { "type": "string", "description": "要标记达成的目标 ID，必填" }
            },
            "required": ["goal_id"],
            "additionalProperties": false
        })
    }
    fn deferred(&self) -> bool {
        true
    }
    fn policy(&self) -> ToolPolicy {
        ToolPolicy::WriteConfirm
    }
    fn run(&self, args: &Value, ctx: &ToolContext) -> Result<String> {
        let goal_id = arg_str(args, "goal_id")?;
        let goal = ctx
            .store
            .get_goal(&goal_id)?
            .ok_or_else(|| anyhow::anyhow!("未找到 ID 为 {goal_id} 的目标"))?;
        if goal.status != crate::storage::GoalStatus::Active {
            anyhow::bail!("目标「{}」当前不是活跃状态，无法标记达成", goal.content);
        }
        let action_args = json!({
            "goal_id": goal_id,
            "goal_content": goal.content,
        });
        store_create_pending(ctx.store, ctx.conversation_id, "complete_goal", &action_args)?;
        Ok(format!(
            "已为你草拟达成目标「{}」的提议（待确认，尚未生效）：\n—— 请在审阅卡片中核对完成判据，点击接受后归档为成就 🎉。",
            goal.content
        ))
    }
}

/// 提议复活暂停目标（确认后重新激活，可带新抓手/卡点）
pub(super) struct ReactivateGoalTool;
impl Tool for ReactivateGoalTool {
    fn name(&self) -> &'static str {
        "reactivate_goal"
    }
    fn description(&self) -> &'static str {
        "提议把一条暂停搁置的目标重新激活（需用户在审阅卡确认）。活跃目标严格上限 3 条，须有空位；可顺带更新重启后的抓手与卡点。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "goal_id": { "type": "string", "description": "要复活的目标 ID（状态须为 paused），必填" },
                "current_focus": { "type": "string", "description": "重启后的首个抓手切片，可选（保留原值若省略）" },
                "blocker": { "type": "string", "description": "重启后的已知卡点，可选（保留原值若省略）" }
            },
            "required": ["goal_id"],
            "additionalProperties": false
        })
    }
    fn deferred(&self) -> bool {
        true
    }
    fn policy(&self) -> ToolPolicy {
        ToolPolicy::WriteConfirm
    }
    fn run(&self, args: &Value, ctx: &ToolContext) -> Result<String> {
        let goal_id = arg_str(args, "goal_id")?;
        let goal = ctx
            .store
            .get_goal(&goal_id)?
            .ok_or_else(|| anyhow::anyhow!("未找到 ID 为 {goal_id} 的目标"))?;
        if goal.status != crate::storage::GoalStatus::Paused {
            anyhow::bail!("目标「{}」不在暂停搁置状态，无法读档复活", goal.content);
        }
        if ctx.store.count_active_goals()? >= crate::storage::MAX_ACTIVE_GOALS as i64 {
            anyhow::bail!(
                "活跃目标已达 {} 条上限，请先暂停或达成一条再复活",
                crate::storage::MAX_ACTIVE_GOALS
            );
        }
        let current_focus = arg_str_opt(args, "current_focus");
        let blocker = arg_str_opt(args, "blocker");
        let action_args = json!({
            "goal_id": goal_id,
            "goal_content": goal.content,
            "current_focus": current_focus,
            "blocker": blocker,
        });
        store_create_pending(ctx.store, ctx.conversation_id, "reactivate_goal", &action_args)?;
        Ok(format!(
            "已为你草拟复活目标「{}」的提议（待确认，尚未生效）：\n—— 请在审阅卡片中核对重启抓手，点击接受后目标重新激活。",
            goal.content
        ))
    }
}
