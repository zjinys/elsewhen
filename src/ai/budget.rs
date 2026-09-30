//! A single request boundary counts serialized input, tools and output reserve.
//! Known tokenizer families use local BPE; unknown families use UTF-8 bytes as
//! a conservative upper estimate. Neither guesses a proxy's real model window.
use anyhow::Result;
use serde_json::Value;

#[derive(Debug)]
pub struct RequestBudgetError(pub String);

impl std::fmt::Display for RequestBudgetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for RequestBudgetError {}

fn input_limit(window: usize, output: usize) -> Result<usize> {
    let margin = (window / 20).clamp(256, 4096);
    if output == 0 || output >= window.saturating_sub(margin) {
        return Err(RequestBudgetError(
            "模型输出预留大于上下文窗口，请调整 Provider 的上下文窗口或输出上限".into(),
        )
        .into());
    }
    Ok(window - output - margin)
}

/// Indices refer to the initial payload. Clear them before any history removal
/// changes message positions; on a smaller-window retry they must not be reused.
pub fn fit_request_with_background(
    model: &str,
    request: &mut Value,
    window: usize,
    output: usize,
    optional_indices: &mut Vec<usize>,
) -> Result<usize> {
    let limit = input_limit(window, output)?;
    let mut count = input_tokens(model, request);
    if count > limit && !optional_indices.is_empty() {
        let messages = request["messages"]
            .as_array_mut()
            .expect("request messages");
        for index in optional_indices.drain(..).rev() {
            messages.remove(index);
        }
        messages.insert(1.min(messages.len()), serde_json::json!({"role":"system", "content":"可选的跨对话背景因请求预算已省略；不要假装记得未提供的背景。"}));
        count = input_tokens(model, request);
    }
    // If the payload already fits, fit_request cannot shift its indices.
    if count <= limit {
        return Ok(count);
    }
    optional_indices.clear();
    fit_request(model, request, window, output)
}

pub fn text_tokens(model: &str, text: &str) -> usize {
    if model.starts_with("gpt-4o")
        || model.starts_with("gpt-5")
        || model.starts_with("o1")
        || model.starts_with("o3")
        || model.starts_with("o4")
    {
        tiktoken_rs::o200k_base_singleton()
            .encode_ordinary(text)
            .len()
    } else if model.starts_with("gpt-4") {
        tiktoken_rs::cl100k_base_singleton()
            .encode_ordinary(text)
            .len()
    } else {
        text.len()
    }
}
/// 上下文窗口的保守默认（provider 未显式配置时）。
/// 收录门槛与 rikkahub request-dialect 同理：主流模型族 + 官方文档口径，
/// 宁小勿大（偏小只是预算紧，偏大是请求被拒）。未知模型一律 32768。
pub fn default_window(model: &str) -> usize {
    let m = model.to_ascii_lowercase();
    let m = m.rsplit('/').next().unwrap_or(&m); // 兼容 openrouter 风格 vendor/model
    if m.starts_with("gpt-4o") || m.starts_with("gpt-5") {
        65536
    } else if m.starts_with("deepseek") {
        // DeepSeek 官方 API 现役模型 64K 上下文
        65536
    } else if m.starts_with("glm") || m.starts_with("qwen") || m.starts_with("kimi")
        || m.starts_with("moonshot") || m.starts_with("doubao")
        || m.starts_with("claude") || m.starts_with("gemini")
    {
        // 智谱 GLM / 通义千问 / Kimi(月之暗面) / 豆包 / Claude / Gemini 现役均为 128K 档
        131072
    } else {
        32768
    }
}
pub fn input_tokens(model: &str, request: &Value) -> usize {
    // Includes message roles, call IDs/arguments, reasoning, tool schemas and
    // wrapper keys. The extra per-message allowance covers server chat framing.
    text_tokens(model, &request.to_string())
        + request["messages"].as_array().map_or(0, |m| m.len() * 12)
}

pub fn fit_request(
    model: &str,
    request: &mut Value,
    window: usize,
    output: usize,
) -> Result<usize> {
    let limit = input_limit(window, output)?;
    let mut removed = false;
    loop {
        let count = input_tokens(model, request);
        if count <= limit {
            return Ok(count);
        }
        let messages = request["messages"]
            .as_array_mut()
            .expect("request messages");
        let turns: Vec<_> = messages
            .iter()
            .enumerate()
            .filter(|(_, m)| m["role"] == "user")
            .map(|(i, _)| i)
            .collect();
        if turns.len() <= 1 {
            return Err(RequestBudgetError(format!("请求输入约 {count} token，超过当前输入预算 {limit}（窗口 {window}，输出预留 {output}）；当前问题与必要上下文未截断，请检查 Provider 窗口或减少附带上下文")).into());
        }
        // Remove whole earlier user/assistant/tool groups. System instructions,
        // source evidence and the complete current tool exchange survive.
        let (start, end) = (turns[0], turns[1]);
        let old = messages.drain(start..end).collect::<Vec<_>>();
        let retained: Vec<_> = old.into_iter().filter(|m| m["role"] == "system").collect();
        messages.splice(start..start, retained);
        if !removed {
            messages.insert(1.min(messages.len()),serde_json::json!({"role":"system","content":"较早对话因请求预算已省略；不得假装记得省略内容。如当前问题依赖缺失事实，请明确说明。"}));
            removed = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_window_recognizes_mainstream_model_families() {
        assert_eq!(default_window("gpt-4o"), 65536);
        assert_eq!(default_window("deepseek-chat"), 65536);
        assert_eq!(default_window("deepseek-v4.1-flash"), 65536);
        assert_eq!(default_window("glm-5.3-flash"), 131072);
        assert_eq!(default_window("qwen3-235b"), 131072);
        assert_eq!(default_window("kimi-k2"), 131072);
        assert_eq!(default_window("moonshot-v1-128k"), 131072);
        assert_eq!(default_window("doubao-seed-1.6"), 131072);
        assert_eq!(default_window("claude-opus-4-6"), 131072);
        assert_eq!(default_window("gemini-3.0-pro"), 131072);
        // openrouter 风格 vendor 前缀
        assert_eq!(default_window("zai-org/glm-5.3"), 131072);
        // 未知模型保守兜底
        assert_eq!(default_window("llama3-8b"), 32768);
        assert_eq!(default_window("my-finetune-v2"), 32768);
    }

    #[test]
    fn optional_background_is_removed_without_touching_evidence_or_tool_exchange() {
        let evidence = "经核验的原文，不得裁剪。".repeat(15);
        let mut req = serde_json::json!({"messages":[
            {"role":"system","content":"不可变规则"},
            {"role":"system","content":"可省略的其他会话背景".repeat(500)},
            {"role":"user","content":"当前问题"},
            {"role":"system","content":evidence},
            {"role":"assistant","tool_calls":[{"id":"a","function":{"name":"search","arguments":"{}"}}]},
            {"role":"tool","content":"真实执行结果","tool_call_id":"a"}
        ]});
        let protected = req["messages"].as_array().unwrap()[2..].to_vec();
        let mut optional = vec![1];
        let count =
            fit_request_with_background("unknown", &mut req, 4096, 512, &mut optional).unwrap();
        assert!(count <= 4096 - 512 - 256);
        assert!(optional.is_empty());
        assert_eq!(
            &req["messages"].as_array().unwrap()[2..],
            protected.as_slice()
        );
        assert!(req["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains("已省略"));
    }

    #[test]
    fn smaller_retry_removes_only_marked_background() {
        let mut req = serde_json::json!({"messages":[
            {"role":"system","content":"核心规则"},
            {"role":"system","content":"背景".repeat(1000)},
            {"role":"user","content":"当前问题"}
        ]});
        let mut optional = vec![1];
        fit_request_with_background("unknown", &mut req, 16384, 512, &mut optional).unwrap();
        assert_eq!(optional, vec![1]);
        fit_request_with_background("unknown", &mut req, 4096, 512, &mut optional).unwrap();
        assert!(optional.is_empty());
        assert_eq!(req["messages"][2]["content"], "当前问题");
        fit_request_with_background("unknown", &mut req, 2048, 512, &mut optional).unwrap();
        assert_eq!(req["messages"][2]["content"], "当前问题");
    }

    #[test]
    fn budget_errors_are_typed_and_invalid_reserves_do_not_overflow() {
        let mut req =
            serde_json::json!({"messages":[{"role":"user","content":"原文".repeat(2000)}]});
        for (window, output) in [(4096, 512), (4096, 0), (4096, usize::MAX)] {
            let error = fit_request("unknown", &mut req, window, output).unwrap_err();
            assert!(error.downcast_ref::<RequestBudgetError>().is_some());
        }
    }

    #[test]
    fn full_request_counts_tools_and_preserves_current_exchange() {
        let schema = "嵌套参数描述：日期、条件、原文。".repeat(80);
        let mut req = serde_json::json!({"messages":[{"role":"system","content":"系统要求"},{"role":"user","content":"旧话题".repeat(1200)},{"role":"assistant","content":"旧回答"},{"role":"user","content":"当前问题"},{"role":"assistant","content":"","tool_calls":[{"id":"a","function":{"name":"search","arguments":"{}"}}]},{"role":"tool","content":"结果","tool_call_id":"a"}],"tools":[{"parameters":{"description":schema}}]});
        let bare = input_tokens("gpt-4o", &req["messages"]);
        assert!(input_tokens("gpt-4o", &req) > bare);
        let count = fit_request("gpt-4o", &mut req, 4096, 512).unwrap();
        assert!(count + 512 + 256 <= 4096);
        assert!(req.to_string().contains("当前问题"));
        assert!(req.to_string().contains("tool_call_id"));
        assert!(req.to_string().contains("已省略"));
    }
    #[test]
    fn oversized_current_evidence_errors_without_silent_truncation() {
        let mut req = serde_json::json!({"messages":[{"role":"system","content":"原料".repeat(3000)},{"role":"user","content":"请阅读全文"}]});
        let before = req.clone();
        assert!(fit_request("unknown", &mut req, 4096, 512).is_err());
        assert_eq!(req, before);
        assert!(text_tokens("unknown", "中文🙂") >= text_tokens("gpt-4o", "中文🙂"));
    }
}
