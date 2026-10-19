//! Real-model quality checks are explicitly ignored in ordinary test runs.
use super::*;
use crate::ai::tool::ToolRegistry;
use crate::storage::{ContentPolicy, WikiPageDraft, chat_page_tasks::PageTask};
use anyhow::{Context, ensure};
use serde_json::{Value, json};

#[path = "acceptance_controls.rs"]
mod controls;

struct LiveFixture {
    store: Store,
    directory: std::path::PathBuf,
    provider: OpenAiCompatibleConfig,
}
impl LiveFixture {
    fn new() -> Self {
        let provider: OpenAiCompatibleConfig = serde_json::from_str(
            &std::env::var("ELSEWHEN_ACCEPTANCE_PROVIDER_JSON")
                .expect("explicit synthetic acceptance only"),
        )
        .unwrap();
        let directory =
            std::env::temp_dir().join(format!("elsewhen-live-workflows-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let store = Store::open(&directory.join("synthetic.db")).unwrap();
        let id = store
            .save_ai_provider_config(
                None,
                "synthetic-acceptance",
                "openai-compatible",
                "https://synthetic.invalid/v1",
                &provider.model,
                "",
                provider.temperature as f64,
                provider.max_tokens.map(i64::from),
            )
            .unwrap();
        store
            .connection
            .execute(
                "UPDATE ai_provider_configs SET context_window=?1 WHERE id=?2",
                rusqlite::params![provider.context_window.map(i64::from), id],
            )
            .unwrap();
        Self {
            store,
            directory,
            provider,
        }
    }

    fn page(&self, slug: &str, title: &str, text: &str, kind: &str) -> crate::storage::WikiPage {
        self.store
            .upsert_wiki_page(
                &WikiPageDraft {
                    slug: slug.into(),
                    kind: kind.into(),
                    title: title.into(),
                    summary: title.into(),
                    content_md: text.into(),
                    tags: vec!["合成验收".into()],
                    source_event_ids: vec![],
                    status: "active".into(),
                    reason: "synthetic fixture".into(),
                    source_url: None,
                },
                ContentPolicy::Always,
            )
            .unwrap()
            .page
    }

    fn run(
        &self,
        conversation: &str,
        input: &str,
        scenario: &str,
        config: Option<ConversationConfig>,
    ) -> Result<(String, String)> {
        let owner = uuid::Uuid::new_v4().to_string();
        let run = self.store.create_chat_run(
            conversation,
            input,
            &uuid::Uuid::new_v4().to_string(),
            &owner,
        )?;
        let mut snapshot = self.store.snapshot_for_input(conversation, input)?;
        snapshot.chat_run_identity = Some(crate::ai::generation::RunIdentity {
            id: run.id.clone(),
            owner: owner.clone(),
        });
        let mut runtime = self.store.list_ai_provider_configs_for_runtime()?.remove(0);
        runtime.base_url = self.provider.base_url.clone();
        runtime.api_key = self.provider.api_key.clone();
        let started = std::time::Instant::now();
        let answer = generate_with_provider_configs(conversation, &snapshot, config, vec![runtime]);
        self.store.finish_chat_run(
            &run.id,
            &owner,
            if answer.is_ok() {
                "completed"
            } else {
                "failed"
            },
            answer.as_ref().ok().map(String::as_str),
            None,
        )?;
        let usage: (Option<i64>,Option<i64>) = self.store.connection.query_row("SELECT SUM(prompt_tokens),SUM(completion_tokens) FROM chat_run_requests WHERE run_id=?1",[&run.id],|r|Ok((r.get(0)?,r.get(1)?)))?;
        let events = self.store.chat_run_events(&run.id, 0, 256)?;
        let derived: Vec<Value> = events
            .iter()
            .filter(|e| {
                matches!(
                    e.kind.as_str(),
                    "summary_ready"
                        | "summary_omitted"
                        | "summary_unavailable"
                        | "page_task"
                        | "page_task_progress"
                )
            })
            .filter_map(|e| serde_json::from_str(&e.payload_json).ok())
            .collect();
        println!(
            "CHAT_REAL_ACCEPTANCE {}",
            json!({"fixture":"synthetic-chat-v2","scenario":scenario,"model":self.provider.model,"answer":answer.as_ref().ok(),"generation_ok":answer.is_ok(),"elapsed_ms":started.elapsed().as_millis(),"prompt_tokens":usage.0,"completion_tokens":usage.1,"requests":self.store.chat_run(&run.id)?.unwrap().request_count,"derived_context":derived})
        );
        Ok((
            answer.map_err(|_| {
                anyhow::anyhow!("synthetic generation failed; see scenario diagnostics")
            })?,
            run.id,
        ))
    }

    fn tool_succeeded(&self, run: &str, name: &str) -> bool {
        self.store
            .chat_run_events(run, 0, 256)
            .unwrap()
            .iter()
            .any(|e| {
                e.kind == "tool_result"
                    && serde_json::from_str::<Value>(&e.payload_json)
                        .is_ok_and(|v| v["name"] == name && v["success"] == true)
            })
    }

    fn task(
        &self,
        conversation: &str,
        page: &crate::storage::WikiPage,
        kind: &str,
        prompt: &str,
    ) -> String {
        self.store
            .save_page_task_input(
                conversation,
                prompt,
                &uuid::Uuid::new_v4().to_string(),
                &PageTask {
                    kind: kind.into(),
                    pages: vec![self.store.page_task_basis(&page.slug).unwrap().1],
                    start_char: None,
                    end_char: None,
                },
            )
            .unwrap()
    }
}
impl Drop for LiveFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

#[test]
#[ignore = "requires explicit permission; synthetic real-model main-chat tool review acceptance"]
fn real_model_chat_workflow_acceptance() {
    review_workflow(false);
}

#[test]
#[ignore = "requires explicit permission; synthetic real-model wiki-chat tool review acceptance"]
fn real_model_chat_wiki_review_acceptance() {
    review_workflow(true);
}

fn review_workflow(wiki: bool) {
    let fixture = LiveFixture::new();
    let scenario = if wiki {
        "wiki_tool_review"
    } else {
        "main_tool_review"
    };
    let result = (|| -> Result<()> {
        let page = fixture.page(&format!("synthetic/{scenario}"),"合成安全流程","合成验收材料：紫杉夹具只能在离线环境校准；扭矩阈值为 17 N·m。完成校准后检查防护盖，不得对公网发布。","source");
        let conversation = if wiki {
            fixture
                .store
                .create_wiki_chat_conversation(&page.slug, scenario)?
        } else {
            fixture.store.create_conversation(Some(scenario), None)?
        };
        for accept in [false, true] {
            let mut prompt = format!(
                "这是一项合成测试。请先调用 get_wiki_page 读取 {}，然后调用 save_wiki_revision，以 save_as=derivative 为该页草拟一份独立安全检查清单。必须保留 17 N·m、离线校准和不得对公网发布的限制。仅建立待审稿，不要说已执行保存；不要创建待办或规则。",
                page.slug
            );
            if accept {
                prompt.push_str("上一份草稿已由我拒绝。现在我明确要求重新生成一份新的待审稿；此前拒绝只针对上一份，不代表取消本次新请求。这仍不是保存授权，必须等待我对新稿点击接受。");
            }
            let input = if wiki {
                fixture.task(&conversation, &page, "local_question", &prompt)
            } else {
                fixture
                    .store
                    .send_message(&conversation, "user", &prompt, None)?
            };
            let (answer, run) = fixture.run(&conversation, &input, scenario, None)?;
            ensure!(
                fixture.tool_succeeded(&run, "get_wiki_page")
                    && fixture.tool_succeeded(&run, "save_wiki_revision"),
                "must execute retrieval and draft tools, not merely claim them"
            );
            let proposal = fixture
                .store
                .list_chat_proposals(Some(&conversation), 0, 50)?
                .into_iter()
                .find(|p| p.action == "save_wiki_revision")
                .context("missing review draft")?;
            let args: Value = serde_json::from_str(&proposal.args_json)?;
            let draft = args["content_md"].as_str().context("missing draft body")?;
            ensure!(
                draft.contains("17") && draft.contains("离线") && draft.contains("公网"),
                "draft lost safety constraints"
            );
            let decision = if accept { "accept" } else { "reject" };
            let receipt = crate::ai::approval::resolve_chat_proposal(
                &fixture.store,
                &proposal.kind,
                &proposal.id,
                Some(&conversation),
                proposal.version,
                &proposal.approval_token,
                decision,
            )?;
            let repeated = crate::ai::approval::resolve_chat_proposal(
                &fixture.store,
                &proposal.kind,
                &proposal.id,
                Some(&conversation),
                proposal.version,
                &proposal.approval_token,
                decision,
            )?;
            ensure!(
                receipt.status == repeated.status && receipt.result == repeated.result,
                "review retry must return the same receipt"
            );
            ensure!(
                receipt.status == if accept { "succeeded" } else { "declined" },
                "review did not reach expected outcome"
            );
            ensure!(
                fixture.store.get_wiki_page(&page.slug)?.unwrap().content_md == page.content_md,
                "derivative review changed original"
            );
            println!(
                "CHAT_REAL_QUALITY {}",
                json!({"scenario":scenario,"decision":decision,"passed":true,"draft":draft,"answer":answer,"receipt_status":receipt.status})
            );
        }
        Ok(())
    })();
    result.unwrap_or_else(|error| panic!("{scenario}: {error}"));
}

#[test]
#[ignore = "requires explicit permission; synthetic real-model summary and exact recall acceptance"]
fn real_model_chat_summary_acceptance() {
    let fixture = LiveFixture::new();
    let result = (|| -> Result<()> {
        let conversation = fixture
            .store
            .create_conversation(Some("long history"), None)?;
        let first = fixture.store.send_message(&conversation,"user","合成任务目标：离线校准紫杉夹具。预算上限 7300 元。禁止公网发布。尚未完成的是尾部回归。验收原话：SYNTHETIC-CONSTRAINT-7300。",None)?;
        for i in 0..2 {
            fixture.store.send_message(
                &conversation,
                "user",
                &format!(
                    "第{i}次合成讨论；不改变最初约束。{}",
                    "讨论夹具材料、温度测试和线缆布置，仅为重复背景，不表示任务完成。".repeat(60)
                ),
                None,
            )?;
            fixture.store.send_message(
                &conversation,
                "assistant",
                "这是合成历史建议，不代表已经执行。",
                None,
            )?;
        }
        let prompt = format!(
            "请回读消息 {first} 的原文（调用 read_conversation_history），然后列出最初预算、发布限制、未完成项和验收原话。不要把历史建议写成已完成，不创建任何记录或草稿。"
        );
        let input = fixture
            .store
            .send_message(&conversation, "user", &prompt, None)?;
        let before = fixture
            .store
            .list_messages(&conversation)?
            .into_iter()
            .map(|m| (m.id, m.content))
            .collect::<Vec<_>>();
        // Trigger summary selection without changing the real provider's final
        // window. A fixed total budget can be swallowed by evolving tool schemas.
        let definitions = ToolRegistry::default()
            .provider_specs_for(true)
            .into_iter()
            .map(|spec| {
                json!({"type":"function","function":{
                    "name":spec.name,"description":spec.description,"parameters":spec.parameters
                }})
            })
            .collect::<Vec<_>>();
        let tool_reserve = crate::ai::budget::text_tokens(
            &fixture.provider.model,
            &serde_json::to_string(&definitions)?,
        );
        let config = || {
            Some(ConversationConfig {
                memory_type: MemoryType::SlidingWindow {
                    max_tokens: tool_reserve + 6000,
                },
                provider_type: ProviderType::OpenAiCompatible,
            })
        };
        let (answer, run) = fixture.run(
            &conversation,
            &input,
            "long_history_original_read",
            config(),
        )?;
        ensure!(
            ["7300", "公网", "回归", "SYNTHETIC-CONSTRAINT-7300"]
                .iter()
                .all(|value| answer.contains(value)),
            "late answer lost original constraints"
        );
        ensure!(
            fixture.tool_succeeded(&run, "read_conversation_history"),
            "exact recall did not read original"
        );
        let nodes: i64 = fixture.store.connection.query_row(
            "SELECT COUNT(*) FROM chat_summary_nodes WHERE conversation_id=?1",
            [&conversation],
            |r| r.get(0),
        )?;
        ensure!(nodes > 0, "fixture must actually trigger derived summaries");
        ensure!(
            fixture
                .store
                .chat_run_events(&run, 0, 256)?
                .iter()
                .any(|event| event.kind == "summary_ready"),
            "summary must enter the request, not merely leave cached leaves"
        );
        let (_, cached_run) =
            fixture.run(&conversation, &input, "long_history_cached_retry", config())?;
        ensure!(
            fixture
                .store
                .chat_run_events(&cached_run, 0, 256)?
                .iter()
                .any(|event| event.kind == "summary_ready"
                    && serde_json::from_str::<Value>(&event.payload_json)
                        .is_ok_and(|value| value["requests"] == 0
                            && value["cache_hits"].as_u64().unwrap_or(0) > 0)),
            "unchanged history must reuse a valid summary without another summary request"
        );
        for (id, content) in before {
            let actual: String = fixture.store.connection.query_row(
                "SELECT content FROM messages WHERE id=?1",
                [id],
                |r| r.get(0),
            )?;
            ensure!(actual == content, "summary mutated original");
        }
        println!(
            "CHAT_REAL_QUALITY {}",
            json!({"scenario":"long_history_original_read","passed":true,"summary_nodes":nodes})
        );
        Ok(())
    })();
    result.expect("real-model summary quality failed");
}

#[test]
#[ignore = "requires explicit permission; synthetic real-model full-page coverage acceptance"]
fn real_model_chat_full_page_acceptance() {
    let fixture = LiveFixture::new();
    let result = (|| -> Result<()> {
        let text = format!(
            "# 合成全文覆盖验收\n开头决定：使用靛蓝夹具，标记 HEAD-INDIGO-17。\n{}\n中部约束：禁止公网发布，标记 MIDDLE-OFFLINE-42。\n{}\n结尾唯一例外：冬季必须降低扭矩，标记 TAIL-WINTER-93。",
            "合成背景段落讨论材料寿命、温度和库存，并不改变任何约束。\n".repeat(80),
            "合成补充段落讨论试验记录和常规复核，不表示试验已经完成。\n".repeat(80)
        );
        let page = fixture.page(
            "synthetic/full-coverage",
            "合成全文覆盖验收",
            &text,
            "topic",
        );
        let conversation = fixture
            .store
            .create_wiki_chat_conversation(&page.slug, "full coverage")?;
        let input=fixture.task(&conversation,&page,"full_summary","请总结全文，必须覆盖开头决定、中部限制和结尾唯一例外。逐字列出三处英文标记。只回答，不保存、不调用写工具。");
        let (answer, _) = fixture.run(&conversation, &input, "full_page_tail_coverage", None)?;
        ensure!(
            ["HEAD-INDIGO-17", "MIDDLE-OFFLINE-42", "TAIL-WINTER-93"]
                .iter()
                .all(|value| answer.contains(value)),
            "full reading missed a source region"
        );
        ensure!(
            answer.contains("冬季") && answer.contains("扭矩"),
            "tail exception lost"
        );
        ensure!(
            fixture.store.get_wiki_page(&page.slug)?.unwrap().content_md == text,
            "summary mutated page"
        );
        println!(
            "CHAT_REAL_QUALITY {}",
            json!({"scenario":"full_page_tail_coverage","passed":true,"source_characters":text.chars().count()})
        );
        Ok(())
    })();
    result.expect("real-model full-page quality failed");
}
