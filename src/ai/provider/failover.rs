//! 后台 AI 渠道的顺序 failover 包装。
//!
//! 后台任务（知识消化、周期报告、洞察等）与主对话分流：它们只需要一个
//! `AiProvider`，但单个渠道的 5xx/429/网络错误不应让整条后台链路停摆。
//! 这里把「候选渠道 + 顺序尝试 + 记住上次成功者 + 失败渠道冷却」封装成
//! 一个 provider，调用方无需感知多渠道。

use super::*;
use crate::storage::Store;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

/// 失败渠道冷却状态（进程内）：连续失败次数与下次可试时间。
/// 指数退避 2→4→…→30 分钟封顶；成功即清零。只影响排序/跳过，
/// 所有渠道都冷却时仍然全试（不因为状态就让请求无路可走）。
struct CooldownState {
    consecutive_failures: u32,
    retry_after: std::time::Instant,
}

impl CooldownState {
    fn backoff(failures: u32) -> std::time::Duration {
        const BASE_SECS: u64 = 120;
        const MAX_SECS: u64 = 1800;
        let exp = BASE_SECS
            .saturating_mul(1u64 << failures.saturating_sub(1).min(4))
            .min(MAX_SECS);
        std::time::Duration::from_secs(exp)
    }

    fn record_failure(&mut self) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        self.retry_after = std::time::Instant::now() + Self::backoff(self.consecutive_failures);
    }

    fn record_success(&mut self) {
        *self = CooldownState {
            consecutive_failures: 0,
            retry_after: std::time::Instant::now(),
        };
    }

    fn cooling(&self, now: std::time::Instant) -> bool {
        now < self.retry_after
    }
}

fn cooldown_slot() -> &'static Mutex<Vec<(String, CooldownState)>> {
    static SLOT: OnceLock<Mutex<Vec<(String, CooldownState)>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(Vec::new()))
}

#[cfg(test)]
pub(crate) fn reset_provider_cooldowns() {
    cooldown_slot().lock().unwrap_or_else(|e| e.into_inner()).clear();
}

fn cooling_provider(id: &str, now: std::time::Instant) -> bool {
    cooldown_slot()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .any(|(key, state)| key == id && state.cooling(now))
}

/// 主对话管线用的冷却查询（公开给 generate_pipeline 预扫）。
pub(crate) fn provider_in_cooldown(id: &str, now: std::time::Instant) -> bool {
    cooling_provider(id, now)
}

/// 主对话管线的成功落点：清零冷却。
pub(crate) fn record_channel_success(id: &str) {
    record_provider_success(id);
}

/// 主对话管线的失败落点：累积退避。by-design 只记渠道 id，
/// 不区分错误类别（鉴权失败也退避——换渠道就是唯一出路）。
pub(crate) fn record_channel_failure(id: &str) {
    record_provider_failure(id);
}

fn record_provider_failure(id: &str) {
    let mut slot = cooldown_slot().lock().unwrap_or_else(|e| e.into_inner());
    match slot.iter_mut().find(|(key, _)| key == id) {
        Some((_, state)) => state.record_failure(),
        None => slot.push((
            id.to_string(),
            CooldownState {
                consecutive_failures: 1,
                retry_after: std::time::Instant::now() + CooldownState::backoff(1),
            },
        )),
    }
}

fn record_provider_success(id: &str) {
    cooldown_slot()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter_mut()
        .for_each(|(key, state)| {
            if key == id {
                state.record_success();
            }
        });
}

/// 一个候选渠道。`id` 用于记住上次成功者，`name` 只用于错误信息。
pub(crate) struct FailoverEntry {
    id: String,
    name: String,
    provider: Box<dyn AiProvider>,
}

impl FailoverEntry {
    pub(crate) fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        provider: Box<dyn AiProvider>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            provider,
        }
    }
}

/// 顺序 failover：任一渠道返回完整结果即返回；全部失败时汇总每个渠道的错误。
pub(crate) struct FailoverProvider {
    entries: Vec<FailoverEntry>,
    /// 本链上次成功的下标。后台一个 tick 内多个任务复用同一条链，
    /// 从这里开始试可避免每个任务都先撞已知失败的渠道。
    preferred: AtomicUsize,
}

impl FailoverProvider {
    pub(crate) fn new(entries: Vec<FailoverEntry>) -> Self {
        Self {
            entries,
            preferred: AtomicUsize::new(0),
        }
    }

    fn preferred_entry(&self) -> Result<&FailoverEntry> {
        let len = self.entries.len();
        anyhow::ensure!(len > 0, "没有可用的 AI provider");
        let index = self.preferred.load(Ordering::Relaxed) % len;
        Ok(&self.entries[index])
    }

    /// 尝试顺序：跳过冷却中的渠道（记录冷却错误摘要）；若全部冷却则
    /// 照常全试（冷却只是优先级提示，不是熔断死锁）。
    fn attempt_order(&self) -> (Vec<usize>, Vec<String>) {
        let len = self.entries.len();
        let start = self.preferred.load(Ordering::Relaxed) % len;
        let mut order = Vec::with_capacity(len);
        let mut skipped = Vec::new();
        let now = std::time::Instant::now();
        for offset in 0..len {
            let index = (start + offset) % len;
            let entry = &self.entries[index];
            if cooling_provider(&entry.id, now) {
                skipped.push(format!("[{}] 冷却中，跳过", entry.name));
                crate::logging::log_always(&format!(
                    "[provider-failover] 后台渠道「{}」冷却中，跳过",
                    entry.name
                ));
            } else {
                order.push(index);
            }
        }
        if order.is_empty() {
            // 全部在冷却：放弃跳过，按原顺序全试。
            order = (0..len).map(|offset| (start + offset) % len).collect();
            skipped.clear();
        }
        (order, skipped)
    }
}

/// 后台结构化调用只接受完整正文。空正文、只有推理、未执行的工具调用
/// 或输出被截断的 HTTP 200 都不是可用结果，必须换下一个渠道；否则
/// 调用方会把它当成合法 JSON 解析，并在 `EOF ... column 0` 处失败。
fn unusable_background_reply(reply: &AiReply) -> Option<String> {
    let reasoning_chars = reply
        .reasoning_content
        .as_ref()
        .map_or(0, |reasoning| reasoning.chars().count());
    if reply.finish_reason.as_deref() == Some("length") {
        return Some(format!(
            "响应达到输出上限（content {} 字，reasoning {} 字）",
            reply.content.chars().count(),
            reasoning_chars
        ));
    }
    if !reply.tool_calls.is_empty() {
        return Some(format!(
            "响应包含 {} 个未执行的工具调用",
            reply.tool_calls.len()
        ));
    }
    if reply.content.trim().is_empty() {
        return Some(format!(
            "响应为空（finish_reason={:?}，reasoning {} 字）",
            reply.finish_reason, reasoning_chars
        ));
    }
    None
}

impl AiProvider for FailoverProvider {
    fn generate_reply_with_tools(
        &self,
        messages: Vec<ContextMessage>,
        tools: Option<&[ToolSpec]>,
    ) -> Result<AiReply> {
        let len = self.entries.len();
        anyhow::ensure!(len > 0, "没有可用的 AI provider");
        let (order, skipped) = self.attempt_order();
        let mut errors = Vec::with_capacity(len);
        for index in order {
            let entry = &self.entries[index];
            match entry
                .provider
                .generate_reply_with_tools(messages.clone(), tools)
            {
                Ok(reply) => {
                    if let Some(reason) = unusable_background_reply(&reply) {
                        crate::logging::log_always(&format!(
                            "[provider-failover] 后台渠道「{}」返回不可用响应，换下一个：{reason}",
                            entry.name
                        ));
                        record_provider_failure(&entry.id);
                        errors.push(format!("[{}] {reason}", entry.name));
                        continue;
                    }
                    self.preferred.store(index, Ordering::Relaxed);
                    record_provider_success(&entry.id);
                    remember_last_good(&entry.id);
                    return Ok(reply);
                }
                Err(error) => {
                    // 渠道级错误（5xx/429/超时/鉴权/余额）都继续试下一个：
                    // 后台链路没有前台那样的用户可重试入口，能换就换。
                    crate::logging::log_always(&format!(
                        "[provider-failover] 后台渠道「{}」失败，换下一个：{error:#}",
                        entry.name
                    ));
                    record_provider_failure(&entry.id);
                    errors.push(format!("[{}] {error:#}", entry.name));
                }
            }
        }
        errors.extend(skipped);
        anyhow::bail!("所有 AI provider 均失败：{}", errors.join(" | "))
    }

    fn generate_observed(
        &self,
        messages: Vec<ContextMessage>,
        tools: Option<&[ToolSpec]>,
        observer: &mut dyn FnMut(ProviderEvent<'_>) -> Result<()>,
    ) -> Result<AiReply> {
        // 后台链路只用非流式 `generate_reply`；这里保留首选渠道的流式
        // 语义，避免已经吐出半截文本后再换渠道造成重复/拼接。
        let entry = self.preferred_entry()?;
        entry.provider.generate_observed(messages, tools, observer)
    }

    fn accepts_supplement(
        &self,
        messages: &[ContextMessage],
        tools: Option<&[ToolSpec]>,
    ) -> Result<bool> {
        match self.preferred_entry() {
            Ok(entry) => entry.provider.accepts_supplement(messages, tools),
            Err(_) => Ok(false),
        }
    }
}

/// 构建「显式激活渠道优先、其余渠道兜底」的后台链。每个渠道保留自己的
/// 温度/输出上限；用于分析队列这类需要与主对话同渠道口径的调用方。
pub(crate) fn active_first_provider_chain(store: &Store) -> Result<Option<Box<dyn AiProvider>>> {
    let mut configs: Vec<_> = store
        .list_ai_provider_configs_for_runtime()?
        .into_iter()
        .filter(|c| c.provider_type == "openai-compatible")
        .filter(|c| !c.api_key.trim().is_empty())
        .collect();
    if configs.is_empty() {
        return Ok(None);
    }
    // 稳定排序：显式激活渠道排最前，其余保持 runtime 顺序
    // （runtime 落点 > 手动排序 > 创建序）。
    configs.sort_by_key(|c| !c.is_active);
    let entries = configs
        .into_iter()
        .map(|config| {
            let provider = OpenAiCompatibleProvider::new(OpenAiCompatibleConfig {
                base_url: config.base_url,
                api_key: config.api_key,
                model: config.model,
                temperature: config.temperature as f32,
                max_tokens: config.max_tokens.map(|v| v as u32),
                context_window: config.context_window.map(|v| v as u32),
            })?;
            Ok(FailoverEntry::new(
                config.id,
                config.name,
                Box::new(provider),
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Some(Box::new(FailoverProvider::new(entries))))
}

fn last_good_slot() -> &'static Mutex<Option<String>> {
    static SLOT: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

/// 上次成功的后台渠道 id。用于下一次 tick 把它排到最前，避免每轮都先撞
/// 已知失败的渠道（进程重启后回到数据库排序，最坏多试一次）。
pub(crate) fn last_good_background_provider() -> Option<String> {
    last_good_slot()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

fn remember_last_good(id: &str) {
    *last_good_slot().lock().unwrap_or_else(|e| e.into_inner()) = Some(id.to_string());
}

#[cfg(test)]
pub(crate) fn reset_last_good_background_provider() {
    *last_good_slot().lock().unwrap_or_else(|e| e.into_inner()) = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// cooldown 槽是进程级全局状态；并行测试用这把锁串行化，
    /// 避免 A 测试的 reset 清掉 B 测试刚记的失败。
    static COOLDOWN_TEST_LOCK: Mutex<()> = Mutex::new(());

    struct ScriptedProvider {
        result: Mutex<Option<Result<AiReply>>>,
    }

    impl ScriptedProvider {
        fn ok(content: &str) -> Self {
            Self {
                result: Mutex::new(Some(Ok(AiReply::text(content)))),
            }
        }

        fn err(message: &str) -> Self {
            Self {
                result: Mutex::new(Some(Err(anyhow::anyhow!(message.to_string())))),
            }
        }

        fn reply(reply: AiReply) -> Self {
            Self {
                result: Mutex::new(Some(Ok(reply))),
            }
        }
    }

    impl AiProvider for ScriptedProvider {
        fn generate_reply_with_tools(
            &self,
            _messages: Vec<ContextMessage>,
            _tools: Option<&[ToolSpec]>,
        ) -> Result<AiReply> {
            self.result
                .lock()
                .unwrap()
                .take()
                .expect("scripted provider called more than once")
        }
    }

    #[test]
    fn tries_next_provider_after_failure() {
        let _guard = COOLDOWN_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_last_good_background_provider();
        reset_provider_cooldowns();
        let chain = FailoverProvider::new(vec![
            FailoverEntry::new("bad", "bad", Box::new(ScriptedProvider::err("503"))),
            FailoverEntry::new("good", "good", Box::new(ScriptedProvider::ok("ok"))),
        ]);

        let reply = chain
            .generate_reply_with_tools(Vec::new(), None)
            .expect("second provider should succeed");

        assert_eq!(reply.content, "ok");
        assert_eq!(last_good_background_provider().as_deref(), Some("good"));
    }

    #[test]
    fn skips_empty_and_truncated_replies() {
        let _guard = COOLDOWN_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_last_good_background_provider();
        reset_provider_cooldowns();
        let chain = FailoverProvider::new(vec![
            FailoverEntry::new(
                "empty",
                "empty",
                Box::new(ScriptedProvider::reply(AiReply::text(""))),
            ),
            FailoverEntry::new(
                "truncated",
                "truncated",
                Box::new(ScriptedProvider::reply(AiReply {
                    content: "{\"title\":".into(),
                    finish_reason: Some("length".into()),
                    ..AiReply::text("")
                })),
            ),
            FailoverEntry::new(
                "tools",
                "tools",
                Box::new(ScriptedProvider::reply(AiReply {
                    tool_calls: vec![crate::ai::tool::ToolCall::new(
                        "search",
                        serde_json::json!({}),
                    )],
                    ..AiReply::text("")
                })),
            ),
            FailoverEntry::new("good", "good", Box::new(ScriptedProvider::ok("ok"))),
        ]);

        let reply = chain
            .generate_reply_with_tools(Vec::new(), None)
            .expect("fourth provider should succeed");

        assert_eq!(reply.content, "ok");
        assert_eq!(last_good_background_provider().as_deref(), Some("good"));
    }

    struct RecordingProvider {
        name: &'static str,
        ok: bool,
        calls: Arc<Mutex<Vec<String>>>,
    }

    impl AiProvider for RecordingProvider {
        fn generate_reply_with_tools(
            &self,
            _messages: Vec<ContextMessage>,
            _tools: Option<&[ToolSpec]>,
        ) -> Result<AiReply> {
            self.calls.lock().unwrap().push(self.name.to_string());
            if self.ok {
                Ok(AiReply::text(self.name))
            } else {
                anyhow::bail!("503")
            }
        }
    }

    #[test]
    fn reuses_successful_provider_for_next_call() {
        let _guard = COOLDOWN_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_last_good_background_provider();
        reset_provider_cooldowns();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let chain = FailoverProvider::new(vec![
            FailoverEntry::new(
                "bad",
                "bad",
                Box::new(RecordingProvider {
                    name: "bad",
                    ok: false,
                    calls: calls.clone(),
                }),
            ),
            FailoverEntry::new(
                "good",
                "good",
                Box::new(RecordingProvider {
                    name: "good",
                    ok: true,
                    calls: calls.clone(),
                }),
            ),
        ]);

        chain.generate_reply_with_tools(Vec::new(), None).unwrap();
        chain.generate_reply_with_tools(Vec::new(), None).unwrap();

        assert_eq!(
            calls.lock().unwrap().as_slice(),
            ["bad", "good", "good"],
            "second call should start with the provider that succeeded"
        );
    }

    #[test]
    fn reports_every_provider_when_all_fail() {
        let _guard = COOLDOWN_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_last_good_background_provider();
        reset_provider_cooldowns();
        let chain = FailoverProvider::new(vec![
            FailoverEntry::new("first", "first", Box::new(ScriptedProvider::err("503"))),
            FailoverEntry::new("second", "second", Box::new(ScriptedProvider::err("429"))),
        ]);

        let error = chain
            .generate_reply_with_tools(Vec::new(), None)
            .expect_err("all providers should fail")
            .to_string();

        assert!(error.contains("[first]"));
        assert!(error.contains("[second]"));
        assert!(error.contains("503"));
        assert!(error.contains("429"));
    }

    #[test]
    fn failure_puts_provider_into_cooldown_and_next_call_skips_it() {
        let _guard = COOLDOWN_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_last_good_background_provider();
        reset_provider_cooldowns();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let chain = FailoverProvider::new(vec![
            FailoverEntry::new(
                "bad",
                "bad",
                Box::new(RecordingProvider {
                    name: "bad",
                    ok: false,
                    calls: calls.clone(),
                }),
            ),
            FailoverEntry::new(
                "good",
                "good",
                Box::new(RecordingProvider {
                    name: "good",
                    ok: true,
                    calls: calls.clone(),
                }),
            ),
        ]);

        chain.generate_reply_with_tools(Vec::new(), None).unwrap();
        chain.generate_reply_with_tools(Vec::new(), None).unwrap();

        assert_eq!(
            calls.lock().unwrap().as_slice(),
            ["bad", "good", "good"],
            "cooled-down provider must not be retried on the next call"
        );
        assert!(provider_in_cooldown("bad", std::time::Instant::now()));
        assert!(!provider_in_cooldown("good", std::time::Instant::now()));
    }

    #[test]
    fn success_clears_cooldown() {
        let _guard = COOLDOWN_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_last_good_background_provider();
        reset_provider_cooldowns();
        record_provider_failure("flaky");
        assert!(provider_in_cooldown("flaky", std::time::Instant::now()));
        record_provider_success("flaky");
        assert!(!provider_in_cooldown("flaky", std::time::Instant::now()));
    }

    #[test]
    fn all_cooled_down_still_tries_every_provider() {
        let _guard = COOLDOWN_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_last_good_background_provider();
        reset_provider_cooldowns();
        record_provider_failure("a");
        record_provider_failure("b");
        let chain = FailoverProvider::new(vec![
            FailoverEntry::new("a", "a", Box::new(ScriptedProvider::err("503"))),
            FailoverEntry::new("b", "b", Box::new(ScriptedProvider::ok("recovered"))),
        ]);

        let reply = chain
            .generate_reply_with_tools(Vec::new(), None)
            .expect("all-cooled chain must still attempt every provider");
        assert_eq!(reply.content, "recovered");
    }

    #[test]
    fn cooldown_backoff_grows_and_caps() {
        let first = CooldownState::backoff(1);
        let second = CooldownState::backoff(2);
        let huge = CooldownState::backoff(40);
        assert_eq!(first, std::time::Duration::from_secs(120));
        assert_eq!(second, std::time::Duration::from_secs(240));
        assert_eq!(huge, std::time::Duration::from_secs(1800));
    }
}
