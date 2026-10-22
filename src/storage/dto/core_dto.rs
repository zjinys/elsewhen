//! 会话/消息/规则/待办/统计等高频查询 DTO 与标题派生。

#[derive(Debug, Clone)]
pub struct ConversationSummary {
    pub id: String,
    pub title: Option<String>,
    pub tag: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: i32,
    pub last_message_preview: Option<String>,
    pub archived: bool,
    /// 关联的知识页 slug（页内 AI 处理会话）；None 为普通对话
    pub wiki_page_slug: Option<String>,
    /// AI 对话角色：personal_secretary 或 knowledge_mentor
    pub assistant_mode: String,
}

#[derive(Debug, Clone)]
pub struct MessageSummary {
    pub id: String,
    pub conversation_id: String,
    pub parent_message_id: Option<String>,
    pub role: String,
    pub content: String,
    pub created_at: String,
    /// 仅展示用：本条 assistant 回复对应的输入消息（message_response_links）。
    /// AI 链路的因果序仍走 list_messages，不带此字段。
    pub input_message_id: Option<String>,
    /// 消息类型：chat（普通消息）/ habit_nudge（习惯问候）/ habit_checkin（问候回答）。
    pub kind: String,
    /// 习惯问候的去重锚：`<nudge_id>:<YYYY-MM-DD>`；普通消息为 None。
    pub message_ref: Option<String>,
}

/// 用首条用户消息自动生成对话标题：取第一行 → 折叠连续空白 → 截断到 24 字（超出加省略号）。
pub(crate) fn derive_conversation_title(content: &str) -> Option<String> {
    const MAX_CHARS: usize = 24;
    let first_line = content.lines().next().unwrap_or("").trim();
    if first_line.is_empty() {
        return None;
    }
    let mut cleaned = String::with_capacity(first_line.len());
    let mut prev_space = false;
    for ch in first_line.chars() {
        if ch.is_whitespace() {
            if !prev_space {
                cleaned.push(' ');
            }
            prev_space = true;
        } else {
            cleaned.push(ch);
            prev_space = false;
        }
    }
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        return None;
    }
    let mut chars: Vec<char> = cleaned.chars().collect();
    let truncated = chars.len() > MAX_CHARS;
    if truncated {
        chars.truncate(MAX_CHARS);
    }
    let mut title: String = chars.into_iter().collect();
    if truncated {
        title.push('…');
    }
    Some(title)
}

/// 个人经验规则的状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleStatus {
    /// 已生效，注入到 AI 回复的上下文中
    Active,
    /// AI 刚从对话中提议、等待用户确认
    Pending,
}

impl RuleStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            RuleStatus::Active => "active",
            RuleStatus::Pending => "pending",
        }
    }
}

/// 个人经验规则库的一条规则
#[derive(Debug, Clone)]
pub struct RuleSummary {
    pub id: String,
    pub content: String,
    pub status: RuleStatus,
    pub created_at: String,
    /// 触发场景（capture/conversation/daily_review；None = 全场景，不预筛时无意义）
    pub trigger_scene: Option<String>,
    /// 触发关键词 JSON 数组字符串（None = 不参与事件预筛）
    pub trigger_keywords: Option<String>,
    /// 语义内核：AI 在提议时提取的一句话语义标签（None = 旧规则或AI未产出）
    pub essence: Option<String>,
}

/// 待办状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TodoStatus {
    Open,
    Done,
    Archived,
}

impl TodoStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TodoStatus::Open => "open",
            TodoStatus::Done => "done",
            TodoStatus::Archived => "archived",
        }
    }

    pub fn parse(s: &str) -> TodoStatus {
        match s {
            "done" => TodoStatus::Done,
            "archived" => TodoStatus::Archived,
            _ => TodoStatus::Open,
        }
    }
}

/// 一条个人待办（AI 提议确认后创建，或手动创建）
#[derive(Debug, Clone)]
pub struct Todo {
    pub id: String,
    pub title: String,
    pub status: TodoStatus,
    pub priority: String, // high / normal / low
    pub due_at: Option<String>,
    /// 是否为「提醒」类待办（migration 50）：到期时由本机提醒通道弹窗/通知。
    /// 普通待办恒为 false；语义与 due_at 无关（待办也可有截止时间）。
    pub reminder: bool,
    pub related_event_id: Option<String>,
    pub related_wiki_slug: Option<String>,
    pub note: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 一条已确认的习惯主动问候计划（同步行）。
#[derive(Debug, Clone)]
pub struct HabitNudge {
    pub id: String,
    pub page_slug: String,
    pub question: String,
    pub done_label: String,
    pub done_text: String,
    pub missed_label: String,
    pub missed_text: String,
    /// 周几位掩码：周一=1、周二=2 … 周日=64；127 = 每天。
    pub weekdays: i32,
    /// 本地墙钟 `HH:MM`。
    pub local_time: String,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// 习惯问候的本机触发/回答状态（不随快照同步）。
#[derive(Debug, Clone)]
pub struct HabitNudgeFire {
    pub nudge_id: String,
    pub local_date: String,
    pub fired_at: String,
    pub ask_count: i32,
    pub snooze_until: Option<String>,
    pub response: Option<String>,
    pub device_id: String,
    pub message_id: Option<String>,
}

/// 一条最近的历史用户消息（跨对话，供 AI 回忆近期事件）
#[derive(Debug, Clone)]
pub struct RecentUserMessage {
    pub conversation_id: String,
    pub content: String,
}

/// 一条待确认动作（AI 写类工具草拟，用户确认后才执行）
#[derive(Debug, Clone)]
pub struct PendingAction {
    pub id: String,
    pub conversation_id: String,
    pub action: String,
    pub args_json: String,
    pub created_at: String,
}

/// 一条知识库/事件搜索命中
#[derive(Debug, Clone)]
pub struct KnowledgeHit {
    pub kind: String,
    pub title: String,
    pub snippet: String,
}

/// 按天聚合的 token 用量统计
#[derive(Debug, Clone)]
pub struct DailyTokenUsage {
    pub date: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub call_count: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AnalysisJobStats {
    pub pending: i64,
    pub running: i64,
    pub retry: i64,
    pub succeeded: i64,
    pub failed: i64,
}

/// 一条已生成的认知洞察（derived data）
#[derive(Debug, Clone)]
pub struct InsightSummary {
    pub id: String,
    pub created_at: String,
    pub window_days: i64,
    pub prompt_version: String,
    pub lens: String,
    pub title: String,
    pub observation: String,
    pub related_events: Vec<String>,
    pub action: Option<String>,
    pub status: String,
}
