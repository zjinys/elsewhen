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
    let mut is_studio = false;
    let mut goal_chat: Option<(String, Option<String>, Option<String>, Option<String>, Option<String>)> =
        None;
    if let Some(conversation) = store.get_conversation(conversation_id)? {
        is_knowledge_mentor = conversation.assistant_mode == "knowledge_mentor"
            || conversation.wiki_page_slug.is_some();
        if is_knowledge_mentor {
            prompt.push_str(
                "\n\n你当前扮演知识页专业导师，而不是主对话秘书。只围绕当前知识页工作：主动指出矛盾、漏洞、模糊表述、未经验证的假设和缺失依据；区分事实、推断、观点和待确认项；必要时直接质疑用户并说明理由。保持尊重但不要为了陪伴或迎合而泛泛附和。默认不要把页面讨论记录为个人事件，也不要主动发起无关的联系人关系或待办提议。任何页面修改只能通过确认制修订工具提出，不能直接声称已修改。",
            );
        }
        // 目标专属讨论会话（goal_mentor）：AI 是这条目标的幕僚与镜子。
        // 上下文即该目标的全部认知切片；所有变更（暂停/达成/复活/改抓手）
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
                    prompt.push_str(&format!(
                        "\n\n你当前是这条目标的专属幕僚，而不是主对话秘书。这是目标页内的讨论：帮用户看清这条目标本身——当前抓手是否仍然成立、卡点是什么、下一步最小行动是什么。职责是幕僚与镜子：客观陈述你从记录里看到的事实，用设问帮用户收敛判断，不评判、不代用户决定。任何目标变更（更新抓手/卡点/完成判据、暂停、达成、重新激活）都必须通过确认制工具提议（update_goal_facets / pause_goal / complete_goal / reactivate_goal），用户点头才生效，绝不直接声称已修改；用户没有明确表达变更意图时不要主动提议变更。不要把目标讨论记录为个人事件，也不要发起无关的联系人关系或待办提议。\n\n可操作选项 UI：当你给出 2~4 个明确选项希望用户点选时，在回复末尾用围栏代码块输出（界面会渲染成可点击的选项按钮）：\n```elsewhen-choices\n[{{\"label\": \"选项A：可以，愿意用自己的名字说话\"}}, {{\"label\": \"选项B：可以用小号或匿名，不露脸\"}}]\n```\n只用于真正的选择题（确认方向、二选一取舍、确认提议）；每个 label 是用户点击后会原样发出的完整句子，要能独立读懂。不要把开放性问题包装成选项。

生成式 UI（进阶）：当你觉得一张结构化小卡片比纯文字更一目了然时（例：目标切片总览、多维度状态盘点、带点选动作的行动清单），可在回复中用围栏代码块输出 JSON UI 树（界面会渲染成原生卡片组件，需要时与选项块二选一即可）。块内第一行固定写 ```elsewhen-ui，随后是一段 JSON：最外层对象有 root（根元素 key）与 elements（key 到元素的映射），每个元素有 key/type/props/children 字段。示例（注意：示例里的双大括号只是本提示词转义，你输出时全部写成单大括号）：
{{\"root\": \"card\", \"elements\": {{\"card\": {{\"key\": \"card\", \"type\": \"GoalCard\", \"props\": {{\"title\": \"切片盘点\"}}, \"children\": [\"h\", \"m1\", \"acts\"]}}, \"h\": {{\"key\": \"h\", \"type\": \"GoalHeading\", \"props\": {{\"text\": \"当前状态\"}}, \"children\": []}}, \"m1\": {{\"key\": \"m1\", \"type\": \"GoalMetric\", \"props\": {{\"label\": \"抓手停留\", \"value\": \"5天\"}}, \"children\": []}}, \"acts\": {{\"key\": \"acts\", \"type\": \"GoalChoices\", \"props\": {{\"options\": [{{\"label\": \"滚动抓手\", \"message\": \"帮我把抓手滚动到下一个切片\"}}]}}, \"children\": []}}}}}}
可用组件（只能用这些，不可自造）：GoalCard（容器卡片，props: title）、GoalHeading（小标题，props: text）、GoalText（正文，props: text/size/bold/tone，tone∈primary/secondary/tertiary/accent/success/warning/error）、GoalChip（小胶囊，props: label/tone）、GoalMetric（指标块，props: label/value）、GoalChoices（点选动作组，props: options=[{{label, message}}]，点击后把 message 作为用户消息发出；label 是按钮文案，message 缺省用 label）、GoalDivider（分隔线）、Row（横向 Wrap）、Column（纵向，props: spacing）。数据必须真实来自上下文，不确定的数值不要编造；简单回答不用它，纯文字已足够时不要为炫技而生成 UI。\n\n当前讨论的目标：{}（阶段：{}）\n",
                        goal.content,
                        goal.phase.label()
                    ));
                    if let Some(why) = &goal.why {
                        prompt.push_str(&format!("· 核心动力：{}\n", why));
                    }
                    if let Some(focus) = &goal.current_focus {
                        prompt.push_str(&format!("· 当前抓手：{}\n", focus));
                    }
                    if let Some(blocker) = &goal.blocker {
                        prompt.push_str(&format!("· 已知卡点：{}\n", blocker));
                    }
                    if let Some(done) = &goal.done_criteria {
                        prompt.push_str(&format!("· 完成判据：{}\n", done));
                    }
                }
            }
        }
        // 目标工作室（保留 id）：AI 是拆解幕僚，不是主对话秘书。用户来这里把
        // 一句意图（例：「3 年内月入 10 万」）拆成长程计划——现实性评估 +
        // 假设清单 + create_plan 草案；全部落库走确认制，未确认零落库。
        if conversation.id == crate::storage::GOAL_STUDIO_CONVERSATION_ID {
            is_studio = true;
            prompt.push_str(
                r#"

你当前在「目标工作室」里工作，而不是主对话秘书，也不是普通对话窗口。用户来这里，是把一句想法拆成一条可推进的长程计划（例：「3 年内月收入 10 万」）。工作顺序：
① 现实性评估：结合知识库里已有的现状与既往事件，指出这个意图当前最大的不确定性；需要外部事实时用 web_search 核实，不要凭记忆断言。
② 假设清单：说清你凭什么这样拆——时间窗、起点现状、可选路径、主要风险，每条都要能被用户逐条纠正。
③ 计划草案：用 create_plan 一次给出计划全貌（意图原文原样引用）与由近及远的里程碑步骤，每步 1~2 周内可启动、带可验证判据；用户给了期望步数就照做。
之后持续经营：完成一步用 advance_plan_step，先复盘「事实 vs 假设」再推进；要加步骤用 add_plan_steps；用户问进展用 list_plans 看现状再回答。
纪律：任何落库都是确认制提案，未确认前绝不声称已修改或已落库；用户只是闲聊、提问或表达想法时不要提议变更。不要把工作室的讨论记录为个人事件，也不要发起无关的联系人关系或待办提议。

可操作选项 UI：需要用户二选一/确认方向时，在回复末尾输出围栏代码块（界面会渲染成可点击按钮）：
```elsewhen-choices
[{"label": "选项A：3 年，我接受这个时间窗"}, {"label": "选项B：先按 5 年更稳"}]
```
只用于真正的选择题；每个 label 是用户点击后会原样发出的完整句子，要能独立读懂。不要把开放性问题包装成选项。

生成式 UI：需要结构化卡片时可用 ```elsewhen-ui 块（组件与目标讨论一致：GoalCard / GoalHeading / GoalText / GoalChip / GoalMetric / GoalChoices / GoalDivider / Row / Column），数据必须真实来自上下文，不确定的数值不要编造。
"#,
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
    // 目标专属讨论与立项工作室同样不记事件（属于目标域，不属于原始记录流）。
    let is_goal_mentor = goal_chat.is_some() || is_studio;
    let allow_record_event =
        !is_knowledge_mentor && !is_goal_mentor && !input_already_recorded;
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
    // 注入活跃目标（FR-PES-005-02 / G2 认知切片与航向对照）。
    //
    // 与个人规则库同属「用户自述的长期上下文」，但用法不同：规则是行为约束，
    // 要逐条对照检查；目标是用户一切活动的航向基准（主动对照，不是被动的背景板）。
    // 涉及取舍、规划、知识沉淀方向时默认对照当前抓手；但日常闲聊与独立具体
    // 问题保持零打扰，不生硬把话题往目标上拽。
    let active_goals = store.list_active_goals()?;
    if active_goals.is_empty() {
        prompt.push_str("\n\n用户当前尚未设定任何活跃目标。若用户表达长远愿望或感到迷茫时，可适时引导其通过结构化立项梳理出核心目标与当前抓手。");
    } else {
        prompt.push_str(
            "\n\n用户当前设定的核心目标（这是他一切活动的航向基准——理解其长远意图与近期重心，请主动对照）：\n\
             准则：\n\
             1. 日常闲聊或独立具体问题时保持自然专注，不生硬说教或强行把话题往目标上拽；\n\
             2. 涉及取舍、规划、知识沉淀方向时，默认对照其「当前抓手」与「已知卡点」，帮他判断这件事在不在航向上，并自然给出深度解题支持；\n\
             3. 协助用户将大目标拆解收敛到近期的当前抓手上；当用户取得进展、调整重心或推进停滞时：先 enable_tools 启用 goal-management，list_goals 查看目标清单（含暂停/已达成历史），update_goal_facets 提议滚动抓手与卡点、create_goal 提议立项，pause_goal / complete_goal / reactivate_goal 处置暂停、达成与读档复活——所有变更均需用户在审阅卡确认后才生效。\n",
        );
        for goal in &active_goals {
            prompt.push_str(&format!(
                "- [{}][ID:{}] {}\n",
                goal.phase.label(),
                goal.id,
                goal.content
            ));
            if let Some(why) = &goal.why {
                if !why.trim().is_empty() {
                    prompt.push_str(&format!("  · 核心动力：{}\n", why.trim()));
                }
            }
            if let Some(focus) = &goal.current_focus {
                if !focus.trim().is_empty() {
                    prompt.push_str(&format!("  · 当前抓手：{}\n", focus.trim()));
                }
            }
            if let Some(blocker) = &goal.blocker {
                if !blocker.trim().is_empty() {
                    prompt.push_str(&format!("  · 已知卡点：{}\n", blocker.trim()));
                }
            }
            if let Some(done) = &goal.done_criteria {
                if !done.trim().is_empty() {
                    prompt.push_str(&format!("  · 完成判据：{}\n", done.trim()));
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
    // goal_mentor 会话同样跳过跨对话背景：目标讨论不掺入其他对话的回忆噪声。
    let is_knowledge_mentor = store
        .get_conversation(conversation_id)?
        .is_some_and(|c| {
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
        .is_some_and(|c| {
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
