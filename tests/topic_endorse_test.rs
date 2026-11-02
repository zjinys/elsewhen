use elsewhen::storage::Store;
use elsewhen::wiki;

#[test]
fn save_topic_page_endorse_should_not_bail() {
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
