//! The same scripted behaviors run in both languages. Replies are synthetic;
//! these tests verify execution/review/continuity, not real model quality.
use super::super::super::{continuity::FollowUp, run_agent_loop};
use super::super::continuity_tests::{RecordingProvider, native_reply};
use super::*;
use crate::ai::tool::ToolRegistry;

#[test]
fn bilingual_question_retrieval_draft_refusal_and_failure_preserve_execution_semantics() {
    for (locale, question, answer, request, draft_reply, refusal, failure_reply) in [
        (
            "zh-Hans",
            "这条规则是什么意思？",
            "先核对事实，再决定。",
            "请保存成笔记",
            "草稿已准备，尚未保存。",
            "不要保存",
            "页面不存在，所以这一步没有完成。",
        ),
        (
            "en",
            "What does this rule mean?",
            "Check the facts before deciding.",
            "Please save this as a note.",
            "The draft is ready and awaits review.",
            "Do not save it.",
            "The page is missing, so this step did not complete.",
        ),
    ] {
        let (store, _path) = temporary_database();
        store
            .save_prompt_language_preferences(Some(locale), "fixed", Some(locale), 0)
            .unwrap();
        let _scope = store.prompt_scope("preview", "bilingual-behavior").unwrap();
        let conversation = store
            .create_conversation(Some("same scenario"), None)
            .unwrap();
        store
            .add_rule(
                "用户原始规则 <<body>>",
                crate::storage::RuleStatus::Active,
                None,
            )
            .unwrap();
        let registry = ToolRegistry::default();
        let provider = RecordingProvider::new(vec![
            native_reply("", "list_rules", json!({})),
            AiReply::text(answer),
        ]);
        let mut context = vec![ContextMessage::new("user", question)];
        let result = run_agent_loop(
            &provider,
            "fixture",
            &mut context,
            &registry,
            &store,
            &conversation,
            chrono::Utc::now(),
        )
        .unwrap();
        assert_eq!(result.content, answer, "{locale}");
        assert_eq!(provider.requests.borrow().len(), 2);
        assert!(
            provider.requests.borrow()[1]
                .0
                .iter()
                .any(|m| m.role == "tool" && m.content.contains("用户原始规则 <<body>>"))
        );
        assert!(
            store
                .pending_actions_for_conversation(&conversation)
                .unwrap()
                .is_empty()
        );
        let provider = RecordingProvider::new(vec![
            native_reply(
                "",
                "save_knowledge_draft",
                json!({"title":"原文 Note","content_md":"Original source <<body>>"}),
            ),
            AiReply::text(draft_reply),
        ]);
        let mut context = vec![ContextMessage::new("user", request)];
        run_agent_loop(
            &provider,
            "fixture",
            &mut context,
            &registry,
            &store,
            &conversation,
            chrono::Utc::now(),
        )
        .unwrap();
        let drafts = store
            .pending_actions_for_conversation(&conversation)
            .unwrap();
        assert_eq!(drafts.len(), 1, "{locale}");
        assert!(
            store
                .find_wiki_page_by_title("原文 Note")
                .unwrap()
                .is_none()
        );
        let provider = RecordingProvider::new(vec![AiReply::text(draft_reply)]);
        let mut context = vec![ContextMessage::new("user", refusal)];
        run_agent_loop(
            &provider,
            "fixture",
            &mut context,
            &registry,
            &store,
            &conversation,
            chrono::Utc::now(),
        )
        .unwrap();
        assert_eq!(
            store
                .pending_actions_for_conversation(&conversation)
                .unwrap()[0]
                .id,
            drafts[0].id
        );
        assert!(
            store
                .list_chat_decisions(Some(&conversation), 0, 10)
                .unwrap()
                .is_empty()
        );
        let provider = RecordingProvider::new(vec![
            native_reply("", "get_wiki_page", json!({"slug":"missing-page-fixture"})),
            AiReply::text(failure_reply),
        ]);
        let mut context = vec![ContextMessage::new("user", question)];
        let result = run_agent_loop(
            &provider,
            "fixture",
            &mut context,
            &registry,
            &store,
            &conversation,
            chrono::Utc::now(),
        )
        .unwrap();
        assert_eq!(result.content, failure_reply);
        let requests = provider.requests.borrow();
        assert_eq!(requests.len(), 2);
        assert!(
            requests[1]
                .0
                .iter()
                .any(|m| m.role == "tool" && m.content.contains("没有找到该知识页")),
            "{locale}: {:?}",
            requests[1]
                .0
                .iter()
                .filter(|m| m.role == "tool")
                .map(|m| &m.content)
                .collect::<Vec<_>>()
        );
        assert!(store.list_events().unwrap().is_empty());
    }
}

#[test]
fn bilingual_progress_questions_keep_original_request_and_pending_review() {
    for (locale, request, promise, followup, marker) in [
        (
            "zh-Hans",
            "保存这份笔记，保留所有原文。",
            "稍等片刻，我准备处理。",
            "继续",
            "等待确认",
        ),
        (
            "en",
            "Save this note and preserve the original text.",
            "Please wait. I'll prepare the note.",
            "Any updates?",
            "awaiting review",
        ),
    ] {
        let (store, _path) = temporary_database();
        store
            .save_prompt_language_preferences(Some(locale), "fixed", Some(locale), 0)
            .unwrap();
        let _scope = store.prompt_scope("preview", "bilingual-followup").unwrap();
        let conversation = store
            .create_conversation(Some("same followup"), None)
            .unwrap();
        for (role, text) in [
            ("user", request),
            ("assistant", promise),
            ("user", followup),
        ] {
            store.send_message(&conversation, role, text, None).unwrap();
        }
        let previous = FollowUp::load(&store, &conversation).unwrap().unwrap();
        assert!(previous.unresolved, "{locale}");
        assert_eq!(previous.request, request);
        store
            .create_pending_action(
                &conversation,
                "save_knowledge_draft",
                &json!({"title":"原文 Note","content_md":"Source <<body>>"}).to_string(),
            )
            .unwrap();
        let reply = super::super::super::continuity::progress_fallback(
            &store,
            &conversation,
            &[],
            &[],
            None,
        )
        .unwrap();
        assert!(reply.contains(marker), "{locale}: {reply}");
        assert!(reply.contains("原文 Note"));
        assert!(reply.contains("Source <<body>>"));
        assert!(
            store
                .find_wiki_page_by_title("原文 Note")
                .unwrap()
                .is_none()
        );
        assert_eq!(
            store
                .pending_actions_for_conversation(&conversation)
                .unwrap()
                .len(),
            1
        );
    }
}

#[test]
fn bilingual_goal_profile_guidance_collects_only_explicit_facts_in_profile_page() {
    for (locale, query, reply, marker) in [
        (
            "zh-Hans",
            "我会 Rust，帮我规划第一个付费项目。",
            "个人资料已更新，先确定你想服务的客户。",
            "每轮只问最影响路线的一两个问题",
        ),
        (
            "en",
            "I know Rust. Help me plan my first paid project.",
            "Your profile is updated. Let's identify the customers you want to serve.",
            "one or two questions",
        ),
    ] {
        let (store, _path) = temporary_database();
        store
            .save_prompt_language_preferences(Some(locale), "fixed", Some(locale), 0)
            .unwrap();
        let _scope = store
            .prompt_scope("preview", "bilingual-goal-profile")
            .unwrap();
        let goal = store
            .create_goal("首个付费项目 <<phase>>", crate::storage::GoalPhase::Near)
            .unwrap();
        let conversation = store
            .create_goal_chat_conversation(&goal.id, "goal fixture")
            .unwrap();
        store
            .send_message(&conversation, "user", query, None)
            .unwrap();
        let mut context = crate::ai::memory::build_system_context(&store, &conversation).unwrap();
        assert!(
            context
                .iter()
                .any(|m| m.content.contains("profile/me") && m.content.contains(marker)),
            "{locale}"
        );
        context.push(ContextMessage::new("user", query));
        let provider = RecordingProvider::new(vec![
            native_reply("", "update_personal_profile", json!({"skills":"Rust"})),
            AiReply::text(reply),
        ]);
        let result = run_agent_loop(
            &provider,
            "fixture",
            &mut context,
            &ToolRegistry::default(),
            &store,
            &conversation,
            chrono::Utc::now(),
        )
        .unwrap();
        assert_eq!(result.content, reply);
        let (profile, _) = store.planning_profile().unwrap();
        assert_eq!(profile.skills, "Rust");
        assert!(
            profile.resources.is_empty()
                && profile.limits.is_empty()
                && profile.preferences.is_empty()
        );
        let page = store.get_wiki_page("profile/me").unwrap().unwrap();
        assert!(page.human_edited_at.is_some());
        assert_eq!(store.list_active_goals().unwrap()[0].content, goal.content);
        assert!(
            store
                .pending_actions_for_conversation(&conversation)
                .unwrap()
                .is_empty()
        );
        assert!(store.list_events().unwrap().is_empty());
        assert_eq!(provider.requests.borrow().len(), 2);
    }
}
