//! 目标 FRB 门面（FR-PES-005-01/02 & 2026-11 认知型目标升级）。
//!
//! 活跃目标上限由数据库触发器强制，本层只做参数校验与错误转译；
//! 达到上限时不做任何自动顶替，错误直接上抛由界面提示用户先归档。

use crate::api::conversations::ConversationDto;
use crate::storage::{Goal, GoalCompassNudge, GoalPhase, Store};
use anyhow::Result;

/// 目标 DTO
#[derive(Clone, Debug)]
pub struct GoalDto {
    pub id: String,
    pub content: String,
    /// near / mid / long
    pub phase: String,
    /// active / paused / completed
    pub status: String,
    pub why: Option<String>,
    pub current_focus: Option<String>,
    pub blocker: Option<String>,
    pub done_criteria: Option<String>,
    /// 所属长程计划（P1 计划拆解；None = 未挂计划/独立目标）
    pub plan_id: Option<String>,
    /// 计划内步骤序号（None = 未挂计划）
    pub step_index: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
    pub superseded_at: Option<String>,
}

impl From<Goal> for GoalDto {
    fn from(g: Goal) -> Self {
        Self {
            id: g.id,
            content: g.content,
            phase: g.phase.as_str().to_string(),
            status: g.status.as_str().to_string(),
            why: g.why,
            current_focus: g.current_focus,
            blocker: g.blocker,
            done_criteria: g.done_criteria,
            plan_id: g.plan_id,
            step_index: g.step_index,
            created_at: g.created_at,
            updated_at: g.updated_at,
            superseded_at: g.superseded_at,
        }
    }
}

/// 航向偏离校准提醒 DTO（Compass Nudge，FR-PES-005-04）
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoalCompassNudgeDto {
    pub id: String,
    pub goal_id: String,
    pub goal_content: String,
    pub current_focus: String,
    pub stalled_days: i64,
    pub recent_topics: String,
    pub status: String,
    pub created_at: String,
    pub dismissed_until: Option<String>,
}

impl From<GoalCompassNudge> for GoalCompassNudgeDto {
    fn from(n: GoalCompassNudge) -> Self {
        Self {
            id: n.id,
            goal_id: n.goal_id,
            goal_content: n.goal_content,
            current_focus: n.current_focus,
            stalled_days: n.stalled_days,
            recent_topics: n.recent_topics,
            status: n.status,
            created_at: n.created_at,
            dismissed_until: n.dismissed_until,
        }
    }
}

fn parse_phase(phase: &str) -> Result<GoalPhase> {
    GoalPhase::parse(phase)
        .ok_or_else(|| anyhow::anyhow!("目标阶段只能是 near / mid / long，收到: {phase}"))
}

/// 活跃目标条数上限，供界面在达到上限时预先禁用新增。
pub const MAX_ACTIVE_GOALS: usize = crate::storage::MAX_ACTIVE_GOALS;

/// 列出全部活跃目标（最多 3 条，按近期→中期→长远排序）
/// 获取（不存在则创建）某个目标的专属讨论会话，返回会话 DTO。
/// 目标页内 AI 讨论通过该会话进行；生成时注入目标全部认知切片。
pub fn ensure_goal_chat(goal_id: String) -> Result<ConversationDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let Some(goal) = store.get_goal(&goal_id)? else {
        anyhow::bail!("目标不存在: {goal_id}");
    };
    let conversation_id = match store.find_goal_chat_conversation(&goal_id)? {
        Some(id) => id,
        None => store.create_goal_chat_conversation(&goal_id, &format!("[目标] {}", goal.content))?,
    };
    let conversation = store
        .get_conversation(&conversation_id)?
        .ok_or_else(|| anyhow::anyhow!("会话创建失败"))?;
    Ok(ConversationDto {
        id: conversation.id,
        title: conversation.title,
        tag: conversation.tag,
        created_at: conversation.created_at,
        updated_at: conversation.updated_at,
        message_count: conversation.message_count,
        last_message_preview: conversation.last_message_preview,
        archived: conversation.archived,
        wiki_page_slug: conversation.wiki_page_slug,
        goal_id: conversation.goal_id,
    })
}

pub fn list_active_goals() -> Result<Vec<GoalDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    Ok(store
        .list_active_goals()?
        .into_iter()
        .map(GoalDto::from)
        .collect())
}

/// 列出已归档目标（历史所有非活跃，含暂停与达成，按时间倒序）
pub fn list_archived_goals() -> Result<Vec<GoalDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    Ok(store
        .list_archived_goals()?
        .into_iter()
        .map(GoalDto::from)
        .collect())
}

/// 列出暂停搁置的目标（历史，按暂停时间倒序）
pub fn list_paused_goals() -> Result<Vec<GoalDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    Ok(store
        .list_paused_goals()?
        .into_iter()
        .map(GoalDto::from)
        .collect())
}

/// 列出已达成的目标（成就，按达成时间倒序）
pub fn list_completed_goals() -> Result<Vec<GoalDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    Ok(store
        .list_completed_goals()?
        .into_iter()
        .map(GoalDto::from)
        .collect())
}

/// 获取单个目标
pub fn get_goal(id: String) -> Result<Option<GoalDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    Ok(store.get_goal(&id)?.map(GoalDto::from))
}

/// 新建一条目标（支持传入各认知切片）
///
/// 活跃名额未满时直接激活；已满 3 条时以 paused（暂停搁置）状态创建，
/// 保留完整上下文供随时读档唤醒，不报错也不自动顶替既有目标。
pub fn create_goal(
    content: String,
    phase: String,
    why: Option<String>,
    current_focus: Option<String>,
    blocker: Option<String>,
    done_criteria: Option<String>,
) -> Result<GoalDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    if content.trim().is_empty() {
        anyhow::bail!("目标内容不能为空");
    }
    let goal = store.create_goal_with_facets(
        content.trim(),
        parse_phase(&phase)?,
        why.as_deref(),
        current_focus.as_deref(),
        blocker.as_deref(),
        done_criteria.as_deref(),
    )?;
    Ok(GoalDto::from(goal))
}

/// 编辑一条目标的正文与阶段（状态不变，故不受上限限制）
pub fn update_goal(id: String, content: String, phase: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    if content.trim().is_empty() {
        anyhow::bail!("目标内容不能为空");
    }
    store.update_goal(&id, content.trim(), parse_phase(&phase)?)
}

/// 滚动更新目标抓手与卡点（最常用的轻量认知更新操作）
pub fn update_goal_focus(
    id: String,
    current_focus: Option<String>,
    blocker: Option<String>,
) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.update_goal_focus(&id, current_focus.as_deref(), blocker.as_deref())
}

/// 更新目标的所有认知切片字段
pub fn update_goal_facets(
    id: String,
    why: Option<String>,
    current_focus: Option<String>,
    blocker: Option<String>,
    done_criteria: Option<String>,
) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.update_goal_facets(
        &id,
        why.as_deref(),
        current_focus.as_deref(),
        blocker.as_deref(),
        done_criteria.as_deref(),
    )
}

/// 完整更新目标的所有正文与切片字段
pub fn update_goal_full(
    id: String,
    content: String,
    phase: String,
    why: Option<String>,
    current_focus: Option<String>,
    blocker: Option<String>,
    done_criteria: Option<String>,
) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    if content.trim().is_empty() {
        anyhow::bail!("目标内容不能为空");
    }
    store.update_goal_full(
        &id,
        content.trim(),
        parse_phase(&phase)?,
        why.as_deref(),
        current_focus.as_deref(),
        blocker.as_deref(),
        done_criteria.as_deref(),
    )
}

/// 暂停搁置一条目标，让出活跃名额但保留内容与切片上下文作为历史以备随时复活
pub fn pause_goal(id: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.pause_goal(&id)
}

/// 达成一条目标，让出活跃名额并记录为成就勋章
pub fn complete_goal(id: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.complete_goal(&id)
}

/// 归档一条目标（向后兼容接口，等价于 pause_goal）
pub fn archive_goal(id: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.archive_goal(&id)
}

/// 复活一条非活跃目标。活跃目标已满 3 条时同样拒绝。可同时指定当前新抓手与卡点。
pub fn reactivate_goal(
    id: String,
    current_focus: Option<String>,
    blocker: Option<String>,
) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.reactivate_goal_with_focus(&id, current_focus.as_deref(), blocker.as_deref())
}

// ── Compass Nudge（长程偏离检测与航向校准引擎）────────────────────────

/// 文本是否提及目标或抓手
fn matches_text(text: &str, focus: &str, content: &str) -> bool {
    let lower_text = text.to_lowercase();
    let lower_focus = focus.to_lowercase();
    let lower_content = content.to_lowercase();

    // 1. 直接包含
    if lower_text.contains(&lower_focus) || lower_text.contains(&lower_content) {
        return true;
    }

    // 2. 标点与空格切分
    let is_sep = |c: char| {
        c.is_whitespace()
            || [
                '，', '。', '、', '；', '：', ',', ';', ':', '!', '?', '-', '_', '/', '\\',
            ]
            .contains(&c)
    };
    for word in lower_focus.split(is_sep).chain(lower_content.split(is_sep)) {
        let trimmed = word.trim();
        if trimmed.chars().count() >= 2 && lower_text.contains(trimmed) {
            return true;
        }
    }

    // 3. 中文/连续短语的 3-gram / 4-gram 核心实体匹配
    let chars: Vec<char> = lower_focus.chars().collect();
    if chars.len() >= 3 {
        for window_size in [4, 3] {
            if chars.len() >= window_size {
                for window in chars.windows(window_size) {
                    let sub: String = window.iter().collect();
                    if ["攻克了", "推进了", "完成了", "进行中", "优化了"].contains(&sub.as_str()) {
                        continue;
                    }
                    if lower_text.contains(&sub) {
                        return true;
                    }
                }
            }
        }
    }

    false
}

/// 从近期的记录与对话中提炼 2~3 个代表性话题/切片短语
fn summarize_recent_topics(events: &[String], messages: &[String]) -> String {
    let mut topics = Vec::new();
    let is_sep = |c: char| c == '\n' || c == '\r' || c == '。' || c == '；' || c == ';';

    for text in events.iter().chain(messages.iter()) {
        let first_chunk = text.split(is_sep).next().unwrap_or("").trim();
        if first_chunk.chars().count() >= 2 {
            let snippet: String = first_chunk.chars().take(12).collect();
            if !topics.contains(&snippet) {
                topics.push(snippet);
            }
        }
        if topics.len() >= 3 {
            break;
        }
    }

    if topics.is_empty() {
        "近期其他日常事务".to_string()
    } else {
        topics.join("、")
    }
}

/// 核心长程偏离判定引擎（3~5 天滑动窗口）
pub(crate) fn evaluate_goal_compass(store: &Store) -> Result<Option<GoalCompassNudge>> {
    // 1. 若已有生效中的 pending 卡片，直接返回现存卡片，不重复打扰或堆积
    if let Some(existing) = store.get_active_compass_nudge()? {
        return Ok(Some(existing));
    }

    let active_goals = store.list_active_goals()?;
    if active_goals.is_empty() {
        return Ok(None);
    }

    let now = chrono::Utc::now();
    let window_start = (now - chrono::Duration::hours(96)).to_rfc3339();

    // 2. 查询过去 4 天内记录的 events
    let mut stmt = store.connection.prepare(
        "SELECT raw_text FROM events WHERE recorded_at >= ?1 ORDER BY recorded_at DESC LIMIT 50",
    )?;
    let event_texts: Vec<String> = stmt
        .query_map(rusqlite::params![window_start], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    // 查询过去 4 天内用户发送的 messages
    let mut stmt_msg = store.connection.prepare(
        "SELECT content FROM messages WHERE role='user' AND created_at >= ?1 ORDER BY created_at DESC LIMIT 50",
    )?;
    let msg_texts: Vec<String> = stmt_msg
        .query_map(rusqlite::params![window_start], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    // 活跃度阈值守卫：事件 >= 5 条 或 对话 >= 3 场（排除假期、完全不用软件的情况）
    if event_texts.len() < 5 && msg_texts.len() < 3 {
        return Ok(None);
    }

    let recent_topics = summarize_recent_topics(&event_texts, &msg_texts);

    // 3. 寻找抓手停滞且脱靶的活跃目标
    let mut candidate: Option<(Goal, i64)> = None;

    for goal in active_goals {
        let Some(focus) = goal
            .current_focus
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };

        // 7 天防烦频控：同目标近期提醒过或仍在退避期则跳过
        if store.has_recent_compass_nudge_for_goal(&goal.id, 7)? {
            continue;
        }

        // 计算该目标当前抓手的停滞天数
        let updated_at = chrono::DateTime::parse_from_rfc3339(&goal.updated_at)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or(now);
        let days = now.signed_duration_since(updated_at).num_days();
        if days < 3 {
            continue;
        }

        // 检查近 4 天的事件和对话中是否提及
        let mut mentioned = false;
        for text in event_texts.iter().chain(msg_texts.iter()) {
            if matches_text(text, focus, &goal.content) {
                mentioned = true;
                break;
            }
        }

        if !mentioned {
            if let Some((_, max_days)) = candidate {
                if days > max_days {
                    candidate = Some((goal, days));
                }
            } else {
                candidate = Some((goal, days));
            }
        }
    }

    let Some((stalled_goal, stalled_days)) = candidate else {
        return Ok(None);
    };

    let nudge = GoalCompassNudge {
        id: uuid::Uuid::new_v4().to_string(),
        goal_id: stalled_goal.id.clone(),
        goal_content: stalled_goal.content.clone(),
        current_focus: stalled_goal.current_focus.clone().unwrap_or_default(),
        stalled_days: stalled_days.max(3),
        recent_topics,
        status: "pending".to_string(),
        created_at: now.to_rfc3339(),
        dismissed_until: None,
    };
    store.insert_compass_nudge(&nudge)?;
    Ok(Some(nudge))
}

/// 获取当前生效中的航向校准卡片（若无则返回 None）
pub fn get_active_compass_nudge() -> Result<Option<GoalCompassNudgeDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    Ok(store
        .get_active_compass_nudge()?
        .map(GoalCompassNudgeDto::from))
}

/// 后台 worker 定时调用：执行长程偏离检测评估
pub fn tick_goal_compass() -> Result<Option<GoalCompassNudgeDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    Ok(evaluate_goal_compass(&store)?.map(GoalCompassNudgeDto::from))
}

/// 用户点击「处理急事，暂不打扰」：静默退避 7 天
pub fn dismiss_compass_nudge(id: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.dismiss_compass_nudge(&id, 7)
}

/// 航向已校准（例如用户点击了「聚焦攻克卡点」或「更新当前抓手」）
pub fn realign_compass_nudge(id: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.realign_compass_nudge(&id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempStore {
        store: Store,
        path: std::path::PathBuf,
    }

    impl Drop for TempStore {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    fn store() -> TempStore {
        let path = std::env::temp_dir().join(format!(
            "elsewhen-api-goals-test-{}.db",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = Store::open(&path).unwrap();
        TempStore { store, path }
    }

    #[test]
    fn compass_nudge_triggers_on_stalled_focus_with_unrelated_activity() {
        let store = store();
        let now = chrono::Utc::now();
        let four_days_ago = (now - chrono::Duration::days(4)).to_rfc3339();

        // 1. 建立一个 4 天前更新过抓手的目标
        let g = store
            .store
            .create_goal_with_facets(
                "发布 Elsewhen 1.0",
                GoalPhase::Near,
                Some("个人事件系统核心突破"),
                Some("攻克多端同步快照收敛"),
                Some("网络分区下的分片停滞"),
                Some("所有集成测试全绿"),
            )
            .unwrap();

        // 将该目标的 updated_at 改为 4 天前
        store
            .store
            .connection
            .execute(
                "UPDATE goals SET updated_at = ?2 WHERE id = ?1",
                rusqlite::params![g.id, four_days_ago],
            )
            .unwrap();

        // 2. 模拟用户处于活跃期（记了 5 条琐事事件，但都和目标无关）
        for (i, text) in [
            "买了一杯美式咖啡",
            "修理办公室的旧打印机",
            "整理上个月的差旅发票",
            "看了一部科幻电影",
            "晚上去超市采购日常用品",
        ]
        .iter()
        .enumerate()
        {
            let t = (now - chrono::Duration::hours((i as i64) * 6 + 1)).to_rfc3339();
            store
                .store
                .connection
                .execute(
                    "INSERT INTO events(id, occurred_at, recorded_at, raw_text, source, status, created_at, updated_at) VALUES (?1, ?3, ?3, ?2, 'manual', 'active', ?3, ?3)",
                    rusqlite::params![format!("ev-{i}"), text, t],
                )
                .unwrap();
        }

        // 3. 运行评估，应当触发偏离提醒！
        let nudge = evaluate_goal_compass(&store.store).unwrap();
        assert!(nudge.is_some(), "应当检测到长程偏离并生成卡片");
        let n = nudge.unwrap();
        assert_eq!(n.goal_id, g.id);
        assert_eq!(n.goal_content, "发布 Elsewhen 1.0");
        assert_eq!(n.current_focus, "攻克多端同步快照收敛");
        assert!(n.stalled_days >= 4);
        assert!(n.recent_topics.contains("超市采购") || n.recent_topics.contains("科幻电影") || n.recent_topics.contains("打印机"));

        // 4. 重复评估，应直接返回现存卡片（不重复生成）
        let second = evaluate_goal_compass(&store.store).unwrap().unwrap();
        assert_eq!(second.id, n.id);

        // 5. 用户选择「处理急事，暂不打扰」（静默 7 天）
        store.store.dismiss_compass_nudge(&n.id, 7).unwrap();
        let third = evaluate_goal_compass(&store.store).unwrap();
        assert!(third.is_none(), "退避期内不再打扰");
    }

    #[test]
    fn compass_nudge_skips_when_user_is_inactive_or_progressing() {
        let store = store();
        let now = chrono::Utc::now();
        let four_days_ago = (now - chrono::Duration::days(4)).to_rfc3339();

        let g = store
            .store
            .create_goal_with_facets(
                "发布 Elsewhen 1.0",
                GoalPhase::Near,
                None,
                Some("攻克多端同步快照收敛"),
                None,
                None,
            )
            .unwrap();

        store
            .store
            .connection
            .execute(
                "UPDATE goals SET updated_at = ?2 WHERE id = ?1",
                rusqlite::params![g.id, four_days_ago],
            )
            .unwrap();

        // 场景 A: 仅有 2 条事件（少于 5 条，且无对话，说明休假完全没用软件），不误报！
        for (i, text) in ["买面包", "散步"].iter().enumerate() {
            let t = (now - chrono::Duration::hours((i as i64) * 12 + 1)).to_rfc3339();
            store
                .store
                .connection
                .execute(
                    "INSERT INTO events(id, occurred_at, recorded_at, raw_text, source, status, created_at, updated_at) VALUES (?1, ?3, ?3, ?2, 'manual', 'active', ?3, ?3)",
                    rusqlite::params![format!("ev-{i}"), text, t],
                )
                .unwrap();
        }
        assert!(evaluate_goal_compass(&store.store).unwrap().is_none());

        // 场景 B: 补充达到活跃阈值，但其中包含涉及该抓手的事件，不判定为脱靶！
        for (i, text) in [
            "今天排查了多端同步的快照协议",
            "测试快照分片传输",
            "同步状态机测试",
        ]
        .iter()
        .enumerate()
        {
            let t = (now - chrono::Duration::hours((i as i64) * 4 + 1)).to_rfc3339();
            store
                .store
                .connection
                .execute(
                    "INSERT INTO events(id, occurred_at, recorded_at, raw_text, source, status, created_at, updated_at) VALUES (?1, ?3, ?3, ?2, 'manual', 'active', ?3, ?3)",
                    rusqlite::params![format!("ev-prog-{i}"), text, t],
                )
                .unwrap();
        }
        assert!(evaluate_goal_compass(&store.store).unwrap().is_none(), "有对应抓手推进，不应报警");
    }

    #[test]
    fn rolling_focus_updates_realign_pending_compass_nudge() {
        let store = store();
        let now = chrono::Utc::now();
        let four_days_ago = (now - chrono::Duration::days(4)).to_rfc3339();

        let g = store
            .store
            .create_goal_with_facets(
                "发布 Elsewhen 1.0",
                GoalPhase::Near,
                None,
                Some("攻克多端同步快照收敛"),
                None,
                None,
            )
            .unwrap();

        store
            .store
            .connection
            .execute(
                "UPDATE goals SET updated_at = ?2 WHERE id = ?1",
                rusqlite::params![g.id, four_days_ago],
            )
            .unwrap();

        for (i, text) in [
            "买了一杯美式咖啡",
            "修理办公室的旧打印机",
            "整理上个月的差旅发票",
            "看了一部科幻电影",
            "晚上去超市采购日常用品",
        ]
        .iter()
        .enumerate()
        {
            let t = (now - chrono::Duration::hours((i as i64) * 6 + 1)).to_rfc3339();
            store
                .store
                .connection
                .execute(
                    "INSERT INTO events(id, occurred_at, recorded_at, raw_text, source, status, created_at, updated_at) VALUES (?1, ?3, ?3, ?2, 'manual', 'active', ?3, ?3)",
                    rusqlite::params![format!("ev-{i}"), text, t],
                )
                .unwrap();
        }

        // 触发偏离提醒卡片
        let nudge = evaluate_goal_compass(&store.store).unwrap().unwrap();

        // P1 回归：用户在工作台滚动抓手（不经横幅按钮路径）后，
        // 旧卡片必须立即失效，不能永久展示旧抓手/旧停滞天数。
        store
            .store
            .update_goal_focus(&g.id, Some("转向清理构建产物"), None)
            .unwrap();
        assert!(
            store.store.get_active_compass_nudge().unwrap().is_none(),
            "更新抓手后旧 pending 卡片应被同事务校准"
        );

        // 且 7 天频控下不应立即重复生成新卡片（避免更新抓手反被骚扰）
        assert!(evaluate_goal_compass(&store.store).unwrap().is_none());
    }
}
