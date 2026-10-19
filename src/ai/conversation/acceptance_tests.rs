//! Explicit opt-in acceptance against a real provider, using synthetic data only.
use super::*;
use crate::{ai::provider::OpenAiCompatibleConfig, event::NewEvent};
use chrono::{Local, TimeZone};
use serde_json::{Value, json};

#[path = "acceptance_workflows.rs"]
mod workflows;

#[test]
#[ignore = "requires explicit permission and ELSEWHEN_ACCEPTANCE_PROVIDER_JSON; sends synthetic material to a real model"]
fn real_model_chat_time_acceptance() {
    let raw = std::env::var("ELSEWHEN_ACCEPTANCE_PROVIDER_JSON")
        .expect("Pass an explicitly authorized provider configuration; never open a personal database from this test");
    let provider: OpenAiCompatibleConfig =
        serde_json::from_str(&raw).expect("invalid acceptance configuration");
    let directory =
        std::env::temp_dir().join(format!("elsewhen-real-acceptance-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&directory).unwrap();
    let store = Store::open(&directory.join("synthetic.db")).unwrap();
    let provider_id = store
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
    // Use the authorized provider window without modifying its real configuration.
    store
        .connection
        .execute(
            "UPDATE ai_provider_configs SET context_window=?1 WHERE id=?2",
            rusqlite::params![provider.context_window.map(i64::from), provider_id],
        )
        .unwrap();
    let today = Local::now().date_naive();
    let yesterday = today.pred_opt().unwrap();
    let tomorrow = today.succ_opt().unwrap();
    for (date, text) in [
        (
            yesterday,
            "合成验收记录：完成紫杉样件的扭矩校准。校验号 SYNTHETIC-YESTERDAY-731。",
        ),
        (
            today,
            "合成验收记录：完成今日外壳检查。校验号 SYNTHETIC-TODAY-914。",
        ),
    ] {
        let at = Local
            .from_local_datetime(&date.and_hms_opt(12, 0, 0).unwrap())
            .earliest()
            .unwrap()
            .with_timezone(&chrono::Utc);
        store
            .insert_event(NewEvent {
                raw_text: text,
                occurred_at: at,
                recorded_at: at,
                source: "capture",
            })
            .unwrap();
    }
    let scenarios = [
        (
            "current_date",
            "今天是哪一天？请只回答应用当前日期，格式 YYYY-MM-DD。",
            today.to_string(),
        ),
        (
            "yesterday_records",
            "请查昨天的真实事件记录，列出任务与校验号，不要列今天的记录。",
            "SYNTHETIC-YESTERDAY-731".to_string(),
        ),
        (
            "tomorrow_deadline",
            "合成测试项目明天截止。请仅给出截止日期 YYYY-MM-DD，不创建待办或规则。",
            tomorrow.to_string(),
        ),
    ];
    let mut failures = Vec::new();
    for (scenario, prompt, expected) in scenarios {
        let conversation = store.create_conversation(Some(scenario), None).unwrap();
        let input = store
            .send_message(&conversation, "user", prompt, None)
            .unwrap();
        let owner = uuid::Uuid::new_v4().to_string();
        let run = store
            .create_chat_run(&conversation, &input, scenario, &owner)
            .unwrap();
        let mut snapshot = store.snapshot_for_input(&conversation, &input).unwrap();
        snapshot.chat_run_identity = Some(crate::ai::generation::RunIdentity {
            id: run.id.clone(),
            owner: owner.clone(),
        });
        let mut runtime = store
            .list_ai_provider_configs_for_runtime()
            .unwrap()
            .remove(0);
        runtime.base_url = provider.base_url.clone();
        runtime.api_key = provider.api_key.clone();
        let start = std::time::Instant::now();
        let result = generate_with_provider_configs(&conversation, &snapshot, None, vec![runtime]);
        let _ = store.finish_chat_run(
            &run.id,
            &owner,
            if result.is_ok() {
                "completed"
            } else {
                "failed"
            },
            result.as_ref().ok().map(String::as_str),
            if result.is_err() {
                Some(("generation", "synthetic acceptance request failed"))
            } else {
                None
            },
        );
        let run = store.latest_chat_run(&conversation).unwrap().unwrap();
        let mut read_evidence = false;
        for event in store.chat_run_events(&run.id, 0, 256).unwrap() {
            if event.kind == "tool_result" {
                let payload: Value = serde_json::from_str(&event.payload_json).unwrap();
                if payload["name"] == "list_events_by_date" && payload["success"] == true {
                    read_evidence = payload["content"].as_str().is_some_and(|text| {
                        text.contains("SYNTHETIC-YESTERDAY-731")
                            && !text.contains("SYNTHETIC-TODAY-914")
                    });
                }
            }
        }
        let (answer, error) = match result {
            Ok(answer) => (Some(answer), None),
            Err(_) => (
                None,
                Some(
                    run.error_kind
                        .clone()
                        .unwrap_or_else(|| "generation".into()),
                ),
            ),
        };
        let passed = answer.as_ref().is_some_and(|answer| {
            answer.contains(&expected)
                && (scenario != "yesterday_records"
                    || (!answer.contains("SYNTHETIC-TODAY-914") && read_evidence))
        });
        let usage: (Option<i64>,Option<i64>) = store.connection.query_row(
            "SELECT SUM(prompt_tokens),SUM(completion_tokens) FROM chat_run_requests WHERE run_id=?1", [&run.id], |r| Ok((r.get(0)?,r.get(1)?))).unwrap();
        println!(
            "CHAT_REAL_ACCEPTANCE {}",
            json!({"fixture":"synthetic-time-v1","scenario":scenario,"model":provider.model,
            "window":provider.context_window,"output_reserve":provider.max_tokens,"expected":expected,"answer":answer,
            "passed":passed,"verified_date_retrieval":read_evidence,"error_kind":error,"elapsed_ms":start.elapsed().as_millis(),
            "requests":run.request_count,"prompt_tokens":usage.0,"completion_tokens":usage.1,"cache_usage":null})
        );
        if !passed {
            failures.push(scenario);
        }
    }
    let stored_keys: i64 = store.connection.query_row(
        "SELECT COUNT(*) FROM ai_provider_configs WHERE COALESCE(api_key,'')<>'' OR base_url<>'https://synthetic.invalid/v1'", [], |r| r.get(0)).unwrap();
    assert_eq!(
        stored_keys, 0,
        "live credentials and endpoint never enter the temporary database"
    );
    drop(store);
    std::fs::remove_dir_all(directory).unwrap();
    assert!(
        failures.is_empty(),
        "real-model acceptance failed scenarios: {failures:?}"
    );
}
