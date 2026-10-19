// 公共测试辅助（原文件平移，pub(super) 供分组子文件复用）。
// Included in conversation::tests to reuse the isolated Store and provider fixtures.

use super::*;
use crate::ai::provider::AiReply;
use crate::ai::tool::ToolSpec;
use crate::storage::Store;
use serde_json::json;
use std::collections::VecDeque;

pub(super) struct RecordingProvider {
    pub(super) replies: std::cell::RefCell<VecDeque<AiReply>>,
    pub(super) requests: std::cell::RefCell<Vec<(Vec<ContextMessage>, bool)>>,
}

impl RecordingProvider {
    pub(super) fn new(replies: Vec<AiReply>) -> Self {
        Self {
            replies: std::cell::RefCell::new(replies.into()),
            requests: Default::default(),
        }
    }
}

impl AiProvider for RecordingProvider {
    fn generate_reply_with_tools(
        &self,
        messages: Vec<ContextMessage>,
        tools: Option<&[ToolSpec]>,
    ) -> Result<AiReply> {
        self.requests.borrow_mut().push((messages, tools.is_some()));
        Ok(self
            .replies
            .borrow_mut()
            .pop_front()
            .expect("unexpected extra model request"))
    }
}

pub(super) fn native_reply(content: &str, name: &str, args: Value) -> AiReply {
    AiReply {
        content: content.into(),
        tool_calls: vec![ToolCall::new(name, args)],
        model: None,
        usage: None,
        reasoning_content: None,
        finish_reason: None,
    }
}

mod progress_recovery;

mod save_task_lifecycle;
