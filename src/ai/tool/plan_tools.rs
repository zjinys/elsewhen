//! plan_tools.rs：长程计划拆解工具（P2「AI 为中心的目标拆解」）。
//!
//! 确认制：`create_plan` / `add_plan_steps` 均为 WriteConfirm——只登记待确认
//! 动作；用户在审阅卡接受（或逐条纠正假设后接受）才真正落库。计划的执行在
//! `revision_tools::execute_pending_action`（计划整行 + 全部步骤目标单事务提交）。

use super::Tool;
use super::dispatch::{arg_str, arg_str_opt};
use super::write_tools::store_create_pending;
use super::{ToolContext, ToolPolicy};
use anyhow::Result;
use serde_json::{Value, json};

/// 校验步骤数组（1~12 步、每步 content 必填）；返回原数组引用透传。
fn steps_array(args: &Value) -> Result<&Vec<Value>> {
    let steps = args
        .get("steps")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("steps 必须是数组"))?;
    if steps.is_empty() {
        anyhow::bail!("至少需要一个步骤（由近及远）");
    }
    if steps.len() > 12 {
        anyhow::bail!("单个计划最多 12 个步骤，请合并或拆成多次追加");
    }
    for (i, step) in steps.iter().enumerate() {
        let content = step
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        if content.is_empty() {
            anyhow::bail!("第 {} 步缺少内容", i + 1);
        }
    }
    Ok(steps)
}

/// 列出全部长程计划与推进进度（拆解、追加步骤前的查询入口）
pub(super) struct ListPlansTool;
impl Tool for ListPlansTool {
    fn name(&self) -> &'static str {
        "list_plans"
    }
    fn description(&self) -> &'static str {
        "查询用户的长程计划清单（含每计划进度：推进中 / 排队步骤数）。用户问起计划、需要 plan_id 追加步骤或回看拆解时使用。"
    }
    fn parameters_schema(&self) -> Value {
        json!({ "type": "object", "properties": {}, "additionalProperties": false })
    }
    fn deferred(&self) -> bool {
        true
    }
    fn policy(&self) -> ToolPolicy {
        ToolPolicy::Read
    }
    fn run(&self, _args: &Value, ctx: &ToolContext) -> Result<String> {
        let plans = ctx.store.list_plans()?;
        if plans.is_empty() {
            return Ok("当前暂无长程计划。".to_string());
        }
        let mut out = String::from("长程计划清单：\n");
        for plan in plans {
            let steps = ctx.store.list_plan_steps(&plan.id)?;
            let active = steps
                .iter()
                .filter(|g| g.status == crate::storage::GoalStatus::Active)
                .count();
            let waiting = steps
                .iter()
                .filter(|g| g.status == crate::storage::GoalStatus::Paused)
                .count();
            out.push_str(&format!(
                "- [{}] [ID:{}] {}\n  · 推进中 {active} 步，排队 {waiting} 步\n",
                plan.status.label(),
                plan.id,
                plan.intent_text
            ));
            for g in steps
                .iter()
                .filter(|g| g.status == crate::storage::GoalStatus::Active)
            {
                out.push_str(&format!("    · 推进 [ID:{}] {}\n", g.id, g.content));
            }
            for g in steps
                .iter()
                .filter(|g| g.status == crate::storage::GoalStatus::Paused)
            {
                out.push_str(&format!("    · 等待 [ID:{}] {}\n", g.id, g.content));
            }
        }
        Ok(out)
    }
}

/// 把用户的长期意图拆解为长程计划草案（计划全貌 + 假设清单 + 里程碑步骤；
/// 确认后第一步进入推进，其余排队）
pub(super) struct CreatePlanTool;
impl Tool for CreatePlanTool {
    fn name(&self) -> &'static str {
        "create_plan"
    }
    fn description(&self) -> &'static str {
        "把用户的长期意图（如「3 年内月入 10 万」）拆解为长程计划草案，需用户在审阅卡确认后落库。方法论：里程碑由近及远；每步 1~2 周内可启动并带可验证判据；先给现实性评估（结合用户现状与知识库，关键事实可先用 web_search 核实），再列假设清单（软时间窗/现状/路径/风险），每条假设都要能被用户纠正。intent_text 必须引用用户原话，不做改写；用户给出期望步数（「用 5 步实现」）时按该数量拆。确认后第一步直接进入推进，其余步骤排队等待——不需要和用户谈名额。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "intent_text": { "type": "string", "description": "用户长期意图的原话，必填（一字不改地引用）" },
                "vision": { "type": "string", "description": "一句话愿景（终点状态长什么样），可选" },
                "assessment": { "type": "string", "description": "现实性评估：结合用户现状说明可行性与关键不确定点，可选但建议给出" },
                "assumptions": {
                    "type": "array",
                    "description": "假设清单：这次拆解建立在哪些假设上（可被用户逐条纠正）",
                    "maxItems": 16,
                    "items": {
                        "type": "object",
                        "properties": {
                            "kind": { "type": "string", "enum": ["时间窗", "现状", "路径", "风险", "其他"] },
                            "text": { "type": "string" }
                        },
                        "required": ["kind", "text"],
                        "additionalProperties": false
                    }
                },
                "steps": {
                    "type": "array",
                    "description": "里程碑步骤（由近及远）；第一步确认后直接进入推进，其余排队",
                    "minItems": 1,
                    "maxItems": 12,
                    "items": {
                        "type": "object",
                        "properties": {
                            "content": { "type": "string", "description": "这一步要达成什么（可验证的一句话），必填" },
                            "phase": { "type": "string", "enum": ["near", "mid", "long"], "description": "周期定位，缺省 near" },
                            "why": { "type": "string", "description": "为什么是这一步（可选）" },
                            "current_focus": { "type": "string", "description": "这一步的前 1~2 周行动（可选）" },
                            "blocker": { "type": "string", "description": "预判的困难（可选）" },
                            "done_criteria": { "type": "string", "description": "完成判据 / 验收标准（可选但建议给出）" }
                        },
                        "required": ["content"],
                        "additionalProperties": false
                    }
                }
            },
            "required": ["intent_text", "steps"],
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
        let intent_text = arg_str(args, "intent_text")?;
        let vision = arg_str_opt(args, "vision");
        let assessment = arg_str_opt(args, "assessment");
        let assumptions = args
            .get("assumptions")
            .cloned()
            .unwrap_or_else(|| json!([]));
        if assumptions.as_array().map(Vec::len).unwrap_or(0) > 16 {
            anyhow::bail!("假设清单最多 16 条");
        }
        let steps = steps_array(args)?;
        let first = steps[0]
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let action_args = json!({
            "intent_text": intent_text,
            "vision": vision,
            "assessment": assessment,
            "assumptions": assumptions,
            "steps": steps,
        });
        store_create_pending(ctx.store, ctx.conversation_id, "create_plan", &action_args)?;
        Ok(format!(
            "已为你草拟长程计划（待确认，尚未落库）：\n意图：「{intent_text}」\n共 {} 步，第一步「{first}」。\n—— 请在审阅卡核对假设清单与里程碑；接受后第一步直接进入推进，其余步骤排队等待（活跃名额不足时第一步也先排队，可随时唤醒）。",
            steps.len()
        ))
    }
}

/// 为既有长程计划追加拆解步骤（确认后全部进入等待队列）
pub(super) struct AddPlanStepsTool;
impl Tool for AddPlanStepsTool {
    fn name(&self) -> &'static str {
        "add_plan_steps"
    }
    fn description(&self) -> &'static str {
        "为既有长程计划追加拆解步骤（需用户确认；确认后全部进入等待队列，活跃名额空出时激活）。计划推进到下一里程碑、用户细化路径、或原拆解需要补步时使用。plan_id 来自上下文中的计划清单或 list_plans；不要重复追加已有步骤，不要为同一意图重复建计划。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "plan_id": { "type": "string", "description": "目标计划 ID，必填" },
                "steps": {
                    "type": "array",
                    "description": "追加的步骤（接在现有步骤之后，由近及远）",
                    "minItems": 1,
                    "maxItems": 12,
                    "items": {
                        "type": "object",
                        "properties": {
                            "content": { "type": "string", "description": "这一步要达成什么，必填" },
                            "phase": { "type": "string", "enum": ["near", "mid", "long"] },
                            "why": { "type": "string" },
                            "current_focus": { "type": "string" },
                            "blocker": { "type": "string" },
                            "done_criteria": { "type": "string" }
                        },
                        "required": ["content"],
                        "additionalProperties": false
                    }
                }
            },
            "required": ["plan_id", "steps"],
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
        let plan_id = arg_str(args, "plan_id")?;
        let plan = ctx
            .store
            .get_plan(&plan_id)?
            .ok_or_else(|| anyhow::anyhow!("未找到 ID 为 {plan_id} 的计划"))?;
        let steps = steps_array(args)?;
        let action_args = json!({
            "plan_id": plan_id,
            "plan_intent": plan.intent_text,
            "steps": steps,
        });
        store_create_pending(
            ctx.store,
            ctx.conversation_id,
            "add_plan_steps",
            &action_args,
        )?;
        Ok(format!(
            "已为你草拟为计划「{}」追加 {} 个步骤的提议（待确认）：\n—— 确认后新步骤进入等待队列，活跃名额空出时激活。",
            plan.intent_text,
            steps.len()
        ))
    }
}

/// P3：完成一步并推进到下一步（达成标记 + 激活等待步骤 / 追加新步骤，
/// 单提案一次确认；并发闸满额时保持排队，零拒绝）
pub(super) struct AdvancePlanStepTool;
impl Tool for AdvancePlanStepTool {
    fn name(&self) -> &'static str {
        "advance_plan_step"
    }
    fn description(&self) -> &'static str {
        "计划推进提案（需用户确认）：先复盘——对照该步的完成判据与假设清单，向用户说明「事实 vs 假设」；再推进——完成当前步骤（completed_step_goal_id，可选）、激活队列中既有步骤（activate_goal_id，取自 list_plans 的等待名单）或追加一个新步骤（new_step）。活跃名额已满时新步骤会自动保持排队，不要因此拒绝推进。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "plan_id": { "type": "string", "description": "计划 ID，必填" },
                "completed_step_goal_id": { "type": "string", "description": "已达成的那一步的目标 ID（可选；给出即标记为已达成）" },
                "activate_goal_id": { "type": "string", "description": "要激活的等待步骤（可选；取自 list_plans 的等待名单）" },
                "new_step": {
                    "type": "object",
                    "description": "要追加并激活的新步骤（可选；队列空或路径调整时用）",
                    "properties": {
                        "content": { "type": "string" },
                        "phase": { "type": "string", "enum": ["near", "mid", "long"] },
                        "why": { "type": "string" },
                        "current_focus": { "type": "string" },
                        "blocker": { "type": "string" },
                        "done_criteria": { "type": "string" }
                    },
                    "required": ["content"],
                    "additionalProperties": false
                }
            },
            "required": ["plan_id"],
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
        let plan_id = arg_str(args, "plan_id")?;
        let plan = ctx
            .store
            .get_plan(&plan_id)?
            .ok_or_else(|| anyhow::anyhow!("未找到 ID 为 {plan_id} 的计划"))?;
        let completed = arg_str_opt(args, "completed_step_goal_id");
        let activate = arg_str_opt(args, "activate_goal_id");
        let new_step = args.get("new_step").cloned().filter(|v| !v.is_null());
        anyhow::ensure!(
            completed.is_some() || activate.is_some() || new_step.is_some(),
            "至少给出完成、激活或新增其中一项"
        );
        let action_args = json!({
            "plan_id": plan_id,
            "plan_intent": plan.intent_text,
            "completed_step_goal_id": completed,
            "activate_goal_id": activate,
            "new_step": new_step,
        });
        store_create_pending(
            ctx.store,
            ctx.conversation_id,
            "advance_plan_step",
            &action_args,
        )?;
        Ok(format!(
            "已为你草拟计划「{}」的推进提案（待确认）：{}。\n—— 请在审阅卡核对复盘结论与下一步；确认后执行（活跃名额满时新步骤保持排队）。",
            plan.intent_text,
            match (&completed, &activate, &new_step) {
                (Some(_), Some(_), _) => "达成当前步骤并激活等待步骤",
                (Some(_), None, Some(_)) => "达成当前步骤并追加下一步",
                (None, Some(_), _) => "激活等待步骤",
                (None, None, Some(_)) => "追加并推进新步骤",
                _ => "记录进展",
            }
        ))
    }
}
