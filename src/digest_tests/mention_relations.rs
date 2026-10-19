use super::super::*;
use super::drain;
use super::job_status;
use super::make_ready;
use super::record;
use super::temporary_database;
use super::*;

#[test]
fn mention_relations_derive_from_tags_and_retract() {
    let t = TempStore::new();
    t.store
        .upsert_wiki_page(
            &WikiPageDraft {
                source_event_ids: vec![],
                tags: vec!["咖啡豆".into(), "重复项".into()],
                ..draft("routine/morning", "routine", "早晨流程", "- 冲咖啡", vec![])
            },
            ContentPolicy::Always,
        )
        .unwrap();
    t.store
        .upsert_wiki_page(
            &WikiPageDraft {
                source_event_ids: vec![],
                ..draft("asset/coffee", "asset", "咖啡豆", "- 每月一包", vec![])
            },
            ContentPolicy::Always,
        )
        .unwrap();
    let (added, removed) = t.store.rebuild_mention_relations().unwrap();
    assert_eq!((added, removed), (1, 0));
    let from: String = t
        .store
        .connection
        .query_row(
            "SELECT f.slug FROM knowledge_relations r JOIN wiki_pages f ON f.id=r.from_page_id",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(from, "routine/morning", "mentions 从提方指向被提方");
    // 幂等 + 事实消失则边回撤
    let (added, removed) = t.store.rebuild_mention_relations().unwrap();
    assert_eq!((added, removed), (0, 0));
    t.store
        .upsert_wiki_page(
            &WikiPageDraft {
                source_event_ids: vec![],
                tags: vec!["晨间".into()],
                ..draft("routine/morning", "routine", "早晨流程", "- 冲咖啡", vec![])
            },
            ContentPolicy::Always,
        )
        .unwrap();
    let (added, removed) = t.store.rebuild_mention_relations().unwrap();
    assert_eq!((added, removed), (0, 1), "tag 摘除后派生边回撤");
    // 人工边不受重建影响
    t.store
        .add_knowledge_relation("routine/morning", "asset/coffee", "related", None)
        .unwrap();
    let (added, removed) = t.store.rebuild_mention_relations().unwrap();
    let human_edges: i64 = t
        .store
        .connection
        .query_row(
            "SELECT COUNT(*) FROM knowledge_relations WHERE origin='human'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(human_edges, 1);
    assert_eq!((added, removed), (0, 0));
}

#[test]
fn relation_add_and_remove_have_guards() {
    let t = TempStore::new();
    t.store
        .upsert_wiki_page(
            &draft("a/one", "topic", "一", "- 内容", vec![]),
            ContentPolicy::Always,
        )
        .unwrap();
    t.store
        .upsert_wiki_page(
            &draft("a/two", "topic", "二", "- 内容", vec![]),
            ContentPolicy::Always,
        )
        .unwrap();
    let id1 = t
        .store
        .add_knowledge_relation("a/one", "a/two", "related", Some("同主题".into()))
        .unwrap();
    // 幂等：重复添加返回既有边
    let id2 = t
        .store
        .add_knowledge_relation("a/one", "a/two", "related", None)
        .unwrap();
    assert_eq!(id1, id2);
    // 自环 / 未知页 / 派生边不可删
    assert!(
        t.store
            .add_knowledge_relation("a/one", "a/one", "related", None)
            .is_err()
    );
    assert!(
        t.store
            .add_knowledge_relation("a/one", "a/ghost", "related", None)
            .is_err()
    );
    assert!(
        t.store
            .remove_knowledge_relation_from_page("a/one", "不存在的边")
            .is_err()
    );
    // 人工边可删
    t.store
        .remove_knowledge_relation_from_page("a/one", &id1)
        .unwrap();
    let count: i64 = t
        .store
        .connection
        .query_row("SELECT COUNT(*) FROM knowledge_relations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
    // 上限 12
    for i in 0..12 {
        let slug = format!("a/n{i}");
        t.store
            .upsert_wiki_page(
                &draft(&slug, "topic", &format!("第{i}页"), "- x", vec![]),
                ContentPolicy::Always,
            )
            .unwrap();
        t.store
            .add_knowledge_relation("a/one", &slug, "related", None)
            .unwrap();
    }
    assert!(
        t.store
            .add_knowledge_relation("a/two", "a/one", "related", None)
            .is_ok(),
        "不同页各自计上限"
    );
    assert!(
        t.store
            .add_knowledge_relation("a/one", "a/two", "part_of", None)
            .is_err(),
        "同页第 13 条被拒"
    );
}
