//! Request-only context provenance. Identifiers and versions are hashes so
//! budget diagnostics never contain message text, page titles or source URLs.
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SegmentPurpose {
    StableSetup,
    ConversationState,
    Goals,
    PersonalRules,
    History,
    CurrentInput,
    Summary,
    CrossConversation,
    CurrentPage,
    SelectedEvidence,
    ToolResult,
    ToolProtocol,
    TimeContext,
    ReviewState,
    TaskContinuity,
    TaskSupplement,
    RequiredContext,
    BudgetNotice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SegmentOrigin {
    Application,
    Conversation,
    StoredMessage,
    Goals,
    Rules,
    KnowledgePage,
    KnowledgeSelection,
    ToolExecution,
    Clock,
    Review,
    Derived,
    ProvidedMaterial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OmissionPolicy {
    Required,
    OptionalBackground,
    WholeEarlierTurn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContextSegment {
    pub purpose: SegmentPurpose,
    pub origin: SegmentOrigin,
    pub identity_hash: Option<String>,
    pub version: String,
    pub omission: OmissionPolicy,
}

impl Default for ContextSegment {
    fn default() -> Self {
        Self::for_role("system", "")
    }
}

impl ContextSegment {
    pub(crate) fn new(
        purpose: SegmentPurpose,
        origin: SegmentOrigin,
        identity: Option<&str>,
        text: &str,
        omission: OmissionPolicy,
    ) -> Self {
        Self {
            purpose,
            origin,
            omission,
            identity_hash: identity.map(crate::storage::knowledge::content_hash),
            version: crate::storage::knowledge::content_hash(text),
        }
    }

    pub(crate) fn for_role(role: &str, text: &str) -> Self {
        let (purpose, origin, omission) = match role {
            "user" => (
                SegmentPurpose::CurrentInput,
                SegmentOrigin::ProvidedMaterial,
                OmissionPolicy::Required,
            ),
            "assistant" => (
                SegmentPurpose::History,
                SegmentOrigin::Derived,
                OmissionPolicy::WholeEarlierTurn,
            ),
            "tool" => (
                SegmentPurpose::ToolResult,
                SegmentOrigin::ToolExecution,
                OmissionPolicy::Required,
            ),
            _ => (
                SegmentPurpose::RequiredContext,
                SegmentOrigin::Application,
                OmissionPolicy::Required,
            ),
        };
        Self::new(purpose, origin, None, text, omission)
    }
}

/// Preliminary selection may remove many original messages. Aggregate by kind
/// rather than carrying unbounded per-message diagnostics into every request.
#[derive(Debug, Clone, Serialize)]
pub struct SelectionOmission {
    pub purpose: SegmentPurpose,
    pub origin: SegmentOrigin,
    pub reason: &'static str,
    pub model: String,
    pub messages: usize,
    pub estimated_tokens: usize,
}

pub(crate) fn note_omission(
    omissions: &mut Vec<SelectionOmission>,
    segment: &ContextSegment,
    model: &str,
    tokens: usize,
    reason: &'static str,
) {
    if let Some(existing) = omissions.iter_mut().find(|item| {
        item.purpose == segment.purpose
            && item.origin == segment.origin
            && item.reason == reason
            && item.model == model
    }) {
        existing.messages += 1;
        existing.estimated_tokens += tokens;
    } else {
        omissions.push(SelectionOmission {
            purpose: segment.purpose,
            origin: segment.origin,
            reason,
            model: model.into(),
            messages: 1,
            estimated_tokens: tokens,
        });
    }
}
