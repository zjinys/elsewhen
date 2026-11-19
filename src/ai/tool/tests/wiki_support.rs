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
            Default::default(),
        )
    }
    /// 带结果预算的调用：模拟代理循环量出余量后交给工具。
    fn call_with_budget(&self, name: &str, args: Value, tokens: usize) -> ToolResultMsg {
        dispatch_with_knowledge(
            &ToolCall::new(name, args),
            &ToolRegistry::default(),
            &self.store,
            &self.conv,
            &[],
            ToolRuntime {
                provider: None,
                result_budget: Some((tokens, "fixture")),
            },
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn wiki_support_long_page_reading_reaches_tail_without_mixing_versions() {
    let f = Fixture::new();
    let body = format!(
        "{}结尾要求：不带手，不切镜头。",
        "编织🧵过程。".repeat(1500)
    );
    let page = f.page("method/long-embroidery", "编织视频要求", &body);
    let hash = crate::storage::knowledge::content_hash(&body);
    let mut offset = 0;
    let mut collected = String::new();
    loop {
        let mut args = json!({"slug":page.slug,"offset":offset});
        if offset > 0 {
            args["content_hash"] = hash.clone().into();
        }
        let result = f.call("get_wiki_page", args);
        assert!(result.success, "{}", result.content);
        let value: Value = serde_json::from_str(&result.content).unwrap();
        let read = &value["knowledge_reading"][&page.slug];
        assert_eq!(read["coverage"], "excerpt");
        assert_eq!(read["start_char"], offset);
        assert_eq!(read["total_chars"], body.chars().count());
        assert_eq!(read["content_hash"], hash);
        let text = value["knowledge_candidates"][0]["excerpt"]
            .as_str()
            .unwrap();
        assert!(text.chars().count() <= 8000);
        assert_eq!(read["end_char"], offset + text.chars().count());
        collected.push_str(text);
        let Some(next) = read["next_offset"].as_u64() else {
            break;
        };
        assert!(next > offset as u64);
        offset = next as usize;
    }
    assert_eq!(collected, body);
    assert!(collected.ends_with("不切镜头。"));
    for args in [
        json!({"slug":page.slug,"offset":-1}),
        json!({"slug":page.slug,"offset":8000}),
        json!({"slug":page.slug,"offset":body.chars().count()+1,"content_hash":hash}),
    ] {
        assert!(!f.call("get_wiki_page", args).success);
    }
    f.store
        .save_wiki_page_content(&page.slug, "新版本，不可与旧文拼接。", "manual", None)
        .unwrap();
    let stale = f.call(
        "get_wiki_page",
        json!({"slug":page.slug,"offset":8000,"content_hash":hash}),
    );
    assert!(!stale.success);
    assert!(stale.content.contains("wiki_read_version_changed"));
}

#[test]
fn wiki_support_page_reading_fits_the_remaining_request_budget() {
    let f = Fixture::new();
    let body = "预算内分段读取的正文。".repeat(2000);
    let page = f.page("method/budgeted", "预算读取", &body);
    let hash = crate::storage::knowledge::content_hash(&body);
    // 结果作为 JSON 字符串嵌进请求后的成本。
    let cost = |result: &ToolResultMsg| {
        crate::ai::budget::text_tokens(
            "fixture",
            &Value::String(result.content.clone()).to_string(),
        )
    };

    // 余量充足：与不带预算时一样返回 8000 字符。
    let roomy = f.call_with_budget("get_wiki_page", json!({"slug":page.slug}), 100_000);
    let value: Value = serde_json::from_str(&roomy.content).unwrap();
    assert_eq!(value["knowledge_reading"][&page.slug]["end_char"], 8000);

    // 余量有限：每段嵌入请求后的成本不超过预算，片段首尾相接直到读完。
    let budget = 6000;
    let mut offset = 0;
    let mut collected = String::new();
    loop {
        let mut args = json!({"slug":page.slug,"offset":offset});
        if offset > 0 {
            args["content_hash"] = hash.clone().into();
        }
        let result = f.call_with_budget("get_wiki_page", args, budget);
        assert!(result.success, "{}", result.content);
        assert!(cost(&result) <= budget);
        let value: Value = serde_json::from_str(&result.content).unwrap();
        let read = &value["knowledge_reading"][&page.slug];
        let text = value["knowledge_candidates"][0]["excerpt"]
            .as_str()
            .unwrap();
        assert_eq!(read["start_char"], offset);
        assert_eq!(read["end_char"], offset + text.chars().count());
        collected.push_str(text);
        let Some(next) = read["next_offset"].as_u64() else {
            break;
        };
        // 没读完的片段不低于最小长度，也没有用满 8000。
        assert!((1500..8000).contains(&text.chars().count()));
        offset = next as usize;
    }
    assert_eq!(collected, body);

    // 余量连最小片段都放不下：明确报错，不返回注定被截断的碎片。
    let exhausted = f.call_with_budget("get_wiki_page", json!({"slug":page.slug}), 1200);
    assert!(!exhausted.success);
    assert!(exhausted.content.contains("wiki_read_budget_exhausted"));
    // 页尾剩下的一小段读得完时照常返回。
    let tail = body.chars().count() - 300;
    let last = f.call_with_budget(
        "get_wiki_page",
        json!({"slug":page.slug,"offset":tail,"content_hash":hash}),
        2000,
    );
    assert!(last.success, "{}", last.content);
}

#[test]
fn knowledge_search_matches_events_by_terms_not_the_whole_query() {
    let f = Fixture::new();
    let at = chrono::Utc::now();
    let long = format!(
        "{}周三复盘：SQLite 写入失败后没有回滚，导致重复扣款。{}",
        "铺垫。".repeat(200),
        "后记。".repeat(50)
    );
    for text in [long.as_str(), "今天散步了四十分钟。"] {
        f.store
            .insert_event(NewEvent {
                raw_text: text,
                occurred_at: at,
                recorded_at: at,
                source: "capture",
            })
            .unwrap();
    }
    let result = f.call("search_knowledge_base", json!({"query":"SQLite 回滚 扣款"}));
    assert!(result.success, "{}", result.content);
    let value: Value = serde_json::from_str(&result.content).unwrap();
    let events = value["events"].as_array().unwrap();
    assert_eq!(events.len(), 1, "多词问法按词命中，无关事件不返回");
    let excerpt = events[0]["excerpt"].as_str().unwrap();
    assert!(
        excerpt.contains("没有回滚"),
        "摘录取命中处而不是事件开头：{excerpt}"
    );
    assert!(excerpt.chars().count() <= 352);
    // 没有长页命中时不提示改用合成工具。
    assert!(value.get("suggest").is_none());
}

#[test]
fn knowledge_search_points_to_synthesis_when_hits_exceed_what_one_turn_can_read() {
    let f = Fixture::new();
    f.page("method/short-note", "短页", "SQLite 事务要点很短。");
    let small = f.call("search_knowledge_base", json!({"query":"SQLite 事务"}));
    let value: Value = serde_json::from_str(&small.content).unwrap();
    assert_eq!(value["knowledge_candidates"].as_array().unwrap().len(), 1);
    assert!(value.get("suggest").is_none());

    f.page(
        "method/long-manual",
        "长手册",
        &"SQLite 事务的长篇说明。".repeat(900),
    );
    let large = f.call("search_knowledge_base", json!({"query":"SQLite 事务"}));
    let value: Value = serde_json::from_str(&large.content).unwrap();
    assert_eq!(value["suggest"], "synthesize_knowledge");
}

#[test]
fn wiki_support_reading_coverage_keeps_unverified_material_unattributed() {
    let f = Fixture::new();
    let page = f.page(
        "method/short-embroidery",
        "编织要求",
        "不带手，不切镜头；未限定针。",
    );
    let result = f.call("get_wiki_page", json!({"slug":page.slug}));
    assert!(result.success, "{}", result.content);
    let value: Value = serde_json::from_str(&result.content).unwrap();
    assert_eq!(value["knowledge_reading"][&page.slug]["coverage"], "full");
    assert!(value["knowledge_reading"][&page.slug]["next_offset"].is_null());
    assert_eq!(value["knowledge_candidates"][0]["excerpt"], page.content_md);
    f.store
        .connection
        .execute(
            "UPDATE wiki_pages SET status='archived' WHERE id=?1",
            [&page.id],
        )
        .unwrap();
    let result = f.call("get_wiki_page", json!({"slug":page.slug}));
    assert!(result.success, "{}", result.content);
    let value: Value = serde_json::from_str(&result.content).unwrap();
    assert!(value["knowledge_candidates"].as_array().unwrap().is_empty());
    assert_eq!(value["unverified_page"]["content_md"], page.content_md);
    assert_eq!(value["unverified_page"]["reading"]["coverage"], "full");
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

#[test]
fn wiki_support_search_delivers_rejections_via_paraphrases_separately() {
    let f = Fixture::new();
    let page = f
        .store
        .upsert_wiki_page(
            &WikiPageDraft {
                slug: "source/rejected-marketing".into(),
                title: "旧推广方案".into(),
                content_md: "推广投放面向大型客户。".into(),
                summary: "推广投放".into(),
                kind: "source".into(),
                tags: vec![],
                source_event_ids: vec![],
                source_url: Some("https://example.com/rejected-marketing".into()),
                status: "active".into(),
                reason: "fixture".into(),
            },
            ContentPolicy::Always,
        )
        .unwrap()
        .page;
    crate::knowledge::workflows::decide_source(
        &f.store,
        &page.slug,
        "reject",
        "当前预算不足，不适合小店客户群。",
    )
    .unwrap();
    let result = f.call(
        "search_knowledge_base",
        json!({"query":"acquisition experiments", "alternate_queries":["推广投放"]}),
    );
    assert!(result.success, "{}", result.content);
    let value: Value = serde_json::from_str(&result.content).unwrap();
    assert!(value["knowledge_candidates"].as_array().unwrap().is_empty());
    assert_eq!(value["knowledge_warnings"].as_array().unwrap().len(), 1);
    assert_eq!(value["knowledge_warnings"][0]["page_slug"], page.slug);
    assert_eq!(
        value["knowledge_warnings"][0]["rejection_reason"],
        "当前预算不足，不适合小店客户群。"
    );
    assert!(
        crate::knowledge::candidates_in_context(&[crate::ai::memory::ContextMessage::new(
            "tool",
            result.content
        )])
        .is_empty()
    );
    assert_eq!(
        f.store.get_wiki_page(&page.slug).unwrap().unwrap().status,
        "archived"
    );
}
