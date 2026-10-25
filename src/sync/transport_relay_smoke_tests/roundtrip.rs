//! Relay 应用层往返冒烟测试（自 transport_relay_smoke_tests.rs 超长测试机械平移，行为零变化）。

//! 自 transport_relay_smoke_tests.rs 的超长测试机械平移（行为零变化）：
//! 端到端测试前段留在 mod.rs，应用层往返抽为本文件函数，由测试末段调用。

use super::{key, synthetic_provider};
use crate::sync::transport_relay::RelayClient;

pub(super) fn application_roundtrip(
    client: &crate::sync::transport_relay::RelayClient,
    sender: &str,
    mailbox: &str,
    owner_a: [u8; 32],
    replacement: [u8; 32],
    expiry: i64,
) {
    // Complete a real encrypted event + application-receipt roundtrip using
    // two isolated application databases, not a mirrored transport fixture.
    use crate::storage::{Store, sync::ReplicaIdentity};
    use crate::sync::{
        crypto::{ContentKey, DeviceIdentity},
        engine,
    };
    let directory =
        std::env::temp_dir().join(format!("elsewhen-relay-roundtrip-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&directory).unwrap();
    let a = Store::open(&directory.join("a.db")).unwrap();
    let b = Store::open(&directory.join("b.db")).unwrap();
    for (store, device) in [(&a, "a"), (&b, "b")] {
        store
            .initialize_sync_replica(&ReplicaIdentity {
                dataset_id: "relay-dataset".into(),
                device_id: device.into(),
                stream_id: format!("stream-{device}"),
                processor_device_id: "a".into(),
            })
            .unwrap();
    }
    a.authorize_sync_member("b", "stream-b").unwrap();
    b.authorize_sync_member("a", "stream-a").unwrap();
    let forward = key();
    let backward = key();
    client
        .register_sender(&mailbox, &replacement, &sender, &forward, expiry, 2)
        .unwrap();
    client
        .register_sender(&sender, &owner_a, &mailbox, &backward, expiry, 0)
        .unwrap();
    let signer_a = DeviceIdentity::generate().unwrap();
    let signer_b = DeviceIdentity::generate().unwrap();
    let content_ab = ContentKey::generate().unwrap();
    let content_ba = ContentKey::generate().unwrap();
    a.submit_input_as_event("isolated encrypted event", "capture", None)
        .unwrap();
    engine::stage_operations(&a, "b", 1, &content_ab, &signer_a).unwrap();
    let pushed = engine::push_due(
        &a,
        &client,
        "b",
        &mailbox,
        &sender,
        &forward,
        chrono::Utc::now().timestamp_millis(),
    )
    .unwrap();
    assert_eq!(pushed.uploaded, 1);
    assert_eq!(pushed.failed, 0);
    let delivered = client.fetch(&mailbox, &replacement, "0", 100).unwrap();
    assert_eq!(delivered.len(), 1);
    assert!(
        engine::accept_relay_payload(
            &b,
            &delivered[0].ciphertext,
            "a",
            1,
            &content_ab,
            &signer_a.public_key()
        )
        .unwrap()
    );
    assert_eq!(
        b.list_events().unwrap()[0].raw_text,
        "isolated encrypted event"
    );
    client
        .ack(&mailbox, &replacement, &[delivered[0].id.clone()])
        .unwrap();
    assert_eq!(
        a.pending_sync_operations("b", 10).unwrap().len(),
        1,
        "relay ack is not an application receipt"
    );
    engine::stage_receipts(&b, "a", 1, &content_ba, &signer_b).unwrap();
    let pushed = engine::push_due(
        &b,
        &client,
        "a",
        &sender,
        &mailbox,
        &backward,
        chrono::Utc::now().timestamp_millis(),
    )
    .unwrap();
    assert_eq!(pushed.uploaded, 1);
    let returned = client.fetch(&sender, &owner_a, "0", 100).unwrap();
    assert!(
        engine::accept_relay_payload(
            &a,
            &returned[0].ciphertext,
            "b",
            1,
            &content_ba,
            &signer_b.public_key()
        )
        .unwrap()
    );
    assert!(a.pending_sync_operations("b", 10).unwrap().is_empty());
    client
        .ack(&sender, &owner_a, &[returned[0].id.clone()])
        .unwrap();
    let provider_id = a
        .save_ai_provider_config(
            None,
            "synthetic provider",
            "openai-compatible",
            "https://synthetic.invalid/v1",
            "synthetic-model",
            "SYNTHETIC_PROVIDER_KEY",
            0.4,
            Some(2048),
        )
        .unwrap();
    a.connection
        .execute(
            "UPDATE ai_provider_configs SET context_window=65536 WHERE id=?1",
            [&provider_id],
        )
        .unwrap();
    let bundle = a.export_provider_bundle("provider-revision", None).unwrap();
    engine::stage_provider_bundle(&a, &bundle, "b", 1, &content_ab, &signer_a).unwrap();
    let pushed = engine::push_due(
        &a,
        &client,
        "b",
        &mailbox,
        &sender,
        &forward,
        chrono::Utc::now().timestamp_millis(),
    )
    .unwrap();
    assert_eq!(pushed.uploaded, 1);
    let delivered = client.fetch(&mailbox, &replacement, "0", 100).unwrap();
    assert_eq!(delivered.len(), 1);
    assert!(!delivered[0].ciphertext.contains("SYNTHETIC_PROVIDER_KEY"));
    assert!(
        engine::accept_relay_payload(
            &b,
            &delivered[0].ciphertext,
            "a",
            1,
            &content_ab,
            &signer_a.public_key()
        )
        .unwrap()
    );
    client
        .ack(&mailbox, &replacement, &[delivered[0].id.clone()])
        .unwrap();
    let restored = b.active_ai_provider_config().unwrap().unwrap();
    assert_eq!(restored.id, provider_id);
    assert_eq!(restored.api_key, "SYNTHETIC_PROVIDER_KEY");
    assert_eq!(restored.context_window, Some(65536));
    engine::stage_receipts(&b, "a", 1, &content_ba, &signer_b).unwrap();
    assert_eq!(
        engine::push_due(
            &b,
            &client,
            "a",
            &sender,
            &mailbox,
            &backward,
            chrono::Utc::now().timestamp_millis()
        )
        .unwrap()
        .uploaded,
        1
    );
    let receipts = client.fetch(&sender, &owner_a, "0", 100).unwrap();
    assert_eq!(receipts.len(), 1);
    assert!(
        engine::accept_relay_payload(
            &a,
            &receipts[0].ciphertext,
            "b",
            1,
            &content_ba,
            &signer_b.public_key()
        )
        .unwrap()
    );
    client
        .ack(&sender, &owner_a, &[receipts[0].id.clone()])
        .unwrap();
    assert!(
        a.due_sync_wire_envelopes("b", i64::MAX, 10)
            .unwrap()
            .is_empty()
    );
    // Initial checkpoint over the same real Worker, using a separate fresh
    // target. Poison first pages must not starve later authenticated chunks.
    let source = Store::open(&directory.join("snapshot-source.db")).unwrap();
    let target = Store::open(&directory.join("snapshot-target.db")).unwrap();
    let (provider_url, provider_calls, provider_server) = synthetic_provider();
    source
        .submit_input_as_event(&"snapshot original ".repeat(65_000), "capture", None)
        .unwrap();
    source
        .save_ai_provider_config(
            None,
            "snapshot profile",
            "openai-compatible",
            &provider_url,
            "synthetic",
            "SNAPSHOT_WORKER_SYNTHETIC_KEY",
            0.2,
            Some(1024),
        )
        .unwrap();
    for (store, device) in [(&source, "source"), (&target, "target")] {
        store
            .initialize_sync_replica(&ReplicaIdentity {
                dataset_id: "snapshot-dataset".into(),
                device_id: device.into(),
                stream_id: format!("stream-{device}"),
                processor_device_id: "source".into(),
            })
            .unwrap();
    }
    source
        .authorize_sync_member("target", "stream-target")
        .unwrap();
    target
        .authorize_sync_member("source", "stream-source")
        .unwrap();
    let source_owner = key();
    let target_owner = key();
    let source_mailbox = client.create_mailbox(&source_owner).unwrap();
    let target_mailbox = client.create_mailbox(&target_owner).unwrap();
    client
        .register_sender(
            &target_mailbox,
            &target_owner,
            &source_mailbox,
            &forward,
            expiry,
            0,
        )
        .unwrap();
    client
        .register_sender(
            &source_mailbox,
            &source_owner,
            &target_mailbox,
            &backward,
            expiry,
            0,
        )
        .unwrap();
    engine::snapshot::stage_snapshot(
        &source,
        "target",
        "worker-checkpoint",
        1,
        &content_ab,
        &signer_a,
        None,
    )
    .unwrap();
    target
        .begin_snapshot_import("worker-checkpoint", "source", 1)
        .unwrap();
    for _ in 0..4 {
        client
            .send(
                &target_mailbox,
                &source_mailbox,
                &forward,
                0,
                "bm90LWFuLWVudmVsb3Bl",
                expiry,
            )
            .unwrap();
    }
    let source_public = signer_a.public_key();
    let target_public = signer_b.public_key();
    let source_peer = [engine::ReceivingPeer {
        device_id: "source",
        mailbox_id: &source_mailbox,
        key_version: 1,
        key: &content_ab,
        public_key: &source_public,
    }];
    let target_peer = [engine::ReceivingPeer {
        device_id: "target",
        mailbox_id: &target_mailbox,
        key_version: 1,
        key: &content_ba,
        public_key: &target_public,
    }];
    let now = chrono::Utc::now().timestamp_millis();
    let mut rejects = 0;
    let mut accepted = 0;
    let mut dropped = 0;
    for tick in 0..40 {
        let pushed = engine::push_due(
            &source,
            &client,
            "target",
            &target_mailbox,
            &source_mailbox,
            &forward,
            now + tick * 60_000,
        )
        .unwrap();
        assert_eq!(pushed.failed, 0);
        let pulled = engine::pull_page(
            &target,
            &client,
            &target_mailbox,
            &target_owner,
            &source_peer,
        )
        .unwrap();
        rejects += pulled.rejected;
        accepted += pulled.accepted;
        dropped += pulled.dropped;
        assert!(!pulled.acknowledgement_failed);
        if !target.snapshot_import_pending().unwrap() {
            break;
        }
    }
    assert!(rejects >= 4);
    assert!(accepted > 4);
    // 「必表态」（R5-2）：开头那 4 封毒信连信封都不是，是**不可恢复**的拒绝。
    // 超过阈值后必须被 drop（relay 立即删）+ 本地留审计，否则它们会一直绕圈到
    // 7 天 TTL —— 事故里 9996 封死信就是这么堆到 MAX_MAILS 的。
    // 每拍只扫 3 封（一页），所以要多扫几圈才能把开头那 4 封都数到 3 次。
    for _ in 0..12 {
        if dropped >= 4 {
            break;
        }
        let pulled = engine::pull_page(
            &target,
            &client,
            &target_mailbox,
            &target_owner,
            &source_peer,
        )
        .unwrap();
        dropped += pulled.dropped;
    }
    assert_eq!(dropped, 4, "每封毒信只能被放弃一次");
    assert_eq!(
        client.status(&target_mailbox, &target_owner).unwrap().count,
        0,
        "放弃之后 relay 上不该再剩死信"
    );
    assert!(
        !target.snapshot_import_pending().unwrap(),
        "snapshot must complete despite poison mail at the start"
    );
    assert_eq!(target.list_events().unwrap().len(), 1);
    assert_eq!(
        target.active_ai_provider_config().unwrap().unwrap().api_key,
        "SNAPSHOT_WORKER_SYNTHETIC_KEY"
    );
    assert_eq!(
        provider_calls.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "receiving configuration must not probe a model"
    );
    let restored = target.active_ai_provider_config().unwrap().unwrap();
    assert_eq!(
        restored.base_url, provider_url,
        "do not rewrite the configured URL"
    );
    use crate::ai::provider::{AiProvider, OpenAiCompatibleConfig, OpenAiCompatibleProvider};
    let provider = OpenAiCompatibleProvider::new(OpenAiCompatibleConfig {
        base_url: restored.base_url,
        api_key: restored.api_key,
        model: restored.model,
        temperature: restored.temperature as f32,
        max_tokens: restored.max_tokens.map(|n| n as u32),
        context_window: restored.context_window.map(|n| n as u32),
    })
    .unwrap();
    let reply = provider
        .generate_reply(vec![crate::ai::memory::ContextMessage::new(
            "user",
            "synthetic explicit request",
        )])
        .unwrap();
    assert_eq!(reply.content, "configured on new device");
    provider_server.join().unwrap();
    assert_eq!(provider_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    engine::stage_receipts(&target, "source", 1, &content_ba, &signer_b).unwrap();
    assert_eq!(
        engine::push_due(
            &target,
            &client,
            "source",
            &source_mailbox,
            &target_mailbox,
            &backward,
            now
        )
        .unwrap()
        .uploaded,
        1
    );
    let returned = engine::pull_page(
        &source,
        &client,
        &source_mailbox,
        &source_owner,
        &target_peer,
    )
    .unwrap();
    assert_eq!(returned.accepted, 1);
    assert!(
        source
            .due_sync_wire_envelopes("target", i64::MAX, 10)
            .unwrap()
            .is_empty()
    );
    // A large capture created after the checkpoint travels as an incremental
    // operation, not a replacement snapshot or a second user submission.
    source
        .submit_input_as_event(
            &"large incremental original ".repeat(50_000),
            "capture",
            None,
        )
        .unwrap();
    engine::stage_operations(&source, "target", 1, &content_ab, &signer_a).unwrap();
    for tick in 0..60 {
        let time = now + (tick + 40) * 60_000;
        assert_eq!(
            engine::push_due(
                &source,
                &client,
                "target",
                &target_mailbox,
                &source_mailbox,
                &forward,
                time
            )
            .unwrap()
            .failed,
            0
        );
        let pulled = engine::pull_page(
            &target,
            &client,
            &target_mailbox,
            &target_owner,
            &source_peer,
        )
        .unwrap();
        assert!(!pulled.acknowledgement_failed);
        engine::stage_receipts(&target, "source", 1, &content_ba, &signer_b).unwrap();
        assert_eq!(
            engine::push_due(
                &target,
                &client,
                "source",
                &source_mailbox,
                &target_mailbox,
                &backward,
                time
            )
            .unwrap()
            .failed,
            0
        );
        engine::pull_page(
            &source,
            &client,
            &source_mailbox,
            &source_owner,
            &target_peer,
        )
        .unwrap();
        if source
            .pending_sync_operations("target", 100)
            .unwrap()
            .is_empty()
        {
            break;
        }
    }
    assert_eq!(target.list_events().unwrap().len(), 2);
    assert!(
        source
            .pending_sync_operations("target", 100)
            .unwrap()
            .is_empty()
    );
    assert!(
        source
            .due_sync_wire_envelopes("target", i64::MAX, 10)
            .unwrap()
            .is_empty()
    );
    let job = source.claim_analysis_job().unwrap().unwrap();
    source
        .complete_analysis(
            &job,
            "event-analysis",
            r#"{"recordable":true,"summary":"controlled analysis"}"#,
        )
        .unwrap();
    engine::stage_operations(&source, "target", 1, &content_ab, &signer_a).unwrap();
    for tick in 0..20 {
        let time = now + (100 + tick) * 60_000;
        assert_eq!(
            engine::push_due(
                &source,
                &client,
                "target",
                &target_mailbox,
                &source_mailbox,
                &forward,
                time
            )
            .unwrap()
            .failed,
            0
        );
        engine::pull_page(
            &target,
            &client,
            &target_mailbox,
            &target_owner,
            &source_peer,
        )
        .unwrap();
        engine::stage_receipts(&target, "source", 1, &content_ba, &signer_b).unwrap();
        engine::push_due(
            &target,
            &client,
            "source",
            &source_mailbox,
            &target_mailbox,
            &backward,
            time,
        )
        .unwrap();
        engine::pull_page(
            &source,
            &client,
            &source_mailbox,
            &source_owner,
            &target_peer,
        )
        .unwrap();
        if source
            .pending_sync_operations("target", 100)
            .unwrap()
            .is_empty()
        {
            break;
        }
    }
    assert!(
        source
            .pending_sync_operations("target", 100)
            .unwrap()
            .is_empty()
    );
    assert_eq!(target.analysis_job_stats().unwrap().succeeded, 1);
    assert!(target.claim_analysis_job().unwrap().is_none());
    assert_eq!(
        provider_calls.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "syncing analysis history is not a model request"
    );
    // Reopen preserves the scan cursor; sweep eventually revisits poison mail
    // but never acknowledges it or mistakes it for application progress.
    let reopened = Store::open(&target.path).unwrap();
    assert_eq!(
        reopened.relay_scan_cursor(&target_mailbox).unwrap(),
        target.relay_scan_cursor(&target_mailbox).unwrap()
    );
    drop(reopened);
    drop(source);
    drop(target);
    crate::sync::transport_relay_smoke_tests::membership::roundtrip(&client, &directory);
    crate::sync::transport_relay_smoke_tests::owner_ui::roundtrip(&client, &directory);
    crate::sync::transport_relay_smoke_tests::joiner_ui::roundtrip(&client, &directory);
    drop(a);
    drop(b);
}
