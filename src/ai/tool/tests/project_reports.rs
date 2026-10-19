use super::temp_db;
use super::*;

#[test]
fn project_import_confirmation_saves_the_previewed_report() {
    let (store, path) = temp_db();
    let conv = store.create_conversation(Some("t"), None).unwrap();
    let directory = std::env::temp_dir().join(format!(
        "elsewhen-project-preview-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("README.md"),
        "# Preview fixture\nA test project.",
    )
    .unwrap();

    let call = ToolCall::new(
        "import_directory_as_project",
        json!({"directory": directory.to_string_lossy()}),
    );
    let preview = dispatch(&call, &ToolRegistry::default(), &store, &conv);
    assert!(
        preview.content.contains("尚未写入知识库"),
        "{}",
        preview.content
    );
    let actions = store.pending_actions_for_conversation(&conv).unwrap();
    assert_eq!(actions.len(), 1);
    let args: Value = serde_json::from_str(&actions[0].args_json).unwrap();
    let expected = args["content_md"].as_str().unwrap().to_string();

    // The original directory is removed after preview; confirmation must save
    // the captured report and must not rescan or invoke the provider again.
    std::fs::remove_dir_all(&directory).unwrap();
    execute_pending_action(&store, &actions[0]).unwrap();
    let page = store
        .list_wiki_pages(Some("project"), None)
        .unwrap()
        .into_iter()
        .find(|p| p.content_md == expected)
        .expect("confirmed project page should contain the previewed report");
    assert_eq!(page.content_md, expected);

    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn project_report_staging_rejects_cancelled_and_expired_runs() {
    for expired in [false, true] {
        let (mut store, path) = temp_db();
        Store::mark_initialized(&path).unwrap();
        let conv = store
            .create_conversation(Some("staging fence"), None)
            .unwrap();
        let input = store
            .send_message(&conv, "user", "inspect test project", None)
            .unwrap();
        let run = store
            .create_chat_run(&conv, &input, "request", "owner")
            .unwrap();
        store.chat_run_identity = Some(crate::ai::generation::RunIdentity {
            id: run.id.clone(),
            owner: "owner".into(),
        });
        crate::ai::generation::check(&store).unwrap();

        // The report is already prepared; a separate connection stops the run
        // before the network tool reaches its final staging boundary.
        let other = Store::open(&path).unwrap();
        if expired {
            other
                .connection
                .execute(
                    "UPDATE chat_runs SET lease_until='2000-01-01T00:00:00+00:00' WHERE id=?1",
                    [&run.id],
                )
                .unwrap();
        } else {
            other.cancel_chat_run(&conv, &run.id).unwrap();
        }
        let error = store_create_pending(
            &store,
            &conv,
            "import_directory_as_project",
            &json!({"directory":"synthetic-project", "content_md":"prepared report"}),
        )
        .unwrap_err();
        assert!(
            error
                .downcast_ref::<crate::storage::chat_runs::ChatRunStopped>()
                .is_some()
        );
        assert!(
            store
                .pending_actions_for_conversation(&conv)
                .unwrap()
                .is_empty()
        );
        assert!(store.connection.is_autocommit());
        drop(other);
        drop(store);
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn project_report_staging_commits_for_live_run_and_joins_outer_transaction() {
    let (mut store, path) = temp_db();
    let conv = store
        .create_conversation(Some("staging transaction"), None)
        .unwrap();
    let input = store
        .send_message(&conv, "user", "inspect test project", None)
        .unwrap();
    let run = store
        .create_chat_run(&conv, &input, "request", "owner")
        .unwrap();
    store.chat_run_identity = Some(crate::ai::generation::RunIdentity {
        id: run.id,
        owner: "owner".into(),
    });
    let args = json!({"directory":"synthetic-project", "content_md":"prepared report"});
    let id = store_create_pending(&store, &conv, "import_directory_as_project", &args).unwrap();
    assert!(store.connection.is_autocommit());
    let transaction = store.begin_write_transaction().unwrap().unwrap();
    store_create_pending(&store, &conv, "import_directory_as_project", &args).unwrap();
    assert!(!store.connection.is_autocommit());
    assert_eq!(
        store.pending_actions_for_conversation(&conv).unwrap().len(),
        2
    );
    drop(transaction);
    let actions = store.pending_actions_for_conversation(&conv).unwrap();
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].id, id);
    drop(store);
    let _ = std::fs::remove_file(path);
}
