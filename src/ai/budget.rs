//! A single request boundary counts serialized input, tools and output reserve.
//! Known tokenizer families use local BPE; unknown families use UTF-8 bytes as
//! a conservative upper estimate. Neither guesses a proxy's real model window.
use anyhow::Result;
use serde_json::Value;

#[cfg(test)]
#[path = "budget/context_overflow_tests.rs"]
mod context_overflow_tests;
#[cfg(test)]
mod exchange_tests;
mod exchanges;
mod report;
#[cfg(test)]
#[path = "budget/tests.rs"]
mod tests;
#[cfg(test)]
#[path = "budget/tool_result_prune_tests.rs"]
mod tool_result_prune_tests;
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

    /// 找到还能被截断的最大补充材料（TaskSupplement）的下标。
    /// 只碰纯文本（Plain）消息：多模态数组与工具消息不动。
    fn largest_trimable_supplement(&self, messages: &[Value]) -> Option<usize> {
        let mut best: Option<(usize, usize)> = None;
        for (i, policy) in self.messages.iter().enumerate() {
            if policy.segment.purpose != SegmentPurpose::TaskSupplement
                || policy.native_exchange != exchanges::MessageExchange::Plain
            {
                continue;
            }
            let trimable = messages
                .get(i)
                .and_then(|m| m["content"].as_str())
                .and_then(supplement_halved)
                .map(|halved| halved.chars().count())
                .unwrap_or(0);
            if trimable > 0 && best.is_none_or(|(_, n)| trimable > n) {
                best = Some((i, trimable));
            }
        }
        best.map(|(i, _)| i)
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
            // 碍轮之前先截断补充材料（2026-10-18 事故「预算前移」）：补充材料
            // 是唯一可以无损重取的必要段（原文保存在输入队列记录），手机端曾
            // 一次挂 27 页全文导致整轮被拒、用户看不到任何回答。截断挂载并
            // 如实声明优于拒绝整轮；每次砰掉最大的超限材料的一半，直到回到
            // 预算内或材料已到保留下限。
            let supplement_index = {
                let messages = request["messages"].as_array().expect("request messages");
                self.largest_trimable_supplement(messages)
            };
            if let Some(index) = supplement_index {
                let messages = request["messages"]
                    .as_array_mut()
                    .expect("request messages");
                let content = messages[index]["content"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                if let Some(halved) = supplement_halved(&content) {
                    messages[index]["content"] = serde_json::Value::String(halved.clone());
                    if let Some(policy) = self.messages.get_mut(index) {
                        policy.segment.version = crate::storage::knowledge::content_hash(&halved);
                    }
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
            // 10-19 档案 P0-b（根因 A）：最近两个交换不可裁剪。只保当前轮
            // 时模型不知道上一轮刚完成的承诺（出事的 340 条会话里 preselection
            // 与砍轮只剩 7 条，「invisible turn」直接 completed 幻觉）。宁可
            // 在此 fail-fast 走诚实失败，也不交付一个注定失忆的 run。
            if turns.len() <= 2 {
                return Err(RequestBudgetError(format!("请求输入约 {count} token，超过当前输入预算 {limit}（窗口 {window}，输出预留 {output}）；最近两轮对话不可省略，无法再压缩——请检查 Provider 窗口或减少附带上下文")).into());
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

/// 补充材料截断声明的固定尾巴（strip 用，勿改动文案）。
const SUPPLEMENT_HALVED_SUFFIX: &str = "……\n【补充材料因超过请求预算被截断；以上只是部分内容，不得当作全文使用。需要完整材料请缩小范围或分批重新提供。】";

/// 超预算时截断挂载：把最大的补充材料砰半后（含截断声明）返回。
/// 已到保留下限（砰半后不足 2000 字符）则返回 None，交给后续路径处理。
fn supplement_halved(content: &str) -> Option<String> {
    let body = content
        .strip_suffix(SUPPLEMENT_HALVED_SUFFIX)
        .unwrap_or(content);
    let total = body.chars().count();
    if total / 2 < 2000 {
        return None;
    }
    let head: String = body.chars().take(total / 2).collect();
    Some(format!("{head}{SUPPLEMENT_HALVED_SUFFIX}"))
}

/// 内容启发式估算的安全系数：所有比率均在样本基准上加 12% 余量，
/// 保证估算偏低（宁高不低红线）。
const HEURISTIC_SAFETY: f64 = 1.12;
/// ASCII（英文/JSON/代码，工具 schema 主体）：约 3.4 bytes/token。
/// 基准：34,204 字节 Gitea schema → ~11,200 估算（真实 ~9-12K）。
const ASCII_CHARS_PER_TOKEN: f64 = 3.4;
/// CJK 每字符 token 系数：真实 deepseek/qwen 中文约 0.6~0.8 token/字，
/// 取 2.0 保守放大 2.5~3 倍。基准：1000 汉字 → 2240 估算（验收 [1.8K,2.4K]）。
const CJK_TOKENS_PER_CHAR: f64 = 2.0;

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
        heuristic_tokens(text)
    }
}

/// 非 tiktoken 族的分层估算（L1/L2 合一：族识别已无增量信息，
/// 内容启发式按字符类别分别计费）：
/// - CJK 按字符 × 2.0；
/// - ASCII 按字符 ÷ 3.4（JSON/代码/英文经验 BPE 比率）；
/// - 其余字符（带附加符号拉丁文、emoji 等低频）按 1 字/1 token 保守计；
/// - 全部乘 `HEURISTIC_SAFETY`。单次 O(n) 扫描。
fn heuristic_tokens(text: &str) -> usize {
    let mut ascii = 0usize;
    let mut cjk = 0usize;
    let mut other = 0usize;
    for ch in text.chars() {
        if is_cjk(ch) {
            cjk += 1;
        } else if ch.is_ascii() {
            ascii += 1;
        } else {
            other += 1;
        }
    }
    let estimate =
        ascii as f64 / ASCII_CHARS_PER_TOKEN + cjk as f64 * CJK_TOKENS_PER_CHAR + other as f64;
    (estimate * HEURISTIC_SAFETY).ceil() as usize
}

fn is_cjk(ch: char) -> bool {
    matches!(ch as u32,
        0x4E00..=0x9FFF   // CIR 汉字
        | 0x3400..=0x4DBF // 扩展 A
        | 0x3000..=0x303F // CJK 标点
        | 0x3040..=0x30FF // 日文假名
        | 0xAC00..=0xD7AF // 谚文
        | 0xF900..=0xFAFF // 兼容汉字
    )
}
#[cfg(test)]
#[cfg(test)]

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
