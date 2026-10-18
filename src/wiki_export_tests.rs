//! 知识库 Markdown 导出（AIcove 吸收计划：数据主权/生态兼容）。
//! 口径红线：导出的是主列表页面（派生产物、归档、拒绝页不导出）。

use crate::storage::Store;

fn temp_db(label: &str) -> Store {
    let path = std::env::temp_dir().join(format!(
        "elsewhen-wiki-export-{}-{}.db",
        label,
        uuid::Uuid::new_v4()
    ));
    Store::open(&path).unwrap()
}

fn temp_out_dir(label: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "elsewhen-wiki-export-out-{}-{}",
        label,
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn insert_page(
    store: &Store,
    id: &str,
    slug: &str,
    title: &str,
    content: &str,
    extra: &[(&str, &str)],
) {
    let mut columns: Vec<String> = vec![
        "id".into(),
        "slug".into(),
        "kind".into(),
        "title".into(),
        "summary".into(),
        "content_md".into(),
        "first_seen_at".into(),
        "last_seen_at".into(),
        "created_at".into(),
        "updated_at".into(),
    ];
    let mut values: Vec<String> = vec![
        format!("'{id}'"),
        format!("'{slug}'"),
        "'habit'".into(),
        format!("'{title}'"),
        "'摘要'".into(),
        format!("'{content}'"),
        "'t'".into(),
        "'t'".into(),
        "'t'".into(),
        "'t'".into(),
    ];
    for (column, value) in extra {
        columns.push((*column).to_string());
        values.push(format!("'{value}'"));
    }
    store
        .connection
        .execute(
            &format!(
                "INSERT INTO wiki_pages({}) VALUES({})",
                columns.join(","),
                values.join(",")
            ),
            [],
        )
        .unwrap();
}

#[test]
fn exports_active_pages_with_frontmatter_and_skips_derived_archived_rejected() {
    let store = temp_db("mixed");
    insert_page(
        &store,
        "p1",
        "topic/fitness",
        "健身",
        "# 健身\n\n膝盖注意事项。",
        &[],
    );
    insert_page(
        &store,
        "p2",
        "der/summary",
        "旧产物",
        "派生正文",
        &[("area", "derivative"), ("based_on", "topic/fitness")],
    );
    insert_page(
        &store,
        "p3",
        "topic/old",
        "已归档",
        "归档正文",
        &[("status", "archived")],
    );
    insert_page(
        &store,
        "p4",
        "topic/no",
        "已拒绝",
        "拒绝正文",
        &[("opinion", "reject")],
    );

    let out = temp_out_dir("mixed");
    let exported = store.export_wiki_markdown(&out).unwrap();
    assert_eq!(exported, 1);

    let entries: Vec<_> = std::fs::read_dir(&out)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    // 仅主列表页；'/' 清洗为 '__'。
    assert_eq!(entries, vec!["topic__fitness.md".to_string()]);

    let text = std::fs::read_to_string(out.join("topic__fitness.md")).unwrap();
    assert!(text.starts_with("---\n"));
    assert!(text.contains(r#"title: "健身""#));
    assert!(text.contains(r#"slug: "topic/fitness""#));
    assert!(text.contains("evidence_count: 1"));
    assert!(text.contains("# 健身"));
}

#[test]
fn repeated_export_overwrites_without_duplicates() {
    let store = temp_db("repeat");
    insert_page(&store, "p1", "topic/notes", "笔记", "正文一", &[]);
    let out = temp_out_dir("repeat");
    assert_eq!(store.export_wiki_markdown(&out).unwrap(), 1);
    assert_eq!(store.export_wiki_markdown(&out).unwrap(), 1);
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 1);
}

#[test]
fn source_url_and_tags_render_into_frontmatter() {
    let store = temp_db("meta");
    insert_page(
        &store,
        "p1",
        "topic/imported",
        "导入页",
        "正文",
        &[
            ("source_url", "https://example.com/a"),
            ("tags", "[\"a\",\"b\"]"),
        ],
    );
    let out = temp_out_dir("meta");
    store.export_wiki_markdown(&out).unwrap();
    let text = std::fs::read_to_string(out.join("topic__imported.md")).unwrap();
    assert!(text.contains(r#"source_url: "https://example.com/a""#));
    assert!(text.contains(r#"tags: ["a","b"]"#));
}
