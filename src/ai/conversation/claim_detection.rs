//! 写声明判定与修复提示（实体平移）。
use super::*;
use crate::storage::Store;
use anyhow::Result;

/// 写完成声明检测：回复声称“已保存/已创建”类动作完成。
/// 排除讲解/检索/否定语境（搜、查、没有、还没、未、失败、疑问），只抓肯定式完成声明。
pub(crate) fn write_claim_detected(content: &str) -> bool {
    const CLAIMS: &[&str] = &[
        "存好了",
        "保存好了",
        "已经保存",
        "已保存",
        "保存成功",
        "创建好了",
        "已创建",
        "新建好了",
        "建好了",
        "记好了",
        "已经记下",
        "归档好了",
        "已归档",
        "已生效",
        "已经在知识库",
        "已经在里面",
        // 2026-10-08 kimi 漏拦补充：模型用变体措辞绕过原词表。
        "已写入",
        "已存入",
        "存进了",
        "存到",
        "收录了",
        "已收录",
        "已入库",
        "放进知识库",
        "放进了知识库",
        "存进了知识库",
        "整理成了知识页",
        "整理成知识页",
        "已整理成",
        "已经整理成",
    ];
    // 守卫故意收窄到多字形态：单字（未/查/没）会误伤“未动/查看/没问题，存好了”等正常表述。
    // 漏拦的代价只是一次回炉纠正，误拦则会放过谎言，所以宁可少拦。
    // 2026-10-08：去掉「？」「?」守卫——模型只要在句尾加一句「需要我再补充吗？」
    // 就整体豁免核验，等于给谎言留了万能后门。否定/未遂语境仍由多字守卫覆盖。
    // 「抱歉/对不起」同理：承认「之前没保存」是诚实认错，但「抱歉，已保存」仍是谎言，
    // 不该豁免（kimi 实测用「抱歉，我刚才说已保存其实没保存」绕过核验后继续不执行）。
    const GUARDS: &[&str] = &[
        "搜",
        "没有",
        "还没",
        "失败",
        "是否",
        "如果",
        "假如",
        "抱歉",
        "对不起",
    ];
    CLAIMS.iter().any(|c| content.contains(c)) && !GUARDS.iter().any(|g| content.contains(g))
}
/// 未完成宣告检测：回复预告了「接下来我要做」，但本轮并没有动手。
///
/// 与 `write_claim_detected` 是两个方向——那边抓「谎报已经做完」，这边抓「预告还没做」。
/// 后者曾真实发生：用户贴入新素材要求合并保存，模型回「我准备一个更新版本…我将为你
/// 草拟更新后的版本，稍等片刻。」，零工具调用、零写入，用户干等。写声明词表一个都
/// 命中不了（没有「已保存」类完成态），于是直接收敛。
pub(crate) fn future_promise_detected(content: &str) -> bool {
    const PROMISES: &[&str] = &[
        "稍等片刻",
        "请稍等",
        "稍等一下",
        "稍等我",
        "请稍候",
        "稍候片刻",
        "马上就好",
        "我这就",
        "这就来",
        "接下来我",
        "我接下来",
        "我准备",
        "我来帮你",
        "让我先",
        "我去处理",
        "我去做",
        "我来处理",
        "我会处理",
        "我去查",
        "我先去",
        "一会给你",
        "一会儿给你",
        "稍后给你",
        "稍后回复",
        "一会回复",
        "回头给你",
        "完成后回复",
        "做好后发给你",
        "等我整理好",
    ];
    // 守卫：已经交付了内容并在求确认 —— 那是有实质答复的征求，不算空头预告。
    // 修复受总轮数限制；交付正文和征求确认不应仅因开场措辞被拦截。
    const GUARDS: &[&str] = &[
        "请确认",
        "确认一下",
        "是否需要",
        "要我现在",
        "草稿如下",
        "结果如下",
        "改进稿如下",
        "具体如下",
        "你看这样",
        "这样可以吗",
    ];
    // Examples and quoted/source code are data, not assistant commitments.
    let mut in_code = false;
    let prose = content
        .lines()
        .filter(|line| {
            let line = line.trim();
            if line.starts_with("```") {
                in_code = !in_code;
                return false;
            }
            !in_code && !line.starts_with('>')
        })
        .collect::<Vec<_>>()
        .join("\n");
    PROMISES.iter().any(|p| prose.contains(p)) && !GUARDS.iter().any(|g| prose.contains(g))
}
/// 用户文本是否表达写意图（存/建/记/归档类）。疑问句式（…吗/？）是在问状态，不是下指令。
pub(crate) fn user_text_wants_write(text: &str) -> bool {
    let t = text.trim();
    if t.ends_with('吗') || t.ends_with('？') || t.ends_with('?') {
        return false;
    }
    // 否定/搁置守卫：「可以不保存」「别保存」「先放着」「不用了」含写意图词但语义是拒绝。
    // 原测试 natural_replies_never_execute 的「好的，别保存」「可以不保存」靠这个排除。
    const NEGATIVE: &[&str] = &[
        "不保存",
        "别保存",
        "不用",
        "别存",
        "不要存",
        "先不",
        "先放着",
        "先别",
        "暂不",
        "取消",
    ];
    if NEGATIVE.iter().any(|n| t.contains(n)) {
        return false;
    }
    // 「提交」补于 2026-10-08：kimi 对「再提交古籍那页」「保存为topic」零工具响应。
    // 「整理」不列入——「完成了整理/整理房间/整理思路」是日常陈述非写指令，误伤面太大。
    const WANT: &[&str] = &[
        "存", "保存", "记住", "记下", "创建", "新建", "归档", "入库", "提交",
    ];
    WANT.iter().any(|w| t.contains(w))
}
/// 宽声明面：比 write_claim_detected 更松的「声称完成」信号，用于收敛前接地重发。
/// 不含守卫——这里不是判定撒谎，只是决定「要不要给模型看一眼事实清单」，误报代价
/// 仅是一轮重发。确认/否决短语（「好」「先放着」）天然不含这些词，不受影响。
pub(crate) fn broad_write_claim(content: &str) -> bool {
    const SIGNALS: &[&str] = &[
        "已保存",
        "存好了",
        "保存好了",
        "已创建",
        "建好了",
        "记好了",
        "已归档",
        "已入库",
        "已收录",
        "已写入",
        "已存入",
        "存进了",
        "已经在知识库",
        "完成了",
        "搞定了",
        "妥了",
        "办好了",
        "处理好了",
        "整理好了",
    ];
    SIGNALS.iter().any(|s| content.contains(s))
}
/// 接地注入（治本）：把本轮「行为事实」结构化摆进模型上下文。
///
/// 设计原则：凡是需要模型诚实报告自己行为的地方，把行为事实直接注入，
/// 而不是从它的输出文本反推（关键词匹配永远追不上措辞创造力）。
/// 事实 = 两层：工具执行结果（成败）+ 库内本轮新增（写定/待确认），后者卡时间窗，
/// 与 verify_claimed_writes 同源。模型看着清单复述，措辞空间被事实封死。
pub(crate) fn turn_grounding(
    store: &Store,
    registry: &ToolRegistry,
    conversation_id: &str,
    results: &[ToolResultMsg],
    turn_start: chrono::DateTime<chrono::Utc>,
) -> Result<String> {
    // 第一层：工具执行清单（含成败），写/读分开说。
    let mut lines = vec!["【本轮事实——回复必须与此一致】".to_string()];
    if results.is_empty() {
        lines.push("本轮未执行任何工具调用，没有任何写入发生。".to_string());
    } else {
        for r in results {
            let policy = registry
                .get(&r.call_name)
                .map(|t| t.policy())
                .unwrap_or(ToolPolicy::Read);
            let kind = match policy {
                ToolPolicy::Read => "查询",
                ToolPolicy::WriteDirect => "写入",
                ToolPolicy::WriteConfirm => "草拟（待确认）",
            };
            let outcome = if r.success { "成功" } else { "失败" };
            lines.push(format!("- {} {}：{}", r.call_name, kind, outcome));
        }
    }
    // 第二层：库内本轮新增（卡时间窗，与写声明核验同一事实源）。
    let mut drafts = Vec::new();
    for pa in store.pending_actions_for_conversation(conversation_id)? {
        if touched_this_turn(&pa.created_at, &turn_start) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&pa.args_json) {
                if let Some(t) = v.get("title").and_then(|x| x.as_str()) {
                    drafts.push(format!("「{t}」（{}）", pa.action));
                }
            }
        }
    }
    if !drafts.is_empty() {
        lines.push(format!("本轮登记的待确认草稿：{}。", drafts.join("、")));
    }
    lines.push(
        "如实说明：什么做成了、什么失败了及原因、哪些仍在等用户确认。\
         未执行的不得说成已完成；失败的要给出原因。"
            .to_string(),
    );
    Ok(lines.join("\n"))
}
/// 写声明回炉提示：把库内事实（缺页/旧页/待确认）摆出来，让模型无法再编。
pub(crate) fn write_claim_repair_nudge(
    store: &Store,
    conversation_id: &str,
    verdict: &ClaimVerdict,
) -> Result<String> {
    let detail = match verdict {
        ClaimVerdict::Clean => String::new(),
        ClaimVerdict::DraftOnly(items) | ClaimVerdict::Unproven(items) => items.join("；"),
    };
    // 草稿标题一并给出（slug 要到执行落定时才生成，此处不编造），模型求确认时能点名。
    let mut draft_slugs = Vec::new();
    for pa in store.pending_actions_for_conversation(conversation_id)? {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&pa.args_json) {
            if let Some(t) = v
                .get("title")
                .or_else(|| v.get("project_name"))
                .and_then(|t| t.as_str())
            {
                draft_slugs.push(format!("《{t}》(待确认，未入库)"));
            }
        }
    }
    let drafts = if draft_slugs.is_empty() {
        "当前没有待确认草稿".to_string()
    } else {
        format!("当前待确认：{}", draft_slugs.join("、"))
    };
    Ok(format!(
        "系统核实（以知识库为准，你的上一句与事实不符）：{detail}。{drafts}。\
         你有两个选择，必须二选一，不得再用文字声称已保存：\
         (1) 现在调用工具实际执行（如 save_knowledge_draft），拿到成功结果后才能说已保存；\
         (2) 明确告诉用户「我还没有保存」，并说明原因或需要用户提供什么。\
         请区分：待确认草稿仅存于对话，尚未入库；确认执行并返回真实结果才算已保存。\
         若已有草稿，告知用户可在“今天”栏打开待入库草稿列表查看并单独保存或删除，\
         不要重新草拟同名页，更不要声称草稿已在知识库。"
    ))
}
