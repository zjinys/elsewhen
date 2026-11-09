use super::*;
fn store() -> Store {
    Store::open(&std::env::temp_dir().join(format!("prompt-store-{}.db", uuid::Uuid::new_v4())))
        .unwrap()
}

#[test]
fn language_preferences_are_normalized_durable_and_compare_and_swap() {
    let s = store();
    let initial = s.prompt_language_preferences().unwrap();
    assert_eq!(initial.generation, 0);
    assert_eq!(initial.response_mode, "auto");
    assert_eq!(initial.instruction_locale, None);
    let saved = s
        .save_prompt_language_preferences(Some("EN_gb"), "fixed", Some("zh_CN"), 0)
        .unwrap();
    assert_eq!(saved.instruction_locale.as_deref(), Some("en-GB"));
    assert_eq!(saved.response_locale.as_deref(), Some("zh-Hans"));
    assert_eq!(saved.generation, 1);
    assert!(
        s.save_prompt_language_preferences(None, "auto", None, 0)
            .is_err()
    );
    assert!(
        s.save_prompt_language_preferences(None, "auto", Some("en"), 1)
            .is_err()
    );
    assert!(
        s.save_prompt_language_preferences(None, "fixed", None, 1)
            .is_err()
    );
    assert!(
        s.save_prompt_language_preferences(Some("../bad"), "auto", None, 1)
            .is_err()
    );
    assert_eq!(s.prompt_language_preferences().unwrap(), saved);
    let automatic = s
        .save_prompt_language_preferences(None, "auto", None, 1)
        .unwrap();
    assert_eq!(automatic.generation, 2);
    assert_eq!(automatic.instruction_locale, None);
    assert_eq!(automatic.response_locale, None);
    // A second independent SQLite connection sees the committed preference.
    let path: String = s
        .connection
        .query_row(
            "SELECT file FROM pragma_database_list WHERE name='main'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        Store::open(std::path::Path::new(&path))
            .unwrap()
            .prompt_language_preferences()
            .unwrap(),
        automatic
    );
    let (locale, version): (String, i64) = s
        .connection
        .query_row(
            "SELECT output_locale,policy_version FROM dataset_content_language",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((locale.as_str(), version), ("zh-Hans", 1));
}
#[test]
fn builtin_registration_revision_activation_and_bundle_are_stable() {
    let s = store();
    s.register_prompts().unwrap();
    s.register_prompts().unwrap();
    let initial = s.resolve_prompt("chat.personal.policy", "zh-Hans").unwrap();
    assert_eq!(initial.revision.origin, "builtin");
    let id = s
        .save_prompt_revision(
            "chat.personal.policy",
            "zh-Hans",
            "自定义角色。",
            Some(&initial.revision.id),
            "测试",
        )
        .unwrap();
    assert_eq!(
        s.resolve_prompt("chat.personal.policy", "zh-Hans")
            .unwrap()
            .revision
            .id,
        initial.revision.id
    );
    s.activate_prompt("chat.personal.policy", "zh-Hans", Some(&id), 0)
        .unwrap();
    let selected = s.resolve_prompt("chat.personal.policy", "zh-Hans").unwrap();
    assert_eq!(selected.revision.id, id);
    assert_eq!(selected.generation, 1);
    let bundle = s.capture_prompt_bundle("preview", "p1").unwrap();
    assert_eq!(
        s.prompt_from_bundle(&bundle, "chat.personal.policy")
            .unwrap(),
        "自定义角色。"
    );
    s.activate_prompt("chat.personal.policy", "zh-Hans", None, 1)
        .unwrap();
    assert_eq!(
        s.resolve_prompt("chat.personal.policy", "zh-Hans")
            .unwrap()
            .revision
            .id,
        initial.revision.id
    );
}
#[test]
fn prompt_scope_and_reading_task_pin_versions_and_dependencies() {
    let s = store();
    s.register_prompts().unwrap();
    let before = s
        .prompt_dependency(&["summary.policy", "summary.fixed_contract"])
        .unwrap();
    let (bundle, strategy) = s
        .reading_prompt_task("source-1", "reading-contract-v1")
        .unwrap();
    let scope = s.prompt_scope("preview", "old-run").unwrap();
    let id = s
        .save_prompt_revision("chat.personal.policy", "zh-Hans", "新角色", None, "")
        .unwrap();
    s.activate_prompt("chat.personal.policy", "zh-Hans", Some(&id), 0)
        .unwrap();
    assert_ne!(s.prompt_for_run("chat.personal.policy").unwrap(), "新角色");
    drop(scope);
    assert_eq!(s.prompt_for_run("chat.personal.policy").unwrap(), "新角色");
    assert_eq!(
        before,
        s.prompt_dependency(&["summary.policy", "summary.fixed_contract"])
            .unwrap()
    );
    let id = s
        .save_prompt_revision("reading.policy", "zh-Hans", "新阅读方法", None, "")
        .unwrap();
    s.activate_prompt("reading.policy", "zh-Hans", Some(&id), 0)
        .unwrap();
    let task = s
        .reading_prompt_task("source-1", "reading-contract-v1")
        .unwrap();
    assert_eq!(task, (bundle.clone(), strategy));
    assert_ne!(
        s.reading_prompt_task("source-2", "reading-contract-v1")
            .unwrap()
            .1,
        "reading-contract-v1"
    );
    assert!(
        s.connection
            .execute("UPDATE prompt_revisions SET body='altered'", [])
            .is_err()
    );
    assert!(
        s.connection
            .execute("DELETE FROM prompt_bundles WHERE id=?1", [bundle])
            .is_err()
    );
    let id = s
        .save_prompt_revision("summary.policy", "zh-Hans", "新摘要重点", None, "")
        .unwrap();
    s.activate_prompt("summary.policy", "zh-Hans", Some(&id), 0)
        .unwrap();
    assert_ne!(
        before,
        s.prompt_dependency(&["summary.policy", "summary.fixed_contract"])
            .unwrap()
    );
}
#[test]
fn prompt_locale_activation_and_explicit_default_are_independent() {
    let s = store();
    s.register_prompts().unwrap();
    let id = s
        .save_prompt_revision("chat.personal.policy", "en", "English", None, "")
        .unwrap();
    s.activate_prompt("chat.personal.policy", "en", Some(&id), 0)
        .unwrap();
    assert_eq!(
        s.resolve_prompt("chat.personal.policy", "en-GB")
            .unwrap()
            .revision
            .body,
        "English"
    );
    s.activate_prompt("chat.personal.policy", "en-GB", None, 0)
        .unwrap();
    assert_eq!(
        s.resolve_prompt("chat.personal.policy", "en-GB")
            .unwrap()
            .revision
            .origin,
        "builtin"
    );
    assert_eq!(
        s.resolve_prompt("chat.personal.policy", "zh-Hant-TW")
            .unwrap()
            .revision
            .origin,
        "builtin"
    );
}
#[test]
fn prompt_activation_is_cas_and_fixed_contract_is_read_only() {
    let s = store();
    s.register_prompts().unwrap();
    assert!(
        s.save_prompt_revision("chat.fixed_contract", "zh-Hans", "x", None, "")
            .is_err()
    );
    let r = s.resolve_prompt("chat.personal.policy", "en-GB").unwrap();
    assert_eq!(r.revision.locale, "en");
    assert!(!r.fallback.is_empty());
    let id = s
        .save_prompt_revision("chat.personal.policy", "zh-Hans", "x", None, "")
        .unwrap();
    assert!(
        s.activate_prompt("chat.personal.policy", "zh-Hans", Some(&id), 1)
            .is_err()
    );
    s.activate_prompt("chat.personal.policy", "zh-Hans", Some(&id), 0)
        .unwrap();
    assert!(
        s.activate_prompt("chat.personal.policy", "zh-Hans", None, 0)
            .is_err()
    );
}

#[test]
fn bilingual_catalog_registers_append_only_and_restores_matching_builtin() {
    let s = store();
    s.register_prompts().unwrap();
    let count: i64 = s
        .connection
        .query_row(
            "SELECT count(*) FROM prompt_revisions WHERE origin='builtin'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    // Nineteen built-in definitions, each registered in both locales.
    assert_eq!(count, 38);
    let compass = catalog()
        .into_iter()
        .find(|d| d.key == "compass.fixed_contract")
        .unwrap();
    assert!(!compass.editable);
    for locale in ["zh-Hans", "en"] {
        assert!(
            s.save_prompt_revision(compass.key, locale, "override", None, "")
                .is_err()
        );
    }
    s.register_prompts().unwrap();
    assert_eq!(
        count,
        s.connection
            .query_row(
                "SELECT count(*) FROM prompt_revisions WHERE origin='builtin'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .unwrap()
    );
    for d in catalog() {
        let zh = s.resolve_prompt(d.key, "zh-Hans").unwrap();
        let en = s.resolve_prompt(d.key, "en").unwrap();
        assert_eq!(zh.revision.body, d.body);
        assert_eq!(en.revision.body, d.english_body);
        assert_eq!(zh.revision.locale, "zh-Hans");
        assert_eq!(en.revision.locale, "en");
        assert_ne!(zh.revision.id, en.revision.id);
        assert!(en.fallback.is_empty());
        assert!(en.revision.id.contains(":en:"));
    }
    let key = "chat.personal.policy";
    let zh = s
        .save_prompt_revision(key, "zh-Hans", "中文覆盖", None, "")
        .unwrap();
    let en = s
        .save_prompt_revision(key, "en", "English override", None, "")
        .unwrap();
    s.activate_prompt(key, "zh-Hans", Some(&zh), 0).unwrap();
    s.activate_prompt(key, "en", Some(&en), 0).unwrap();
    assert_eq!(s.resolve_prompt(key, "en-GB").unwrap().revision.id, en);
    s.activate_prompt(key, "en-GB", None, 0).unwrap();
    let default = s.resolve_prompt(key, "en-GB").unwrap();
    assert_eq!(default.revision.locale, "en");
    assert_eq!(default.revision.origin, "builtin");
    assert_eq!(
        s.resolve_builtin_prompt(key, "en").unwrap().revision.id,
        default.revision.id
    );
    assert_eq!(
        s.resolve_prompt(key, "zh-Hant-TW").unwrap().revision.origin,
        "builtin"
    );
    let missing = s.resolve_prompt(key, "ja-JP").unwrap();
    assert_eq!(missing.revision.locale, "zh-Hans");
    assert_eq!(missing.revision.origin, "builtin");
    assert_eq!(missing.fallback, "missing_translation:ja-JP:zh-Hans");
    assert_eq!(s.resolve_prompt(key, "zh-Hans").unwrap().revision.id, zh);
    // Drafts can only inherit a revision with the same key and locale.
    assert!(
        s.save_prompt_revision(key, "en", "new", Some(&zh), "")
            .is_err()
    );
}

#[test]
fn incompatible_override_uses_english_builtin_and_never_parent_user_override() {
    let s = store();
    s.register_prompts().unwrap();
    let key = "chat.personal.policy";
    let parent = s
        .save_prompt_revision(key, "en", "parent override", None, "")
        .unwrap();
    s.activate_prompt(key, "en", Some(&parent), 0).unwrap();
    // Simulate an old incompatible immutable revision from another app version.
    s.connection.execute(
        "INSERT INTO prompt_revisions(id,prompt_key,locale,origin,body,body_hash,contract_version,variables_version,created_at) VALUES('old',?1,'en-GB','user','old policy',?2,999,1,datetime('now'))",
        params![key, content_hash("old policy")],
    ).unwrap();
    s.connection
        .execute(
            "INSERT INTO prompt_activations VALUES(?1,'en-GB','pinned','old',1,datetime('now'))",
            [key],
        )
        .unwrap();
    let resolved = s.resolve_prompt(key, "en-GB").unwrap();
    assert_eq!(resolved.revision.origin, "builtin");
    assert_eq!(resolved.revision.locale, "en");
    assert_eq!(resolved.fallback, "incompatible_revision");
    assert_eq!(resolved.generation, 1);
    assert_eq!(s.resolve_prompt(key, "en").unwrap().revision.id, parent);
}

#[test]
fn english_fixed_contract_retains_machine_protocol_and_approval_boundaries() {
    let s = store();
    s.register_prompts().unwrap();
    let chat = s
        .resolve_prompt("chat.fixed_contract", "en")
        .unwrap()
        .revision
        .body;
    for literal in [
        "[工具调用]",
        "[规则提议]",
        "[规则本质]",
        "[事件 #123]",
        "[知识页 slug]",
        "[工具:xxx]",
        "save_wiki_revision",
        "save_as=derivative",
    ] {
        assert!(chat.contains(literal), "missing protocol literal {literal}");
    }
    assert!(chat.contains("Ordinary short replies do not confirm a review"));
    let summary = s
        .resolve_prompt("summary.fixed_contract", "en")
        .unwrap()
        .revision
        .body;
    for field in [
        "goals",
        "constraints",
        "facts",
        "decisions",
        "unfinished",
        "message_id",
    ] {
        assert!(summary.contains(field));
    }
    assert!(
        s.save_prompt_revision("chat.fixed_contract", "en", "relax permissions", None, "")
            .is_err()
    );
}

#[test]
fn upgrading_builtin_identity_preserves_old_revisions_and_bundle_entries() {
    let s = store();
    s.register_prompts().unwrap();
    let d = definition("chat.personal.policy").unwrap();
    let old_id = format!("builtin:{}:{}", d.key, content_hash(d.body));
    s.connection.execute(
        "INSERT INTO prompt_revisions(id,prompt_key,locale,origin,body,body_hash,contract_version,variables_version,builtin_pack_version,change_note,created_at) VALUES(?1,?2,'zh-Hans','builtin',?3,?4,1,1,'prompts-v1-old','内置默认',datetime('now'))",
        params![old_id, d.key, d.body, content_hash(d.body)],
    ).unwrap();
    let user = s
        .save_prompt_revision(d.key, LOCALE, "旧版自定义", Some(&old_id), "")
        .unwrap();
    s.activate_prompt(d.key, LOCALE, Some(&user), 0).unwrap();
    let old_bundle = s
        .capture_prompt_bundle("preview", "before-upgrade")
        .unwrap();
    s.register_prompts().unwrap();
    assert_eq!(s.prompt_revision(&old_id).unwrap().body, d.body);
    assert_eq!(
        s.prompt_from_bundle(&old_bundle, d.key).unwrap(),
        "旧版自定义"
    );
    assert_eq!(s.resolve_prompt(d.key, LOCALE).unwrap().revision.id, user);
    assert_eq!(
        s.resolve_prompt(d.key, "en").unwrap().revision.body,
        d.english_body
    );
}

#[test]
fn run_creation_freezes_language_preferences_and_templates_before_worker_dispatch() {
    let s = store();
    s.save_prompt_language_preferences(Some("en"), "fixed", Some("zh-Hans"), 0)
        .unwrap();
    s.register_prompts().unwrap();
    let first = s
        .save_prompt_revision(
            "chat.personal.policy",
            "en",
            "First English style",
            None,
            "",
        )
        .unwrap();
    s.activate_prompt("chat.personal.policy", "en", Some(&first), 0)
        .unwrap();
    let conversation = s.create_conversation(None, None).unwrap();
    let input = s
        .send_message(
            &conversation,
            "user",
            "Please reply in English. Help me compare these two options.",
            None,
        )
        .unwrap();
    let run = s
        .create_chat_run(&conversation, &input, "language-run", "worker")
        .unwrap();
    let bundle = s
        .capture_prompt_bundle_for_ui("chat_run", &run.id, "zh-Hans")
        .unwrap();
    let scope = s.use_prompt_bundle(&bundle);
    let old = s.run_language().unwrap();
    assert_eq!(old.instruction_locale, "en");
    assert_eq!(old.background_instruction_locale, "en");
    assert_eq!(old.response.locale, "en");
    assert_eq!(old.response.source, "explicit_request");
    assert_eq!(old.knowledge_locale, "zh-Hans");
    assert!(
        s.output_language_directive(true)
            .unwrap()
            .contains("Output language: en.")
    );
    assert!(
        s.output_language_directive(false)
            .unwrap()
            .contains("Output language: zh-Hans.")
    );
    let dependency = s
        .prompt_dependency(&["summary.policy", "summary.fixed_contract"])
        .unwrap();
    s.save_prompt_language_preferences(Some("zh-Hans"), "fixed", Some("en"), 1)
        .unwrap();
    let next = s
        .save_prompt_revision(
            "chat.personal.policy",
            "en",
            "Changed English style",
            None,
            "",
        )
        .unwrap();
    s.activate_prompt("chat.personal.policy", "en", Some(&next), 1)
        .unwrap();
    assert_eq!(s.run_language().unwrap(), old);
    assert_eq!(
        s.prompt_for_run("chat.personal.policy").unwrap(),
        "First English style"
    );
    assert_eq!(
        s.prompt_dependency(&["summary.policy", "summary.fixed_contract"])
            .unwrap(),
        dependency
    );
    drop(scope);
    s.finish_chat_run(
        &run.id,
        "worker",
        "failed",
        None,
        Some(("fixture", "fixture")),
    )
    .unwrap();
    let next_run = s
        .create_chat_run(&conversation, &input, "language-run-new", "worker")
        .unwrap();
    let next_bundle = s.capture_prompt_bundle("chat_run", &next_run.id).unwrap();
    let scope = s.use_prompt_bundle(&next_bundle);
    assert_eq!(s.run_language().unwrap().instruction_locale, "zh-Hans");
    assert_ne!(
        s.prompt_for_run("chat.personal.policy").unwrap(),
        "First English style"
    );
    drop(scope);
    // An independent connection (restart) can read the old immutable snapshot.
    let path: String = s
        .connection
        .query_row(
            "SELECT file FROM pragma_database_list WHERE name='main'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let reopened = Store::open(std::path::Path::new(&path)).unwrap();
    let _old_scope = reopened.use_prompt_bundle(&bundle);
    assert_eq!(reopened.run_language().unwrap(), old);
    assert_eq!(
        reopened.prompt_for_run("chat.personal.policy").unwrap(),
        "First English style"
    );
}

#[test]
fn background_language_and_dependencies_ignore_ui_and_response_preferences() {
    let s = store();
    let a = s
        .capture_prompt_bundle_for_ui("background_task", "a", "en")
        .unwrap();
    let scope = s.use_prompt_bundle(&a);
    let language = s.run_language().unwrap();
    assert_eq!(language.ui_locale, "en");
    assert_eq!(language.instruction_locale, "zh-Hans");
    assert_eq!(language.response.locale, "zh-Hans");
    assert_eq!(language.response.source, "dataset_policy");
    let dependency = s
        .prompt_dependency(&["summary.policy", "summary.fixed_contract"])
        .unwrap();
    drop(scope);
    s.save_prompt_language_preferences(None, "fixed", Some("en"), 0)
        .unwrap();
    let b = s
        .capture_prompt_bundle_for_ui("background_task", "b", "zh-Hans")
        .unwrap();
    let scope = s.use_prompt_bundle(&b);
    assert_eq!(s.run_language().unwrap().knowledge_locale, "zh-Hans");
    assert_eq!(
        s.prompt_dependency(&["summary.policy", "summary.fixed_contract"])
            .unwrap(),
        dependency
    );
    drop(scope);
    s.save_prompt_language_preferences(Some("en"), "auto", None, 1)
        .unwrap();
    let c = s
        .capture_prompt_bundle_for_ui("background_task", "c", "zh-Hans")
        .unwrap();
    let _scope = s.use_prompt_bundle(&c);
    assert_eq!(s.run_language().unwrap().instruction_locale, "en");
    assert_eq!(s.run_language().unwrap().knowledge_locale, "zh-Hans");
    assert_ne!(
        s.prompt_dependency(&["summary.policy", "summary.fixed_contract"])
            .unwrap(),
        dependency
    );
}

#[test]
fn context_catalog_matches_variables_and_keeps_legacy_bundles_readable() {
    let s = store();
    s.register_prompts().unwrap();
    for locale in [LOCALE, "en"] {
        let selected = s
            .resolve_prompt("chat.context.fixed_contract", locale)
            .unwrap();
        let resources: serde_json::Value = serde_json::from_str(&selected.revision.body).unwrap();
        let strings = render::TextResources::parse(&selected.revision.body).unwrap();
        for (key, template) in resources.as_object().unwrap() {
            let variables = template["variables"]
                .as_array()
                .unwrap()
                .iter()
                .map(|k| (k.as_str().unwrap(), format!("原文 <<{key}>>")))
                .collect::<Vec<_>>();
            let rendered = strings.render(key, &variables).unwrap();
            for (_, value) in variables {
                assert!(rendered.contains(&value));
            }
        }
    }
    let zh: serde_json::Value =
        serde_json::from_str(definition("chat.context.fixed_contract").unwrap().body).unwrap();
    let en: serde_json::Value = serde_json::from_str(
        definition("chat.context.fixed_contract")
            .unwrap()
            .english_body,
    )
    .unwrap();
    assert_eq!(
        zh.as_object().unwrap().keys().collect::<Vec<_>>(),
        en.as_object().unwrap().keys().collect::<Vec<_>>()
    );
    for (key, template) in zh.as_object().unwrap() {
        assert_eq!(template["variables"], en[key]["variables"]);
    }
    // Old manifests have no entry for the new context key and must retain
    // the frozen historical Chinese behavior rather than today's UI choice.
    s.connection.execute(
        "INSERT INTO prompt_bundles VALUES('legacy','preview','old','zh-Hans','zh-Hans','{\"mode\":\"auto\"}','zh-Hans',1,'prompts-v1','locale-v1','render-v1',?1,datetime('now'))",
        [content_hash("legacy")],
    ).unwrap();
    let summary = s.resolve_prompt("summary.policy", LOCALE).unwrap();
    s.connection.execute(
        "INSERT INTO prompt_bundle_entries VALUES('legacy',0,'summary.policy','zh-Hans','zh-Hans',?1,'')",
        [&summary.revision.id],
    ).unwrap();
    let context = s
        .prompt_from_bundle("legacy", "chat.context.fixed_contract")
        .unwrap();
    assert_eq!(
        context,
        include_str!("../../resources/prompts/chat-context.legacy-v1.json")
    );
    let _scope = s.use_prompt_bundle("legacy");
    assert_eq!(s.run_language().unwrap().response.locale, "zh-Hans");
}

#[test]
fn execution_resources_have_matching_variables_and_preserve_legacy_runs() {
    let s = store();
    s.register_prompts().unwrap();
    let d = definition("chat.execution.fixed_contract").unwrap();
    let zh: serde_json::Value = serde_json::from_str(d.body).unwrap();
    let en: serde_json::Value = serde_json::from_str(d.english_body).unwrap();
    assert_eq!(
        zh.as_object().unwrap().keys().collect::<Vec<_>>(),
        en.as_object().unwrap().keys().collect::<Vec<_>>()
    );
    for (key, template) in zh.as_object().unwrap() {
        assert_eq!(template["variables"], en[key]["variables"], "{key}");
    }
    for body in [d.body, d.english_body] {
        let strings = render::TextResources::parse(body).unwrap();
        strings.validate().unwrap();
        let source = "原文 <<kind>> {{secret:key}} [工具调用]";
        assert!(
            strings
                .render(
                    "page_selection",
                    &[
                        ("start", "4".into()),
                        ("end", "9".into()),
                        ("total", "20".into()),
                        ("body", source.into()),
                    ]
                )
                .unwrap()
                .contains(source)
        );
        assert!(
            strings
                .render("page_selection", &[("body", source.into())])
                .is_err()
        );
    }
    s.connection.execute(
        "INSERT INTO prompt_bundles VALUES('legacy-execution','preview','old-execution','zh-Hans','zh-Hans','{\"mode\":\"auto\"}','zh-Hans',1,'prompts-v5','locale-v2','render-v2',?1,datetime('now'))",
        [content_hash("legacy-execution")],
    ).unwrap();
    assert_eq!(
        s.prompt_from_bundle("legacy-execution", d.key).unwrap(),
        include_str!("../../resources/prompts/chat-execution.legacy-v1.json")
    );
    let _scope = s.use_prompt_bundle("legacy-execution");
    s.save_prompt_language_preferences(Some("en"), "fixed", Some("en"), 0)
        .unwrap();
    assert!(
        s.render_execution_instruction("text_protocol", &[])
            .unwrap()
            .contains("系统提示：")
    );
}

#[test]
fn temporary_content_chat_pins_explicit_and_followup_language_without_storing_messages() {
    let s = store();
    s.save_prompt_language_preferences(Some("en"), "fixed", Some("zh-Hans"), 0)
        .unwrap();
    let scope = s
        .content_chat_prompt_scope("请用英文回答。Explain the source.", &[])
        .unwrap();
    let old = s.run_language().unwrap();
    assert_eq!(old.response.locale, "en");
    assert_eq!(old.response.source, "explicit_request");
    assert_eq!(old.instruction_locale, "en");
    s.save_prompt_language_preferences(Some("zh-Hans"), "auto", None, 1)
        .unwrap();
    assert_eq!(s.run_language().unwrap(), old);
    assert!(
        s.render_execution_instruction("content_mentor", &[("content", "原文 <<content>>".into())])
            .unwrap()
            .contains("Original captured content")
    );
    drop(scope);
    let _next = s
        .content_chat_prompt_scope(
            "?",
            &["Explain the original source and its main limitations.".into()],
        )
        .unwrap();
    let next = s.run_language().unwrap();
    assert_eq!(next.instruction_locale, "zh-Hans");
    assert_eq!(next.response.locale, "en");
    assert_eq!(next.response.source, "conversation");
    let messages: i64 = s
        .connection
        .query_row("SELECT count(*) FROM messages", [], |r| r.get(0))
        .unwrap();
    assert_eq!(messages, 0);
    assert!(s.list_events().unwrap().is_empty());
}

#[test]
fn processing_resources_match_variables_and_follow_background_instruction_policy() {
    let s = store();
    s.register_prompts().unwrap();
    let d = definition("task.processing.fixed_contract").unwrap();
    let zh: serde_json::Value = serde_json::from_str(d.body).unwrap();
    let en: serde_json::Value = serde_json::from_str(d.english_body).unwrap();
    assert_eq!(
        zh.as_object().unwrap().keys().collect::<Vec<_>>(),
        en.as_object().unwrap().keys().collect::<Vec<_>>()
    );
    for (key, value) in zh.as_object().unwrap() {
        assert_eq!(value["variables"], en[key]["variables"], "{key}");
    }
    for body in [d.body, d.english_body] {
        render::TextResources::parse(body)
            .unwrap()
            .validate()
            .unwrap();
    }
    s.save_prompt_language_preferences(Some("en"), "fixed", Some("en"), 0)
        .unwrap();
    let _scope = s
        .prompt_scope("background_task", "processing-fixture")
        .unwrap();
    s.save_prompt_language_preferences(Some("zh-Hans"), "fixed", Some("zh-Hans"), 1)
        .unwrap();
    let raw = "原始记录 <<record>> {{secret:key}}";
    let input = s
        .render_processing_instruction(
            "analysis_input",
            &[
                ("context", "context".into()),
                ("record", raw.into()),
                ("rules", "".into()),
            ],
        )
        .unwrap();
    assert!(input.starts_with("Analyze the personal record"));
    assert!(input.contains(raw));
    assert!(input.contains("schema_version must be event-analysis-v3"));
    assert!(
        s.output_language_directive(false)
            .unwrap()
            .contains("Output language: zh-Hans")
    );
    assert!(
        s.render_processing_instruction("judge_system", &[])
            .unwrap()
            .contains("applicable (boolean)")
    );
    assert!(
        s.save_prompt_revision(d.key, "en", "changed", None, "")
            .is_err()
    );
}

#[test]
fn knowledge_processing_is_bilingual_readonly_and_keeps_v6_runs_frozen() {
    let s = store();
    s.register_prompts().unwrap();
    let d = definition("knowledge.processing.fixed_contract").unwrap();
    let zh: serde_json::Value = serde_json::from_str(d.body).unwrap();
    let en: serde_json::Value = serde_json::from_str(d.english_body).unwrap();
    assert_eq!(
        zh.as_object().unwrap().keys().collect::<Vec<_>>(),
        en.as_object().unwrap().keys().collect::<Vec<_>>()
    );
    for (key, value) in zh.as_object().unwrap() {
        assert_eq!(value["variables"], en[key]["variables"]);
    }
    for body in [d.body, d.english_body] {
        render::TextResources::parse(body)
            .unwrap()
            .validate()
            .unwrap();
    }
    s.connection.execute(
        "INSERT INTO prompt_bundles VALUES('legacy-knowledge','background_task','old-knowledge','en','en','{\"mode\":\"knowledge\"}','zh-Hans',1,'prompts-v6','locale-v2','render-v2',?1,datetime('now'))",
        [content_hash("legacy-knowledge")],
    ).unwrap();
    assert_eq!(
        s.prompt_from_bundle("legacy-knowledge", d.key).unwrap(),
        include_str!("../../resources/prompts/knowledge-processing.legacy-v1.json")
    );
    s.save_prompt_language_preferences(Some("en"), "auto", None, 0)
        .unwrap();
    let _scope = s
        .prompt_scope("background_task", "knowledge-fixture")
        .unwrap();
    assert!(
        s.render_knowledge_instruction("compile_system", &[])
            .unwrap()
            .starts_with("You edit knowledge")
    );
    assert!(
        s.output_language_directive(false)
            .unwrap()
            .contains("Output language: zh-Hans")
    );
    assert!(
        s.save_prompt_revision(d.key, "en", "change permissions", None, "")
            .is_err()
    );
}

#[test]
fn reply_and_lookup_resources_keep_variables_and_independent_language_selection() {
    let s = store();
    s.register_prompts().unwrap();
    for key in ["chat.reply.fixed_contract", "chat.lookup.fixed_contract"] {
        let d = definition(key).unwrap();
        let zh: serde_json::Value = serde_json::from_str(d.body).unwrap();
        let en: serde_json::Value = serde_json::from_str(d.english_body).unwrap();
        assert_eq!(
            zh.as_object().unwrap().keys().collect::<Vec<_>>(),
            en.as_object().unwrap().keys().collect::<Vec<_>>()
        );
        for (key, value) in zh.as_object().unwrap() {
            assert_eq!(value["variables"], en[key]["variables"]);
        }
        for body in [d.body, d.english_body] {
            render::TextResources::parse(body)
                .unwrap()
                .validate()
                .unwrap();
        }
    }
    s.save_prompt_language_preferences(Some("zh-Hans"), "fixed", Some("en"), 0)
        .unwrap();
    let _scope = s.prompt_scope("preview", "reply-language").unwrap();
    assert!(
        s.render_reply_instruction("pending_action", &[])
            .unwrap()
            .contains("awaiting review")
    );
    assert!(
        s.render_lookup_instruction("rules_heading", &[])
            .unwrap()
            .contains("已生效")
    );
    let raw = "原文 <<request>> {{secret:key}}";
    assert!(
        s.render_reply_instruction("task", &[("request", raw.into())])
            .unwrap()
            .contains(raw)
    );
}

#[test]
fn supplement_resources_validate_variables_and_freeze_v8_compatibility() {
    let s = store();
    s.register_prompts().unwrap();
    let d = definition("chat.supplement.fixed_contract").unwrap();
    let zh: serde_json::Value = serde_json::from_str(d.body).unwrap();
    let en: serde_json::Value = serde_json::from_str(d.english_body).unwrap();
    assert_eq!(
        zh.as_object().unwrap().keys().collect::<Vec<_>>(),
        en.as_object().unwrap().keys().collect::<Vec<_>>()
    );
    for (key, value) in zh.as_object().unwrap() {
        assert_eq!(value["variables"], en[key]["variables"]);
    }
    for body in [d.body, d.english_body] {
        render::TextResources::parse(body)
            .unwrap()
            .validate()
            .unwrap();
    }
    s.connection.execute(
        "INSERT INTO prompt_bundles VALUES('legacy-supplement','preview','old-supplement','en','en','{\"mode\":\"auto\"}','zh-Hans',1,'prompts-v8','locale-v2','render-v2',?1,datetime('now'))",
        [content_hash("legacy-supplement")],
    ).unwrap();
    assert_eq!(
        s.prompt_from_bundle("legacy-supplement", d.key).unwrap(),
        include_str!("../../resources/prompts/chat-supplement.legacy-v1.json")
    );
}
