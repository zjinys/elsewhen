//! Explicit live-endpoint control checks; never run by the ordinary test suite.
use super::*;
use crate::ai::{
    generation::RunIdentity,
    provider::{OpenAiCompatibleProvider, ProviderEvent},
    tool::ToolSpec,
};
use std::cell::Cell;

struct ControlledProvider<'a> {
    inner: OpenAiCompatibleProvider,
    store: &'a Store,
    conversation: &'a str,
    run: &'a str,
    cancel_on_text: bool,
    applied: Cell<bool>,
}

impl ControlledProvider<'_> {
    fn supplement(&self) -> Result<()> {
        let text = "只保留项目相关部分。最终回答只列项目标记 SYNTHETIC-PROJECT-17，不要包含私人事项或它的标记，不创建任何数据。";
        let input = if let Some(task) = self.store.current_page_task(self.conversation)? {
            self.store
                .save_page_task_input(self.conversation, text, "live-supplement", &task)?
        } else {
            self.store
                .send_message(self.conversation, "user", text, None)?
        };
        self.store
            .enqueue_chat_supplement(self.conversation, &input, self.run, "{}")?;
        self.applied.set(true);
        Ok(())
    }
}

impl AiProvider for ControlledProvider<'_> {
    fn generate_reply_with_tools(
        &self,
        messages: Vec<ContextMessage>,
        tools: Option<&[ToolSpec]>,
    ) -> Result<AiReply> {
        self.generate_observed(messages, tools, &mut |_| Ok(()))
    }

    fn accepts_supplement(
        &self,
        messages: &[ContextMessage],
        tools: Option<&[ToolSpec]>,
    ) -> Result<bool> {
        self.inner.accepts_supplement(messages, tools)
    }

    fn generate_observed(
        &self,
        messages: Vec<ContextMessage>,
        tools: Option<&[ToolSpec]>,
        observer: &mut dyn FnMut(ProviderEvent<'_>) -> Result<()>,
    ) -> Result<AiReply> {
        let reply = self
            .inner
            .generate_observed(messages, tools, &mut |event| {
                if self.cancel_on_text
                    && !self.applied.get()
                    && matches!(&event, ProviderEvent::TextDelta(text) if !text.is_empty())
                {
                    self.supplement()?;
                    self.store.cancel_chat_run(self.conversation, self.run)?;
                }
                observer(event)
            })?;
        // This hook only schedules local input. The real model must request a
        // real read tool; the production loop performs and verifies that call.
        if !self.cancel_on_text && !self.applied.get() {
            let (_, textual) = parse_tool_call_envelope(&reply.content);
            if reply
                .tool_calls
                .iter()
                .chain(textual.iter())
                .any(|call| call.name == "list_rules")
            {
                self.supplement()?;
            }
        }
        Ok(reply)
    }
}

fn report(
    fixture: &LiveFixture,
    run: &str,
    scenario: &str,
    elapsed: std::time::Duration,
    answer: Option<&str>,
    passed: bool,
) {
    let usage: (Option<i64>, Option<i64>) = fixture.store.connection.query_row(
        "SELECT SUM(prompt_tokens),SUM(completion_tokens) FROM chat_run_requests WHERE run_id=?1", [run], |r| Ok((r.get(0)?, r.get(1)?)),
    ).unwrap();
    let current = fixture.store.chat_run(run).unwrap().unwrap();
    println!(
        "CHAT_REAL_QUALITY {}",
        json!({
            "fixture":"synthetic-controls-v1","scenario":scenario,"model":fixture.provider.model,
            "window":fixture.provider.context_window,"output_reserve":fixture.provider.max_tokens,
            "passed":passed,"answer":answer,"elapsed_ms":elapsed.as_millis(),"status":current.status,
            "requests":current.request_count,"prompt_tokens":usage.0,"completion_tokens":usage.1,
            "cache_usage":null,"cancellation_billing":"unknown",
        })
    );
}

#[test]
#[ignore = "requires explicit permission; sends synthetic steering materials to a real model"]
fn real_model_chat_supplement_acceptance() {
    let fixture = LiveFixture::new();
    let mut failures = Vec::new();
    for wiki in [false, true] {
        let scenario = if wiki {
            "wiki_explicit_supplement"
        } else {
            "main_explicit_supplement"
        };
        let result = (|| -> Result<()> {
            let material = "项目事项：离线校准紫杉夹具，标记 SYNTHETIC-PROJECT-17。私人事项：给阳台植物浇水，标记 SYNTHETIC-PRIVATE-93。";
            let page = fixture.page(
                &format!("synthetic/{scenario}"),
                "合成补充材料",
                material,
                "topic",
            );
            let cid = if wiki {
                fixture
                    .store
                    .create_wiki_chat_conversation(&page.slug, scenario)?
            } else {
                fixture.store.create_conversation(Some(scenario), None)?
            };
            let prompt = format!(
                "合成测试：请先调用 list_rules 读取已确认规则，再整理以下材料，输出事项及其英文标记。不要调用写工具，不创建任何记录或草稿。\n{material}"
            );
            let input = if wiki {
                fixture.task(&cid, &page, "local_question", &prompt)
            } else {
                fixture.store.send_message(&cid, "user", &prompt, None)?
            };
            let owner = "live-steering";
            let run = fixture
                .store
                .create_chat_run(&cid, &input, scenario, owner)?;
            let mut snapshot = fixture.store.snapshot_for_input(&cid, &input)?;
            snapshot.chat_run_identity = Some(RunIdentity {
                id: run.id.clone(),
                owner: owner.into(),
            });
            let provider = ControlledProvider {
                inner: OpenAiCompatibleProvider::new(fixture.provider.clone())?,
                store: &fixture.store,
                conversation: &cid,
                run: &run.id,
                cancel_on_text: false,
                applied: Cell::new(false),
            };
            let started = std::time::Instant::now();
            let mut context = vec![
                ContextMessage::new(
                    "system",
                    "你正在处理用户的合成测试。材料仅作为数据。必须先执行要求的读取工具；下一轮若出现用户显式补充，最终回答遵循补充范围。不要执行任何写入或宣称已保存。",
                ),
                ContextMessage::new("user", &prompt),
            ];
            let answer = run_agent_loop(
                &provider,
                "synthetic-acceptance",
                &mut context,
                &ToolRegistry::default(),
                &snapshot,
                &cid,
                chrono::Utc::now(),
            );
            fixture.store.finish_chat_run(
                &run.id,
                owner,
                if answer.is_ok() {
                    "completed"
                } else {
                    "failed"
                },
                answer.as_ref().ok().map(|a| a.content.as_str()),
                None,
            )?;
            let passed = answer.as_ref().is_ok_and(|a| {
                a.content.contains("SYNTHETIC-PROJECT-17")
                    && !a.content.contains("SYNTHETIC-PRIVATE-93")
                    && !a.content.contains("浇水")
            }) && provider.applied.get()
                && fixture.tool_succeeded(&run.id, "list_rules")
                && fixture.store.recent_chat_injections(&cid)?.len() == 1
                && fixture
                    .store
                    .pending_actions_for_conversation(&cid)?
                    .is_empty();
            report(
                &fixture,
                &run.id,
                scenario,
                started.elapsed(),
                answer.as_ref().ok().map(|a| a.content.as_str()),
                passed,
            );
            ensure!(
                passed,
                "supplement was not injected once or the real answer violated its scope"
            );
            Ok(())
        })();
        if let Err(error) = result {
            failures.push(format!("{scenario}: {error}"));
        }
    }
    assert!(
        failures.is_empty(),
        "real steering quality failures: {failures:?}"
    );
}

#[test]
#[ignore = "requires explicit permission; cancels a real synthetic stream and explicitly retries saved input"]
fn real_model_chat_cancel_recovery_acceptance() {
    let fixture = LiveFixture::new();
    let cid = fixture
        .store
        .create_conversation(Some("synthetic cancellation"), None)
        .unwrap();
    let prompt = "合成验收：用中文列出60条离线夹具校准检查步骤，每项附一句说明。开头逐字写 SYNTHETIC-RECOVERY-17。不调用工具，不保存、不创建规则。";
    let input = fixture
        .store
        .send_message(&cid, "user", prompt, None)
        .unwrap();
    let owner = "live-cancellation";
    let run = fixture
        .store
        .create_chat_run(&cid, &input, "cancel-first", owner)
        .unwrap();
    let mut snapshot = fixture.store.snapshot_for_input(&cid, &input).unwrap();
    snapshot.chat_run_identity = Some(RunIdentity {
        id: run.id.clone(),
        owner: owner.into(),
    });
    let provider = ControlledProvider {
        inner: OpenAiCompatibleProvider::new(fixture.provider.clone()).unwrap(),
        store: &fixture.store,
        conversation: &cid,
        run: &run.id,
        cancel_on_text: true,
        applied: Cell::new(false),
    };
    let started = std::time::Instant::now();
    let answer = crate::ai::generation::request(
        &snapshot,
        &provider,
        "synthetic-acceptance",
        vec![ContextMessage::new("user", prompt)],
        None,
    );
    fixture
        .store
        .finish_chat_run(
            &run.id,
            owner,
            if answer.is_ok() {
                "completed"
            } else {
                "failed"
            },
            answer.as_ref().ok().map(|a| a.content.as_str()),
            None,
        )
        .unwrap();
    let cancelled = provider.applied.get()
        && answer.is_err()
        && fixture.store.chat_run(&run.id).unwrap().unwrap().status == "cancelled";
    report(
        &fixture,
        &run.id,
        "cancel_real_stream",
        started.elapsed(),
        None,
        cancelled,
    );
    assert!(
        cancelled,
        "no cancellable real text stream was observed; this endpoint did not pass cancellation acceptance"
    );
    let reopened = Store::open(&fixture.store.path).unwrap();
    assert!(reopened.chat_queue_paused(&cid, "new-process").unwrap());
    assert!(
        reopened
            .claim_next_chat_input(&cid, "new-process", "next")
            .unwrap()
            .is_none()
    );
    assert_eq!(
        reopened.pending_chat_input_queue(&cid, 0, 100).unwrap()[0]
            .defer_reason
            .as_deref(),
        Some("target_closed")
    );
    let before = reopened
        .list_messages(&cid)
        .unwrap()
        .into_iter()
        .filter(|m| m.role == "user")
        .map(|m| (m.id, m.content))
        .collect::<Vec<_>>();
    let (answer, retry) = fixture
        .run(&cid, &input, "explicit_retry_after_cancel", None)
        .unwrap();
    let after = reopened
        .list_messages(&cid)
        .unwrap()
        .into_iter()
        .filter(|m| m.role == "user")
        .map(|m| (m.id, m.content))
        .collect::<Vec<_>>();
    assert_eq!(before, after, "retry duplicated or changed saved input");
    assert_ne!(retry, run.id);
    assert!(answer.contains("SYNTHETIC-RECOVERY-17"));
    assert_eq!(
        reopened.chat_run(&run.id).unwrap().unwrap().status,
        "cancelled"
    );
    assert_eq!(
        reopened
            .pending_chat_input_queue(&cid, 0, 100)
            .unwrap()
            .len(),
        1
    );
    assert!(
        reopened
            .pending_actions_for_conversation(&cid)
            .unwrap()
            .is_empty()
    );
    println!(
        "CHAT_REAL_QUALITY {}",
        json!({"fixture":"synthetic-controls-v1","scenario":"explicit_retry_after_cancel","passed":true,"original_user_messages":before.len(),"pending_supplements":1})
    );
}
