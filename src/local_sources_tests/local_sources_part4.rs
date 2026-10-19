use super::*;

#[test]
fn ingest_project_summary_persists_exact_preview_without_rescanning() {
    let db_path = temp_path("summary-test.db");
    let directory = temp_path("missing-source");
    let store = Store::open(&db_path).unwrap();
    let summary = ProjectSummary {
        project_name: "Preview Project".into(),
        files: 7,
        skipped: 2,
        scan_truncated: false,
        content_md: "# Exact preview\n\nGenerated once.".into(),
    };

    let report = ingest_project_summary(&store, &directory, summary).unwrap();

    assert_eq!(report.files, 7);
    assert_eq!(report.pages, vec!["project/preview-project"]);
    assert_eq!(
        store
            .get_wiki_page("project/preview-project")
            .unwrap()
            .unwrap()
            .content_md,
        "# Exact preview\n\nGenerated once."
    );
    drop(store);
    let _ = fs::remove_file(db_path);
}
