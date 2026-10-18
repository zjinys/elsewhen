//! Generation execution shared by the synchronous and observable chat APIs.
use crate::storage::{
    Store,
    chat_runs::{ChatRun, ChatRunStopped},
};
use anyhow::{Context, Result};
use serde_json::json;
use std::{
    path::Path,
    time::{Duration, Instant},
};

mod draft;
#[cfg(test)]
mod tests;

// Only this process's workers are stopped on normal shutdown. An empty local
// registry never licenses recovery of another process's healthy lease.
type Workers = std::collections::HashMap<String, (std::path::PathBuf, String)>;
fn workers() -> &'static std::sync::Mutex<Workers> {
    static WORKERS: std::sync::OnceLock<std::sync::Mutex<Workers>> = std::sync::OnceLock::new();
    WORKERS.get_or_init(Default::default)
}
struct WorkerGuard(String);
impl Drop for WorkerGuard {
    fn drop(&mut self) {
        workers()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.0);
    }
}

pub(crate) fn shutdown(path: &Path) -> Result<usize> {
    let registered = workers().lock().unwrap_or_else(|e| e.into_inner()).clone();
    if !registered.values().any(|(p, _)| p == path) {
        return Ok(0);
    }
    let until = Instant::now() + Duration::from_millis(500);
    // Existing registered workers already opened/migrated this database. Avoid
    // schema initialization (and its long busy wait) during bounded shutdown.
    let connection = rusqlite::Connection::open(path)?;
    connection.busy_timeout(Duration::from_millis(25))?;
    let store = Store {
        connection,
        path: path.to_owned(),
        provider_master: None,
        conversation_snapshot: None,
        chat_run_identity: None,
        captured_page_task: Default::default(),
    };
    for (id, (worker_path, owner)) in &registered {
        if worker_path != path || Instant::now() >= until {
            continue;
        }
        if store.check_chat_run_owner(id, owner).is_ok() {
            if let Some(run) = store.chat_run_metadata(id)? {
                let _ = store.cancel_chat_run(&run.conversation_id, id);
            }
        }
    }
    loop {
        let remaining = workers()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .filter(|(p, _)| p == path)
            .count();
        if remaining == 0 || Instant::now() >= until {
            return Ok(remaining);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[derive(Clone)]
pub(crate) struct RunIdentity {
    pub id: String,
    pub owner: String,
}

/// The database is the cancellation signal as well as the terminal arbiter. A
/// different process/connection can stop a worker without sharing a Dart flag.
pub(crate) fn check(store: &Store) -> Result<()> {
    if let Some(run) = &store.chat_run_identity {
        store.check_chat_run_owner(&run.id, &run.owner)?;
    }
    Ok(())
}

pub(crate) fn event(store: &Store, kind: &str, payload: serde_json::Value) -> Result<()> {
    if let Some(run) = &store.chat_run_identity {
        store.append_chat_run_event(&run.id, &run.owner, kind, &payload)?;
    }
    Ok(())
}

pub(crate) fn pause(store: &Store, duration: Duration) -> Result<()> {
    let until = Instant::now() + duration;
    loop {
        check(store)?;
        let remaining = until.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(());
        }
        std::thread::sleep(remaining.min(Duration::from_millis(25)));
    }
}

pub(crate) fn request(
    store: &Store,
    provider: &dyn super::provider::AiProvider,
    provider_name: &str,
    messages: Vec<super::memory::ContextMessage>,
    tools: Option<&[super::tool::ToolSpec]>,
) -> Result<super::provider::AiReply> {
    request_on_channel(store, provider, provider_name, messages, tools, true)
}

/// Derived-context requests share cancellation, budgeting and accounting, but
/// never reset or publish the user's answer draft.
pub(crate) fn auxiliary_request(
    store: &Store,
    provider: &dyn super::provider::AiProvider,
    provider_name: &str,
    messages: Vec<super::memory::ContextMessage>,
) -> Result<super::provider::AiReply> {
    event(store, "phase", json!({"phase":"summarizing"}))?;
    request_on_channel(store, provider, provider_name, messages, None, false)
}

fn request_on_channel(
    store: &Store,
    provider: &dyn super::provider::AiProvider,
    provider_name: &str,
    messages: Vec<super::memory::ContextMessage>,
    tools: Option<&[super::tool::ToolSpec]>,
    publish_draft: bool,
) -> Result<super::provider::AiReply> {
    check(store)?;
    let Some(run) = &store.chat_run_identity else {
        return provider.generate_reply_with_tools(messages, tools);
    };
    store.renew_chat_run(&run.id, &run.owner)?;
    if publish_draft {
        event(store, "draft_reset", json!({}))?;
    }
    let (request, _) = store.start_chat_request(&run.id, &run.owner, provider_name, None)?;
    if !publish_draft {
        event(store, "phase", json!({"phase":"summarizing"}))?;
    }
    let mut draft = draft::DraftBuffer::default();
    let reply = provider.generate_observed(messages, tools, &mut |notification| {
        if let Err(error) = check(store) {
            while let Some(text) = draft.take_checkpoint(true) {
                let _ = store.flush_chat_run_checkpoint(&run.id, &run.owner, &text);
            }
            return Err(error);
        }
        match notification {
            super::provider::ProviderEvent::Poll => {}
            super::provider::ProviderEvent::TextDelta(delta) => {
                if publish_draft { draft.push(delta); }
            }
            super::provider::ProviderEvent::Budget { attempt, trigger, report } => {
                event(store, "request_budget", json!({"request_id":request,"attempt":attempt,"trigger":trigger,"report":report}))?;
            }
        }
        while let Some(text) = draft.take_checkpoint(false) {
            store.flush_chat_run_checkpoint(&run.id, &run.owner, &text)?;
        }
        Ok(())
    });
    while let Some(text) = draft.take_checkpoint(true) {
        store.flush_chat_run_checkpoint(&run.id, &run.owner, &text)?;
    }
    check(store)?;
    match &reply {
        Ok(reply) => {
            store.finish_chat_request(
                &run.id,
                &run.owner,
                &request,
                "completed",
                reply.model.as_deref(),
                reply
                    .usage
                    .as_ref()
                    .map(|u| (u.prompt_tokens, u.completion_tokens)),
                reply.usage.as_ref().and_then(|u| u.prompt_cached_tokens),
            )?;
        }
        Err(_) => {
            store.finish_chat_request(&run.id, &run.owner, &request, "failed", None, None, None)?;
        }
    }
    if !publish_draft {
        if let Ok(reply) = &reply {
            if let (Some(usage), Some(metadata)) = (&reply.usage, store.chat_run_metadata(&run.id)?)
            {
                store.record_token_usage(
                    Some(&metadata.conversation_id),
                    usage.prompt_tokens.min(i64::MAX as u64) as i64,
                    usage.completion_tokens.min(i64::MAX as u64) as i64,
                    usage.total_tokens.min(i64::MAX as u64) as i64,
                    reply.model.as_deref(),
                )?;
            }
        }
    }
    reply
}

pub(crate) fn execute(
    store: &Store,
    run: &ChatRun,
    owner: &str,
    config: Option<super::ConversationConfig>,
) -> Result<String> {
    workers()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(run.id.clone(), (store.path.clone(), owner.into()));
    let _guard = WorkerGuard(run.id.clone());
    let result = (|| {
        let mut snapshot = store.snapshot_for_input(&run.conversation_id, &run.input_message_id)?;
        snapshot.validate_chat_run_context(&run.id, &run.conversation_id)?;
        snapshot.chat_run_identity = Some(RunIdentity {
            id: run.id.clone(),
            owner: owner.into(),
        });
        super::conversation::generate_conversation_reply_core(
            &run.conversation_id,
            &snapshot,
            config,
        )
    })();
    match result {
        Ok(content) => {
            match store.finish_chat_run(&run.id, owner, "completed", Some(&content), None) {
                Ok(_) => Ok(content),
                Err(error) => {
                    let _ = store.finish_chat_run(
                        &run.id,
                        owner,
                        "failed",
                        None,
                        Some(("commit", "最终回复未提交；已保存的步骤保留，请检查运行状态")),
                    );
                    Err(error)
                }
            }
        }
        Err(error) => {
            let stopped = error.downcast_ref::<ChatRunStopped>().is_some()
                || store
                    .chat_run(&run.id)?
                    .is_some_and(|run| run.status == "cancelling");
            let (kind, text) = if stopped {
                ("cancelled", "已停止；已提交的操作不会撤销")
            } else if error
                .downcast_ref::<super::budget::RequestBudgetError>()
                .is_some()
            {
                (
                    "budget",
                    "本轮必要上下文超过预算；输入已保存，请缩小任务范围",
                )
            } else {
                // Never persist raw provider bodies/URLs/arguments in diagnostics.
                (
                    "generation",
                    "生成失败；输入和已完成步骤保留，可重试此条输入",
                )
            };
            let _ = store.finish_chat_run(
                &run.id,
                owner,
                if stopped { "cancelled" } else { "failed" },
                None,
                Some((kind, text)),
            );
            Err(error)
        }
    }
}

pub(crate) fn generate_sync(
    conversation: &str,
    store: &Store,
    config: Option<super::ConversationConfig>,
) -> Result<String> {
    let input = store
        .list_messages(conversation)?
        .into_iter()
        .rev()
        .find(|m| m.role == "user")
        .context("请先保存用户输入")?;
    let owner = uuid::Uuid::new_v4().to_string();
    let run = store.create_chat_run(
        conversation,
        &input.id,
        &uuid::Uuid::new_v4().to_string(),
        &owner,
    )?;
    execute(store, &run, &owner, config)
}

pub(crate) fn start(path: &Path, conversation: &str, input: &str, key: &str) -> Result<ChatRun> {
    let store = Store::open(path)?;
    let owner = uuid::Uuid::new_v4().to_string();
    let run = store.create_chat_run(conversation, input, key, &owner)?;
    launch(path, store, run, owner)
}

pub(crate) fn schedule_input(
    path: &Path,
    conversation: &str,
    input: &str,
    dispatcher: &str,
) -> Result<Option<ChatRun>> {
    let store = Store::open(path)?;
    let owner = uuid::Uuid::new_v4().to_string();
    let Some(run) = store.schedule_chat_input(conversation, input, dispatcher, &owner)? else {
        return Ok(None);
    };
    launch(path, store, run, owner).map(Some)
}

pub(crate) fn dispatch_queued(
    path: &Path,
    conversation: &str,
    dispatcher: &str,
) -> Result<Option<ChatRun>> {
    let store = Store::open(path)?;
    let owner = uuid::Uuid::new_v4().to_string();
    let Some(run) = store.claim_next_chat_input(conversation, dispatcher, &owner)? else {
        return Ok(None);
    };
    launch(path, store, run, owner).map(Some)
}

fn launch(path: &Path, store: Store, run: ChatRun, owner: String) -> Result<ChatRun> {
    // Re-delivery returns the existing run; it never starts a second executor.
    if store.check_chat_run_owner(&run.id, &owner).is_ok() {
        // Open before launching so connection failures can finalize via the
        // caller's live connection instead of leaving a silent preparing lease.
        let worker_store = match Store::open(path) {
            Ok(store) => store,
            Err(error) => {
                let _ = store.finish_chat_run(
                    &run.id,
                    &owner,
                    "failed",
                    None,
                    Some(("startup", "生成连接未能启动；输入已保存，可重试")),
                );
                return Err(error);
            }
        };
        let worker_run = run.clone();
        let worker_owner = owner.clone();
        workers()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(run.id.clone(), (path.to_owned(), owner.clone()));
        let spawned = std::thread::Builder::new()
            .name("chat-generation".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    execute(&worker_store, &worker_run, &worker_owner, None)
                }));
                if result.is_err() {
                    let _ = worker_store.finish_chat_run(
                        &worker_run.id,
                        &worker_owner,
                        "failed",
                        None,
                        Some(("interrupted", "生成意外中断；不会自动重放操作")),
                    );
                }
            });
        if let Err(error) = spawned {
            workers()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&run.id);
            let _ = store.finish_chat_run(
                &run.id,
                &owner,
                "failed",
                None,
                Some(("startup", "生成线程未能启动；输入已保存，可重试")),
            );
            return Err(error.into());
        }
    }
    Ok(run)
}

/// Tool identity and outcome are user-visible business data, not debug logs.
/// For local writes the caller joins this transaction, closing the effect/result
/// window; a crash before commit leaves only the durable tool_started event.
pub(crate) fn tool_result_payload(
    call: &super::tool::ToolCall,
    result: &super::tool::ToolResultMsg,
    policy: super::tool::ToolPolicy,
) -> serde_json::Value {
    json!({"call_id":call.id,"name":call.name,"success":result.success,
        "content":result.content,"outcome":if !result.success {"failed"} else if policy == super::tool::ToolPolicy::WriteConfirm {"proposal_ready"} else if policy == super::tool::ToolPolicy::WriteDirect {"executed"} else {"read"}})
}
