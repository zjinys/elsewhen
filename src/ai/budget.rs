//! A single request boundary counts serialized input, tools and output reserve.
//! Known tokenizer families use local BPE; unknown families use UTF-8 bytes as
//! a conservative upper estimate. Neither guesses a proxy's real model window.
use anyhow::Result;
use serde_json::Value;

#[cfg(test)]
mod exchange_tests;
mod exchanges;
mod report;
use super::memory::{
    ContextSegment, OmissionPolicy, SegmentOrigin, SegmentPurpose, SelectionOmission,
};
pub use report::{BudgetReport, SegmentBudget};

#[derive(Debug)]
pub struct RequestBudgetError(pub String);

impl std::fmt::Display for RequestBudgetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for RequestBudgetError {}

/// 早期工具结果的占位文本（P1 上下文管理：保留最近 3 次，旧的换占位）。
/// 工具结果是可再生内容——重新调用即可取回；对白模型要自明发生了什么。
pub(crate) const PRUNED_TOOL_RESULT_PLACEHOLDER: &str =
    "[此前的工具结果已因上下文预算省略；如当前回答依赖其中的信息，请重新调用相应工具。]";

/// 服务端"输入超长"的识别：各家网关话术不同，匹配 code 与 message 文本
/// （吸收 AIcove isContextOverflow 表；200/2xx 显然不算）。
/// 只在明确的客户端错误码上发表意见，避免把网络错误误判成超长。
pub(crate) fn is_context_overflow(status: u16, body: &Value) -> bool {
    if !matches!(status, 400 | 413 | 422) {
        return false;
    }
    let code = body["error"]["code"].as_str().unwrap_or("");
    let message = body["error"]["message"]
        .as_str()
        // 有的网关把整个 error 放成字符串，或把说明放平级 message 字段
        .or_else(|| body["error"].as_str())
        .or_else(|| body["message"].as_str())
        .unwrap_or("");
    let code = code.to_ascii_lowercase();
    let message = message.to_ascii_lowercase();
    code.contains("context_length_exceeded")
        || [
            "maximum context length",
            "prompt is too long",
            "input token limit",
            "input tokens exceed",
            "too many tokens",
            "exceeds the context",
            "context window exceeded",
            "上下文超限",
            "上下文过长",
            "上下文长度超过",
        ]
        .iter()
        .any(|needle| message.contains(needle))
}

pub(crate) fn validate_context_exchanges(messages: &[super::memory::ContextMessage]) -> Result<()> {
    let exchanges: Vec<_> = messages
        .iter()
        .map(exchanges::MessageExchange::from_context)
        .collect();
    exchanges::validate(&exchanges)
}

pub(crate) fn input_limit(window: usize, output: usize) -> Result<usize> {
    let margin = (window / 20).clamp(256, 4096);
    if output == 0 || output >= window.saturating_sub(margin) {
        return Err(RequestBudgetError(
            "模型输出预留大于上下文窗口，请调整 Provider 的上下文窗口或输出上限".into(),
        )
        .into());
    }
    Ok(window - output - margin)
}

/// 一次装配的降级状态：早期工具结果是否已占位化（幂等保护）。
#[derive(Default)]
struct PruneState {
    done: bool,
    pruned_count: usize,
}

#[derive(Clone, Default)]
struct MessagePolicy {
    optional_background: bool,
    time_reminder: bool,
    time_anchor: Option<String>,
    native_exchange: exchanges::MessageExchange,
    segment: ContextSegment,
    report_id: usize,
}

fn segment_policy(
    message: &super::memory::ContextMessage,
    index: usize,
    latest_user: Option<usize>,
) -> ContextSegment {
    let mut segment = message.segment.clone();
    // Content can change when current-time blocks refresh or text protocols are normalized.
    segment.version = crate::storage::knowledge::content_hash(&message.content);
    if message.time_reminder || message.sampled_time.is_some() {
        segment.purpose = SegmentPurpose::TimeContext;
        segment.origin = SegmentOrigin::Clock;
    } else if Some(index) == latest_user {
        segment.purpose = SegmentPurpose::CurrentInput;
        segment.omission = OmissionPolicy::Required;
    } else if message.role != "system" && latest_user.is_some_and(|latest| index < latest) {
        segment.omission = OmissionPolicy::WholeEarlierTurn;
        if message.role == "user" {
            segment.purpose = SegmentPurpose::History;
        }
    }
    if message.optional_background && message.role == "system" {
        segment.omission = OmissionPolicy::OptionalBackground;
    }
    segment
}

/// Request-only metadata follows message mutations and smaller-window retries.
/// It is never added to the serialized payload.
pub(crate) struct RequestPolicies {
    messages: Vec<MessagePolicy>,
    history_omitted: bool,
    next_report_id: usize,
    selection_omissions: Vec<SelectionOmission>,
    /// 早期工具结果占位化状态（fit 内幂等，只试一轮防死循环）。
    pub(crate) pruned_tool_results: usize,
    tool_results_pruned: bool,
}

impl RequestPolicies {
    pub(crate) fn from_context(messages: &[super::memory::ContextMessage]) -> Self {
        let covered = super::time_context::current_anchor_index(messages);
        let latest_user = messages.iter().rposition(|m| m.role == "user");
        Self {
            pruned_tool_results: 0,
            tool_results_pruned: false,
            messages: messages
                .iter()
                .enumerate()
                .map(|(index, m)| MessagePolicy {
                    optional_background: m.optional_background && m.role == "system",
                    time_reminder: m.time_reminder,
                    native_exchange: exchanges::MessageExchange::from_context(m),
                    report_id: index,
                    segment: segment_policy(m, index, latest_user),
                    time_anchor: (Some(index) != covered)
                        .then(|| {
                            super::time_context::reminder_for(m, &super::time_context::SystemClock)
                        })
                        .flatten()
                        .map(|hint| hint.content),
                })
                .collect(),
            history_omitted: false,
            next_report_id: messages.len(),
            selection_omissions: messages
                .iter()
                .flat_map(|m| m.selection_omissions.clone())
                .collect(),
        }
    }

    fn plain(request: &Value) -> Self {
        let latest_user = request["messages"]
            .as_array()
            .and_then(|messages| messages.iter().rposition(|m| m["role"] == "user"));
        Self {
            pruned_tool_results: 0,
            tool_results_pruned: false,
            messages: request["messages"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
                .map(|(index, message)| MessagePolicy {
                    native_exchange: exchanges::MessageExchange::from_value(message),
                    report_id: index,
                    segment: segment_policy(
                        &super::memory::ContextMessage::new(
                            message["role"].as_str().unwrap_or(""),
                            message["content"].as_str().unwrap_or(""),
                        ),
                        index,
                        latest_user,
                    ),
                    ..Default::default()
                })
                .collect(),
            history_omitted: false,
            next_report_id: request["messages"].as_array().map_or(0, Vec::len),
            selection_omissions: Vec::new(),
        }
    }

    fn insert(
        &mut self,
        messages: &mut Vec<Value>,
        index: usize,
        message: Value,
        mut policy: MessagePolicy,
    ) {
        policy.report_id = self.next_report_id;
        self.next_report_id += 1;
        policy.segment = ContextSegment::new(
            if policy.time_reminder {
                SegmentPurpose::TimeContext
            } else {
                SegmentPurpose::BudgetNotice
            },
            if policy.time_reminder {
                SegmentOrigin::Clock
            } else {
                SegmentOrigin::Application
            },
            None,
            message["content"].as_str().unwrap_or(""),
            OmissionPolicy::Required,
        );
        messages.insert(index, message);
        self.messages.insert(index, policy);
    }

    pub(crate) fn fit(
        &mut self,
        model: &str,
        request: &mut Value,
        window: usize,
        output: usize,
    ) -> Result<usize> {
        let limit = input_limit(window, output)?;
        if request["messages"].as_array().map(Vec::len) != Some(self.messages.len()) {
            return Err(RequestBudgetError("请求上下文元信息与消息数量不一致".into()).into());
        }
        // Reject malformed history before optional removal or turn trimming can
        // hide it. Identity metadata survives provider-specific serialization.
        exchanges::validate(self.messages.iter().map(|m| &m.native_exchange))?;
        if input_tokens(model, request) > limit
            && self.messages.iter().any(|m| m.optional_background)
        {
            let messages = request["messages"]
                .as_array_mut()
                .expect("request messages");
            for i in (0..self.messages.len()).rev() {
                if self.messages[i].optional_background {
                    messages.remove(i);
                    self.messages.remove(i);
                }
            }
            self.insert(messages, 1.min(messages.len()), serde_json::json!({"role":"system", "content":"可选的跨对话背景因请求预算已省略；不要假装记得未提供的背景。"}), MessagePolicy::default());
        }
        loop {
            // History removal may expose a user that previously needed no
            // reminder. Give the first retained user its own true timestamp.
            // （块作用域：量长度之前释放 request 的可变借用）
            {
                let messages = request["messages"]
                    .as_array_mut()
                    .expect("request messages");
                if let Some(first) = messages.iter().position(|m| m["role"] == "user") {
                    if !self
                        .messages
                        .get(first + 1)
                        .is_some_and(|m| m.time_reminder)
                    {
                        if let Some(anchor) = self.messages[first].time_anchor.clone() {
                            self.insert(
                                messages,
                                first + 1,
                                serde_json::json!({"role":"system", "content":anchor}),
                                MessagePolicy {
                                    time_reminder: true,
                                    ..Default::default()
                                },
                            );
                        }
                    }
                }
            } // 释放 request 可变借用
            let count = input_tokens(model, request);
            if count <= limit {
                exchanges::validate(self.messages.iter().map(|m| &m.native_exchange))?;
                return Ok(count);
            }
            // 砍轮之前先瘦身可再生内容：早期工具结果换占位（保留最近 3 次）。
            // 工具结果重新调用即可取回，比丢对话历史便宜；消息本身留在位上，
            // tool_call_id 配对不动，exchange 校验不受影响。占位后若无净收益
            // （工具结果本来就小），标记完成并落到砍轮路径，防死循环。
            if !self.tool_results_pruned {
                self.tool_results_pruned = true;
                let before = count;
                let pruned = {
                    let messages = request["messages"]
                        .as_array_mut()
                        .expect("request messages");
                    prune_old_tool_results(messages, 3)
                };
                self.pruned_tool_results = pruned;
                if pruned > 0 && input_tokens(model, request) < before {
                    continue;
                }
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
            // Remove whole earlier user/assistant/tool groups. Required system
            // evidence survives; temporal hints follow their owner instead.
            let (start, end) = (turns[0], turns[1]);
            let old = messages.drain(start..end).collect::<Vec<_>>();
            let policies = self.messages.drain(start..end).collect::<Vec<_>>();
            let retained: Vec<_> = old
                .into_iter()
                .zip(policies)
                .filter(|(m, p)| m["role"] == "system" && !p.time_reminder)
                .collect();
            let (retained_messages, retained_policies): (Vec<_>, Vec<_>) =
                retained.into_iter().unzip();
            messages.splice(start..start, retained_messages);
            self.messages.splice(start..start, retained_policies);
            if !self.history_omitted {
                self.insert(messages, 1.min(messages.len()), serde_json::json!({"role":"system", "content":"较早对话因请求预算已省略；不得假装记得省略内容。如当前问题依赖缺失事实，请明确说明。"}), MessagePolicy::default());
                self.history_omitted = true;
            }
        }
    }
}

pub fn fit_request_with_background(
    model: &str,
    request: &mut Value,
    window: usize,
    output: usize,
    optional_indices: &mut Vec<usize>,
) -> Result<usize> {
    let mut policies = RequestPolicies::plain(request);
    for &index in optional_indices.iter() {
        let policy = policies
            .messages
            .get_mut(index)
            .ok_or_else(|| RequestBudgetError("可选上下文索引越界".into()))?;
        policy.optional_background = true;
        policy.segment.omission = OmissionPolicy::OptionalBackground;
    }
    let result = policies.fit(model, request, window, output);
    *optional_indices = policies
        .messages
        .iter()
        .enumerate()
        .filter_map(|(i, p)| p.optional_background.then_some(i))
        .collect();
    result
}

/// 把除最近 `keep` 个之外的 role=="tool" 消息内容换成占位文本。
/// 返回实际替换的数量；替换只针对 string content（OpenAI wire 格式）。
fn prune_old_tool_results(messages: &mut [Value], keep: usize) -> usize {
    let tool_indices: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, m)| m["role"].as_str() == Some("tool"))
        .map(|(i, _)| i)
        .collect();
    if tool_indices.len() <= keep {
        return 0;
    }
    let cut = tool_indices.len() - keep;
    let mut replaced = 0;
    for &i in &tool_indices[..cut] {
        let already_pruned =
            messages[i]["content"].as_str() == Some(PRUNED_TOOL_RESULT_PLACEHOLDER);
        if already_pruned {
            continue;
        }
        if let Some(content) = messages[i].get_mut("content").filter(|c| c.is_string()) {
            *content = Value::String(PRUNED_TOOL_RESULT_PLACEHOLDER.into());
            replaced += 1;
        }
    }
    replaced
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
#[cfg(test)]
mod context_overflow_tests {
    use super::*;

    #[test]
    fn recognizes_known_gateway_wordings() {
        let openai =
            serde_json::json!({"error":{"code":"context_length_exceeded","message":"..." }});
        let anthropic = serde_json::json!({"error":{"message":"prompt is too long: 200000 tokens > 100000 maximum"}});
        let ollamaish = serde_json::json!({"error":"input token limit exceeded"});
        let chinese = serde_json::json!({"error":{"message":"上下文长度超过限制"}});
        for body in [openai, anthropic, ollamaish, chinese] {
            assert!(is_context_overflow(400, &body), "未识别: {body}");
        }
    }

    #[test]
    fn ignores_other_errors_and_statuses() {
        let rate = serde_json::json!({"error":{"message":"rate limit exceeded"}});
        let tool_err = serde_json::json!({"error":{"message":"prompt is too long but impossible"}});
        assert!(!is_context_overflow(400, &rate));
        // 仅在 400/413/422 判定
        assert!(!is_context_overflow(
            500,
            &serde_json::json!({"error":{"message":"maximum context length"}})
        ));
        assert!(!is_context_overflow(429, &tool_err));
        // 网关 5xx 带超长文案也不误判为重试信号
        assert!(!is_context_overflow(
            502,
            &serde_json::json!({"error":{"message":"maximum context length"}})
        ));
    }
}

#[cfg(test)]
mod tool_result_prune_tests {
    use super::*;

    fn tool_msg(id: &str, size: usize) -> Value {
        serde_json::json!({"role":"tool","tool_call_id":id,"content":"x".repeat(size)})
    }

    #[test]
    fn prunes_all_but_recent_three() {
        let mut messages = vec![
            serde_json::json!({"role":"user","content":"q"}),
            tool_msg("c1", 1000),
            serde_json::json!({"role":"assistant","content":"a1"}),
            tool_msg("c2", 1000),
            tool_msg("c3", 10),
            tool_msg("c4", 10),
            tool_msg("c5", 10),
        ];
        let replaced = prune_old_tool_results(&mut messages, 3);
        assert_eq!(replaced, 2, "7 条里应替换最早的 2 条工具结果");
        assert_eq!(
            messages[1]["content"].as_str().unwrap(),
            PRUNED_TOOL_RESULT_PLACEHOLDER
        );
        assert_eq!(
            messages[3]["content"].as_str().unwrap(),
            PRUNED_TOOL_RESULT_PLACEHOLDER
        );
        // 最近 3 个原样保留；tool_call_id 配对不动（exchange 校验依赖）
        assert_eq!(messages[4]["tool_call_id"], "c3");
        assert_eq!(messages[6]["content"].as_str().map(str::len), Some(10));
        assert_eq!(
            prune_old_tool_results(&mut messages, 3),
            0,
            "幂等：再跑不再替换"
        );
    }

    /// 场景一：早期工具结果占位化即足以回到预算内——第一轮对话必须完整保留。
    #[test]
    fn pruning_old_tool_results_keeps_history_intact() {
        let mut request = serde_json::json!({
            "messages": [
                {"role":"user","content":"第一问"},
                {"role":"assistant","content":"第一答","tool_calls":[{"id":"c0","type":"function","function":{"name":"t","arguments":"{}"}}]},
                tool_msg("c0", 5000),
                {"role":"user","content":"第二问"},
                {"role":"assistant","content":"第二答","tool_calls":[{"id":"c1","type":"function","function":{"name":"t","arguments":"{}"}}]},
                tool_msg("c1", 6000),
                {"role":"user","content":"第三问"},
                {"role":"assistant","content":"第三答","tool_calls":[{"id":"c2","type":"function","function":{"name":"t","arguments":"{}"}}]},
                tool_msg("c2", 6000),
                {"role":"user","content":"第四问，当前问题必须完整保留"},
            ]
        });
        let first_user = request["messages"][0]["content"].clone();
        let last_user = request["messages"][9]["content"].clone();
        let mut policies = RequestPolicies::plain(&request);
        let count = policies
            .fit("unknown", &mut request, 14000, 256)
            .expect("占位化后应回到预算内");
        assert!(count > 0);
        assert_eq!(
            policies.pruned_tool_results, 1,
            "只有 c0（最早一条）应被占位化"
        );
        let msgs = request["messages"].as_array().unwrap();
        // 第一轮完整保留（先于砍轮被应用）；当前问题在
        assert!(
            msgs.iter()
                .any(|m| m["role"] == "user" && m["content"] == first_user),
            "占位化即足够时不得砍历史轮"
        );
        assert!(
            msgs.iter()
                .any(|m| m["role"] == "user" && m["content"] == last_user)
        );
        // 近 3 个身位规则：c1、c2 原样；c0 占位
        let c0 = msgs.iter().find(|m| m["tool_call_id"] == "c0").unwrap();
        assert_eq!(c0["content"].as_str(), Some(PRUNED_TOOL_RESULT_PLACEHOLDER));
        let c1 = msgs.iter().find(|m| m["tool_call_id"] == "c1").unwrap();
        assert_eq!(c1["content"].as_str().map(str::len), Some(6000));
        let c2 = msgs.iter().find(|m| m["tool_call_id"] == "c2").unwrap();
        assert_eq!(c2["content"].as_str().map(str::len), Some(6000));
    }

    /// 场景二：占位化不够仍超预算 → 回落到整轮删除路径，且如实计数。
    #[test]
    fn when_pruning_is_not_enough_turn_dropping_takes_over() {
        let mut request = serde_json::json!({
            "messages": [
                {"role":"user","content":"第一问"},
                {"role":"assistant","content":"第一答","tool_calls":[{"id":"c0","type":"function","function":{"name":"t","arguments":"{}"}}]},
                tool_msg("c0", 5000),
                {"role":"user","content":"第二问"},
                {"role":"assistant","content":"第二答","tool_calls":[{"id":"c1","type":"function","function":{"name":"t","arguments":"{}"}}]},
                tool_msg("c1", 6000),
                {"role":"user","content":"第三问"},
                {"role":"assistant","content":"第三答","tool_calls":[{"id":"c2","type":"function","function":{"name":"t","arguments":"{}"}}]},
                tool_msg("c2", 6000),
                {"role":"user","content":"第四问，当前问题必须完整保留"},
            ]
        });
        let first_user = request["messages"][0]["content"].clone();
        let last_user = request["messages"][9]["content"].clone();
        let mut policies = RequestPolicies::plain(&request);
        policies
            .fit("unknown", &mut request, 8000, 256)
            .expect("砍轮后应回到预算内");
        let msgs = request["messages"].as_array().unwrap();
        // 第一问被整轮删除（占位不够），当前问题与省略提示在
        assert!(
            !msgs
                .iter()
                .any(|m| m["role"] == "user" && m["content"] == first_user),
            "预算太紧时应砍整轮"
        );
        assert!(
            msgs.iter()
                .any(|m| m["role"] == "user" && m["content"] == last_user)
        );
        assert!(msgs.iter().any(|m| {
            m["role"] == "system"
                && m["content"]
                    .as_str()
                    .unwrap_or("")
                    .contains("较早对话因请求预算已省略")
        }));
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
    } else if m.starts_with("glm")
        || m.starts_with("qwen")
        || m.starts_with("kimi")
        || m.starts_with("moonshot")
        || m.starts_with("doubao")
        || m.starts_with("claude")
        || m.starts_with("gemini")
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
    RequestPolicies::plain(request).fit(model, request, window, output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_hints_follow_removed_turns_and_reanchor_after_smaller_retry() {
        use crate::ai::memory::ContextMessage;
        use crate::ai::time_context::{SystemClock, enrich_history, reminder_for};
        let user = |time: &str, content: String| {
            let mut message = ContextMessage::new("user", content);
            message.message_time = Some(chrono::DateTime::parse_from_rfc3339(time).unwrap());
            message
        };
        let oldest = user("2026-09-30T10:00:00Z", "old".repeat(4000));
        let middle = user("2026-09-30T10:01:00Z", "middle".repeat(300));
        let latest = user("2026-09-30T10:02:00Z", "当前问题".into());
        let oldest_hint = reminder_for(&oldest, &SystemClock).unwrap().content;
        let middle_hint = reminder_for(&middle, &SystemClock).unwrap().content;
        let latest_hint = reminder_for(&latest, &SystemClock).unwrap().content;
        let context = enrich_history(
            vec![
                ContextMessage::new("system", "规则"),
                oldest,
                middle,
                latest,
                ContextMessage::new("system", "必要来源"),
                ContextMessage::assistant_with_tool_calls(
                    String::new(),
                    vec![super::super::tool::ToolCall {
                        id: "call_current".into(),
                        name: "search".into(),
                        arguments: serde_json::json!({}),
                        raw_arguments: Some("{}".into()),
                    }],
                    None,
                ),
                ContextMessage::tool_result("call_current".into(), "真实结果".into()),
            ],
            &SystemClock,
        );
        assert_eq!(context.iter().filter(|m| m.time_reminder).count(), 1);
        let mut policies = RequestPolicies::from_context(&context);
        let mut messages: Vec<_> = context
            .iter()
            .map(|m| serde_json::json!({"role":m.role,"content":m.content}))
            .collect();
        let n = messages.len();
        messages[n - 2] = serde_json::json!({"role":"assistant", "tool_calls":[{"id":"call_current","type":"function","function":{"name":"search","arguments":"{}"}}]});
        messages[n - 1] =
            serde_json::json!({"role":"tool", "tool_call_id":"call_current", "content":"真实结果"});
        let exchange = messages[n - 3..].to_vec();
        let mut request = serde_json::json!({"messages":messages});
        policies.fit("unknown", &mut request, 4096, 512).unwrap();
        assert!(!request.to_string().contains(&oldest_hint));
        assert!(
            request["messages"]
                .as_array()
                .unwrap()
                .iter()
                .any(|m| m["content"] == middle_hint)
        );
        policies.fit("unknown", &mut request, 2000, 256).unwrap();
        let messages = request["messages"].as_array().unwrap();
        assert!(
            !messages
                .iter()
                .any(|m| m["content"] == oldest_hint || m["content"] == middle_hint)
        );
        assert!(messages.iter().any(|m| m["content"] == latest_hint));
        assert_eq!(&messages[messages.len() - 3..], exchange.as_slice());
        assert_eq!(messages.len(), policies.messages.len());
        assert_eq!(
            messages
                .iter()
                .filter(|m| m["content"]
                    .as_str()
                    .is_some_and(|c| c.starts_with("较早对话因")))
                .count(),
            1
        );
        assert!(!request.to_string().contains("time_reminder\""));
    }

    #[test]
    fn optional_removal_keeps_time_policy_indices_in_sync() {
        use crate::ai::memory::ContextMessage;
        let mut background = ContextMessage::new("system", "optional".repeat(2000));
        background.optional_background = true;
        let mut hint = ContextMessage::new("system", "<message_time>old hint</message_time>");
        hint.time_reminder = true;
        let context = vec![
            ContextMessage::new("system", "rules"),
            background,
            ContextMessage::new("user", "old".repeat(3000)),
            hint,
            ContextMessage::new("user", "latest"),
            ContextMessage::new("system", "evidence"),
        ];
        let mut policies = RequestPolicies::from_context(&context);
        let mut request = serde_json::json!({"messages":context.iter().map(|m| serde_json::json!({"role":m.role,"content":m.content})).collect::<Vec<_>>()});
        policies.fit("unknown", &mut request, 2048, 256).unwrap();
        assert!(!request.to_string().contains("old hint"));
        assert!(request.to_string().contains("evidence"));
        assert!(request.to_string().contains("latest"));
        policies.fit("unknown", &mut request, 1536, 256).unwrap();
        assert_eq!(
            request["messages"].as_array().unwrap().len(),
            policies.messages.len()
        );
    }

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
        assert!(
            req["messages"][1]["content"]
                .as_str()
                .unwrap()
                .contains("已省略")
        );
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
