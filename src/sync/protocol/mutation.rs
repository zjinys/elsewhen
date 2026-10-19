//! SyncMutation：同步变更高层载荷与整体校验。

use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SyncMutation {
    Capture {
        event: EventRecord,
        input: Option<InputRecord>,
        message: Option<MessageRecord>,
    },
    MessageAppended {
        message: MessageRecord,
    },
    AnalysisCompleted {
        analysis: AnalysisRecord,
    },
    ConversationChanged {
        conversation: ConversationRecord,
        base_revision: Option<String>,
        deleted: bool,
        observed_message_ids: Vec<String>,
    },
    WikiPageChanged {
        page: WikiPageRecord,
        base_revision: Option<String>,
        deleted: bool,
    },
    /// 重命名（slug 迁移 + 标题）。收端必须做与本端 rename_wiki_page 相同
    /// 的关系/会话迁移；两端各自改名（old_slug 对不上本端行）属于显式冲突。
    WikiPageRenamed {
        old_slug: String,
        page: WikiPageRecord,
        base_revision: Option<String>,
    },
    /// 修订历史行追加（起源端权威：收端不为远端 op 另立修订）。id 为起源
    /// 端 uuid，两端一致；行只追加，不参与 head/CAS。
    WikiPageRevisionAppended {
        revision: WikiRevisionRecord,
    },
    /// 可变小对象整行复制（待办/目标/规则）。与维基页同 CAS 框架：base 为
    /// head 锚点，收端冲突显式立案，不做时间覆盖。已删语义与业务删除一致
    /// （全端物理删除）。
    TodoChanged {
        todo: TodoRecord,
        base_revision: Option<String>,
        deleted: bool,
    },
    GoalChanged {
        goal: GoalRecord,
        base_revision: Option<String>,
        deleted: bool,
    },
    RuleChanged {
        rule: RuleRecord,
        base_revision: Option<String>,
        deleted: bool,
    },
    /// 知识维护评审行追加（issue 已阅/处置）。append-only：fingerprint 为
    /// 幂等键，收端 INSERT OR IGNORE，不参与 head/CAS，不存在冲突语义。
    KnowledgeReviewAppended {
        review: KnowledgeReviewRecord,
    },
    /// 阅读状态（五态选择型字段）最终写胜。字段是纯选择不存在语义合并，
    /// 丢弃旧值的代价=重复点一次，不值得 CAS 立案 UI——updated_at 决胜。
    KnowledgeReadingStateChanged {
        state: KnowledgeReadingStateRecord,
    },
}

impl SyncMutation {
    pub(crate) fn validate(&self) -> Result<()> {
        match self {
            Self::Capture {
                event,
                input,
                message,
            } => {
                validate_id(&event.id)?;
                ensure!(!event.raw_text.trim().is_empty(), "sync_empty_event");
                if let Some(input) = input {
                    validate_id(&input.id)?;
                    ensure!(
                        input.event_id == event.id && input.raw_text == event.raw_text,
                        "sync_capture_link_mismatch"
                    );
                    ensure!(
                        input.message_id.as_deref() == message.as_ref().map(|m| m.id.as_str()),
                        "sync_capture_link_mismatch"
                    );
                } else {
                    ensure!(message.is_none(), "sync_capture_link_missing");
                }
                if let Some(message) = message {
                    message.validate()?;
                    ensure!(
                        message.role == "user" && message.content == event.raw_text,
                        "sync_capture_message_mismatch"
                    );
                }
            }
            Self::MessageAppended { message } => message.validate()?,
            Self::AnalysisCompleted { analysis } => {
                validate_id(&analysis.id)?;
                validate_id(&analysis.event_id)?;
                ensure!(
                    !analysis.prompt_version.is_empty() && analysis.prompt_version.len() <= 128,
                    "sync_analysis_strategy_invalid"
                );
                let json: serde_json::Value = serde_json::from_str(&analysis.result_json)
                    .map_err(|_| anyhow::anyhow!("sync_analysis_result_invalid"))?;
                ensure!(json.is_object(), "sync_analysis_result_invalid");
            }
            Self::ConversationChanged {
                conversation,
                deleted,
                ..
            } => {
                validate_id(&conversation.id)?;
                ensure!(
                    conversation
                        .tag
                        .as_deref()
                        .is_none_or(|s| matches!(s, "diary" | "idea" | "discussion" | "general")),
                    "sync_tag_invalid"
                );
                ensure!(
                    !deleted || conversation.archived,
                    "sync_delete_requires_archive"
                );
                ensure!(
                    matches!(
                        conversation.assistant_mode.as_str(),
                        "personal_secretary" | "knowledge_mentor"
                    ),
                    "sync_assistant_mode_invalid"
                );
                if let Some(slug) = &conversation.wiki_page_slug {
                    ensure!(
                        matches!(conversation.assistant_mode.as_str(), "knowledge_mentor"),
                        "sync_conversation_mode_slug_mismatch"
                    );
                    // slug 规则与 wiki 页一致：身份锚不得携带控制字符，防止
                    // 跨端引用漂移。
                    ensure!(
                        !slug.is_empty()
                            && slug.len() <= 255
                            && !slug
                                .bytes()
                                .any(|b| b.is_ascii_control() || matches!(b, b' ' | b'\t')),
                        "sync_wiki_slug_invalid"
                    );
                }
            }
            Self::WikiPageRevisionAppended { revision } => {
                validate_id(&revision.id)?;
                validate_id(&revision.page_id)?;
                ensure!(!revision.content_md.is_empty(), "sync_wiki_revision_empty");
                ensure!(
                    !revision.created_at.trim().is_empty() && revision.created_at.len() <= 64,
                    "sync_wiki_revision_field_invalid"
                );
                ensure!(
                    revision.reason.len() <= 512,
                    "sync_wiki_revision_field_invalid"
                );
                if let Some(event_id) = &revision.source_event_id {
                    validate_id(event_id)?;
                }
            }
            Self::WikiPageRenamed { old_slug, page, .. } => {
                // old_slug 复用 slug 有效性规则；重命名必须真的换 slug，
                // 未换 slug 的标题修改走 WikiPageChanged。
                ensure!(old_slug != &page.slug, "sync_wiki_rename_noop");
                ensure!(
                    !old_slug.is_empty()
                        && old_slug.len() <= 255
                        && !old_slug
                            .bytes()
                            .any(|b| b.is_ascii_control() || matches!(b, b' ' | b'\t')),
                    "sync_wiki_slug_invalid"
                );
                ensure!(
                    !page.slug.is_empty()
                        && page.slug.len() <= 255
                        && !page
                            .slug
                            .bytes()
                            .any(|b| b.is_ascii_control() || matches!(b, b' ' | b'\t')),
                    "sync_wiki_slug_invalid"
                );
                ensure!(
                    !page.title.trim().is_empty() && page.title.len() <= 512,
                    "sync_wiki_title_invalid"
                );
            }
            Self::WikiPageChanged { page, deleted, .. } => {
                validate_id(&page.id)?;
                ensure!(
                    !page.slug.is_empty()
                        && page.slug.len() <= 255
                        && !page
                            .slug
                            .bytes()
                            .any(|b| b.is_ascii_control() || matches!(b, b' ' | b'\t')),
                    "sync_wiki_slug_invalid"
                );
                ensure!(
                    !page.title.trim().is_empty() && page.title.len() <= 512,
                    "sync_wiki_title_invalid"
                );
                ensure!(
                    page.kind.len() <= 64 && page.area.len() <= 64 && page.status.len() <= 64,
                    "sync_wiki_field_invalid"
                );
                for event_id in &page.source_event_ids {
                    validate_id(event_id)?;
                }
                if let Some(based_on) = &page.based_on {
                    ensure!(
                        !based_on.is_empty() && based_on.len() <= 255,
                        "sync_wiki_based_on_invalid"
                    );
                }
                ensure!(
                    !deleted || page.status == "active",
                    "sync_wiki_delete_status_invalid"
                );
            }
            Self::TodoChanged {
                todo,
                base_revision,
                deleted,
            } => {
                validate_id(&todo.id)?;
                ensure!(!todo.title.trim().is_empty(), "sync_todo_field_invalid");
                ensure!(
                    todo.title.len() <= 512 && todo.status.len() <= 32 && todo.priority.len() <= 16,
                    "sync_todo_field_invalid"
                );
                ensure!(
                    matches!(todo.status.as_str(), "open" | "done" | "archived"),
                    "sync_todo_status_invalid"
                );
                ensure!(
                    matches!(todo.priority.as_str(), "high" | "normal" | "low"),
                    "sync_todo_priority_invalid"
                );
                if let Some(event_id) = &todo.related_event_id {
                    validate_id(event_id)?;
                }
                let _ = (base_revision, deleted);
            }
            Self::GoalChanged { goal, deleted, .. } => {
                validate_id(&goal.id)?;
                ensure!(
                    !goal.content.trim().is_empty() && goal.content.len() <= 2048,
                    "sync_goal_field_invalid"
                );
                ensure!(
                    matches!(goal.phase.as_str(), "near" | "mid" | "long"),
                    "sync_goal_phase_invalid"
                );
                ensure!(
                    matches!(goal.status.as_str(), "active" | "superseded"),
                    "sync_goal_status_invalid"
                );
                ensure!(!deleted, "sync_goal_delete_invalid");
            }
            Self::RuleChanged {
                rule, deleted: _, ..
            } => {
                validate_id(&rule.id)?;
                ensure!(
                    !rule.content.trim().is_empty() && rule.content.len() <= 2048,
                    "sync_rule_field_invalid"
                );
                ensure!(
                    matches!(rule.status.as_str(), "active" | "pending"),
                    "sync_rule_status_invalid"
                );
                if let Some(conversation_id) = &rule.conversation_id {
                    validate_id(conversation_id)?;
                }
            }
            Self::KnowledgeReviewAppended { review } => {
                validate_id(&review.page_id)?;
                ensure!(
                    !review.fingerprint.trim().is_empty() && review.fingerprint.len() <= 128,
                    "sync_knowledge_review_field_invalid"
                );
                ensure!(
                    review.resolution.len() <= 64
                        && !review.created_at.trim().is_empty()
                        && review.created_at.len() <= 64,
                    "sync_knowledge_review_field_invalid"
                );
                if let Some(desc) = &review.issue_description {
                    ensure!(desc.len() <= 4096, "sync_knowledge_review_field_invalid");
                }
            }
            Self::KnowledgeReadingStateChanged { state } => {
                validate_id(&state.page_id)?;
                ensure!(
                    matches!(
                        state.state.as_str(),
                        "unread" | "read" | "valuable" | "adopted" | "archived"
                    ),
                    "sync_knowledge_state_invalid"
                );
                ensure!(
                    !state.updated_at.trim().is_empty() && state.updated_at.len() <= 64,
                    "sync_knowledge_state_field_invalid"
                );
            }
        }
        Ok(())
    }
}
