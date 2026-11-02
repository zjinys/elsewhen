//! 「存为主题」回归测试（事故 2026-11-02）：save_text_page_with_kind(kind="topic")
//! 在入库后误调 set_wiki_opinion(endorse)，而该接口只接受 source/note 页，
//! 导致 UI 保存对话框报「只有采集素材页可以表态评价」——页面已入库但用户看到异常。
use elsewhen::storage::Store;
use elsewhen::wiki;

#[test]
fn save_topic_page_should_not_fail_on_endorse() {
    let db = std::env::temp_dir().join(format!(
        "elsewhen-topic-endorse-{}.db",
        uuid::Uuid::new_v4()
    ));
    let store = Store::open(&db).unwrap();
    let result = wiki::save_text_page_with_kind(
        "nvzhuang 评估正文",
        Some("一个女装换装的应用"),
        &["产品".to_string()],
        "topic",
        &store,
    );
    let _ = std::fs::remove_file(&db);
    let page = result.unwrap_or_else(|e| panic!("save topic failed: {e:#}"));
    assert_eq!(page.kind, "topic");
}

#[test]
fn save_note_page_still_endorses() {
    let db = std::env::temp_dir().join(format!(
        "elsewhen-note-endorse-{}.db",
        uuid::Uuid::new_v4()
    ));
    let store = Store::open(&db).unwrap();
    let result = wiki::save_text_page("随手记正文", Some("测试笔记"), &[], &store);
    let _ = std::fs::remove_file(&db);
    let page = result.unwrap_or_else(|e| panic!("save note failed: {e:#}"));
    assert_eq!(page.kind, "note");
    // note 页保留缺省 endorse（素材语义不变）
    let reloaded = store.get_wiki_page(&page.slug).unwrap().unwrap();
    assert_eq!(reloaded.opinion.as_deref(), Some("endorse"));
}
