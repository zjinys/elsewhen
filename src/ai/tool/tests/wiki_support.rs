use super::*;
use crate::{
    event::NewEvent,
    storage::{WikiPage, WikiPageDraft},
};

struct Fixture {
    store: Store,
    dir: std::path::PathBuf,
    conv: String,
}
impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("wiki-support-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(&dir.join("test.db")).unwrap();
        let conv = store.create_conversation(Some("test"), None).unwrap();
        Self { store, dir, conv }
    }
    fn page(&self, slug: &str, title: &str, body: &str) -> WikiPage {
        let at = chrono::Utc::now();
        let event = self
            .store
            .insert_event(NewEvent {
                raw_text: body,
                occurred_at: at,
                recorded_at: at,
                source: "capture",
            })
            .unwrap();
        self.store
            .upsert_wiki_page(
                &WikiPageDraft {
                    slug: slug.into(),
                    title: title.into(),
                    content_md: body.into(),
                    summary: body.into(),
                    kind: "method".into(),
                    tags: vec![],
                    source_event_ids: vec![event],
                    source_url: None,
                    status: "active".into(),
                    reason: "fixture".into(),
                },
                ContentPolicy::Always,
            )
            .unwrap()
            .page
    }
    fn call(&self, name: &str, args: Value) -> ToolResultMsg {
        dispatch(
            &ToolCall::new(name, args),
            &ToolRegistry::default(),
            &self.store,
            &self.conv,
        )
    }
    fn revision(&self, target: &WikiPage, evidence: &WikiPage) -> ToolResultMsg {
        let citation =
            crate::knowledge::citation_for_page(&self.store, evidence, "fixture".into(), 1000)
                .unwrap()
                .unwrap();
        dispatch_with_knowledge(
            &ToolCall::new(
                "save_wiki_revision",
                json!({"slug":target.slug,"title":target.title,"content_md":"旧经验仍有效。新增经验：询问客户最近一次实际行为。","change_note":"新增访谈经验","save_as":"revision","source_slugs":[evidence.slug]}),
            ),
            &ToolRegistry::default(),
            &self.store,
            &self.conv,
            &[citation],
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn wiki_support_paraphrases_fuse_without_duplicates_and_keep_eligibility() {
    let f = Fixture::new();
    let page = f.page(
        "method/interview",
        "客户访谈方法",
        "询问客户最近一次实际行为，不收集未来购买承诺。",
    );
    f.store
        .confirm_authored_metadata_in_tx(
            &page.id,
            "适用于小店初期验证，不能据此估算市场规模。",
            "reference",
        )
        .unwrap();
    let args = json!({"query":"How can I learn what shop owners need?","alternate_queries":["客户访谈","实际行为","客户访谈"]});
    let result = f.call("search_knowledge_base", args.clone());
    assert!(result.success, "{}", result.content);
    let value: Value = serde_json::from_str(&result.content).unwrap();
    let hits = value["knowledge_candidates"].as_array().unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["page_slug"], page.slug);
    assert!(
        hits[0]["applicable_when"]
            .as_str()
            .unwrap()
            .contains("不能据此估算")
    );
    // A lexical condition mismatch no longer silently discards valid methods.
    assert!(
        crate::knowledge::select_knowledge(&f.store, "实际行为", "test", 4000)
            .unwrap()
            .iter()
            .any(|c| c.page_slug == page.slug)
    );
    f.store
        .connection
        .execute(
            "UPDATE wiki_pages SET status='archived' WHERE id=?1",
            [&page.id],
        )
        .unwrap();
    let value: Value =
        serde_json::from_str(&f.call("search_knowledge_base", args).content).unwrap();
    assert!(value["knowledge_candidates"].as_array().unwrap().is_empty());
    assert!(
        !f.call(
            "search_knowledge_base",
            json!({"query":"a","alternate_queries":["b","c","d","e"]})
        )
        .success
    );
}

#[test]
fn wiki_support_revision_appends_new_evidence_and_records_dependency() {
    let f = Fixture::new();
    let old = f.page("method/old", "客户访谈方法", "旧经验仍有效。");
    let added = f.page(
        "method/new",
        "线下访谈观察",
        "询问最近一次实际行为能避免未来承诺失真。",
    );
    assert!(f.revision(&old, &added).success);
    let pending = f.store.pending_actions_for_conversation(&f.conv).unwrap();
    assert_eq!(
        f.store
            .get_wiki_page(&old.slug)
            .unwrap()
            .unwrap()
            .content_md,
        old.content_md
    );
    let args: Value = serde_json::from_str(&pending[0].args_json).unwrap();
    assert_eq!(args["verified_evidence"][0]["page_slug"], added.slug);
    execute_pending_action(&f.store, &pending[0]).unwrap();
    let updated = f.store.get_wiki_page(&old.slug).unwrap().unwrap();
    assert!(updated.source_event_ids.contains(&old.source_event_ids[0]));
    assert!(
        updated
            .source_event_ids
            .contains(&added.source_event_ids[0])
    );
    assert!(
        crate::knowledge::select_knowledge(&f.store, "实际行为", "test", 4000)
            .unwrap()
            .iter()
            .any(|c| c.page_slug == old.slug)
    );
    f.store
        .save_wiki_page_content(&added.slug, "用户纠正了上游经验", "manual", None)
        .unwrap();
    assert!(crate::knowledge::dependencies::stale(&f.store, &old.id).unwrap());
    assert!(
        !crate::knowledge::select_knowledge(&f.store, "实际行为", "test", 4000)
            .unwrap()
            .iter()
            .any(|c| c.page_slug == old.slug)
    );
}

#[test]
fn wiki_support_stale_new_evidence_rejects_whole_revision() {
    let f = Fixture::new();
    let old = f.page("method/old", "旧方法", "旧正文");
    let new = f.page("method/new", "新观察", "原始新增观察");
    assert!(f.revision(&old, &new).success);
    let pending = f.store.pending_actions_for_conversation(&f.conv).unwrap();
    f.store
        .save_wiki_page_content(&new.slug, "已经更正的观察", "manual", None)
        .unwrap();
    assert!(execute_pending_action(&f.store, &pending[0]).is_err());
    let after = f.store.get_wiki_page(&old.slug).unwrap().unwrap();
    assert_eq!(after.content_md, old.content_md);
    assert_eq!(after.source_event_ids, old.source_event_ids);
}

#[test]
fn wiki_support_only_current_input_can_be_added_as_an_event() {
    let f = Fixture::new();
    let old = f.page("method/old", "旧方法", "旧正文");
    let mut args = json!({"slug":old.slug,"title":old.title,"content_md":"补入用户明确说的访谈经历","change_note":"新增用户事实","save_as":"revision","include_current_event":true});
    assert!(!f.call("save_wiki_revision", args.clone()).success);
    f.store
        .submit_conversation_input(
            &f.conv,
            "今天访谈了三位店主，他们描述了最近一次实际记账经历",
            Some("capture"),
        )
        .unwrap();
    assert!(f.call("save_wiki_revision", args.clone()).success);
    let pending = f.store.pending_actions_for_conversation(&f.conv).unwrap();
    let data: Value = serde_json::from_str(&pending[0].args_json).unwrap();
    assert!(
        data["added_input_evidence"]["excerpt"]
            .as_str()
            .unwrap()
            .contains("三位店主")
    );
    execute_pending_action(&f.store, &pending[0]).unwrap();
    let after = f.store.get_wiki_page(&old.slug).unwrap().unwrap();
    assert_eq!(after.source_event_ids.len(), 2);
    args["include_current_event"] = json!(false);
    args["source_slugs"] = json!([old.slug]);
    assert!(!f.call("save_wiki_revision", args).success);
}

#[test]
fn wiki_support_repeated_content_is_reused_but_distinct_purpose_is_reviewable() {
    let f = Fixture::new();
    let old = f.page(
        "method/old",
        "客户访谈方法",
        "先询问最近一次实际行为，再核对发生场景；不要把未来购买承诺当成已发生的事实。",
    );
    let mut args = json!({"title":"小店需求调研技巧","content_md":old.content_md});
    let hint = f.call("save_knowledge_draft", args.clone());
    assert!(hint.success);
    assert!(hint.content.contains("可能可复用"));
    assert!(hint.content.contains(&old.slug));
    assert!(
        f.store
            .pending_actions_for_conversation(&f.conv)
            .unwrap()
            .is_empty()
    );
    args["distinct_reason"] = json!("用户明确要求为医疗机构另建有独立适用范围的培训页");
    assert!(f.call("save_knowledge_draft", args).success);
    assert_eq!(
        f.store
            .pending_actions_for_conversation(&f.conv)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn wiki_support_noop_and_duplicate_pending_revisions_do_not_write() {
    let f = Fixture::new();
    let old = f.page("method/old", "客户访谈方法", "旧正文");
    let mut args = json!({"slug":old.slug,"title":old.title,"content_md":old.content_md,"change_note":"没有变化","save_as":"revision"});
    assert!(
        f.call("save_wiki_revision", args.clone())
            .content
            .contains("无新增")
    );
    assert!(
        f.store
            .pending_actions_for_conversation(&f.conv)
            .unwrap()
            .is_empty()
    );
    args["content_md"] = json!("新正文");
    assert!(f.call("save_wiki_revision", args.clone()).success);
    let again = f.call("save_wiki_revision", args);
    assert!(again.success);
    assert!(again.content.contains("未重复生成"));
    assert_eq!(
        f.store
            .pending_actions_for_conversation(&f.conv)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn wiki_support_evidence_failure_rolls_back_content_and_preserves_rule_strength() {
    let f = Fixture::new();
    let old = f.page("method/old", "访谈规则", "旧规则正文");
    f.store
        .confirm_authored_metadata_in_tx(&old.id, "小店访谈场景", "rule")
        .unwrap();
    let new = f.page("method/new", "独立观察", "不同观察的来源依据");
    assert!(f.revision(&old, &new).success);
    let pending = f.store.pending_actions_for_conversation(&f.conv).unwrap();
    f.store.connection.execute_batch("CREATE TRIGGER fail_new_dependency BEFORE INSERT ON knowledge_dependencies BEGIN SELECT RAISE(ABORT,'synthetic'); END;").unwrap();
    assert!(execute_pending_action(&f.store, &pending[0]).is_err());
    assert_eq!(
        f.store
            .get_wiki_page(&old.slug)
            .unwrap()
            .unwrap()
            .content_md,
        old.content_md
    );
    assert_eq!(
        f.store.knowledge_metadata(&old.slug).unwrap().strength,
        "rule"
    );
    f.store
        .connection
        .execute_batch("DROP TRIGGER fail_new_dependency;")
        .unwrap();
    execute_pending_action(&f.store, &pending[0]).unwrap();
    assert_eq!(
        f.store.knowledge_metadata(&old.slug).unwrap().strength,
        "rule"
    );
}

#[test]
fn wiki_support_changed_new_evidence_rejects_confirmation_and_fresh_evidence_succeeds() {
    let f = Fixture::new();
    let target = f.page("method/old", "目标方法", "原方法正文");
    let evidence = f.page(
        "method/new",
        "新访谈材料",
        "材料原文：询问实际行为，并保留条件",
    );
    assert!(f.revision(&target, &evidence).success);
    let action = f
        .store
        .pending_actions_for_conversation(&f.conv)
        .unwrap()
        .remove(0);
    f.store
        .save_wiki_page_content(
            &evidence.slug,
            "材料新版本：增加了不同适用条件",
            "manual",
            None,
        )
        .unwrap();
    assert!(execute_pending_action(&f.store, &action).is_err());
    assert_eq!(
        f.store
            .get_wiki_page(&target.slug)
            .unwrap()
            .unwrap()
            .content_md,
        target.content_md
    );
    let fresh = f.store.get_wiki_page(&evidence.slug).unwrap().unwrap();
    assert!(f.revision(&target, &fresh).success);
    let actions = f.store.pending_actions_for_conversation(&f.conv).unwrap();
    let fresh_action = actions.iter().find(|a| a.id != action.id).unwrap();
    execute_pending_action(&f.store, fresh_action).unwrap();
    let updated = f.store.get_wiki_page(&target.slug).unwrap().unwrap();
    assert!(updated.source_event_ids.len() >= 2);
}

#[test]
fn wiki_support_confirm_rechecks_reuse_and_added_evidence_is_not_editable() {
    let f = Fixture::new();
    let args = json!({"title":"一次新的访谈归纳","content_md":"询问最近一次实际行为，记录发生时间、背景与限制；不把未来承诺当成真实需求。"});
    assert!(f.call("save_knowledge_draft", args.clone()).success);
    let action = f
        .store
        .pending_actions_for_conversation(&f.conv)
        .unwrap()
        .remove(0);
    f.page(
        "method/other",
        "另一个名字的相同知识",
        args["content_md"].as_str().unwrap(),
    );
    assert!(execute_pending_action(&f.store, &action).is_err());
    assert!(
        f.store
            .find_wiki_page_by_title("一次新的访谈归纳")
            .unwrap()
            .is_none()
    );
    let target = f.page("method/target", "需更新的方法", "原有的方法正文");
    let evidence = f.page("method/evidence", "独立观察", "新增观察的真实证据");
    assert!(f.revision(&target, &evidence).success);
    let action = f
        .store
        .pending_actions_for_conversation(&f.conv)
        .unwrap()
        .into_iter()
        .find(|a| a.action == "save_wiki_revision")
        .unwrap();
    let proposal = f
        .store
        .chat_proposal("action", &action.id, Some(&f.conv))
        .unwrap()
        .unwrap();
    let mut edited: Value = serde_json::from_str(&proposal.args_json).unwrap();
    edited["verified_evidence"] = json!([]);
    assert!(
        f.store
            .edit_chat_proposal(
                "action",
                &proposal.id,
                Some(&f.conv),
                proposal.version,
                &proposal.approval_token,
                &edited.to_string()
            )
            .is_err()
    );
}

#[test]
fn wiki_support_source_snapshot_is_bound_and_old_snapshot_cannot_be_accepted() {
    let f = Fixture::new();
    let target = f.page("method/old", "目标方法", "原方法正文");
    let mut source = WikiPageDraft {
        slug: "source/new".into(),
        title: "新访谈材料".into(),
        content_md: "材料原文：询问实际行为，并保留条件".into(),
        summary: "原文摘要".into(),
        kind: "source".into(),
        tags: vec![],
        source_event_ids: vec![],
        source_url: Some("https://example.invalid/interview".into()),
        status: "active".into(),
        reason: "fixture".into(),
    };
    let page = f
        .store
        .upsert_wiki_page(&source, ContentPolicy::Always)
        .unwrap()
        .page;
    f.store
        .set_wiki_opinion(&page.slug, Some("endorse"))
        .unwrap();
    let page = f.store.get_wiki_page(&page.slug).unwrap().unwrap();
    assert!(f.revision(&target, &page).success);
    let action = f
        .store
        .pending_actions_for_conversation(&f.conv)
        .unwrap()
        .remove(0);
    source.content_md = "材料新版本：增加了不同适用条件".into();
    f.store
        .upsert_wiki_page(&source, ContentPolicy::Always)
        .unwrap();
    assert!(execute_pending_action(&f.store, &action).is_err());
    assert_eq!(
        f.store
            .get_wiki_page(&target.slug)
            .unwrap()
            .unwrap()
            .content_md,
        target.content_md
    );
    let fresh = f.store.get_wiki_page(&page.slug).unwrap().unwrap();
    assert!(f.revision(&target, &fresh).success);
    let actions = f.store.pending_actions_for_conversation(&f.conv).unwrap();
    let fresh_action = actions.iter().find(|a| a.id != action.id).unwrap();
    execute_pending_action(&f.store, fresh_action).unwrap();
    assert_eq!(
        f.store.page_source_snapshots(&target.slug).unwrap()[0].version,
        2
    );
    assert!(
        f.store
            .get_wiki_page(&target.slug)
            .unwrap()
            .unwrap()
            .source_event_ids
            .contains(&target.source_event_ids[0])
    );
}
