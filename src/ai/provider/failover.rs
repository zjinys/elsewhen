//! 后台 AI 渠道的顺序 failover 包装。
//!
//! 后台任务（知识消化、周期报告、洞察等）与主对话分流：它们只需要一个
//! `AiProvider`，但单个渠道的 5xx/429/网络错误不应让整条后台链路停摆。
//! 这里把「候选渠道 + 顺序尝试 + 记住上次成功者」封装成一个 provider，
//! 调用方无需感知多渠道。

use super::*;
use crate::storage::Store;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

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
        let start = self.preferred.load(Ordering::Relaxed) % len;
        let mut errors = Vec::with_capacity(len);
        for offset in 0..len {
            let index = (start + offset) % len;
            let entry = &self.entries[index];
            match entry
                .provider
                .generate_reply_with_tools(messages.clone(), tools)
            {
                Ok(reply) => {
                    if let Some(reason) = unusable_background_reply(&reply) {
                        if std::env::var_os("ELSEWHEN_DEBUG").is_some() {
                            eprintln!(
                                "[provider-failover] 后台渠道「{}」返回不可用响应，换下一个：{reason}",
                                entry.name
                            );
                        }
                        errors.push(format!("[{}] {reason}", entry.name));
                        continue;
                    }
                    self.preferred.store(index, Ordering::Relaxed);
                    remember_last_good(&entry.id);
                    return Ok(reply);
                }
                Err(error) => {
                    // 渠道级错误（5xx/429/超时/鉴权/余额）都继续试下一个：
                    // 后台链路没有前台那样的用户可重试入口，能换就换。
                    if std::env::var_os("ELSEWHEN_DEBUG").is_some() {
                        eprintln!(
                            "[provider-failover] 后台渠道「{}」失败，换下一个：{error:#}",
                            entry.name
                        );
                    }
                    errors.push(format!("[{}] {error:#}", entry.name));
                }
            }
        }
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
        reset_last_good_background_provider();
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
        reset_last_good_background_provider();
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
        reset_last_good_background_provider();
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
        reset_last_good_background_provider();
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
}
