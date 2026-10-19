//! R2：`query_event_stats` 对话工具——把「这个月散步几次」这类次数/汇总
//! 问题交给本地 SQL 聚合（统计本地算，AI 只解读），聚合实现在
//! `crate::api::event_stats`（R3 周期报告管道复用同一层）。

use super::dispatch::{arg_str, arg_str_opt};
use super::{Tool, ToolContext, ToolPolicy};
use crate::api::event_stats::{self, EventStatsQuery};
use anyhow::Result;
use serde_json::{Value, json};

pub struct QueryEventStatsTool;

impl Tool for QueryEventStatsTool {
    fn name(&self) -> &'static str {
        "query_event_stats"
    }
    fn description(&self) -> &'static str {
        // 预算按字节保守估 token（unknown 模型 1B=1t），工具清单的每个
        // 字符都是全体对话的固定开销：语义集中一句话，schema 不带描述。
        "已记录事件本地统计（只读 SQL 聚合，勿数样本），次数问答用。date_range：today/yesterday/this_week/last_week/this_month/last_month；activity/person/project/text_contains 包含匹配，event_type 精确；group_by：day/event_type/person/project/activity，缺省只给总数与样本；只计可记录事件，不含闲聊元对话。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type":"object",
            "properties":{
                "date_range":{"type":"string"},
                "activity":{"type":"string"},
                "person":{"type":"string"},
                "project":{"type":"string"},
                "event_type":{"type":"string"},
                "text_contains":{"type":"string"},
                "group_by":{"type":"string"}
            },
            "required":["date_range"],
            "additionalProperties":false
        })
    }
    fn policy(&self) -> ToolPolicy {
        ToolPolicy::Read
    }
    fn run(&self, args: &Value, ctx: &ToolContext) -> Result<String> {
        let range = arg_str(args, "date_range")?;
        let group_by = arg_str_opt(args, "group_by")
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "none".to_string());
        let today = chrono::Local::now().date_naive();
        let (start, end) = event_stats::resolve_date_range(&range, today)?;
        let query = EventStatsQuery {
            event_type: arg_str_opt(args, "event_type"),
            person: arg_str_opt(args, "person"),
            project: arg_str_opt(args, "project"),
            activity: arg_str_opt(args, "activity"),
            text_contains: arg_str_opt(args, "text_contains"),
        };
        let outcome = event_stats::query_event_stats(ctx.store, start, end, &query, &group_by)?;
        Ok(render_stats(&range, start, end, &group_by, &outcome))
    }
}

/// 工具输出：先给全貌（总数/命中），再给分组与样本；空结果是合法答案，
/// 明说「这期间有 N 条记录但没有命中」，让模型不要编。
fn render_stats(
    range: &str,
    start: chrono::NaiveDate,
    end: chrono::NaiveDate,
    group_by: &str,
    outcome: &event_stats::EventStatsOutcome,
) -> String {
    let mut lines = vec![format!(
        "{range}（{start} .. {end}）共有 {} 条可记录事件；你的过滤条件命中 {} 条。",
        outcome.total, outcome.matched
    )];
    if outcome.matched == 0 {
        lines.push(
            "没有命中过滤条件的事件（这是合法的空结果，如实告知用户即可，不要估算或编造）。"
                .to_string(),
        );
        return lines.join("\n");
    }
    if group_by != "none" && !outcome.grouped.is_empty() {
        lines.push(format!(
            "按 {group_by} 分组（次数降序，最多 {} 组）：",
            event_stats::GROUP_TOP_N
        ));
        for (label, count) in &outcome.grouped {
            lines.push(format!("- {label}：{count}"));
        }
    }
    if !outcome.samples.is_empty() {
        lines.push(format!(
            "最近样本（每条原文最多截取 {} 字）：",
            event_stats::SAMPLE_TEXT_CHARS
        ));
        for sample in &outcome.samples {
            lines.push(format!("- [{}] {}", sample.when, sample.text));
        }
    }
    lines.join("\n")
}
