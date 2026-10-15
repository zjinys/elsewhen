//! Business-facing generation snapshots and durable event cursors.
use crate::storage::{
    Store,
    chat_runs::{ChatRun, ChatRunEvent},
};
use anyhow::{Context, Result, ensure};

#[derive(Clone, Debug)]
pub struct ChatRunDto {
    pub id: String,
    pub conversation_id: String,
    pub input_message_id: String,
    pub status: String,
    pub phase: String,
    pub sequence: i64,
    pub request_count: i64,
    pub provider_basis_json: String,
    pub content: String,
    pub error_kind: Option<String>,
    pub error_text: Option<String>,
    pub final_message_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<ChatRun> for ChatRunDto {
    fn from(run: ChatRun) -> Self {
        Self {
            id: run.id,
            conversation_id: run.conversation_id,
            input_message_id: run.input_message_id,
            status: run.status,
            phase: run.phase,
            sequence: run.sequence,
            request_count: run.request_count,
            provider_basis_json: run.provider_basis_json,
            content: run.content,
            error_kind: run.error_kind,
            error_text: run.error_text,
            final_message_id: run.final_message_id,
            created_at: run.created_at,
            updated_at: run.updated_at,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ChatRunEventDto {
    pub run_id: String,
    pub sequence: i64,
    pub kind: String,
    pub payload_json: String,
}

impl From<ChatRunEvent> for ChatRunEventDto {
    fn from(event: ChatRunEvent) -> Self {
        Self {
            run_id: event.run_id,
            sequence: event.sequence,
            kind: event.kind,
            payload_json: event.payload_json,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ChatRunEventPageDto {
    pub run_id: String,
    pub events: Vec<ChatRunEventDto>,
    pub next_sequence: i64,
    pub has_more: bool,
}

fn open_store() -> Result<Store> {
    let config = crate::config::AppConfig::load()?;
    Store::open(&config.database_path)
}

fn snapshot(store: &Store, conversation: &str, run_id: &str) -> Result<Option<ChatRunDto>> {
    store.recover_expired_chat_runs()?;
    let run = store.chat_run(run_id)?;
    if let Some(run) = &run {
        ensure!(run.conversation_id == conversation, "生成运行不属于此会话");
    }
    Ok(run.map(Into::into))
}

/// Starts generation for an already saved input. Re-delivery of the same key
/// returns the existing run without saving another message or starting a worker.
pub fn start_chat_run(
    conversation_id: String,
    input_message_id: String,
    idempotency_key: String,
) -> Result<ChatRunDto> {
    let config = crate::config::AppConfig::load()?;
    Ok(crate::ai::generation::start(
        &config.database_path,
        &conversation_id,
        &input_message_id,
        &idempotency_key,
    )?
    .into())
}

/// Load this before observing events, then request events after its sequence.
/// The terminal snapshot is authoritative even if the last event was missed.
pub fn get_chat_run(conversation_id: String, run_id: String) -> Result<Option<ChatRunDto>> {
    snapshot(&open_store()?, &conversation_id, &run_id)
}

pub fn get_latest_chat_run(conversation_id: String) -> Result<Option<ChatRunDto>> {
    Ok(open_store()?
        .latest_chat_run(&conversation_id)?
        .map(Into::into))
}

/// History contains bounded metadata, not full response bodies. Select a run
/// with get_chat_run to restore its checkpoint or final reply.
pub fn list_chat_runs(conversation_id: String, offset: u32, limit: u32) -> Result<Vec<ChatRunDto>> {
    Ok(open_store()?
        .list_chat_runs(&conversation_id, offset, limit)?
        .into_iter()
        .map(Into::into)
        .collect())
}

fn event_page(
    store: &Store,
    conversation: &str,
    run_id: &str,
    after_sequence: i64,
    limit: u32,
) -> Result<ChatRunEventPageDto> {
    ensure!(after_sequence >= 0, "事件序号不能为负数");
    store.recover_expired_chat_runs()?;
    let run = store
        .chat_run_metadata(run_id)?
        .context("生成运行不存在或已删除")?;
    ensure!(run.conversation_id == conversation, "生成运行不属于此会话");
    ensure!(
        after_sequence <= run.sequence,
        "事件游标超出运行序号，请重新读取快照"
    );
    // Storage allows 256 rows; reserve one to determine whether another page exists.
    let limit = limit.clamp(1, 255);
    let mut events = store.chat_run_events(run_id, after_sequence, limit + 1)?;
    let has_more = events.len() > limit as usize;
    events.truncate(limit as usize);
    let next_sequence = events.last().map_or(after_sequence, |event| event.sequence);
    Ok(ChatRunEventPageDto {
        run_id: run_id.to_owned(),
        events: events.into_iter().map(Into::into).collect(),
        next_sequence,
        has_more,
    })
}

/// Replay durable events in sequence order. An empty page is not evidence of
/// completion: query the run snapshot for its authoritative state.
pub fn list_chat_run_events(
    conversation_id: String,
    run_id: String,
    after_sequence: i64,
    limit: u32,
) -> Result<ChatRunEventPageDto> {
    event_page(
        &open_store()?,
        &conversation_id,
        &run_id,
        after_sequence,
        limit,
    )
}

fn cancel(store: &Store, conversation: &str, run_id: &str) -> Result<Option<ChatRunDto>> {
    if snapshot(store, conversation, run_id)?.is_none() {
        return Ok(None);
    }
    store.cancel_chat_run(conversation, run_id)?;
    snapshot(store, conversation, run_id)
}

/// Requests cancellation, not rollback. Repeated requests are harmless and a
/// completed run remains completed; cancelling does not claim HTTP has stopped.
pub fn cancel_chat_run(conversation_id: String, run_id: String) -> Result<Option<ChatRunDto>> {
    cancel(&open_store()?, &conversation_id, &run_id)
}

/// Best-effort normal-exit stop for this process's workers only. Returns the
/// number still running after a bounded wait; crashes recover via durable leases.
pub fn shutdown_chat_runs() -> Result<u32> {
    let config = crate::config::AppConfig::load()?;
    Ok(crate::ai::generation::shutdown(&config.database_path)? as u32)
}

fn dispatcher_identity() -> &'static str {
    static OWNER: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    OWNER.get_or_init(|| uuid::Uuid::new_v4().to_string())
}

#[derive(Clone, Debug)]
pub struct UnscheduledChatInputDto {
    pub input_message_id: String,
    pub preview: String,
    pub created_at: String,
}

/// Saved inputs with no durable generation or queue association. This is not a
/// claim that legacy inputs were never answered, nor permission to generate.
pub fn list_unscheduled_chat_inputs(
    conversation_id: String,
    offset: u32,
    limit: u32,
) -> Result<Vec<UnscheduledChatInputDto>> {
    Ok(open_store()?
        .unscheduled_chat_inputs(&conversation_id, offset, limit)?
        .into_iter()
        .map(|input| UnscheduledChatInputDto {
            input_message_id: input.input_message_id,
            preview: input.preview,
            created_at: input.created_at,
        })
        .collect())
}

#[derive(Clone, Debug)]
pub struct ChatQueuedInputDto {
    pub id: i64,
    pub input_message_id: String,
    pub status: String,
    pub mode: String,
    pub preview: String,
    pub defer_reason: Option<String>,
    pub run_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ChatInputQueueDto {
    pub paused: bool,
    pub items: Vec<ChatQueuedInputDto>,
    pub recent_injections: Vec<ChatQueuedInputDto>,
}

/// Atomically enqueue a saved question and start it only when no prior input or
/// active run exists. Re-delivery does not grant a second generation attempt.
pub fn schedule_chat_input(
    conversation_id: String,
    input_message_id: String,
) -> Result<Option<ChatRunDto>> {
    let config = crate::config::AppConfig::load()?;
    Ok(crate::ai::generation::schedule_input(
        &config.database_path,
        &conversation_id,
        &input_message_id,
        dispatcher_identity(),
    )?
    .map(Into::into))
}

/// Register generation eligibility only; the input must already be saved.
/// A withdrawn or consumed input never becomes pending on duplicate delivery.
pub fn enqueue_chat_input(conversation_id: String, input_message_id: String) -> Result<i64> {
    open_store()?.enqueue_chat_input(&conversation_id, &input_message_id)
}

/// Explicitly supplement one current task. Unsupported context is preserved in
/// the pending queue; this never resumes dispatch or confirms a business action.
pub fn enqueue_chat_supplement(
    conversation_id: String,
    input_message_id: String,
    run_id: String,
    context_json: String,
) -> Result<i64> {
    open_store()?.enqueue_chat_supplement(
        &conversation_id,
        &input_message_id,
        &run_id,
        &context_json,
    )
}

pub fn get_chat_input_queue(
    conversation_id: String,
    after_id: i64,
    limit: u32,
) -> Result<ChatInputQueueDto> {
    let store = open_store()?;
    Ok(ChatInputQueueDto {
        paused: store.chat_queue_paused(&conversation_id, dispatcher_identity())?,
        recent_injections: store
            .recent_chat_injections(&conversation_id)?
            .into_iter()
            .map(|item| ChatQueuedInputDto {
                id: item.id,
                input_message_id: item.input_message_id,
                status: item.status,
                mode: item.mode,
                preview: item.preview,
                defer_reason: item.defer_reason,
                run_id: item.run_id,
            })
            .collect(),
        items: store
            .pending_chat_input_queue(&conversation_id, after_id, limit)?
            .into_iter()
            .map(|item| ChatQueuedInputDto {
                id: item.id,
                input_message_id: item.input_message_id,
                status: item.status,
                mode: item.mode,
                preview: item.preview,
                defer_reason: item.defer_reason,
                run_id: item.run_id,
            })
            .collect(),
    })
}

pub fn withdraw_chat_input(conversation_id: String, queue_id: i64) -> Result<bool> {
    open_store()?.withdraw_chat_input(&conversation_id, queue_id)
}

/// Explicit continuation grants this process a dispatch lease. Reopening the
/// application alone does not grant a new process permission to consume inputs.
pub fn resume_chat_input_queue(conversation_id: String) -> Result<()> {
    open_store()?.resume_chat_queue(&conversation_id, dispatcher_identity())
}

pub fn pause_chat_input_queue(conversation_id: String) -> Result<()> {
    open_store()?.pause_chat_queue(&conversation_id)
}

/// A bounded tick starts at most one input, and never replays an already claimed
/// input. The caller must keep observing the run independently of this result.
pub fn dispatch_chat_input_queue(conversation_id: String) -> Result<Option<ChatRunDto>> {
    let config = crate::config::AppConfig::load()?;
    Ok(crate::ai::generation::dispatch_queued(
        &config.database_path,
        &conversation_id,
        dispatcher_identity(),
    )?
    .map(Into::into))
}

#[cfg(test)]
mod tests;
