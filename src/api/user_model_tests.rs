use super::*;

use super::*;
use crate::ai::provider::AiReply;
use crate::ai::tool::ToolSpec;
use crate::event::NewEvent;
use crate::storage::Store;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_database() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "elsewhen-user-model-test-{}.db",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

struct AnalysisStub(&'static str);

impl crate::ai::provider::AiProvider for AnalysisStub {
    fn generate_reply_with_tools(
        &self,
        _: Vec<ContextMessage>,
        tools: Option<&[ToolSpec]>,
    ) -> Result<AiReply> {
        assert!(tools.is_none());
        Ok(AiReply::text(self.0))
    }
}

fn reply(people: &[&str], projects: &[&str]) -> String {
    format!(
        r#"{{"schema_version":"event-analysis-v3","recordable":true,"kind":"event","event_type":"note","confidence":0.9,"summary":"s","clarifications":[],"people":{p},"projects":{q},"activities":[],"follow_ups":[]}}"#,
        p = serde_json::to_string(people).unwrap(),
        q = serde_json::to_string(projects).unwrap(),
    )
}

fn page_for_test(store: &Store, kind: &str, title: &str) {
    let slug = crate::wiki::slugify(title);
    store
        .upsert_wiki_page(
            &crate::storage::WikiPageDraft {
                slug,
                kind: kind.to_string(),
                title: title.to_string(),
                summary: String::new(),
                content_md: format!("# {title}"),
                tags: vec![],
                source_event_ids: vec![],
                status: "active".to_string(),
                reason: "test".to_string(),
                source_url: None,
            },
            crate::storage::ContentPolicy::Always,
        )
        .unwrap();
}

#[test]
fn annotated_pair_auto_upserts_without_draft_card() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    page_for_test(&store, "person", "李四");
    let slug = crate::wiki::slugify("网站改版");
    page_for_test(&store, "topic", "网站改版");
    let _ = slug;
    // @/# 双标注：权威组合，页面齐备 → 直接增量入库
    store
        .insert_event(NewEvent::now("@李四 负责 #网站改版 上线了"))
        .unwrap();
    let provider = AnalysisStub(Box::leak(reply(&["李四"], &["网站改版"]).into_boxed_str()));
    assert_eq!(process_analysis_queue(&store, &provider).unwrap(), 1);
    let relations = store.list_relations().unwrap();
    let stats = store.analysis_job_stats().unwrap();
    eprintln!("DEBUG stats={stats:?} relations={relations:?}");
    {
        let mut stmt = store
            .connection
            .prepare("SELECT event_id, COALESCE(last_error,'') FROM analysis_jobs")
            .unwrap();
        let errs: Vec<(String, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(|x| x.unwrap())
            .collect();
        eprintln!("DEBUG jobs={errs:?}");
    }
    assert_eq!(relations.len(), 1, "标注组合应自动入库");
    assert_eq!(relations[0].from_slug, crate::wiki::slugify("李四"));
    assert_eq!(relations[0].to_slug, crate::wiki::slugify("网站改版"));
    assert_eq!(relations[0].confidence, 4);
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn chat_shadow_queue_judges_pending_hits_and_skips_bad_payloads() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let rule_id = store
        .add_rule("X 项目联调要留痕", crate::storage::RuleStatus::Active, None)
        .unwrap();
    store
        .connection
        .execute(
            "UPDATE rules SET trigger_keywords=?2 WHERE id=?1",
            rusqlite::params![rule_id, r#"["X项目"]"#],
        )
        .unwrap();
    let conversation = store.create_conversation(None, None).unwrap();
    let message_id = store
        .send_message(&conversation, "user", "今天推进 X项目 联调时踩了个坑", None)
        .unwrap();
    store.screen_chat_message(&conversation, &message_id, "今天推进 X项目 联调时踩了个坑");
    assert_eq!(store.pending_chat_hits(10).unwrap().len(), 1);

    // 判定 provider 返回合规 JSON → pending 回填 applicable。
    let good = AnalysisStub(Box::leak(
        r#"{"applicable":true,"confidence":0.9}"#.to_string().into_boxed_str(),
    ));
    assert_eq!(process_chat_rule_check_queue(&store, &good).unwrap(), 1);
    let hits = store.recent_chat_rule_hits(10).unwrap();
    assert_eq!(hits[0].status, "applicable");
    assert_eq!(hits[0].confidence, Some(0.9));

    // 不合规回包（缺 applicable）：该条保持 pending 待下轮重试，不毒化。
    store
        .resolve_chat_hit(&format!("reset-{}", hits[0].id), true, None)
        .ok();
    store
        .connection
        .execute(
            "UPDATE chat_rule_hits SET status='pending' WHERE id=?1",
            rusqlite::params![hits[0].id],
        )
        .unwrap();
    let bad = AnalysisStub(Box::leak("不是 JSON 的回复".to_string().into_boxed_str()));
    assert_eq!(process_chat_rule_check_queue(&store, &bad).unwrap(), 0);
    assert_eq!(store.pending_chat_hits(10).unwrap().len(), 1);

    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn alias_hit_writes_canonical_slug_and_survives_merge_replay() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    // canonical 页「王大山」，分析输出用别名「小王」——归一管道写 canonical
    page_for_test(&store, "person", "王大山");
    store
        .add_entity_alias("person", &crate::wiki::slugify("王大山"), "小王")
        .unwrap();
    page_for_test(&store, "topic", "网站改版");
    store
        .insert_event(NewEvent::now("@小王 把 #网站改版 上线了"))
        .unwrap();
    let provider = AnalysisStub(Box::leak(reply(&["小王"], &["网站改版"]).into_boxed_str()));
    assert_eq!(process_analysis_queue(&store, &provider).unwrap(), 1);
    let relations = store.list_relations().unwrap();
    assert_eq!(relations.len(), 1, "别名命中应自动入库");
    assert_eq!(
        relations[0].from_slug,
        crate::wiki::slugify("王大山"),
        "必须写 canonical"
    );
    assert_eq!(relations[0].confidence, 4);

    // 归并后重放（新事件、同别名）：alias 已迁到目标页 → 边继续写新 canonical，
    // 不复活已合并页、不产生分裂的第二条边。
    page_for_test(&store, "person", "王大山二号");
    store
        .merge_entity(
            "person",
            &crate::wiki::slugify("王大山"),
            &crate::wiki::slugify("王大山二号"),
        )
        .unwrap();
    store
        .insert_event(NewEvent::now("@小王 又推动了 #网站改版 一步"))
        .unwrap();
    let provider2 = AnalysisStub(Box::leak(reply(&["小王"], &["网站改版"]).into_boxed_str()));
    assert_eq!(process_analysis_queue(&store, &provider2).unwrap(), 1);
    let relations = store.list_relations().unwrap();
    assert_eq!(relations.len(), 1, "重放不得另立第二条边");
    assert_eq!(
        relations[0].from_slug,
        crate::wiki::slugify("王大山二号"),
        "重放边应指向合并后的 canonical"
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn unannotated_or_missing_pages_do_not_auto_upsert() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    page_for_test(&store, "person", "王五");
    page_for_test(&store, "person", "赵六");
    // 无标注：即使页面存在也不自动入库（低置信走草拟）
    store
        .insert_event(NewEvent::now("和王五 赵六 开会聊了网站改版"))
        .unwrap();
    let provider = AnalysisStub(Box::leak(
        reply(&["王五", "赵六"], &["网站改版"]).into_boxed_str(),
    ));
    assert_eq!(process_analysis_queue(&store, &provider).unwrap(), 1);
    assert!(
        store.list_relations().unwrap().is_empty(),
        "未标注组合不得自动入库"
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}
