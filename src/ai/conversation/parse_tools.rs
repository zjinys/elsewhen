//! 工具调用解析：envelope/DSML/标题提取与写核验（实体平移）。
use super::*;
use crate::ai::tool::ToolCall;
use crate::storage::Store;
use anyhow::Result;
use serde_json::Value;

/// 解析文本信封中的工具调用：返回（剥离信封后的文本, 解析出的调用列表）。
/// 支持一行内多个标记，标记所在行会从文本中移除。
/// 再加一层兜底：任何没被识别的协议残留都会被剥掉，绝不直接见用户。
pub(crate) fn parse_tool_call_envelope(content: &str) -> (String, Vec<ToolCall>) {
    if !content.contains(TOOL_CALL_MARKER) {
        let (cleaned, mut calls) = parse_xml_tool_calls(content);
        calls.extend(parse_dsml_tool_calls(&cleaned));
        return (
            strip_tool_protocol_residue(&strip_dsml_artifacts(&cleaned)),
            calls,
        );
    }
    let mut cur = content.to_string();
    let mut calls = Vec::new();
    loop {
        match extract_one_tool_call(&cur) {
            Some((cleaned, call)) => {
                cur = cleaned;
                calls.push(call);
            }
            None => break,
        }
    }
    if cur.contains("<tool_call") {
        let (cleaned, mut xml_calls) = parse_xml_tool_calls(&cur);
        cur = cleaned;
        calls.append(&mut xml_calls);
    }
    (
        strip_tool_protocol_residue(&strip_dsml_artifacts(&cur)),
        calls,
    )
}
/// 解析 XML 方言的工具调用：
/// `<tool_call>工具名<arg_key>参数名</arg_key><arg_value>参数值</arg_value>…</tool_call>`。
/// 部分模型无视文本协议，按自带习惯吐这种格式；认下来并执行，
/// 而不是让它原样漏给用户。返回（剥离调用块后的文本, 调用列表）。
/// 结构残缺（无名、无参数、无闭合）的块不解析，留给兜底清理。
pub(crate) fn parse_xml_tool_calls(content: &str) -> (String, Vec<ToolCall>) {
    const OPEN: &str = "<tool_call>";
    const CLOSE: &str = "</tool_call>";
    const KEY_OPEN: &str = "<arg_key>";
    const KEY_CLOSE: &str = "</arg_key>";
    const VAL_OPEN: &str = "<arg_value>";
    const VAL_CLOSE: &str = "</arg_value>";
    let mut cur = content.to_string();
    let mut calls = Vec::new();
    loop {
        let Some(start) = cur.find(OPEN) else { break };
        let after_open = start + OPEN.len();
        // 工具名：紧跟其后的文本，直到下一个 `<`
        let name_end_rel = cur[after_open..].find('<');
        let Some(name_end_rel) = name_end_rel else {
            break;
        };
        let name = cur[after_open..after_open + name_end_rel].trim();
        if name.is_empty() {
            break;
        }
        // 参数对：连续的 <arg_key>K</arg_key><arg_value>V</arg_value>
        let mut args = serde_json::Map::new();
        let mut pos = after_open + name_end_rel;
        loop {
            let rest = &cur[pos..];
            let Some(ko) = rest.find(KEY_OPEN) else { break };
            // key 与工具名/上一对 value 之间只允许空白
            if !rest[..ko].trim().is_empty() {
                break;
            }
            let k_start = pos + ko + KEY_OPEN.len();
            let Some(k_end_rel) = cur[k_start..].find(KEY_CLOSE) else {
                break;
            };
            let key = cur[k_start..k_start + k_end_rel].trim().to_string();
            let v_open = k_start + k_end_rel + KEY_CLOSE.len();
            if !cur[v_open..].trim_start().starts_with(VAL_OPEN) {
                break;
            }
            let v_start = v_open + cur[v_open..].find(VAL_OPEN).unwrap_or(0) + VAL_OPEN.len();
            let Some(v_end_rel) = cur[v_start..].find(VAL_CLOSE) else {
                break;
            };
            let value = cur[v_start..v_start + v_end_rel].trim().to_string();
            if key.is_empty() {
                break;
            }
            args.insert(key, Value::String(value));
            pos = v_start + v_end_rel + VAL_CLOSE.len();
        }
        if args.is_empty() {
            break;
        }
        // 闭合标签：参数之后只允许空白
        let tail = &cur[pos..];
        let Some(close_rel) = tail.find(CLOSE) else {
            break;
        };
        if !tail[..close_rel].trim().is_empty() {
            break;
        }
        let end = pos + close_rel + CLOSE.len();
        calls.push(ToolCall::new(name, Value::Object(args)));
        cur.replace_range(start..end, "");
    }
    (cur.trim().to_string(), calls)
}
/// 宽口径“调用意图”检测：模型疑似想调工具时返回 true。
/// 故意比严格解析器宽得多——两者解耦：解析器负责“能执行”，这里负责“别漏调”。
/// 漏报 = 该调的工具永远不执行（不可接受）；误报 = 最多浪费一次回炉（有次数上限）。
/// 判定信号分两档：
/// 1. 任一已知协议的结构标记（与具体方言无关，见一个即命中）；
/// 2. 出现已知工具名 + 调用形态（arguments 字样，或 JSON 名值对）。
pub(crate) fn tool_call_attempt_detected(content: &str, tool_names: &[String]) -> bool {
    const STRUCTURAL: &[&str] = &[
        "<tool_call",
        "</tool_call>",
        "<arg_key",
        "<arg_value",
        TOOL_CALL_MARKER,
        "<|invoke",
        "invoke name=",
        "parameter name=",
        "\"tool_calls\"",
        "\"function_call\"",
        "<function",
        "</function>",
        "function name=",
    ];
    if STRUCTURAL.iter().any(|m| content.contains(m)) {
        return true;
    }
    if !tool_names.iter().any(|n| content.contains(n.as_str())) {
        return false;
    }
    let lower = content.to_lowercase();
    lower.contains("arguments") || (content.contains("\"name\"") && content.contains('{'))
}
/// 从回复中提取被声称的实体标题（「」/『』/《》/""/'' 配对引号）。
/// 本应用的写声明几乎总是引用标题（法医语料全部命中），比动词关键词稳定得多。
pub(crate) fn extract_claimed_titles(content: &str) -> Vec<String> {
    const PAIRS: &[(char, char)] = &[
        ('「', '」'),
        ('『', '』'),
        ('《', '》'),
        ('"', '"'),
        ('\'', '\''),
    ];
    let mut titles = Vec::new();
    for &(open, close) in PAIRS {
        let mut rest = content;
        while let Some(start) = rest.find(open) {
            let after = &rest[start + open.len_utf8()..];
            if let Some(end) = after.find(close) {
                let title = after[..end].trim();
                // 过滤单字虚词（「好」/「不要」等多为正文引用，不是实体标题）
                if title.chars().count() >= 2 && !titles.iter().any(|t: &String| t == title) {
                    titles.push(title.to_string());
                }
                rest = &after[end + close.len_utf8()..];
            } else {
                break;
            }
        }
    }
    titles
}
/// RFC3339 时间戳是否落在 [turn_start, now] 内；解析失败则按字符串兜底比较。
pub(crate) fn touched_this_turn(ts: &str, turn_start: &chrono::DateTime<chrono::Utc>) -> bool {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
        dt.with_timezone(&chrono::Utc) >= *turn_start
    } else {
        ts >= turn_start.to_rfc3339().as_str()
    }
}
/// 核验写声明：标题 → 查库（页/待办）→ 卡本轮时间窗。
/// - 页或待办存在且创建/更新在本轮 → 写定；
/// - 只在待确认草稿参数里 → 草稿中；
/// - 查无此页、是旧页面 → 未证实；
/// - 未指明任何标题 → 本轮有任意新增页/待办即放行，否则未证实。
pub(crate) fn verify_claimed_writes(
    store: &Store,
    conversation_id: &str,
    titles: &[String],
    turn_start: chrono::DateTime<chrono::Utc>,
) -> Result<ClaimVerdict> {
    let mut draft_only = Vec::new();
    let mut unproven = Vec::new();
    // 本轮待确认草稿的参数快照（标题键因动作而异，兜底用全文包含判断）
    let pendings = store.pending_actions_for_conversation(conversation_id)?;

    let check_title = |title: &str| -> Result<Option<bool>> {
        if let Some(page) = store.find_wiki_page_by_title(title)? {
            // 旧页面≠本轮写入：创建和更新都要卡时间窗（补充修订走 updated_at）。
            if touched_this_turn(&page.created_at, &turn_start)
                || touched_this_turn(&page.updated_at, &turn_start)
            {
                return Ok(Some(true));
            }
            return Ok(Some(false));
        }
        for todo in store.list_todos(None)? {
            if todo.title.trim() == title.trim()
                && (touched_this_turn(&todo.created_at, &turn_start)
                    || touched_this_turn(&todo.updated_at, &turn_start))
            {
                return Ok(Some(true));
            }
        }
        Ok(None)
    };
    let in_draft = |title: &str| -> bool {
        pendings.iter().any(|p| {
            serde_json::from_str::<serde_json::Value>(&p.args_json)
                .ok()
                .and_then(|v| {
                    v.get("title")
                        .or_else(|| v.get("project_name"))
                        .and_then(|t| t.as_str())
                        .map(|t| t.trim() == title.trim())
                        .or_else(|| {
                            // 兜底：标题出现在参数全文里（如 content_md 首行）
                            Some(p.args_json.contains(title))
                        })
                })
                .unwrap_or(false)
        })
    };

    if titles.is_empty() {
        // 裸声明（“存好了”但没点名）：本轮有任意新增/更新即放行，否则未证实。
        // NOTE：直接读库扫描，failover 重试也不丢；个人库规模下全量可接受。
        let mut any = false;
        for page in store.list_wiki_pages(None, None)? {
            if touched_this_turn(&page.created_at, &turn_start)
                || touched_this_turn(&page.updated_at, &turn_start)
            {
                any = true;
                break;
            }
        }
        if !any {
            for todo in store.list_todos(None)? {
                if touched_this_turn(&todo.created_at, &turn_start)
                    || touched_this_turn(&todo.updated_at, &turn_start)
                {
                    any = true;
                    break;
                }
            }
        }
        if any {
            return Ok(ClaimVerdict::Clean);
        }
        return Ok(ClaimVerdict::Unproven(vec!["（未指明页面）".to_string()]));
    }

    for title in titles {
        match check_title(title)? {
            Some(true) => {}
            Some(false) => unproven.push(format!("《{title}》是旧页面，本轮未写入")),
            None if in_draft(title) => draft_only.push(format!("《{title}》还在待确认")),
            None => unproven.push(format!("《{title}》库中无此页")),
        }
    }
    if !unproven.is_empty() {
        Ok(ClaimVerdict::Unproven(unproven))
    } else if !draft_only.is_empty() {
        Ok(ClaimVerdict::DraftOnly(draft_only))
    } else {
        Ok(ClaimVerdict::Clean)
    }
}
/// 兜底清理：解析器没认出来的工具协议残留，绝不直接见用户。
/// 先删整段 `<tool_call>…</tool_call>` 残块，再按行过滤其他协议行
///（`[工具调用]` / DSML invoke 等；协议要求调用独占一行，按行删安全）。
pub(crate) fn strip_tool_protocol_residue(content: &str) -> String {
    const CLOSE: &str = "</tool_call>";
    let mut cur = content.to_string();
    loop {
        let Some(start) = cur.find("<tool_call") else {
            break;
        };
        let Some(rel_end) = cur[start..].find(CLOSE) else {
            break;
        };
        cur.replace_range(start..start + rel_end + CLOSE.len(), "");
    }
    cur.lines()
        .filter(|line| {
            !(line.contains("<tool_call")
                || line.contains(CLOSE)
                || line.contains("<arg_key>")
                || line.contains("</arg_key>")
                || line.contains("<arg_value>")
                || line.contains("</arg_value>")
                || line.contains(TOOL_CALL_MARKER)
                || line.contains("invoke name=")
                || line.contains("<|invoke")
                || line.contains("parameter name="))
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}
pub(crate) fn parse_dsml_tool_calls(content: &str) -> Vec<ToolCall> {
    if !content.contains("DSML") {
        return Vec::new();
    }
    let Some(invoke) = content.lines().find(|line| line.contains("invoke name=")) else {
        return Vec::new();
    };
    let Some(name) = quoted_attribute(invoke, "name") else {
        return Vec::new();
    };
    let mut arguments = serde_json::Map::new();
    for line in content
        .lines()
        .filter(|line| line.contains("parameter name="))
    {
        let Some(mut parameter) = quoted_attribute(line, "name") else {
            continue;
        };
        if name == "record_event" && parameter == "content" {
            parameter = "raw_text".to_string();
        }
        let value = line
            .split_once('>')
            .map(|(_, value)| value)
            .unwrap_or("")
            .split("<|")
            .next()
            .unwrap_or("")
            .split("| DSML")
            .next()
            .unwrap_or("")
            .trim();
        if !value.is_empty() {
            arguments.insert(parameter, Value::String(value.to_string()));
        }
    }
    vec![ToolCall::new(name, Value::Object(arguments))]
}
fn quoted_attribute(line: &str, attribute: &str) -> Option<String> {
    let marker = format!("{attribute}=\"");
    let value = line.split_once(&marker)?.1;
    Some(value.split_once('"')?.0.to_string())
}
/// Some OpenAI-compatible gateways expose tool calls as a DSML/XML-like text
/// protocol instead of structured `tool_calls`. Never persist that protocol in
/// the user-facing conversation. Keep surrounding natural-language text.
pub(crate) fn strip_dsml_artifacts(content: &str) -> String {
    let mut kept = Vec::new();
    let mut in_dsml = false;
    for line in content.lines() {
        let is_dsml = line.contains("DSML")
            || line.contains("<|invoke")
            || line.contains("</|invoke")
            || line.contains("<|parameter")
            || line.contains("</|parameter")
            || line.contains("invoke name=")
            || line.contains("parameter name=");
        if is_dsml {
            in_dsml = true;
            continue;
        }
        if in_dsml {
            // Continue only across adjacent protocol-looking lines. This
            // avoids dropping a normal answer that follows a malformed block.
            let protocolish = line.contains("<|")
                || line.contains("|>")
                || line.contains("parameter")
                || line.contains("invoke");
            if protocolish {
                continue;
            }
            in_dsml = false;
        }
        kept.push(line);
    }
    kept.join("\n").trim().to_string()
}
pub(crate) fn extract_one_tool_call(content: &str) -> Option<(String, ToolCall)> {
    let idx = content.find(TOOL_CALL_MARKER)?;
    let after = &content[idx + TOOL_CALL_MARKER.len()..];
    let line_end = after.find('\n').unwrap_or(after.len());
    let line = after[..line_end].trim();
    let json_start = line.find('{')?;
    let json_str = &line[json_start..];

    // 平衡大括号，定位完整 JSON
    let mut depth = 0i32;
    let mut end = 0usize;
    for (i, ch) in json_str.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            end = i + ch.len_utf8();
            break;
        }
    }
    if end == 0 {
        return None;
    }
    let parsed: Value = serde_json::from_str(&json_str[..end]).ok()?;
    let call = ToolCall::from_json(&parsed)?;

    // 移除标记所在整行
    let line_start = content[..idx].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_end_full = content[idx..]
        .find('\n')
        .map(|i| idx + i + 1)
        .unwrap_or(content.len());
    let mut cleaned = String::with_capacity(content.len());
    cleaned.push_str(&content[..line_start]);
    cleaned.push_str(&content[line_end_full..]);
    Some((cleaned.trim().to_string(), call))
}
