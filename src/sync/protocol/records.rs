//! 同步载荷记录：各域实体快照与 Provider 捆绑。

use super::*;

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

/// 习惯主动问候计划整行（migration 57）。时间存本地墙钟，触发去重是
/// 设备本地行为，由 `habit_nudge_fires` 承担，不进入本记录。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HabitNudgeRecord {
    pub id: String,
    pub page_slug: String,
    pub question: String,
    pub done_label: String,
    pub done_text: String,
    pub missed_label: String,
    pub missed_text: String,
    pub weekdays: i64,
    pub local_time: String,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl HabitNudgeRecord {
    pub(crate) fn validate(&self) -> Result<()> {
        validate_id(&self.id)?;
        ensure!(
            !self.page_slug.is_empty()
                && self.page_slug.chars().count() <= 255
                && !self.page_slug.chars().any(char::is_control),
            "sync_habit_nudge_page_invalid"
        );
        for (value, max) in [
            (&self.question, 500usize),
            (&self.done_label, 40),
            (&self.done_text, 500),
            (&self.missed_label, 40),
            (&self.missed_text, 500),
        ] {
            ensure!(
                !value.trim().is_empty() && value.chars().count() <= max,
                "sync_habit_nudge_text_invalid"
            );
        }
        ensure!(
            (1..=127).contains(&self.weekdays),
            "sync_habit_nudge_weekdays_invalid"
        );
        ensure!(
            valid_local_time(&self.local_time),
            "sync_habit_nudge_time_invalid"
        );
        ensure!(
            !self.created_at.trim().is_empty() && !self.updated_at.trim().is_empty(),
            "sync_habit_nudge_timestamp_invalid"
        );
        Ok(())
    }
}

fn valid_local_time(value: &str) -> bool {
    let Some((hour, minute)) = value.split_once(':') else {
        return false;
    };
    if hour.len() != 2 || minute.len() != 2 {
        return false;
    }
    let (Ok(hour), Ok(minute)) = (hour.parse::<u32>(), minute.parse::<u32>()) else {
        return false;
    };
    (7..=22).contains(&hour) && minute < 60
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
    /// 语义内核：AI 提议时提取的一句话语义标签（迁移 56 新增；旧规则为 None）
    #[serde(default)]
    pub essence: Option<String>,
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

pub fn analysis_task_id(dataset: &str, event: &str, input_hash: &str, strategy: &str) -> String {
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
    /// `chat` / `habit_nudge` / `habit_checkin`；旧消息缺省为 chat，
    /// 序列化时也省略，保持既有消息编码哈希不变。
    #[serde(
        default = "default_message_kind",
        skip_serializing_if = "is_chat_message"
    )]
    pub kind: String,
    /// 习惯问候去重锚：`<nudge_id>:<YYYY-MM-DD>`。
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "ref")]
    pub message_ref: Option<String>,
}

fn default_message_kind() -> String {
    "chat".to_string()
}

fn is_chat_message(kind: &str) -> bool {
    kind == "chat"
}

impl MessageRecord {
    pub(crate) fn validate(&self) -> Result<()> {
        validate_id(&self.id)?;
        validate_id(&self.conversation_id)?;
        if let Some(id) = &self.parent_message_id {
            validate_id(id)?;
        }
        ensure!(
            matches!(self.role.as_str(), "user" | "assistant"),
            "sync_message_role_invalid"
        );
        ensure!(
            matches!(self.kind.as_str(), "chat" | "habit_nudge" | "habit_checkin"),
            "sync_message_kind_invalid"
        );
        if self.kind == "habit_nudge" {
            ensure!(self.role == "assistant", "sync_habit_nudge_role_invalid");
        }
        if self.kind == "habit_checkin" {
            ensure!(self.role == "user", "sync_habit_checkin_role_invalid");
        }
        if self.kind != "chat" {
            let reference = self
                .message_ref
                .as_deref()
                .context("sync_message_ref_missing")?;
            ensure!(
                !reference.is_empty()
                    && reference.len() <= 128
                    && !reference.chars().any(char::is_control),
                "sync_message_ref_invalid"
            );
        }
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
