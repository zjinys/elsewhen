//! Versioned domain operations. Wire data never names a SQLite table or SQL statement.

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_OPERATION_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_BATCH_OPERATIONS: usize = 100;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvelopeReceipt {
    pub dataset_id: String,
    pub acknowledging_device_id: String,
    pub target_device_id: String,
    pub message_id: String,
    pub payload_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplicationReceipt {
    pub dataset_id: String,
    pub acknowledging_device_id: String,
    pub target_device_id: String,
    pub op_id: String,
    pub payload_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SyncOperation {
    pub protocol_version: u32,
    pub dataset_id: String,
    pub origin_device_id: String,
    pub origin_stream_id: String,
    pub origin_sequence: i64,
    pub logical_clock: i64,
    pub op_id: String,
    pub mutation: SyncMutation,
}

impl SyncOperation {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() <= MAX_OPERATION_BYTES,
            "sync_operation_too_large"
        );
        let operation: Self = serde_json::from_slice(bytes)?;
        operation.validate()?;
        Ok(operation)
    }

    /// Tolerant identity-only header parse for quarantine. Reads only the
    /// fields needed to key an isolated row (no version/structural validation),
    /// so an unknown-version op can be recorded without aborting the receive.
    pub(crate) fn identity_header(bytes: &[u8]) -> Result<(String, String, String, i64, String)> {
        ensure!(
            bytes.len() <= MAX_OPERATION_BYTES,
            "sync_operation_too_large"
        );
        let value: serde_json::Value = serde_json::from_slice(bytes)?;
        let get_str = |k: &str| {
            value
                .get(k)
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        };
        let dataset = get_str("dataset_id").context("sync_operation_header_invalid")?;
        let device = get_str("origin_device_id").context("sync_operation_header_invalid")?;
        let stream = get_str("origin_stream_id").context("sync_operation_header_invalid")?;
        let seq = value
            .get("origin_sequence")
            .and_then(serde_json::Value::as_i64)
            .context("sync_operation_header_invalid")?;
        let op_id = get_str("op_id").context("sync_operation_header_invalid")?;
        Ok((dataset, device, stream, seq, op_id))
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.protocol_version == PROTOCOL_VERSION,
            "sync_protocol_unsupported"
        );
        for id in [
            &self.dataset_id,
            &self.origin_device_id,
            &self.origin_stream_id,
        ] {
            validate_id(id)?;
        }
        ensure!(self.origin_sequence > 0, "sync_sequence_invalid");
        ensure!(
            (1..=9_007_199_254_740_991).contains(&self.logical_clock),
            "sync_logical_clock_invalid"
        );
        ensure!(
            self.op_id == format!("{}:{}", self.origin_stream_id, self.origin_sequence),
            "sync_operation_identity_invalid"
        );
        self.mutation.validate()
    }
}

pub(crate) fn validate_id(id: &str) -> Result<()> {
    ensure!(
        !id.is_empty()
            && id.len() <= 128
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_')),
        "sync_identity_invalid"
    );
    Ok(())
}

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
    fn validate(&self) -> Result<()> {
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WikiRevisionRecord {
    pub id: String,
    pub page_id: String,
    pub content_md: String,
    pub reason: String,
    pub source_event_id: Option<String>,
    pub created_at: String,
}

/// 待办整行。`related_event_id` / `related_wiki_slug` 是松引用（无 FK），
/// 收端不等待证据抵达——引用随流自然补齐，缺失只在展示层空置。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TodoRecord {
    pub id: String,
    pub title: String,
    pub status: String,
    pub priority: String,
    pub due_at: Option<String>,
    /// 提醒标记（migration 50 新增）。旧版本对端发来的 op 没有本字段，默认 false。
    #[serde(default)]
    pub reminder: bool,
    pub related_event_id: Option<String>,
    pub related_wiki_slug: Option<String>,
    pub note: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 目标整行。活跃上限 3 由收端触发器强制；远端写入越限按显式冲突立案
/// （`goal_active_limit`），不静默顶掉本端目标。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoalRecord {
    pub id: String,
    pub content: String,
    pub phase: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub superseded_at: Option<String>,
}

/// 规则整行。status 含 pending（草拟归属某会话）；规则行复制不等于执行
/// 授权——同步内容不触发任何模型调用。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleRecord {
    pub id: String,
    pub content: String,
    pub status: String,
    pub conversation_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalysisRecord {
    pub id: String,
    pub event_id: String,
    pub task_id: String,
    pub input_hash: String,
    pub prompt_version: String,
    pub result_json: String,
    pub created_at: String,
}

pub(crate) fn analysis_task_id(
    dataset: &str,
    event: &str,
    input_hash: &str,
    strategy: &str,
) -> String {
    crate::storage::sync::hash(
        &serde_json::to_vec(&(dataset, event, input_hash, strategy))
            .expect("string tuple serialization"),
    )
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventRecord {
    pub id: String,
    pub occurred_at: String,
    pub recorded_at: String,
    pub raw_text: String,
    pub source: String,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputRecord {
    pub id: String,
    pub raw_text: String,
    pub source: String,
    pub event_id: String,
    pub message_id: Option<String>,
    /// Idempotency key the user device assigned to this input. Optional so
    /// captures without a key stay representable; preserved across devices so
    /// the same logical input is deduplicated instead of duplicated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MessageRecord {
    pub id: String,
    pub conversation_id: String,
    pub parent_message_id: Option<String>,
    pub role: String,
    pub content: String,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_message_id: Option<String>,
}

impl MessageRecord {
    fn validate(&self) -> Result<()> {
        validate_id(&self.id)?;
        validate_id(&self.conversation_id)?;
        if let Some(id) = &self.parent_message_id {
            validate_id(id)?;
        }
        ensure!(
            matches!(self.role.as_str(), "user" | "assistant"),
            "sync_message_role_invalid"
        );
        if let Some(input) = &self.input_message_id {
            validate_id(input)?;
            ensure!(
                self.role == "assistant" && input != &self.id,
                "sync_response_input_invalid"
            );
        }
        ensure!(
            self.parent_message_id.as_deref() != Some(self.id.as_str()),
            "sync_parent_cycle"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationRecord {
    pub id: String,
    pub title: Option<String>,
    pub tag: Option<String>,
    pub archived: bool,
    /// Wiki 专属会话绑定：页面 slug 是引用不推断的锚（S05）。普通会话为
    /// None；wiki 聊天（assistant_mode='knowledge_mentor'）必填。
    pub wiki_page_slug: Option<String>,
    /// 'personal_secretary' 普通会话 / 'knowledge_mentor' Wiki 专属。
    pub assistant_mode: String,
    pub created_at: String,
    pub updated_at: String,
}

/// P5a：wiki 页面的可复制记录。`id` 是行身份，`slug` 是对外身份锚；
/// 两者在任何一端的漂移都必须显式冲突处理（`slug_owned_by_other_id` /
/// `slug_changed`），不得静默重挂。证据（`source_event_ids`）在接收端
/// 只做并集与存在性等待，不推断、不采信同名新页。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WikiPageRecord {
    pub id: String,
    pub slug: String,
    pub kind: String,
    pub title: String,
    pub summary: String,
    pub content_md: String,
    pub tags: Vec<String>,
    pub source_event_ids: Vec<String>,
    pub status: String,
    pub area: String,
    pub based_on: Option<String>,
    pub content_type: Option<String>,
    pub source_url: Option<String>,
    pub human_edited_at: Option<String>,
    pub opinion: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Provider secrets are deliberately not a generic SyncMutation. They must be
/// sealed before durable synchronization storage and never use settings' empty-key sentinel.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderBundle {
    pub revision_id: String,
    pub base_revision: Option<String>,
    pub profiles: Vec<ProviderProfile>,
    pub active_id: Option<String>,
    pub failover_order: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderProfile {
    pub id: String,
    pub name: String,
    pub provider_type: String,
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub api_key_present: bool,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
    pub temperature: f64,
    pub max_tokens: Option<i64>,
    pub context_window: Option<i64>,
}

impl ProviderBundle {
    pub fn validate(&self) -> Result<()> {
        use std::collections::HashSet;
        validate_id(&self.revision_id)?;
        ensure!(self.profiles.len() <= 64, "sync_provider_count_exceeded");
        let mut ids = HashSet::new();
        let mut names = HashSet::new();
        for profile in &self.profiles {
            ensure!(
                profile.api_key_present || profile.api_key.is_empty(),
                "sync_provider_key_presence_invalid"
            );
            validate_id(&profile.id)?;
            ensure!(
                ids.insert(profile.id.as_str()),
                "sync_provider_duplicate_id"
            );
            ensure!(
                !profile.name.trim().is_empty() && names.insert(profile.name.as_str()),
                "sync_provider_duplicate_name"
            );
            ensure!(
                !profile.model.trim().is_empty(),
                "sync_provider_model_empty"
            );
            let url = reqwest::Url::parse(&profile.base_url)
                .map_err(|_| anyhow::anyhow!("sync_provider_url_invalid"))?;
            ensure!(
                matches!(url.scheme(), "http" | "https") && url.host_str().is_some(),
                "sync_provider_url_invalid"
            );
            ensure!(
                profile.temperature.is_finite(),
                "sync_provider_temperature_invalid"
            );
            if let Some(output) = profile.max_tokens {
                ensure!(
                    (1..=1_000_000).contains(&output),
                    "sync_provider_output_invalid"
                );
            }
            if let Some(window) = profile.context_window {
                ensure!(
                    (4096..=2_000_000).contains(&window),
                    "sync_provider_window_invalid"
                );
                ensure!(
                    profile.max_tokens.unwrap_or(4096) + (window / 20).clamp(256, 4096) < window,
                    "sync_provider_reserve_invalid"
                );
            }
        }
        ensure!(
            self.active_id.as_deref().is_none_or(|id| ids.contains(id)),
            "sync_provider_active_missing"
        );
        let order: HashSet<_> = self.failover_order.iter().map(String::as_str).collect();
        ensure!(
            order == ids && order.len() == self.failover_order.len(),
            "sync_provider_order_invalid"
        );
        ensure!(
            self.active_id
                .as_ref()
                .is_none_or(|id| self.failover_order.first() == Some(id)),
            "sync_provider_active_order_invalid"
        );
        Ok(())
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len() <= 1024 * 1024, "sync_provider_bundle_too_large");
        // Serde errors can contain input values. Never return a secret-bearing
        // parser diagnostic to the settings UI or a synchronization log.
        let bundle: Self = serde_json::from_slice(bytes)
            .map_err(|_| anyhow::anyhow!("sync_provider_bundle_invalid"))?;
        bundle.validate()?;
        Ok(bundle)
    }
}

impl std::fmt::Debug for ProviderBundle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderBundle")
            .field("profiles", &self.profiles.len())
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for ProviderProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderProfile").finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeReviewRecord {
    pub fingerprint: String,
    pub page_id: String,
    pub resolution: String,
    pub created_at: String,
    pub issue_description: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeReadingStateRecord {
    pub page_id: String,
    pub state: String,
    pub updated_at: String,
}
