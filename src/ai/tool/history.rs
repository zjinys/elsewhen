//! Read-only access to exact chat originals; summaries are never citations.
use super::{Tool, ToolContext, arg_str};
use anyhow::{Result, ensure};
use serde_json::{Value, json};

fn offset(args: &Value, key: &str, default: usize) -> Result<usize> {
    match args.get(key) {
        None => Ok(default),
        Some(value) => {
            let number = value
                .as_u64()
                .ok_or_else(|| anyhow::anyhow!("{key} 必须为非负整数"))?;
            Ok(usize::try_from(number)?)
        }
    }
}

pub(super) struct ReadHistoryTool;
impl Tool for ReadHistoryTool {
    fn name(&self) -> &'static str {
        "read_conversation_history"
    }
    fn description(&self) -> &'static str {
        "按消息身份分页回读当前会话的精确原文，校验本次运行版本。需要原话或精确数字时使用；历史摘要不能代替原文。"
    }
    fn parameters_schema(&self) -> Value {
        json!({"type":"object","properties":{"message_id":{"type":"string"},"start_char":{"type":"integer","minimum":0},"max_chars":{"type":"integer","minimum":1,"maximum":2400}},"required":["message_id"],"additionalProperties":false})
    }
    fn run(&self, args: &Value, ctx: &ToolContext) -> Result<String> {
        let id = arg_str(args, "message_id")?;
        let count = offset(args, "max_chars", 1200)?;
        ensure!(
            (1..=2400).contains(&count),
            "原文回读长度必须为1至2400个字符"
        );
        Ok(serde_json::to_string(
            &ctx.store.read_conversation_history_excerpt(
                ctx.conversation_id,
                &id,
                offset(args, "start_char", 0)?,
                count,
            )?,
        )?)
    }
}

pub(super) struct SummarySpansTool;
impl Tool for SummarySpansTool {
    fn name(&self) -> &'static str {
        "list_summary_history"
    }
    fn description(&self) -> &'static str {
        "分页列出当前摘要覆盖的原始消息身份和字符范围，再用 read_conversation_history 回读所需原文。"
    }
    fn parameters_schema(&self) -> Value {
        json!({"type":"object","properties":{"summary_id":{"type":"string"},"offset":{"type":"integer","minimum":0}},"required":["summary_id"],"additionalProperties":false})
    }
    fn run(&self, args: &Value, ctx: &ToolContext) -> Result<String> {
        let id = arg_str(args, "summary_id")?;
        let start = offset(args, "offset", 0)?;
        let spans = ctx
            .store
            .summary_history_spans(ctx.conversation_id, &id, start, 32)?;
        Ok(json!({"spans":spans,"next_offset":start + spans.len(),"may_have_more":spans.len()==32}).to_string())
    }
}
