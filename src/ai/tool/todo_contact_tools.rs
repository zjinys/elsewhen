//! todo_contact_tools.rs：原 tool/mod.rs 同名分段外移，行为零变化。

use super::Tool;
use super::dispatch::{arg_str, arg_str_opt};
use super::write_tools::store_create_pending;
use super::{ToolContext, ToolPolicy};
use anyhow::Result;
use serde_json::{Value, json};

// ── 个人待办工具 ────────────────────────────────────────────────

/// 列出全部待办（可过滤状态）
pub(super) struct ListTodosTool;
impl Tool for ListTodosTool {
    fn name(&self) -> &'static str {
        "list_todos"
    }
    fn description(&self) -> &'static str {
        "列出个人待办清单。用户问起待办/任务/要做的事情时使用。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type":"object",
            "properties":{
                "status":{"type":"string","description":"可选：open/done，缺省列出所有未归档"}
            },
            "additionalProperties":false
        })
    }
    fn run(&self, args: &Value, ctx: &ToolContext) -> Result<String> {
        let status = args.get("status").and_then(|v| v.as_str());
        let todos = ctx.store.list_todos(status)?;
        if todos.is_empty() {
            return Ok("当前待办为空".to_string());
        }
        let mut out = String::from("待办清单：\n");
        for t in todos {
            let mark = if t.status.as_str() == "done" {
                "✓"
            } else {
                "☐"
            };
            let due = t
                .due_at
                .as_ref()
                .map(|d| format!(" (截止 {d})"))
                .unwrap_or_default();
            out.push_str(&format!("- {mark} {}{due}\n", t.title));
        }
        Ok(out)
    }
}

/// 提议一条待办（确认后创建）
pub(super) struct CreateTodoTool;
impl Tool for CreateTodoTool {
    fn name(&self) -> &'static str {
        "create_todo"
    }
    fn description(&self) -> &'static str {
        "为分析出需要后续跟进的事情创建一条待办。调用后进入待确认状态，需要用户确认才会真正建条。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type":"object",
            "properties":{
                "title":{"type":"string","description":"待办内容，必填"},
                "due_at":{"type":"string","description":"截止时间（ISO 日期），可选"},
                "priority":{"type":"string","description":"优先级：high/normal/low，默认 normal"},
                "related_wiki_slug":{"type":"string","description":"关联知识页 slug，可选"},
                "note":{"type":"string","description":"补充说明，可选"}
            },
            "required":["title"],
            "additionalProperties":false
        })
    }
    fn deferred(&self) -> bool {
        true
    }
    fn policy(&self) -> ToolPolicy {
        ToolPolicy::WriteConfirm
    }
    fn run(&self, args: &Value, ctx: &ToolContext) -> Result<String> {
        let title = arg_str(args, "title")?;
        let action_args = json!({
            "title": title.clone(),
            "due_at": arg_str_opt(args, "due_at"),
            "priority": arg_str_opt(args, "priority").unwrap_or_else(|| "normal".to_string()),
            "related_wiki_slug": arg_str_opt(args, "related_wiki_slug"),
            "note": arg_str_opt(args, "note"),
        });
        store_create_pending(ctx.store, ctx.conversation_id, "create_todo", &action_args)?;
        Ok(format!(
            "已为你草拟一条待办（待确认，尚未创建）：\n「{title}」\n—— 在审阅卡点击接受即可建条。"
        ))
    }
}

/// 提议一条提醒（确认后创建；评审 3.4：必须带 due_at，确认卡回显已解析时间）
pub(super) struct CreateReminderTool;
impl Tool for CreateReminderTool {
    fn name(&self) -> &'static str {
        "create_reminder"
    }
    fn description(&self) -> &'static str {
        "为需要在未来某个时刻提醒用户的事创建一条提醒。必须给出明确的到期时间（ISO 时间）。调用后进入待确认状态，确认卡片回显解析出的时间，用户确认才建条；确认时若时间已过会原样拒绝，绝不静默改期。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type":"object",
            "properties":{
                "title":{"type":"string","description":"提醒内容，必填"},
                "due_at":{"type":"string","description":"到期时间（RFC3339 / ISO 时间），必填"},
                "note":{"type":"string","description":"补充说明，可选"}
            },
            "required":["title","due_at"],
            "additionalProperties":false
        })
    }
    fn deferred(&self) -> bool {
        true
    }
    fn policy(&self) -> ToolPolicy {
        ToolPolicy::WriteConfirm
    }
    fn run(&self, args: &Value, ctx: &ToolContext) -> Result<String> {
        let title = arg_str(args, "title")?;
        let due_at = arg_str(args, "due_at")?;
        // 入库前先把时间验成 RFC3339，避免「确认时才炸」浪费一轮确认。
        let parsed = chrono::DateTime::parse_from_rfc3339(&due_at)
            .map_err(|_| anyhow::anyhow!("due_at 不是有效 RFC3339 时间：{due_at}"))?;
        if parsed <= chrono::Utc::now() {
            anyhow::bail!("due_at 已过（{due_at}），请给一个未来的时间");
        }
        let action_args = json!({
            "title": title.clone(),
            "due_at": due_at.clone(),
            "priority": "normal",
            "related_wiki_slug": null,
            "note": arg_str_opt(args, "note"),
            "reminder": true,
        });
        store_create_pending(ctx.store, ctx.conversation_id, "create_todo", &action_args)?;
        Ok(format!(
            "已为你草拟一条提醒（待确认，尚未创建）：\n「{title}」，提醒时间 {due_at}\n—— 确认卡片上核对时间，点接受才会建条。"
        ))
    }
}

// ── 联系人关系工具 ───────────────────────────────────────────────

/// 把对话里识别出的「联系人 + 人↔事情/项目」关系草拟下来（确认后才建档存关系）
pub(super) struct ProposePeopleRelationsTool;
impl Tool for ProposePeopleRelationsTool {
    fn name(&self) -> &'static str {
        "propose_people_relations"
    }
    fn description(&self) -> &'static str {
        "把对话中出现的对用户重要的联系人，以及「联系人 ↔ 事情/项目」的关系草拟下来。调用后进入待确认状态，用户确认后才建档保存。事件/对话中用户用 @人名 标注的一定是人、#事情/项目 标注的一定是事情，优先纳入草拟。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type":"object",
            "properties":{
                "people":{
                    "type":"array",
                    "description":"本次要建档的联系人（可不填，只补关系时省略）",
                    "items":{
                        "type":"object",
                        "properties":{
                            "name":{"type":"string","description":"联系人姓名，必填"},
                            "role_note":{"type":"string","description":"身份/角色/背景一句话，可选"}
                        },
                        "required":["name"],
                        "additionalProperties":false
                    }
                },
                "relations":{
                    "type":"array",
                    "description":"联系人与事情/项目的关系（可不填，只建档联系人时省略）",
                    "items":{
                        "type":"object",
                        "properties":{
                            "person":{"type":"string","description":"联系人姓名（与 people 中的 name 对应，或知识库已有的联系人页）"},
                            "target":{"type":"string","description":"事情/项目名称"},
                            "relation":{"type":"string","description":"关系类型：负责/参与/合作/对接/跟进/顾问 等，可选，默认参与"},
                            "note":{"type":"string","description":"补充说明，可选"}
                        },
                        "required":["person","target"],
                        "additionalProperties":false
                    }
                }
            },
            "additionalProperties":false
        })
    }
    fn deferred(&self) -> bool {
        true
    }
    fn policy(&self) -> ToolPolicy {
        ToolPolicy::WriteConfirm
    }
    fn run(&self, args: &Value, ctx: &ToolContext) -> Result<String> {
        // 把 people/relations 原样转成待确认动作参数
        let people: Vec<Value> = args
            .get("people")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let relations: Vec<Value> = args
            .get("relations")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        if people.is_empty() && relations.is_empty() {
            anyhow::bail!("请至少提供一位联系人或一条关系");
        }
        let source_event_id = ctx.source_event_id.clone();
        let action_args =
            json!({ "people": people, "relations": relations, "source_event_id": source_event_id });
        store_create_pending(
            ctx.store,
            ctx.conversation_id,
            "propose_people_relations",
            &action_args,
        )?;
        let mut lines = Vec::new();
        for p in &people {
            let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let note = p.get("role_note").and_then(|v| v.as_str()).unwrap_or("");
            lines.push(format!(
                "联系人：{name}{}",
                if note.is_empty() {
                    String::new()
                } else {
                    format!("（{note}）")
                }
            ));
        }
        for r in &relations {
            let person = r.get("person").and_then(|v| v.as_str()).unwrap_or("");
            let target = r.get("target").and_then(|v| v.as_str()).unwrap_or("");
            let rel = r
                .get("relation")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or("参与");
            lines.push(format!("{person} —— {rel} —— {target}"));
        }
        Ok(format!(
            "已为你草拟联系人与关系（待确认，尚未保存）：\n{}\n—— 在审阅卡点击接受即建档保存。",
            lines.join("\n")
        ))
    }
}

// ── 批量提取：事件 → 联系人/关系（草拟确认） ─────────────────────

/// 批量提取：扫描全部事件 → `@人名` / `#事情` 标注 + AI 补全 → 联系人/关系草拟（待确认）
pub(super) struct BatchExtractPeopleRelationsTool;
impl Tool for BatchExtractPeopleRelationsTool {
    fn name(&self) -> &'static str {
        "batch_extract_people_relations"
    }
    fn description(&self) -> &'static str {
        "扫描知识库里全部已保存事件，批量提取联系人与「联系人 ↔ 事情/项目」关系。事件里 @人名 标注的一定是人、#事情/项目 标注的一定是事情（权威实体，必须纳入）；AI 再根据事件上下文补全角色与关系。只产草拟，用户确认后才保存。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    }
    fn deferred(&self) -> bool {
        true
    }
    fn policy(&self) -> ToolPolicy {
        ToolPolicy::WriteConfirm
    }
    fn run(&self, _args: &Value, ctx: &ToolContext) -> Result<String> {
        let action_args = crate::wiki::propose_people_relations_from_events(ctx.store)?;
        let people: Vec<Value> = action_args
            .get("people")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let relations: Vec<Value> = action_args
            .get("relations")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        if people.is_empty() && relations.is_empty() {
            anyhow::bail!("扫描事件后没有提取到联系人或关系");
        }
        store_create_pending(
            ctx.store,
            ctx.conversation_id,
            "propose_people_relations",
            &action_args,
        )?;
        let mut lines = Vec::new();
        for p in &people {
            let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let note = p.get("role_note").and_then(|v| v.as_str()).unwrap_or("");
            lines.push(format!(
                "联系人：{name}{}",
                if note.is_empty() {
                    String::new()
                } else {
                    format!("（{note}）")
                }
            ));
        }
        for r in &relations {
            let person = r.get("person").and_then(|v| v.as_str()).unwrap_or("");
            let target = r.get("target").and_then(|v| v.as_str()).unwrap_or("");
            let rel = r
                .get("relation")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or("参与");
            lines.push(format!("{person} —— {rel} —— {target}"));
        }
        Ok(format!(
            "已从事件批量草拟联系人与关系（待确认，尚未保存）：\n{}\n—— 在审阅卡点击接受即建档保存。",
            lines.join("\n")
        ))
    }
}
