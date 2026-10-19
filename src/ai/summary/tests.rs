use super::*;
use crate::ai::{provider::ProviderEvent, tool::ToolSpec};
use std::cell::{Cell, RefCell};

struct TempDatabase(std::path::PathBuf);
impl Drop for TempDatabase {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_file(format!("{}-wal", self.0.display()));
        let _ = std::fs::remove_file(format!("{}-shm", self.0.display()));
    }
}

fn fixture() -> (TempDatabase, Store, String) {
    let path =
        std::env::temp_dir().join(format!("elsewhen-summary-ai-{}.db", uuid::Uuid::new_v4()));
    let store = Store::open(&path).unwrap();
    let conversation = store
        .create_conversation(Some("summary fixture"), None)
        .unwrap();
    (TempDatabase(path), store, conversation)
}

fn add_history(store: &Store, conversation: &str) -> String {
    store
        .send_message(
            conversation,
            "user",
            &format!("不要改动原文。{}", "historical background ".repeat(2000)),
            None,
        )
        .unwrap();
    store
        .send_message(
            conversation,
            "assistant",
            "方案仍待审阅，不代表已经执行。",
            None,
        )
        .unwrap();
    store
        .send_message(conversation, "user", "保留最近完整交换", None)
        .unwrap();
    store
        .send_message(conversation, "assistant", "这是最近答复", None)
        .unwrap();
    store
        .send_message(conversation, "user", "当前问题必须原样保留", None)
        .unwrap()
}

fn context(store: &Store, conversation: &str) -> Vec<ContextMessage> {
    let mut context = vec![ContextMessage::new("system", "required rules")];
    context.extend(
        store
            .list_messages(conversation)
            .unwrap()
            .into_iter()
            .map(ContextMessage::from_record),
    );
    context
}

fn good_reply() -> AiReply {
    AiReply::text(
        json!({
            "goals":["整理任务"],"constraints":["不修改原文"],"facts":[],
            "decisions":[],"unfinished":["方案等待审阅，尚未执行"]
        })
        .to_string(),
    )
}

struct FakeProvider {
    requests: Cell<usize>,
    reply: AiReply,
    on_request: RefCell<Option<Box<dyn FnOnce()>>>,
}
impl FakeProvider {
    fn new(reply: AiReply) -> Self {
        Self {
            requests: Cell::new(0),
            reply,
            on_request: RefCell::new(None),
        }
    }
}
impl AiProvider for FakeProvider {
    fn generate_reply_with_tools(
        &self,
        messages: Vec<ContextMessage>,
        tools: Option<&[ToolSpec]>,
    ) -> Result<AiReply> {
        assert!(tools.is_none());
        assert_eq!(
            messages.len(),
            2,
            "summary data is one required input, not trimmable history"
        );
        assert_eq!(messages[1].role, "user");
        self.requests.set(self.requests.get() + 1);
        if let Some(action) = self.on_request.borrow_mut().take() {
            action();
        }
        Ok(self.reply.clone())
    }
    fn generate_observed(
        &self,
        messages: Vec<ContextMessage>,
        tools: Option<&[ToolSpec]>,
        observer: &mut dyn FnMut(ProviderEvent<'_>) -> Result<()>,
    ) -> Result<AiReply> {
        observer(ProviderEvent::Poll)?;
        observer(ProviderEvent::TextDelta("摘要内部正文，不应进入用户回复"))?;
        let result = self.generate_reply_with_tools(messages, tools)?;
        observer(ProviderEvent::Poll)?;
        Ok(result)
    }
}

#[test]
fn long_history_gets_a_cached_non_destructive_summary_and_keeps_recent_turns() {
    let (_temp, store, conversation) = fixture();
    add_history(&store, &conversation);
    let originals = store.list_messages(&conversation).unwrap();
    let provider = FakeProvider::new(good_reply());
    let mut first = context(&store, &conversation);
    apply_summary(
        &store,
        &conversation,
        &provider,
        "fixture",
        "config-v1",
        "gpt-4o",
        1800,
        &mut first,
    )
    .unwrap();
    assert!(
        first
            .iter()
            .any(|message| message.segment.purpose == SegmentPurpose::Summary)
    );
    assert!(
        first
            .iter()
            .any(|message| message.content == "保留最近完整交换")
    );
    assert!(
        first
            .iter()
            .any(|message| message.content == "这是最近答复")
    );
    assert!(
        first
            .iter()
            .any(|message| message.content == "当前问题必须原样保留")
    );
    let requests = provider.requests.get();
    assert!(requests > 0 && requests <= MAX_REQUESTS);
    // 第二次走生产装配路径：第一次摘要成功即固化 auto 翻篇标记，重跑
    // 直接读标记跳过边界前原文（零新请求，比节点级缓存更强的复用）。
    let mut second =
        crate::ai::memory::prepare_uncompressed_context(&conversation, &store).unwrap();
    assert_eq!(
        provider.requests.get(),
        requests,
        "持久化标记复用不产生新摘要请求"
    );
    assert!(
        second
            .iter()
            .any(|message| message.segment.purpose == SegmentPurpose::Summary)
    );
    assert!(
        second
            .iter()
            .any(|message| message.content.contains("已翻篇的历史摘要"))
    );
    assert_eq!(
        store
            .list_messages(&conversation)
            .unwrap()
            .iter()
            .map(message_basis)
            .collect::<Vec<_>>(),
        originals.iter().map(message_basis).collect::<Vec<_>>()
    );
}

#[test]
fn queued_history_summary_keeps_answers_with_their_original_requests() {
    let (_temp, store, conversation) = fixture();
    let texts = [
        format!("early constraint {}", "history ".repeat(2500)),
        "second question".into(),
        "third question".into(),
    ];
    let inputs: Vec<_> = texts
        .iter()
        .map(|text| {
            let id = store
                .send_message(&conversation, "user", text, None)
                .unwrap();
            store.enqueue_chat_input(&conversation, &id).unwrap();
            id
        })
        .collect();
    store.resume_chat_queue(&conversation, "process").unwrap();
    for answer in ["first answer", "second answer"] {
        let run = store
            .claim_next_chat_input(&conversation, "process", "worker")
            .unwrap()
            .unwrap();
        store
            .finish_chat_run(&run.id, "worker", "completed", Some(answer), None)
            .unwrap();
    }
    let snapshot = store.snapshot_for_input(&conversation, &inputs[2]).unwrap();
    let mut messages = context(&snapshot, &conversation);
    let provider = FakeProvider::new(good_reply());
    apply_summary(
        &snapshot,
        &conversation,
        &provider,
        "fixture",
        "config",
        "gpt-4o",
        1800,
        &mut messages,
    )
    .unwrap();
    assert_eq!(
        messages
            .iter()
            .filter(|m| m.segment.purpose == SegmentPurpose::History)
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>(),
        ["second question", "second answer", "third question"]
    );
    let history = snapshot.list_messages(&conversation).unwrap();
    let root: String = store.connection.query_row(
        "SELECT id FROM chat_summary_nodes WHERE json_array_length(spans_json)=(SELECT MAX(json_array_length(spans_json)) FROM chat_summary_nodes) LIMIT 1",
        [], |row| row.get(0)).unwrap();
    let spans = snapshot
        .summary_history_spans(&conversation, &root, 0, 32)
        .unwrap();
    assert!(
        spans.iter().any(|span| span.message_id == history[1].id),
        "the first answer is summarized with the first request, not kept as the second turn"
    );
    assert!(
        spans
            .iter()
            .all(|span| span.message_id == inputs[0] || span.message_id == history[1].id)
    );
}

#[test]
fn unicode_chunks_cover_the_whole_message_including_the_tail() {
    let (_temp, store, conversation) = fixture();
    let text = format!("{}末尾的关键约束", "🦀测".repeat(5000));
    store
        .send_message(&conversation, "user", &text, None)
        .unwrap();
    let messages = store.list_messages(&conversation).unwrap();
    let parts = split_history(&messages).unwrap();
    assert_eq!(
        parts
            .iter()
            .map(|part| part.text.as_str())
            .collect::<String>(),
        text
    );
    assert!(parts.last().unwrap().text.ends_with("末尾的关键约束"));
    assert!(
        parts
            .iter()
            .all(|part| part.text.chars().count() <= CHUNK_CHARS)
    );
    store
        .validate_history_spans(
            &conversation,
            &parts
                .iter()
                .map(|part| part.span.clone())
                .collect::<Vec<_>>(),
        )
        .unwrap();
}

#[test]
fn short_or_current_only_history_never_requests_a_summary() {
    let (_temp, store, conversation) = fixture();
    store
        .send_message(&conversation, "user", &"current input ".repeat(2000), None)
        .unwrap();
    let provider = FakeProvider::new(good_reply());
    let mut messages = context(&store, &conversation);
    let before = messages.last().unwrap().content.clone();
    apply_summary(
        &store,
        &conversation,
        &provider,
        "fixture",
        "config",
        "gpt-4o",
        500,
        &mut messages,
    )
    .unwrap();
    assert_eq!(provider.requests.get(), 0);
    assert_eq!(messages.last().unwrap().content, before);
}

#[test]
fn invalid_and_truncated_results_do_not_create_summary_nodes() {
    let (_temp, store, conversation) = fixture();
    add_history(&store, &conversation);
    for reply in [
        AiReply::text("not JSON"),
        AiReply {
            finish_reason: Some("length".into()),
            ..good_reply()
        },
    ] {
        let provider = FakeProvider::new(reply);
        let mut messages = context(&store, &conversation);
        apply_summary(
            &store,
            &conversation,
            &provider,
            "fixture",
            "config",
            "gpt-4o",
            1800,
            &mut messages,
        )
        .unwrap();
        assert!(
            !messages
                .iter()
                .any(|message| message.segment.purpose == SegmentPurpose::Summary)
        );
        assert!(
            messages
                .iter()
                .any(|message| message.content.contains("摘要尚未完成或已失效"))
        );
    }
    let count: i64 = store
        .connection
        .query_row("SELECT COUNT(*) FROM chat_summary_nodes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn summary_auxiliary_stream_never_creates_answer_deltas() {
    let (_temp, store, conversation) = fixture();
    let input = add_history(&store, &conversation);
    let run = store
        .create_chat_run(&conversation, &input, "summary-observer", "worker")
        .unwrap();
    let mut snapshot = store.snapshot_for_input(&conversation, &input).unwrap();
    snapshot.chat_run_identity = Some(crate::ai::generation::RunIdentity {
        id: run.id.clone(),
        owner: "worker".into(),
    });
    let mut messages = context(&snapshot, &conversation);
    let provider = FakeProvider::new(good_reply());
    apply_summary(
        &snapshot,
        &conversation,
        &provider,
        "fixture",
        "config",
        "gpt-4o",
        1800,
        &mut messages,
    )
    .unwrap();
    assert!(
        messages
            .iter()
            .any(|message| message.segment.purpose == SegmentPurpose::Summary)
    );
    let events = store.chat_run_events(&run.id, 0, 256).unwrap();
    assert!(
        !events
            .iter()
            .any(|event| matches!(event.kind.as_str(), "draft_reset" | "text_delta"))
    );
    assert!(store.chat_run(&run.id).unwrap().unwrap().content.is_empty());
}

#[test]
fn cancellation_and_real_history_edits_prevent_injection() {
    for cancel in [false, true] {
        let (_temp, store, conversation) = fixture();
        let input = add_history(&store, &conversation);
        let run = store
            .create_chat_run(&conversation, &input, "summary-conflict", "worker")
            .unwrap();
        let mut snapshot = store.snapshot_for_input(&conversation, &input).unwrap();
        snapshot.chat_run_identity = Some(crate::ai::generation::RunIdentity {
            id: run.id.clone(),
            owner: "worker".into(),
        });
        let provider = FakeProvider::new(good_reply());
        let path = store.path.clone();
        let conversation_copy = conversation.clone();
        let original = store.list_messages(&conversation).unwrap()[0].id.clone();
        *provider.on_request.borrow_mut() = Some(Box::new(move || {
            let writer = Store::open(&path).unwrap();
            if cancel {
                writer.cancel_chat_run(&conversation_copy, &run.id).unwrap();
            } else {
                writer
                    .connection
                    .execute(
                        "UPDATE messages SET content='concurrent edit' WHERE id=?1",
                        [&original],
                    )
                    .unwrap();
            }
        }));
        let mut messages = context(&snapshot, &conversation);
        let result = apply_summary(
            &snapshot,
            &conversation,
            &provider,
            "fixture",
            "config",
            "gpt-4o",
            1800,
            &mut messages,
        );
        assert_eq!(result.is_err(), cancel);
        assert!(
            !messages
                .iter()
                .any(|message| message.segment.purpose == SegmentPurpose::Summary)
        );
        let count: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM chat_summary_nodes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
}

#[test]
fn incomplete_bounded_work_keeps_leaf_cache_without_claiming_full_coverage() {
    let (_temp, store, conversation) = fixture();
    // 上限 64 后 45 条消息一轮就能建完（约 15 个请求）；200 条让全量
    // build 需求（约 90+ 请求）超出单轮配额，维持「撞顶后留下叶缓存、
    // 第二轮续跑」的测试意图。
    for _ in 0..200 {
        store
            .send_message(&conversation, "user", &"historical ".repeat(500), None)
            .unwrap();
    }
    let provider = FakeProvider::new(good_reply());
    let mut first = context(&store, &conversation);
    apply_summary(
        &store,
        &conversation,
        &provider,
        "fixture",
        "config",
        "gpt-4o",
        1800,
        &mut first,
    )
    .unwrap();
    assert_eq!(provider.requests.get(), MAX_REQUESTS);
    assert!(
        !first
            .iter()
            .any(|message| message.segment.purpose == SegmentPurpose::Summary)
    );
    let count: i64 = store
        .connection
        .query_row("SELECT COUNT(*) FROM chat_summary_nodes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, MAX_REQUESTS as i64);
    let mut second = context(&store, &conversation);
    apply_summary(
        &store,
        &conversation,
        &provider,
        "fixture",
        "config",
        "gpt-4o",
        1800,
        &mut second,
    )
    .unwrap();
    assert!(provider.requests.get() <= MAX_REQUESTS * 2);
    let count_after: i64 = store
        .connection
        .query_row("SELECT COUNT(*) FROM chat_summary_nodes", [], |r| r.get(0))
        .unwrap();
    assert!(count_after > count);
}

mod page_marks {
    use super::*;

    fn first_message_id(store: &Store, conversation: &str) -> String {
        store
            .list_messages(conversation)
            .unwrap()
            .first()
            .unwrap()
            .id
            .clone()
    }

    fn boundary_reply() -> AiReply {
        AiReply::text(
            json!({
                "goals":["整理任务"],"constraints":["不修改原文"],
                "facts":["历史背景已归档为派生摘要"],
                "decisions":["先审阅再执行"],"unfinished":["方案等待审阅"]
            })
            .to_string(),
        )
    }

    #[test]
    fn manual_page_mark_skips_boundary_and_injects_summary() {
        let (_temp, store, conversation) = fixture();
        add_history(&store, &conversation);
        let boundary = first_message_id(&store, &conversation);
        let full_history: usize = store
            .list_messages(&conversation)
            .unwrap()
            .iter()
            .map(|m| m.content.chars().count())
            .sum();
        let mark_id = super::generate_page_mark(
            &store,
            &conversation,
            &boundary,
            &FakeProvider::new(boundary_reply()),
            "fixture",
            "config",
            "gpt-4o",
            "manual",
        )
        .unwrap();
        let mark = store.page_mark_by_id(&mark_id).unwrap().unwrap();
        assert!(mark.active);
        assert_eq!(mark.kind, "manual");

        let context =
            crate::ai::memory::prepare_uncompressed_context(&conversation, &store).unwrap();
        let texts: Vec<&str> = context.iter().map(|m| m.content.as_str()).collect();
        let summary_message = texts
            .iter()
            .find(|t| t.contains("已翻篇的历史摘要"))
            .expect("翻篇摘要应注入装配");
        assert!(
            summary_message.contains(boundary.as_str()),
            "摘要应标注覆盖边界"
        );
        // 边界前原文确定性跳过；边界后消息完整保留。
        assert!(texts.iter().all(|t| !t.contains("historical background")));
        assert!(
            texts.iter().any(|t| t.contains("保留最近完整交换")),
            "边界后原文必须保留"
        );
        let context_chars: usize = texts.iter().map(|t| t.chars().count()).sum();
        assert!(context_chars < full_history / 2, "翻篇后装配体积应显著下降");
    }

    #[test]
    fn boundary_at_tail_is_rejected() {
        let (_temp, store, conversation) = fixture();
        let last = add_history(&store, &conversation);
        let error = super::generate_page_mark(
            &store,
            &conversation,
            &last,
            &FakeProvider::new(boundary_reply()),
            "fixture",
            "config",
            "gpt-4o",
            "manual",
        )
        .unwrap_err();
        assert!(error.to_string().contains("不吞最新一轮"));
    }

    #[test]
    fn override_edit_updates_render_and_rejects_invalid_body() {
        let (_temp, store, conversation) = fixture();
        add_history(&store, &conversation);
        let mark_id = super::generate_page_mark(
            &store,
            &conversation,
            &first_message_id(&store, &conversation),
            &FakeProvider::new(boundary_reply()),
            "fixture",
            "config",
            "gpt-4o",
            "manual",
        )
        .unwrap();

        let invalid = r#"{"goals":[],"constraints":[],"facts":[],"decisions":[],"unfinished":[]}"#;
        assert!(
            store.save_page_mark_override(&mark_id, invalid).is_err(),
            "空摘要必须被校验挡下"
        );
        assert!(store.save_page_mark_override(&mark_id, "not json").is_err());

        let edited = json!({
            "goals":["编辑后的目标"],"constraints":["不修改原文"],"facts":[],
            "decisions":[],"unfinished":["方案等待审阅"]
        })
        .to_string();
        store.save_page_mark_override(&mark_id, &edited).unwrap();
        let mark = store.page_mark_by_id(&mark_id).unwrap().unwrap();
        let (text, body_json) = super::render_mark_text(&store, &mark).unwrap();
        assert!(text.contains("编辑后的目标"), "预览渲染应使用用户编辑版");
        assert!(text.contains("summary_id"), "覆盖注记必须随摘要注入");
        let reparsed: super::SummaryBody = serde_json::from_str(&body_json).unwrap();
        assert_eq!(reparsed.goals, vec!["编辑后的目标".to_string()]);
    }

    #[test]
    fn undo_restores_full_history_wiring() {
        let (_temp, store, conversation) = fixture();
        add_history(&store, &conversation);
        let mark_id = super::generate_page_mark(
            &store,
            &conversation,
            &first_message_id(&store, &conversation),
            &FakeProvider::new(boundary_reply()),
            "fixture",
            "config",
            "gpt-4o",
            "manual",
        )
        .unwrap();
        store.set_page_mark_active(&mark_id, false).unwrap();
        assert!(store.active_page_mark(&conversation).unwrap().is_none());
        let context =
            crate::ai::memory::prepare_uncompressed_context(&conversation, &store).unwrap();
        assert!(
            context
                .iter()
                .any(|m| m.content.contains("historical background")),
            "撤销翻篇后原文必须完整回到装配"
        );
    }

    #[test]
    fn auto_marks_keep_only_latest_per_topic() {
        let (_temp, store, conversation) = fixture();
        add_history(&store, &conversation);
        let messages = store.list_messages(&conversation).unwrap();
        let first = messages[0].id.clone();
        let second = messages[1].id.clone();
        let provider = || FakeProvider::new(boundary_reply());
        let old = super::generate_page_mark(
            &store,
            &conversation,
            &first,
            &provider(),
            "fixture",
            "config",
            "gpt-4o",
            "auto",
        )
        .unwrap();
        let new = super::generate_page_mark(
            &store,
            &conversation,
            &second,
            &provider(),
            "fixture",
            "config",
            "gpt-4o",
            "auto",
        )
        .unwrap();
        assert_ne!(old, new);
        assert!(
            !store.page_mark_by_id(&old).unwrap().unwrap().active,
            "旧 auto 必须失活（每话题只留最新一份）"
        );
        assert!(store.page_mark_by_id(&new).unwrap().unwrap().active);
        // 同一边界重复翻篇要被显式拒绝，不静默重建。
        let error = super::generate_page_mark(
            &store,
            &conversation,
            &second,
            &provider(),
            "fixture",
            "config",
            "gpt-4o",
            "auto",
        )
        .unwrap_err();
        assert!(error.to_string().contains("已存在翻篇标记"));
    }

    #[test]
    fn stale_boundary_falls_back_to_full_history() {
        let (_temp, mut store, conversation) = fixture();
        add_history(&store, &conversation);
        let mark_id = super::generate_page_mark(
            &store,
            &conversation,
            &first_message_id(&store, &conversation),
            &FakeProvider::new(boundary_reply()),
            "fixture",
            "config",
            "gpt-4o",
            "manual",
        )
        .unwrap();
        // 直接改库模拟边界消息在别的设备被删除的跨端事实。
        store
            .connection
            .execute(
                "UPDATE conversation_page_marks SET boundary_message_id='deleted-msg' WHERE id=?1",
                [&mark_id],
            )
            .unwrap();
        // 生效标记失效必须显式落事件。挂上真实的 chat run 身份后断言事件表。
        let input_id = {
            let messages = store.list_messages(&conversation).unwrap();
            messages
                .iter()
                .rev()
                .find(|m| m.role == "user")
                .unwrap()
                .id
                .clone()
        };
        let run = store
            .create_chat_run(
                &conversation,
                &input_id,
                "stale-mark-drill",
                "fixture-owner",
            )
            .unwrap();
        store.chat_run_identity = Some(crate::ai::generation::RunIdentity {
            id: run.id.clone(),
            owner: "fixture-owner".into(),
        });
        let context =
            crate::ai::memory::prepare_uncompressed_context(&conversation, &store).unwrap();
        assert!(
            context
                .iter()
                .any(|m| m.content.contains("historical background")),
            "边界失效必须退回全原文"
        );
        assert!(
            !context
                .iter()
                .any(|m| m.content.contains("已翻篇的历史摘要")),
            "失效标记的摘要不得再注入"
        );
        crate::ai::memory::prepare_uncompressed_context(&conversation, &store).unwrap();
        let stale_events: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM chat_run_events WHERE run_id=?1 AND kind='page_mark_stale'",
                [&run.id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(stale_events >= 1, "失效必须显式落事件，不静默");
    }

    #[test]
    fn apply_summary_targets_only_current_page_after_mark() {
        let (_temp, store, conversation) = fixture();
        add_history(&store, &conversation);
        let boundary = first_message_id(&store, &conversation);
        let mark_id = super::generate_page_mark(
            &store,
            &conversation,
            &boundary,
            &FakeProvider::new(boundary_reply()),
            "fixture",
            "config",
            "gpt-4o",
            "auto",
        )
        .unwrap();
        assert!(store.page_mark_by_id(&mark_id).unwrap().unwrap().active);
        // 标记生效后第二篇增长：complete_originals 守护不应挡住新篇摘要。
        store
            .send_message(
                &conversation,
                "user",
                &"new page growth ".repeat(3000),
                None,
            )
            .unwrap();
        store
            .send_message(&conversation, "assistant", "新篇答复", None)
            .unwrap();
        store
            .send_message(
                &conversation,
                "user",
                &"new page growth 2 ".repeat(3000),
                None,
            )
            .unwrap();
        store
            .send_message(&conversation, "assistant", "新篇第二轮答复", None)
            .unwrap();
        store
            .send_message(&conversation, "user", "新篇当前问题", None)
            .unwrap();
        let provider = FakeProvider::new(boundary_reply());
        let mut messages =
            crate::ai::memory::prepare_uncompressed_context(&conversation, &store).unwrap();
        super::apply_summary(
            &store,
            &conversation,
            &provider,
            "fixture",
            "config",
            "gpt-4o",
            4000,
            &mut messages,
        )
        .unwrap();
        // 4000 预算按 80% 触发线：新篇 18000+ 字符远超 3200 阈值 → 应请求摘要。
        assert!(
            provider.requests.get() >= 1,
            "翻篇后的新增长必须仍可触发摘要，不能被守护永久挡住"
        );
    }
}

// ---------------------------------------------------------------------------
// P1 诊断（2026-10-19 摘要事故预案一）：离线重放生产库上的摘要失败。
// 错误是确定性的（归并根节点建不出来），不需要等真实流量——用 P0-a 的
// detail 留痕直接拿真实错误。运行方式：
//   cp ~/.local/share/elsewhen/elsewhen.db /tmp/p1-replay.db
//   P1_REPLAY_DB=/tmp/p1-replay.db cargo test --lib p1_replay -- --ignored --nocapture
// 会真实调用 provider（小请求、最多 16 次：上限与生产一致），用于复现根因。
#[test]
#[ignore = "P1 诊断：真实流量复现，需 P1_REPLAY_DB 指向带真实 key 的库副本"]
fn p1_replay_production_summary_failure() {
    let db_path = std::env::var("P1_REPLAY_DB")
        .expect("请设置 P1_REPLAY_DB 指向库副本（cp 主库到 /tmp 后传入）");
    let store = match Store::open(std::path::Path::new(&db_path)) {
        Ok(store) => store,
        Err(error) => panic!("打开副本库失败：{error:#}"),
    };
    let providers = store.list_ai_provider_configs_for_runtime().unwrap();
    let config = providers
        .first()
        .cloned()
        .expect("副本库里没有可用的 provider 配置");
    println!(
        "provider：{}（{}，窗口 {:?}）",
        config.name, config.model, config.context_window
    );
    // 会话取消息最多的（2026-10-19 出事会话即全库最大）。
    let (conversation, message_count) = store
        .connection
        .query_row(
            "SELECT id, (SELECT COUNT(*) FROM messages WHERE conversation_id=conversations.id) AS n
             FROM conversations ORDER BY n DESC LIMIT 1",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .unwrap();
    println!("出事会话：{conversation}（{message_count} 条消息）");

    // 生产同款装配：无延展 prompt 的纯摘要前置（provider 实例化照抄
    // generate_with_provider_configs 的 OpenAI-compatible 分支）。
    let provider_config = super::super::provider::OpenAiCompatibleConfig {
        base_url: config.base_url.clone(),
        api_key: config.api_key.clone(),
        model: config.model.clone(),
        temperature: config.temperature as f32,
        max_tokens: config.max_tokens.map(|v| v as u32),
        context_window: config.context_window.map(|v| v as u32),
    };
    let provider = super::super::provider::OpenAiCompatibleProvider::new(provider_config).unwrap();
    let mut context =
        crate::ai::memory::prepare_uncompressed_context(&conversation, &store).unwrap();
    let window = config.context_window.unwrap_or(200_000) as usize;
    let budget = window.saturating_sub(40_000).max(1);
    let error = super::apply_summary(
        &store,
        &conversation,
        &provider,
        &config.name,
        &format!("{}|{}", config.base_url, config.model),
        &config.model,
        budget,
        &mut context,
    )
    .err()
    .expect("摘要竟然成功了——根因已被后续修复或环境差异，把输出贴回事故档案");
    println!("真实失败链：{error:#}");
    panic!("P1 复现成功，见上一行");
}

// ---------------------------------------------------------------------------
// 零成本机理实验（P1 配套）：mock provider 下模拟「用户连续聊天、摘要管
// 线逐轮追赶」的动力学，判定 340 条消息会话「根节点永远建不出来」的根因
// 属于哪一类：
//   (a) MAX_REQUESTS=16 单轮配额太小 → 纯粹收敛慢，轮间 cache_hits 线性
//       增长、节点无废料；
//   (b) 分组边界漂移 → 缓存命中率回落或多代 leaf 废料堆积。
// 运行：cargo test --lib p1_chase -- --ignored --nocapture
#[test]
#[ignore = "P1 机理实验：观察逐轮追赶动力学"]
fn p1_chase_dynamics_experiment() {
    let (_db, store, conversation) = fixture();
    let body = serde_json::to_string(&json!({
        "summary": "mock 摘要文本，覆盖本轮输入的主要事实、决定与未解决问题。",
        "coverage_notes": "零成本实验",
        "open_questions": []
    }))
    .unwrap();
    let provider = FakeProvider::new(AiReply {
        content: body,
        tool_calls: Vec::new(),
        model: Some("mock".into()),
        usage: None,
        reasoning_content: None,
        finish_reason: None,
    });
    let pad = "对话与决定占位：血糖记录复盘、饮食调整与随访安排，细节保持稳定。".repeat(4);
    let rounds = 40;
    for round in 0..rounds {
        store
            .send_message(
                &conversation,
                "user",
                &format!("第{round}轮提问：{pad}"),
                None,
            )
            .unwrap();
        store
            .send_message(
                &conversation,
                "assistant",
                &format!("第{round}轮回答：{pad}"),
                None,
            )
            .unwrap();
        let mut context =
            crate::ai::memory::prepare_uncompressed_context(&conversation, &store).unwrap();
        let before = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM chat_summary_nodes WHERE conversation_id=?1",
                [&conversation],
                |r| r.get::<_, i64>(0),
            )
            .unwrap();
        let provider_calls_before = provider.requests.get();
        let result = super::apply_summary(
            &store,
            &conversation,
            &provider,
            "mock",
            "mock-basis",
            "mock",
            8_000,
            &mut context,
        );
        let after = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM chat_summary_nodes WHERE conversation_id=?1",
                [&conversation],
                |r| r.get::<_, i64>(0),
            )
            .unwrap();
        if round < 6 || round == 39 {
            println!(
                "轮 {round:>2}：摘要调用 {}，节点 {before:>3}→{after:>3}，结果 {}",
                provider.requests.get() - provider_calls_before,
                if result.is_ok() { "OK" } else { "ERR" },
            );
        }
        if round == 39 {
            let (hits, reqs) = store
                .connection
                .query_row(
                    "SELECT payload_json FROM chat_run_events WHERE kind='summary_unavailable'
                     ORDER BY created_at DESC LIMIT 1",
                    [],
                    |r| r.get::<_, String>(0),
                )
                .map(|payload| {
                    let value: serde_json::Value = serde_json::from_str(&payload).unwrap();
                    (
                        value["cache_hits"].as_i64().unwrap_or_default(),
                        value["requests"].as_i64().unwrap_or_default(),
                    )
                })
                .unwrap_or((0, 0));
            println!("最后一条 summary_unavailable：cache_hits={hits} requests={reqs}");
            let total: i64 = store
                .connection
                .query_row("SELECT COUNT(*) FROM chat_summary_nodes", [], |r| r.get(0))
                .unwrap();
            println!("全库节点总数：{total}");
        }
    }
}

use std::io::{Read as _, Write as _};

#[test]
fn background_preheat_advances_summary_tree_outside_generation_turns() {
    // P1 追赶修复(b)：后台 tick 以生产同参参数推进摘要树，用户生成轮
    // 零消耗。provider 注入（FakeProvider），HTTP 链路留给解锁态验收。
    let dir = std::env::temp_dir().join(format!("elsewhen-preheat-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let store = Store::open(&dir.join("preheat.db")).unwrap();
    let conversation = store
        .create_conversation(Some("预热 fixture"), None)
        .unwrap();
    let pad = "历史细节与随访决定，保证上下文足够大。".repeat(20);
    for round in 0..12 {
        store
            .send_message(
                &conversation,
                "user",
                &format!("第{round}轮提问：{pad}"),
                None,
            )
            .unwrap();
        store
            .send_message(
                &conversation,
                "assistant",
                &format!("第{round}轮回答：{pad}"),
                None,
            )
            .unwrap();
    }
    let provider = FakeProvider::new(good_reply());
    let nodes_before: i64 = store
        .connection
        .query_row("SELECT COUNT(*) FROM chat_summary_nodes", [], |r| r.get(0))
        .unwrap();
    // 生产同款 budget 尺度（window 128K 级）：触发线必然被 12 轮上下文跨过。
    let advanced = background_preheat_with(
        &store,
        &conversation,
        &provider,
        "mock",
        "mock-basis",
        "gpt-4o",
        8_000,
    )
    .unwrap();
    let nodes_after: i64 = store
        .connection
        .query_row("SELECT COUNT(*) FROM chat_summary_nodes", [], |r| r.get(0))
        .unwrap();
    assert!(advanced > 0, "预热应推进至少一个摘要节点");
    assert_eq!(nodes_after - nodes_before, advanced);
    // auto mark 已建：二次预热走边界 drain 后的早退/稳态小批，不重算全史。
    let mark: i64 = store
        .connection
        .query_row(
            "SELECT COUNT(*) FROM conversation_page_marks WHERE kind='auto'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(mark > 0, "预热成功后应固化为 auto 翻篇标记");
    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
}
