use super::continuity::{FINAL_ANSWER_NUDGE, FollowUp, progress_fallback, unhelpful_failure_reply};
use super::tool::dispatch;
use crate::ai::provider::{OpenAiCompatibleProvider, TokenUsage};
use crate::storage::RuleStatus;
use anyhow::Result;
use serde_json::{Value, json};
// 公共测试辅助（原 mod tests 内嵌段平移，pub(super) 供分组子文件复用）。
use crate::ai::memory::ContextMessage;
use crate::ai::provider::{AiProvider, AiReply};
use crate::ai::tool::{ToolCall, ToolSpec};
use crate::storage::Store;
use std::collections::VecDeque;
use std::time::{SystemTime, UNIX_EPOCH};

impl ScriptedProvider {
    fn new(replies: Vec<Result<AiReply>>) -> Self {
        Self {
            replies: std::cell::RefCell::new(replies.into()),
            seen_tools: std::cell::RefCell::new(false),
        }
    }
    /// 是否曾收到 tools 清单（原生尝试）
    fn saw_tools_on_first_call(&self) -> bool {
        self.seen_tools.borrow().clone()
    }
}
impl AiProvider for ScriptedProvider {
    fn generate_reply_with_tools(
        &self,
        _messages: Vec<ContextMessage>,
        tools: Option<&[ToolSpec]>,
    ) -> Result<AiReply> {
        if tools.is_some() {
            *self.seen_tools.borrow_mut() = true;
        }
        self.replies
            .borrow_mut()
            .pop_front()
            .unwrap_or_else(|| Ok(AiReply::text("（脚本用尽）")))
    }
}

pub(super) fn temporary_database() -> (Store, std::path::PathBuf) {
    let path = std::env::temp_dir().join(format!(
        "elsewhen-conversation-loop-test-{}.db",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = Store::open(&path).unwrap();
    (store, path)
}

/// 脚本化 provider：按顺序吐出预设回复
struct ScriptedProvider {
    replies: std::cell::RefCell<VecDeque<Result<AiReply>>>,
    seen_tools: std::cell::RefCell<bool>,
}

pub(super) fn reply_with(fr: Option<&str>, content: &str, tool_calls: Vec<ToolCall>) -> AiReply {
    AiReply {
        content: content.to_string(),
        tool_calls,
        model: Some("gpt-4o".to_string()),
        usage: None,
        reasoning_content: None,
        finish_reason: fr.map(str::to_string),
    }
}
mod conversation_part1;
mod conversation_part2;

// 原内嵌 mod tests 的 include! 包装子模块平移为显式挂载
// （baseline_tests 依赖 self::protocol_tests / self::continuity_tests，位于当前 mod.rs 所在目录）。
#[path = "../conversation/continuity_tests/mod.rs"]
mod continuity_tests;

#[path = "../conversation/protocol_tests/mod.rs"]
mod protocol_tests;

#[path = "../conversation/baseline_tests.rs"]
mod baseline_tests;
