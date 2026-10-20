//! system 上下文组装：基础设定、背景记忆与内容对话提示。

use super::prompts::{SYSTEM_PROMPT_BASE, TOOL_GUIDES};
use super::*;

/// Stable instructions and live application state have separate provenance and budgets.
pub(crate) fn build_system_context(
    store: &Store,
    conversation_id: &str,
) -> Result<Vec<ContextMessage>> {
    let mut context = vec![
        ContextMessage::new("system", SYSTEM_PROMPT_BASE).with_segment(
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
    for (_, tools, text) in TOOL_GUIDES {
        if tools.iter().any(|t| resident.contains(*t)) {
            guide_text.push_str(text);
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
    if let Some(conversation) = store.get_conversation(conversation_id)? {
        is_knowledge_mentor = conversation.assistant_mode == "knowledge_mentor"
            || conversation.wiki_page_slug.is_some();
        if is_knowledge_mentor {
            prompt.push_str(
                "\n\n你当前扮演知识页专业导师，而不是主对话秘书。只围绕当前知识页工作：主动指出矛盾、漏洞、模糊表述、未经验证的假设和缺失依据；区分事实、推断、观点和待确认项；必要时直接质疑用户并说明理由。保持尊重但不要为了陪伴或迎合而泛泛附和。默认不要把页面讨论记录为个人事件，也不要主动发起无关的联系人关系或待办提议。任何页面修改只能通过确认制修订工具提出，不能直接声称已修改。",
            );
        }
        let title = conversation.title.as_deref().unwrap_or("未命名");
        prompt.push_str("\n\n当前对话：");
        prompt.push_str(title);
        if let Some(tag) = &conversation.tag {
            prompt.push_str("（标签：");
            prompt.push_str(tag);
            prompt.push('）');
        }
    }
    let input_already_recorded = store
        .latest_event_id_for_conversation(conversation_id)?
        .is_some();
    let allow_record_event = !is_knowledge_mentor && !input_already_recorded;
    if input_already_recorded {
        prompt.push_str(
            "\n\n当前这条用户输入已由系统原样保存为个人事件，并已进入后台分析队列。不要再次记录，也不要把你的摘要或解读另存为事件；直接结合上下文回复即可。",
        );
    } else if allow_record_event {
        prompt.push_str(
            "\n\n当前对话没有自动事件记录。只有在用户明确要求记录，或内容是值得长期回看的客观经历、决定、行动或进展时，才调用 record_event；寒暄、纯提问、对 AI 回复的评价和页面讨论不要记录。不要把助手自己的解读改写成事件。",
        );
    }
    context.push(
        ContextMessage::new("system", std::mem::take(&mut prompt)).with_segment(
            SegmentPurpose::ConversationState,
            SegmentOrigin::Conversation,
            Some(conversation_id),
            OmissionPolicy::Required,
        ),
    );
    // 注入活跃目标（FR-PES-005-02）。
    //
    // 与个人规则库同属「用户自述的长期上下文」，但用法相反：规则是行为约束，
    // 要 AI 逐条对照检查；目标是意图背景，只在话题相关时可用。因此这里的措辞
    // 刻意带反向约束——不要求 AI 判断用户是否偏离目标。偏差检测是后台周期任务
    // 的职责（带「不重复 / 宁缺毋滥」纪律），若对话里也开始评判，同一件事会被做
    // 两遍，且对话里那遍更频繁，会把记录变成被审计。
    let active_goals = store.list_active_goals()?;
    if active_goals.is_empty() {
        prompt.push_str("\n\n用户当前尚未设定任何目标。");
    } else {
        prompt.push_str(
            "\n\n用户当前的目标（仅作为理解其意图的背景）。除非与当前话题直接相关，不要提及这些目标；\
             不要评判用户是否偏离目标，也不要把对话引向目标。\n",
        );
        for goal in &active_goals {
            prompt.push_str(&format!("- [{}] {}\n", goal.phase.label(), goal.content));
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
        "\n\n个人规则库当前为空。".to_string()
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
            format!("\n已有规则的语义标签：\n{}", essences.join("\n"))
        };
        format!(
            "\n\n个人规则库现有 {rule_count} 条生效规则，回复需要引用既有经验规范时先用 list_rules 工具查询，再对照检查。{essence_block}"
        )
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
    let is_knowledge_mentor = store
        .get_conversation(conversation_id)?
        .is_some_and(|c| c.assistant_mode == "knowledge_mentor" || c.wiki_page_slug.is_some());
    let recent = if is_knowledge_mentor {
        Vec::new()
    } else {
        store.recent_user_messages_for_context(6, 160, Some(conversation_id))?
    };
    if recent.is_empty() {
        return Ok(None);
    }
    let mut prompt = String::from(
        "你最近和用户聊到过的事（跨对话要点，供回忆；回复时自然带入，不必逐条复述）：\n",
    );
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
    let is_knowledge_mentor = store
        .get_conversation(conversation_id)?
        .is_some_and(|c| c.assistant_mode == "knowledge_mentor" || c.wiki_page_slug.is_some());
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
    let mut prompt = String::from(
        "今天（本地日期）记录下来的原始条目（真检索快照，按记录顺序；被问到今天记了什么时据实转述，只呈现不评价）：\n",
    );
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
