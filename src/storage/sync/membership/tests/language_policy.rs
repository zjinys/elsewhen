use super::*;

fn deliver(source: &Store, target: &Store, recipient: &str, sender: &str) {
    for operation in source.pending_sync_operations(recipient, 100).unwrap() {
        let bytes = serde_json::to_vec(&operation).unwrap();
        target.receive_sync_operation(sender, &bytes).unwrap();
    }
}

#[test]
fn knowledge_language_requires_owner_and_all_current_application_declarations() {
    let a = Db::owner();
    let b = Db::new("b");
    enroll(&a, &b);
    assert!(
        a.store
            .set_knowledge_language_with_signer("en", 0, Some(&a.signer))
            .unwrap_err()
            .to_string()
            .contains("capability_missing")
    );
    assert_eq!(
        a.store.knowledge_language_edit_blocker().unwrap(),
        Some("capability_required")
    );
    assert_eq!(
        b.store.knowledge_language_edit_blocker().unwrap(),
        Some("owner_required")
    );
    assert_eq!(
        a.store.knowledge_language_policy().unwrap().output_locale,
        "zh-Hans"
    );
    assert!(
        b.store
            .set_knowledge_language_with_signer("en", 0, Some(&a.signer))
            .unwrap_err()
            .to_string()
            .contains("owner_required")
    );
    a.store.announce_language_capability().unwrap();
    b.store.announce_language_capability().unwrap();
    deliver(&a.store, &b.store, "b", "a");
    deliver(&b.store, &a.store, "a", "b");
    assert!(a.store.knowledge_language_edit_blocker().unwrap().is_none());
    let p = a
        .store
        .set_knowledge_language_with_signer("en", 0, Some(&a.signer))
        .unwrap();
    deliver(&a.store, &b.store, "b", "a");
    assert_eq!(b.store.knowledge_language_policy().unwrap(), p);
    // Exact replay is idempotent; the receiver never activates templates.
    deliver(&a.store, &b.store, "b", "a");
    assert_eq!(b.store.knowledge_language_policy().unwrap().generation, 1);
    assert_eq!(b.store.prompt_language_preferences().unwrap().generation, 0);
    assert!(b.store.list_events().unwrap().is_empty());
    assert!(!b.store.background_processing_allowed().unwrap());
    let reopened = Store::open(&b.store.path).unwrap();
    assert_eq!(reopened.knowledge_language_policy().unwrap(), p);
    assert!(
        a.store
            .set_knowledge_language_with_signer("zh-Hans", 0, Some(&a.signer))
            .is_err()
    );
    let c = Db::new("c");
    let (_, _, roster) = enroll(&a, &c);
    b.store
        .apply_signed_roster(&roster.encode().unwrap())
        .unwrap();
    // Previously declared support from an earlier roster is insufficient.
    assert!(
        a.store
            .set_knowledge_language_with_signer("en", 1, Some(&a.signer))
            .is_err()
    );
    a.store.announce_language_capability().unwrap();
    b.store.announce_language_capability().unwrap();
    c.store.announce_language_capability().unwrap();
    deliver(&b.store, &a.store, "a", "b");
    deliver(&c.store, &a.store, "a", "c");
    assert_eq!(
        a.store
            .set_knowledge_language_with_signer("en", 1, Some(&a.signer))
            .unwrap()
            .generation,
        1
    );
}

#[test]
fn knowledge_language_out_of_order_replay_and_unknown_strategy_do_not_change_originals() {
    let a = Db::owner();
    let b = Db::new("b");
    enroll(&a, &b);
    a.store.announce_language_capability().unwrap();
    b.store.announce_language_capability().unwrap();
    deliver(&b.store, &a.store, "a", "b");
    a.store
        .set_knowledge_language_with_signer("en", 0, Some(&a.signer))
        .unwrap();
    a.store
        .set_knowledge_language_with_signer("zh-Hans", 1, Some(&a.signer))
        .unwrap();
    let operations = a.store.pending_sync_operations("b", 100).unwrap();
    for op in operations.iter().rev() {
        b.store
            .receive_sync_operation("a", &serde_json::to_vec(op).unwrap())
            .unwrap();
    }
    assert_eq!(b.store.knowledge_language_policy().unwrap().generation, 2);
    assert_eq!(
        b.store.knowledge_language_policy().unwrap().output_locale,
        "zh-Hans"
    );
    let last = operations.last().unwrap();
    b.store
        .receive_sync_operation("a", &serde_json::to_vec(last).unwrap())
        .unwrap();
    assert_eq!(b.store.knowledge_language_policy().unwrap().generation, 2);
    let mut unknown = last.clone();
    unknown.origin_sequence += 1;
    unknown.logical_clock += 1;
    unknown.op_id = format!("{}:{}", unknown.origin_stream_id, unknown.origin_sequence);
    if let crate::sync::protocol::SyncMutation::KnowledgeLanguageChanged {
        policy,
        base_revision,
    } = &mut unknown.mutation
    {
        policy.generation = 3;
        policy.output_locale = "en".into();
        policy.policy_version = 999;
        *base_revision = Some(last.op_id.clone());
        policy.owner_signature = Some(
            STANDARD.encode(
                a.signer
                    .sign_membership(&policy.signed_payload().unwrap())
                    .unwrap(),
            ),
        );
    } else {
        panic!("last operation must be policy");
    }
    b.store
        .receive_sync_operation("a", &serde_json::to_vec(&unknown).unwrap())
        .unwrap();
    assert_eq!(
        b.store.knowledge_language_policy().unwrap().policy_version,
        999
    );
    assert!(!b.store.background_processing_allowed().unwrap());
    assert!(b.store.list_events().unwrap().is_empty());
}

#[test]
fn capability_declarations_cannot_impersonate_other_installations() {
    let a = Db::owner();
    let b = Db::new("b");
    enroll(&a, &b);
    b.store.announce_language_capability().unwrap();
    let mut op = b
        .store
        .pending_sync_operations("a", 100)
        .unwrap()
        .pop()
        .unwrap();
    if let crate::sync::protocol::SyncMutation::LanguageCapabilityDeclared { capability } =
        &mut op.mutation
    {
        capability.device_id = "a".into();
    }
    a.store
        .receive_sync_operation("b", &serde_json::to_vec(&op).unwrap())
        .unwrap();
    assert!(
        a.store
            .language_capability("b", "stream-b")
            .unwrap()
            .is_none()
    );
    assert!(
        a.store
            .language_capability("a", "stream-b")
            .unwrap()
            .is_none()
    );
}

#[test]
fn signed_policy_rejects_tampering_and_unsigned_snapshot_values() {
    let a = Db::owner();
    let b = Db::new("b");
    enroll(&a, &b);
    a.store.announce_language_capability().unwrap();
    b.store.announce_language_capability().unwrap();
    deliver(&b.store, &a.store, "a", "b");
    let p = a
        .store
        .set_knowledge_language_with_signer("en", 0, Some(&a.signer))
        .unwrap();
    b.store.verify_knowledge_language_owner(&p).unwrap();
    let mut tampered = p.clone();
    tampered.output_locale = "zh-Hans".into();
    assert!(b.store.verify_knowledge_language_owner(&tampered).is_err());
    tampered = p.clone();
    tampered.dataset_id = Some("another-dataset".into());
    assert!(b.store.verify_knowledge_language_owner(&tampered).is_err());
    tampered = p;
    tampered.owner_signature = None;
    assert!(b.store.verify_knowledge_language_owner(&tampered).is_err());
}

#[test]
fn nondefault_language_handoff_requires_target_capability_and_current_policy() {
    let a = Db::owner();
    let b = Db::new("b");
    enroll(&a, &b);
    a.store.announce_language_capability().unwrap();
    b.store.announce_language_capability().unwrap();
    deliver(&b.store, &a.store, "a", "b");
    a.store
        .set_knowledge_language_with_signer("en", 0, Some(&a.signer))
        .unwrap();
    deliver(&a.store, &b.store, "b", "a");
    let capability = a
        .store
        .language_capability("b", "stream-b")
        .unwrap()
        .unwrap();
    a.store
        .connection
        .execute(
            "DELETE FROM sync_language_capabilities WHERE device_id='b'",
            [],
        )
        .unwrap();
    assert!(a.store.yield_processor_role(3, "b", &a.signer).is_err());
    assert!(a.store.background_processing_allowed().unwrap());
    a.store.put_language_capability(&capability).unwrap();
    let proof = a.store.yield_processor_role(3, "b", &a.signer).unwrap();
    a.store
        .connection
        .execute(
            "DELETE FROM sync_language_capabilities WHERE device_id='b'",
            [],
        )
        .unwrap();
    assert!(
        a.store
            .approve_processor_handoff(&proof, &a.signer)
            .unwrap_err()
            .to_string()
            .contains("capability_missing")
    );
    a.store.put_language_capability(&capability).unwrap();
    let roster = a
        .store
        .approve_processor_handoff(&proof, &a.signer)
        .unwrap();
    b.store
        .apply_signed_roster(&roster.encode().unwrap())
        .unwrap();
    assert!(!b.store.background_processing_allowed().unwrap());
    deliver(&a.store, &b.store, "b", "a");
    b.store.confirm_processor_takeover().unwrap();
    assert!(b.store.background_processing_allowed().unwrap());
    assert!(!a.store.background_processing_allowed().unwrap());
    assert_eq!(
        b.store.knowledge_language_policy().unwrap().output_locale,
        "en"
    );
}

#[test]
fn signed_policy_first_snapshot_is_verified_and_local_run_state_is_excluded() {
    let a = Db::owner();
    let b = Db::new("b");
    let policy = a
        .store
        .set_knowledge_language_with_signer("en", 0, Some(&a.signer))
        .unwrap();
    a.store
        .insert_event(crate::event::NewEvent::now("UNTRANSLATED source 原文"))
        .unwrap();
    let job = a.store.claim_analysis_job().unwrap().unwrap();
    a.store
        .complete_analysis(
            &job,
            "event-analysis",
            r#"{"recordable":true,"summary":"English analysis"}"#,
        )
        .unwrap();
    let analysis_id: String = a
        .store
        .connection
        .query_row("SELECT id FROM event_analyses", [], |r| r.get(0))
        .unwrap();
    let language = a
        .store
        .artifact_language("analysis", &analysis_id)
        .unwrap()
        .unwrap();
    enroll(&a, &b);
    let key = ContentKey::generate().unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let mut exporter =
        crate::storage::sync::snapshot::SnapshotExporter::new(&a.store, "b", &id, 1).unwrap();
    exporter.declare_blob(&format!("blob-{id}"));
    let mut chunks = Vec::new();
    while let Some(chunk) = exporter.next_encrypted_chunk(&key, &a.signer).unwrap() {
        chunks.push(chunk);
    }
    let manifest = exporter.finish(&key, &a.signer).unwrap();
    b.store.begin_snapshot_import(&id, "a", 1).unwrap();
    for (index, chunk) in chunks.iter().enumerate().rev() {
        b.store
            .stage_snapshot_chunk(&id, index, "a", 1, chunk, &key, &a.signer.public_key())
            .unwrap();
    }
    b.store
        .publish_snapshot(&id, "a", 1, &manifest, &key, &a.signer.public_key())
        .unwrap();
    assert_eq!(b.store.knowledge_language_policy().unwrap(), policy);
    b.store.verify_knowledge_language_owner(&policy).unwrap();
    assert_eq!(
        b.store.artifact_language("analysis", &analysis_id).unwrap(),
        Some(language)
    );
    assert_eq!(
        b.store.list_events().unwrap()[0].raw_text,
        "UNTRANSLATED source 原文"
    );

    assert_eq!(
        b.store
            .connection
            .query_row("SELECT COUNT(*) FROM prompt_bundles", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        b.store
            .connection
            .query_row("SELECT COUNT(*) FROM sync_language_capabilities", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert!(!b.store.background_processing_allowed().unwrap());
    b.store.announce_language_capability().unwrap();
    assert!(
        b.store
            .language_capability("b", "stream-b")
            .unwrap()
            .is_some()
    );
}

#[test]
fn divergent_signed_policy_is_reviewed_with_the_displayed_local_basis() {
    let a = Db::owner();
    let b = Db::new("b");
    enroll(&a, &b);
    a.store.announce_language_capability().unwrap();
    b.store.announce_language_capability().unwrap();
    deliver(&b.store, &a.store, "a", "b");
    a.store
        .set_knowledge_language_with_signer("en", 0, Some(&a.signer))
        .unwrap();
    deliver(&a.store, &b.store, "b", "a");
    let mut incoming = a
        .store
        .pending_sync_operations("b", 100)
        .unwrap()
        .pop()
        .unwrap();
    incoming.origin_sequence += 1;
    incoming.logical_clock += 1;
    incoming.op_id = format!("{}:{}", incoming.origin_stream_id, incoming.origin_sequence);
    let crate::sync::protocol::SyncMutation::KnowledgeLanguageChanged {
        policy,
        base_revision,
    } = &mut incoming.mutation
    else {
        panic!()
    };
    *base_revision = None;
    policy.output_locale = "zh-Hans".into();
    policy.updated_at = "divergent-owner-policy".into();
    policy.owner_signature = Some(
        STANDARD.encode(
            a.signer
                .sign_membership(&policy.signed_payload().unwrap())
                .unwrap(),
        ),
    );
    b.store
        .receive_sync_operation("a", &serde_json::to_vec(&incoming).unwrap())
        .unwrap();
    assert_eq!(
        b.store.knowledge_language_policy().unwrap().output_locale,
        "en"
    );
    let review = b.store.list_object_conflicts().unwrap().pop().unwrap();
    assert_eq!(review.object_kind, "knowledge_language");
    assert_eq!(review.incoming.summary, "zh-Hans");
    assert!(
        b.store
            .resolve_object_conflict(&review.op_id, Some("stale-basis"), true)
            .is_err()
    );
    assert!(
        b.store
            .resolve_object_conflict(&review.op_id, review.local_updated_at.as_deref(), false)
            .is_err()
    );
    b.store
        .resolve_object_conflict(&review.op_id, review.local_updated_at.as_deref(), true)
        .unwrap();
    assert_eq!(
        b.store.knowledge_language_policy().unwrap().output_locale,
        "zh-Hans"
    );
    assert!(b.store.list_object_conflicts().unwrap().is_empty());
}
