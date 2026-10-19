//! ContextMessage：会话消息与 segment 元数据载体。

use super::*;

#[derive(Debug, Clone)]
pub struct ContextMessage {
    pub role: String,
    pub content: String,
    /// 原生 tool-calling：assistant 消息携带的工具调用（用于回传）
    pub tool_calls: Option<Vec<ToolCall>>,
    /// 原生 tool-calling：tool 角色消息对应哪个工具调用
    pub tool_call_id: Option<String>,
    pub reasoning_content: Option<String>,
    /// Optional cross-conversation background; never serialized to the provider.
    pub optional_background: bool,
    /// Never serialized as model messages; used for selection and diagnostics.
    pub segment: ContextSegment,
    pub(crate) selection_omissions: Vec<SelectionOmission>,
    /// Original timestamp, retained for request-only temporal enrichment.
    pub(crate) message_time: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub(crate) sampled_time: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub(crate) time_reminder: bool,
}

impl ContextMessage {
    pub fn new(role: &str, content: impl Into<String>) -> Self {
        let content = content.into();
        let segment = ContextSegment::for_role(role, &content);
        Self {
            role: role.to_string(),
            content,
            segment,
            selection_omissions: Vec::new(),
            tool_calls: None,
            tool_call_id: None,
            reasoning_content: None,
            optional_background: false,
            message_time: None,
            sampled_time: None,
            time_reminder: false,
        }
    }

    pub(crate) fn with_segment(
        mut self,
        purpose: SegmentPurpose,
        origin: SegmentOrigin,
        identity: Option<&str>,
        omission: OmissionPolicy,
    ) -> Self {
        self.segment = ContextSegment::new(purpose, origin, identity, &self.content, omission);
        self.optional_background = omission == OmissionPolicy::OptionalBackground;
        self
    }

    pub(crate) fn from_record(message: crate::storage::MessageSummary) -> Self {
        let mut context = Self::new(&message.role, message.content).with_segment(
            SegmentPurpose::History,
            SegmentOrigin::StoredMessage,
            Some(&message.id),
            OmissionPolicy::WholeEarlierTurn,
        );
        context.message_time = chrono::DateTime::parse_from_rfc3339(&message.created_at).ok();
        context
    }

    /// 原样回传 assistant 的正文、工具调用和必要的推理字段。
    pub fn assistant_with_tool_calls(
        content: String,
        calls: Vec<ToolCall>,
        reasoning_content: Option<String>,
    ) -> Self {
        let mut message = Self::new("assistant", content).with_segment(
            SegmentPurpose::ToolProtocol,
            SegmentOrigin::ToolExecution,
            None,
            OmissionPolicy::Required,
        );
        message.tool_calls = Some(calls);
        message.reasoning_content = reasoning_content;
        message
    }

    /// tool 角色消息：一次工具调用的执行结果
    pub fn tool_result(call_id: String, content: String) -> Self {
        let mut message = Self::new("tool", content).with_segment(
            SegmentPurpose::ToolResult,
            SegmentOrigin::ToolExecution,
            Some(&call_id),
            OmissionPolicy::Required,
        );
        message.tool_call_id = Some(call_id);
        message
    }
}
