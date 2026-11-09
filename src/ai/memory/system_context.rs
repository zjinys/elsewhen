//! system 上下文组装：基础设定、背景记忆与内容对话提示。

use super::prompts::{SYSTEM_PROMPT_BASE, TOOL_GUIDES};
use super::*;

/// Stable instructions and live application state have separate provenance and budgets.
pub(crate) fn build_system_context(
    store: &Store,
    conversation_id: &str,
) -> Result<Vec<ContextMessage>> {
    let resources = store.prompt_for_run("chat.context.fixed_contract")?;
    let strings = crate::storage::prompts::render::TextResources::parse(&resources)?;
    let mut context = vec![
        ContextMessage::new(
            "system",
            format!(
                "{}\n{}\n{}",
                store.prompt_for_run("chat.personal.policy")?,
                store.prompt_for_run("chat.fixed_contract")?,
                store.output_language_directive(true)?
            ),
        )
        .with_segment(
            SegmentPurpose::StableSetup,
            SegmentOrigin::Application,
            Some("elsewhen-chat-setup-v1"),
            OmissionPolicy::Required,
        ),
    ];
    // R2B：按需工具指南——常驻（非延迟）工具的详细句式渲染为独立 system
    // message（只追加、不并入基底，保住 base 消息的前缀稳定性）；延迟工具
    // 的用法由 schema description 承担，激活前不占上下文。
    let resident = crate::ai::tool::ToolRegistry::deferred_runtime().resident_tool_names();
    let mut guide_text = String::new();
    for (label, tools, _) in TOOL_GUIDES {
        if tools.iter().any(|t| resident.contains(*t)) {
            guide_text.push_str(strings.literal(&format!("guide_{label}"))?);
            guide_text.push_str("\n\n");
        }
    }
    if !guide_text.is_empty() {
        context.push(
            ContextMessage::new("system", guide_text.trim_end().to_string()).with_segment(
                SegmentPurpose::StableSetup,
                SegmentOrigin::Application,
                Some("elsewhen-chat-tool-guides-v1"),
                OmissionPolicy::Required,
            ),
        );
    }
    let mut prompt = String::new();
    let mut is_knowledge_mentor = false;
    let mut is_studio = false;
    let mut goal_chat: Option<(
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = None;
    if let Some(conversation) = store.get_conversation(conversation_id)? {
        is_knowledge_mentor = conversation.assistant_mode == "knowledge_mentor"
            || conversation.wiki_page_slug.is_some();
        if is_knowledge_mentor {
            prompt.push_str(strings.literal("knowledge_mentor")?);
        }
        // 目标专属讨论会话（goal_mentor）：AI 是这条目标的幕僚与镜子。
        // 上下文即该目标的全部目标背景；所有变更（暂停/达成/复活/改行动）
        // 都通过确认制工具提议，绝不直接声称已修改。
        if conversation.assistant_mode == "goal_mentor" {
            if let Some(goal_id) = &conversation.goal_id {
                if let Some(goal) = store.get_goal(goal_id)? {
                    goal_chat = Some((
                        goal.id.clone(),
                        goal.why.clone(),
                        goal.current_focus.clone(),
                        goal.blocker.clone(),
                        goal.done_criteria.clone(),
                    ));
                    prompt.push_str(&strings.render(
                        "goal_mentor",
                        &[
                            ("goal", goal.content.clone()),
                            ("phase", strings.literal(goal.phase.as_str())?.to_string()),
                        ],
                    )?);
                    if let Some(why) = &goal.why {
                        prompt.push_str(&strings.render("why", &[("value", (why).to_string())])?);
                    }
                    if let Some(focus) = &goal.current_focus {
                        prompt
                            .push_str(&strings.render("focus", &[("value", (focus).to_string())])?);
                    }
                    if let Some(blocker) = &goal.blocker {
                        prompt.push_str(
                            &strings.render("blocker", &[("value", (blocker).to_string())])?,
                        );
                    }
                    if let Some(done) = &goal.done_criteria {
                        prompt.push_str(&strings.render("done", &[("value", (done).to_string())])?);
                    }
                }
            }
        }
        // 目标工作室（保留 id）：AI 是拆解幕僚，不是主对话秘书。用户来这里把
        // 一句意图（例：「3 年内月入 10 万」）拆成长程计划——现实性评估 +
        // 假设清单 + create_plan 草案；全部落库走确认制，未确认零落库。
        if conversation.id == crate::storage::GOAL_STUDIO_CONVERSATION_ID {
            is_studio = true;
            prompt.push_str(strings.literal("studio")?);
        }
        let title = conversation
            .title
            .as_deref()
            .unwrap_or(strings.literal("untitled")?);
        prompt.push_str(strings.literal("conversation")?);
        prompt.push_str(title);
        if let Some(tag) = &conversation.tag {
            prompt.push_str(strings.literal("tag")?);
            prompt.push_str(tag);
            prompt.push_str(strings.literal("tag_end")?);
        }
    }
    let input_already_recorded = store
        .latest_event_id_for_conversation(conversation_id)?
        .is_some();
    // 目标专属讨论与立项工作室同样不记事件（属于目标域，不属于原始记录流）。
    let is_goal_mentor = goal_chat.is_some() || is_studio;
    if is_goal_mentor {
        prompt.push_str(strings.literal("auto_references")?);
        let (profile, _) = store.planning_profile()?;
        let missing = [
            (
                strings.literal("skills_missing")?,
                profile.skills.trim().is_empty(),
            ),
            (
                strings.literal("resources_missing")?,
                profile.resources.trim().is_empty(),
            ),
            (
                strings.literal("limits_missing")?,
                profile.limits.trim().is_empty(),
            ),
            (
                strings.literal("preferences_missing")?,
                profile.preferences.trim().is_empty(),
            ),
        ]
        .into_iter()
        .filter_map(|(name, missing)| missing.then_some(name))
        .collect::<Vec<_>>();
        prompt.push_str(strings.literal("profile_check")?);
        if missing.len() == 4 {
            prompt.push_str(strings.literal("profile_empty")?);
        } else if !missing.is_empty() {
            prompt.push_str(&strings.render(
                "missing",
                &[(
                    "value",
                    (missing.join(strings.literal("separator")?)).to_string(),
                )],
            )?);
        }
        prompt.push_str(strings.literal("profile_sufficiency")?);
    }

    if !is_knowledge_mentor {
        if let Ok((profile, _)) = store.planning_profile() {
            let fields = [
                (strings.literal("skills")?, profile.skills),
                (strings.literal("resources")?, profile.resources),
                (strings.literal("limits")?, profile.limits),
                (strings.literal("preferences_missing")?, profile.preferences),
            ];
            if fields.iter().any(|(_, v)| !v.trim().is_empty()) {
                prompt.push_str(strings.literal("profile_facts")?);
                for (label, value) in fields {
                    if !value.trim().is_empty() {
                        prompt.push_str(&strings.render(
                            "profile_field",
                            &[
                                ("label", (label.to_string()).to_string()),
                                (
                                    "value",
                                    (value.chars().take(500).collect::<String>()).to_string(),
                                ),
                            ],
                        )?);
                    }
                }
            }
        }
        prompt.push_str(strings.literal("profile_discipline")?);
    }
    if let Some((goal_id, ..)) = &goal_chat {
        if let Ok((references, _)) = store.goal_references(goal_id) {
            if !references.is_empty() {
                prompt.push_str(strings.literal("references")?);
            }
            for reference in references {
                prompt.push_str(&strings.render(
                    "reference",
                    &[
                        ("slug", (reference.slug).to_string()),
                        ("title", (reference.title).to_string()),
                        (
                            "summary",
                            (reference.summary.chars().take(400).collect::<String>()).to_string(),
                        ),
                    ],
                )?);
            }
        }
    }

    let allow_record_event = !is_knowledge_mentor && !is_goal_mentor && !input_already_recorded;
    if input_already_recorded {
        prompt.push_str(strings.literal("already_recorded")?);
    } else if allow_record_event {
        prompt.push_str(strings.literal("recording_discipline")?);
    }
    context.push(
        ContextMessage::new("system", std::mem::take(&mut prompt)).with_segment(
            SegmentPurpose::ConversationState,
            SegmentOrigin::Conversation,
            Some(conversation_id),
            OmissionPolicy::Required,
        ),
    );
    // 注入活跃目标（FR-PES-005-02 / G2 目标背景与航向对照）。
    //
    // 与个人规则库同属「用户自述的长期上下文」，但用法不同：规则是行为约束，
    // 要逐条对照检查；目标是用户一切活动的航向基准（主动对照，不是被动的背景板）。
    // 涉及取舍、规划、知识沉淀方向时默认对照当前行动；但日常闲聊与独立具体
    // 问题保持零打扰，不生硬把话题往目标上拽。
    // A pending semantic card is optional background, not a new task. Legacy
    // pinned bundles predate this resource and retain their original behavior.
    if !is_knowledge_mentor {
        if let Ok(intro) = strings.literal("compass_context") {
            if let Some(nudge) = store.get_active_compass_nudge()? {
                if let Some((_, output, _)) = store.compass_details(&nudge.id)? {
                    prompt.push_str(intro);
                    prompt.push_str(&serde_json::json!({"goal_id":nudge.goal_id,"assessment":serde_json::from_str::<serde_json::Value>(&output)?}).to_string());
                }
            }
        }
    }
    let active_goals = store.list_active_goals()?;
    if active_goals.is_empty() {
        prompt.push_str(strings.literal("no_goals")?);
    } else {
        prompt.push_str(strings.literal("active_goals")?);
        for goal in &active_goals {
            prompt.push_str(&strings.render(
                "active_goal",
                &[
                    ("phase", strings.literal(goal.phase.as_str())?.to_string()),
                    ("id", (goal.id).to_string()),
                    ("goal", (goal.content).to_string()),
                ],
            )?);
            if let Some(why) = &goal.why {
                if !why.trim().is_empty() {
                    prompt.push_str(
                        &strings.render("active_why", &[("value", (why.trim()).to_string())])?,
                    );
                }
            }
            if let Some(focus) = &goal.current_focus {
                if !focus.trim().is_empty() {
                    prompt.push_str(
                        &strings
                            .render("active_focus", &[("value", (focus.trim()).to_string())])?,
                    );
                }
            }
            if let Some(blocker) = &goal.blocker {
                if !blocker.trim().is_empty() {
                    prompt.push_str(
                        &strings
                            .render("active_blocker", &[("value", (blocker.trim()).to_string())])?,
                    );
                }
            }
            if let Some(done) = &goal.done_criteria {
                if !done.trim().is_empty() {
                    prompt.push_str(
                        &strings.render("active_done", &[("value", (done.trim()).to_string())])?,
                    );
                }
            }
        }
    }

    context.push(
        ContextMessage::new("system", std::mem::take(&mut prompt)).with_segment(
            SegmentPurpose::Goals,
            SegmentOrigin::Goals,
            Some("active-goals"),
            OmissionPolicy::Required,
        ),
    );
    // 规则库与主对话解耦（瘦身）：不再全量注入规则文本——规则库会持续增长，
    // 全量注入会让基座随规则数线性膨胀。常驻只播报规模与查询方式；全文按
    // 需经常驻 list_rules 工具获取（模型在触发场景自主查询）。
    let rule_count = store.count_active_rules()?;
    let rules_note = if rule_count == 0 {
        strings.literal("rules_empty")?.to_string()
    } else {
        // essence 轻量注入：每条规则的语义标签一句话，让模型在对话中按本质
        // 匹配到相关规则，而不是靠「觉得需要」才调 list_rules。
        let rules = store.list_active_rules()?;
        let essences: Vec<String> = rules
            .iter()
            .filter_map(|r| r.essence.as_deref().map(|e| format!("- {e}")))
            .collect();
        let essence_block = if essences.is_empty() {
            String::new()
        } else {
            strings.render("rule_tags", &[("value", (essences.join("\n")).to_string())])?
        };
        strings.render(
            "rules",
            &[
                ("count", (rule_count.to_string()).to_string()),
                ("tags", (essence_block).to_string()),
            ],
        )?
    };
    prompt.push_str(&rules_note);
    context.push(ContextMessage::new("system", prompt).with_segment(
        SegmentPurpose::PersonalRules,
        SegmentOrigin::Rules,
        Some("active-personal-rules"),
        OmissionPolicy::Required,
    ));
    Ok(context)
}

pub(crate) fn recent_background(
    store: &Store,
    conversation_id: &str,
) -> Result<Option<ContextMessage>> {
    let resources = store.prompt_for_run("chat.context.fixed_contract")?;
    let strings = crate::storage::prompts::render::TextResources::parse(&resources)?;
    // goal_mentor 会话同样跳过跨对话背景：目标讨论不掺入其他对话的回忆噪声。
    let is_knowledge_mentor = store.get_conversation(conversation_id)?.is_some_and(|c| {
        c.assistant_mode == "knowledge_mentor"
            || c.wiki_page_slug.is_some()
            || c.assistant_mode == "goal_mentor"
    });
    let recent = if is_knowledge_mentor {
        Vec::new()
    } else {
        store.recent_user_messages_for_context(6, 160, Some(conversation_id))?
    };
    if recent.is_empty() {
        return Ok(None);
    }
    let mut prompt = String::from(strings.literal("recent_background")?);
    for msg in recent {
        prompt.push_str("- ");
        prompt.push_str(&msg.content);
        prompt.push('\n');
    }
    let message = ContextMessage::new("system", prompt).with_segment(
        SegmentPurpose::CrossConversation,
        SegmentOrigin::StoredMessage,
        Some("recent-other-conversations"),
        OmissionPolicy::OptionalBackground,
    );
    Ok(Some(message))
}

/// 今日记忆真检索（决策辅助 D6，合并今日记忆提案）：把今天记录下来的
/// 事件原文作为可选背景注入,让 AI 回答「今天记了什么」时是在真检索,
/// 不是靠模型推断编造。红线：只呈现原文快照,不评价不汇总;没有记录时
/// 不注入（AI 无素材,由基础角色的零编造约束兜底,不为此每次多花一段）。
/// knowledge_mentor / 知识库页对话模式跳过（与 recent_background 同口径）。
pub(crate) fn today_memory_background(
    store: &Store,
    conversation_id: &str,
) -> Result<Option<ContextMessage>> {
    let resources = store.prompt_for_run("chat.context.fixed_contract")?;
    let strings = crate::storage::prompts::render::TextResources::parse(&resources)?;
    let is_knowledge_mentor = store.get_conversation(conversation_id)?.is_some_and(|c| {
        c.assistant_mode == "knowledge_mentor"
            || c.wiki_page_slug.is_some()
            || c.assistant_mode == "goal_mentor"
    });
    if is_knowledge_mentor {
        return Ok(None);
    }
    let today = chrono::Local::now().date_naive();
    let mut entries: Vec<(String, String)> = store
        .daily_entries(today)?
        .into_iter()
        .map(|entry| (entry.recorded_at, entry.raw_text))
        .collect();
    if entries.is_empty() {
        return Ok(None);
    }
    // 记录时间正序呈现（今天一整天的时间线）。
    entries.reverse();
    let mut prompt = String::from(strings.literal("personal_today")?);
    let mut count = 0;
    for (recorded_at, raw_text) in entries {
        let time = chrono::DateTime::parse_from_rfc3339(&recorded_at)
            .map(|dt| dt.with_timezone(&chrono::Local).format("%H:%M").to_string())
            .unwrap_or_else(|_| String::new());
        let text: String = raw_text.chars().take(160).collect();
        prompt.push_str(&format!("- {time} {text}\n"));
        count += 1;
        if count >= 12 {
            break;
        }
    }
    let message = ContextMessage::new("system", prompt).with_segment(
        SegmentPurpose::CrossConversation,
        SegmentOrigin::StoredMessage,
        Some("today-recorded-events"),
        OmissionPolicy::OptionalBackground,
    );
    Ok(Some(message))
}

/// 知识库认知画像阶段（2026-11-05 洞见浮现提案 §6）：把近期活跃的高价值
/// 提炼（认知洞察/新习惯/重复固定成本）作为长期记忆注入，让主对话 AI 具备
/// 对用户生活节奏的常识记忆，与卡片浮现形成「即时感知 + 长效唤醒」双轨。
/// knowledge_mentor / goal_mentor / 知识页会话跳过（与 today_memory_background
/// 同口径）；总量限 300 字符，内容只是提示，不要求模型复述。
pub(crate) fn user_cognition_profile_background(
    store: &Store,
    conversation_id: &str,
) -> Result<Option<ContextMessage>> {
    let resources = store.prompt_for_run("chat.context.fixed_contract")?;
    let strings = crate::storage::prompts::render::TextResources::parse(&resources)?;
    let is_isolated = store.get_conversation(conversation_id)?.is_some_and(|c| {
        c.assistant_mode == "knowledge_mentor"
            || c.wiki_page_slug.is_some()
            || c.assistant_mode == "goal_mentor"
    });
    if is_isolated {
        return Ok(None);
    }
    let cutoff = (chrono::Utc::now() - chrono::Duration::days(30)).to_rfc3339();
    let mut statement = store.connection.prepare(
        "SELECT kind,title,summary FROM wiki_pages
         WHERE kind IN ('insight','habit','recurring_cost') AND status='active'
           AND updated_at>=?1
         ORDER BY updated_at DESC LIMIT 5",
    )?;
    let rows = statement.query_map([cutoff], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    let mut lines: Vec<String> = Vec::new();
    let mut used = 0usize;
    for row in rows {
        let (kind, title, summary) = row?;
        let label = match kind.as_str() {
            "insight" => strings.literal("cognition_insight")?,
            "habit" => strings.literal("cognition_habit")?,
            _ => strings.literal("cognition_cost")?,
        };
        let summary: String = summary.chars().take(60).collect();
        let line = if summary.trim().is_empty() {
            format!("- [{label}] {title}")
        } else {
            format!("- [{label}] {title}：{summary}")
        };
        used += line.chars().count() + 1;
        if used > 300 {
            break;
        }
        lines.push(line);
    }
    if lines.is_empty() {
        return Ok(None);
    }
    let prompt = strings.render("cognition_background", &[("facts", lines.join("\n"))])?;
    let message = ContextMessage::new("system", prompt).with_segment(
        SegmentPurpose::CrossConversation,
        SegmentOrigin::StoredMessage,
        Some("user-cognition-profile"),
        OmissionPolicy::OptionalBackground,
    );
    Ok(Some(message))
}

/// 组装 system 消息：基础角色设定 + 一段抓取内容（内容对话用，
/// 让 AI 围绕抓取到的原文回答问题，不评价来源平台）
pub fn build_content_system_prompt(content: &str) -> ContextMessage {
    let mut prompt = SYSTEM_PROMPT_BASE.to_string();
    prompt.push_str(
        "\n\n你当前扮演专业内容导师，而不是主对话秘书。请围绕下面的原始材料进行审阅、解释和提炼：主动区分材料中的事实、观点、推断和缺失依据；指出矛盾、模糊处和未经证实的结论；不要为了迎合用户而泛泛附和，也不要把材料内容自动记录成用户个人事件。保持尊重但可以直接提出质疑。",
    );
    prompt.push_str("\n\n【本次要讨论的抓取内容（原始文本，请围绕它回答用户的问题）】\n");
    prompt.push_str(content);
    ContextMessage::new("system", prompt)
}
