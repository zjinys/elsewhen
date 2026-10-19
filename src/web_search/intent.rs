//! 搜索意图启发式：user_text_wants_search 与最小 URL 编码。

/// 最小 URL 编码（查询参数值用：空格与保留字转义，中文按 UTF-8 字节 %XX）。
pub(crate) fn urlencoding_simple(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// 用户文本是否显式表达了「需要外网搜索」的意图。
///
/// 设计原则（与你的需求讨论一致）：触发来自**用户输入的明确信号**，不靠模型自评
/// 「我不知道」——后者不可靠。三类信号：时效词（最新/最近/新闻/热点）、直接指令
/// （搜一下/网上查/查一下）、明确要外网搜索。拿不准时由模型反问用户是否要搜（见
/// system prompt），而不是自行断定。
pub fn user_text_wants_search(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return false;
    }
    // 否定词优先：用户明确说「别搜/不用查」时不触发，即使句里有「最新」。
    const NEGATIVE: &[&str] = &[
        "别搜",
        "不用搜",
        "不要搜",
        "别查",
        "不用查",
        "不要查",
        "不用上网",
        "别上网",
    ];
    if NEGATIVE.iter().any(|w| t.contains(w)) {
        return false;
    }
    // 直接指令：让 AI 去搜/去查。
    const COMMAND: &[&str] = &[
        "搜一下",
        "搜索一下",
        "搜一搜",
        "上网搜",
        "网上搜",
        "网上查",
        "上网查",
        "查一下",
        "帮我搜",
        "帮我查",
        "去搜",
        "去查",
        "搜索",
        "外网",
        "联网搜",
        "联网查",
    ];
    if COMMAND.iter().any(|w| t.contains(w)) {
        return true;
    }
    // 时效/资讯词：暗示需要当下、模型训练数据之外的信息。
    const TIMELY: &[&str] = &[
        "最新",
        "最近",
        "近期",
        "这几天",
        "这两天",
        "今天",
        "昨日",
        "昨天",
        "本周",
        "这周",
        "本月",
        "新闻",
        "热点",
        "时事",
        "头条",
        "刚发布",
        "刚出",
        "现在怎么样",
        "现状",
    ];
    TIMELY.iter().any(|w| t.contains(w))
}
