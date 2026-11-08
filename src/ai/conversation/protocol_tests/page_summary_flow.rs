use super::super::super::*;
use super::protocol_http_fixture;
use super::*;

#[test]
fn manual_page_summary_reads_the_whole_imported_article_before_answering() {
    for saved_scope in [true, false] {
        let (store, path) = temporary_database();
        let original = format!(
            "{}。{}。尾部限制：仅在周末使用。",
            "首部事实".repeat(875),
            "中部事实".repeat(875)
        );
        let page =
            crate::wiki::save_text_page(&original, Some("七千字导入文章"), &[], &store).unwrap();
        assert_eq!(page.area, "imported");
        let conversation = store
            .create_wiki_chat_conversation(&page.slug, "summary")
            .unwrap();
        let task = crate::storage::chat_page_tasks::PageTask {
            kind: "local_question".into(),
            pages: vec![store.page_task_basis(&page.slug).unwrap().1],
            start_char: None,
            end_char: None,
        };
        if saved_scope {
            store
                .save_page_task_input(
                    &conversation,
                    "请总结这篇文章，不要只读前2800个字",
                    "manual-summary",
                    &task,
                )
                .unwrap();
        } else {
            store
                .send_message(
                    &conversation,
                    "user",
                    "请总结这篇文章，不要只读前2800个字",
                    None,
                )
                .unwrap();
        }
        let summaries = [
            "首部事实归纳。",
            "中部事实归纳。",
            "尾部限制：仅在周末使用。",
        ];
        let answer = "整页要点如下：首部与中部的事实需要结合阅读；文末明确限制仅在周末使用。";
        let mut responses: Vec<_> = summaries
            .iter()
            .map(|summary| {
                (
                    200,
                    json!({"choices":[{"message":{"content":summary},"finish_reason":"stop"}]}),
                )
            })
            .collect();
        responses.push((
            200,
            json!({"choices":[{"message":{"content":answer},"finish_reason":"stop"}]}),
        ));
        let (url, server) = protocol_http_fixture(responses);
        store
            .upsert_ai_provider_config(&url, "fixture", "fixture-key")
            .unwrap();
        let reply = generate_conversation_reply(&conversation, &store, None).unwrap();
        assert_eq!(reply, answer);
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 4);
        let reconstructed: String = requests[..3]
            .iter()
            .map(|request| {
                assert!(request.get("tools").is_none());
                let messages = request["messages"].as_array().unwrap();
                let chunk = messages.last().unwrap()["content"].as_str().unwrap();
                assert!(chunk.chars().count() <= 3000);
                chunk
            })
            .collect();
        assert_eq!(reconstructed, original);
        let material = requests[3]["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find_map(|message| {
                message["content"]
                    .as_str()
                    .filter(|text| text.contains("页面任务=full_summary"))
            })
            .unwrap();
        assert!(material.contains(&format!("覆盖本页全部 {} 字", original.chars().count())));
        for summary in summaries {
            assert!(material.contains(summary));
        }
        assert!(material.contains("非逐字原文"));
        assert!(!material.contains("局部问答自动选材"));
        assert_eq!(
            store.get_wiki_page(&page.slug).unwrap().unwrap().content_md,
            original
        );
        assert!(
            store
                .pending_actions_for_conversation(&conversation)
                .unwrap()
                .is_empty()
        );
        if saved_scope {
            assert_eq!(
                store.current_page_task(&conversation).unwrap().unwrap(),
                task
            );
        }
        drop(store);
        let _ = std::fs::remove_file(path);
    }
}
