use super::*;
use uuid::Uuid;

#[test]
fn single_active_config_conflict_error_message_matches_column() {
    // P2-2 兜底分支依赖识别「同一时刻两条 is_active=1」冲突；裸 SQL 触发
    // 部分唯一索引冲突，确认错误串按列报（同 index 的名字不出现）。
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let now = chrono::Utc::now().to_rfc3339();
    let insert = |name: &str| {
        store
            .connection
            .execute(
                "INSERT INTO ai_provider_configs
                 (id,name,provider_type,base_url,model,api_key_source,is_active,temperature,max_tokens,created_at,updated_at)
                 VALUES (?1,?2,'openai','http://x','gpt','env',1,0.7,NULL,?3,?3)",
                params![Uuid::new_v4().to_string(), name, now],
            )
            .map(|_| ())
    };
    insert("cfg-a").unwrap();
    let err = insert("cfg-b").unwrap_err();
    assert!(
        err.to_string().contains("ai_provider_configs.is_active"),
        "错误串应含激活标记列：{err}"
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}
#[test]
fn daily_reviews_are_versioned_and_require_same_day_sources() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let event_id = store
        .insert_event(NewEvent::now("今天完成总结契约"))
        .unwrap();
    let today = chrono::Local::now().date_naive();

    assert!(
        store
            .save_daily_review(today, "daily-review-v1", "{}", &[])
            .is_err()
    );
    assert!(
        store
            .save_daily_review(
                today - chrono::Duration::days(1),
                "daily-review-v1",
                "{}",
                std::slice::from_ref(&event_id),
            )
            .is_err()
    );

    let first = store
        .save_daily_review(
            today,
            "daily-review-v1",
            r#"{"summary":"第一版"}"#,
            std::slice::from_ref(&event_id),
        )
        .unwrap();
    let second = store
        .save_daily_review(
            today,
            "daily-review-v2",
            r#"{"summary":"第二版"}"#,
            std::slice::from_ref(&event_id),
        )
        .unwrap();
    assert_ne!(first, second);

    let latest = store.latest_daily_review(today).unwrap().unwrap();
    assert_eq!(latest.id, second);
    assert_eq!(latest.prompt_version, "daily-review-v2");
    assert_eq!(latest.source_event_ids, vec![event_id]);
    assert_eq!(store.list_events().unwrap().len(), 1);

    drop(store);
    let _ = std::fs::remove_file(path);
}
#[test]
fn provider_config_is_fully_persisted() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    store
        .upsert_ai_provider_config("https://example.test/v1", "test-model", "secret-value")
        .unwrap();
    let provider = store.active_ai_provider_config().unwrap().unwrap();
    assert_eq!(provider.base_url, "https://example.test/v1");
    assert_eq!(provider.model, "test-model");
    assert_eq!(provider.api_key_source, "database");
    assert_eq!(provider.api_key, "secret-value");
    let _ = std::fs::remove_file(path);
}
#[test]
fn open_background_skips_outdated_database_without_writing() {
    let path = temporary_database();
    // 老版本库（v1）：后台打开应返回 None 且不做任何启动写入。
    {
        let mut connection = rusqlite::Connection::open(&path).unwrap();
        crate::storage::new_migrations::initialize_test_version(&mut connection, 1).unwrap();
    }
    assert!(Store::open_background(&path).unwrap().is_none());
    let version: i64 = rusqlite::Connection::open(&path)
        .unwrap()
        .query_row("PRAGMA main.user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 1, "后台打开不得把库升级到最新版本");
    let _ = std::fs::remove_file(&path);

    // 最新版本库：后台打开正常返回连接。
    let path = temporary_database();
    drop(Store::open(&path).unwrap());
    assert!(Store::open_background(&path).unwrap().is_some());
    let _ = std::fs::remove_file(path);
}
#[test]
fn busy_retry_retries_busy_and_stops_on_other_errors() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    // BUSY 会先退避重试，最终成功。
    let attempts = AtomicUsize::new(0);
    let result = with_busy_retry(|| {
        if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
            Err(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
                None,
            )
            .into())
        } else {
            Ok(42)
        }
    });
    assert_eq!(result.unwrap(), 42);
    assert_eq!(attempts.load(Ordering::SeqCst), 2);

    // 非 BUSY 错误不重试，原样上抛。
    let attempts = AtomicUsize::new(0);
    let result: Result<i64> = with_busy_retry(|| {
        attempts.fetch_add(1, Ordering::SeqCst);
        anyhow::bail!("permanent failure")
    });
    assert!(result.is_err());
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
}
#[test]
fn backfill_folds_legacy_tweet_image_paths_and_collects_assets() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let rel = "tweet_images/2099471972589576451/img_1.jpg";
    // 复刻存量形态：导入时正文被写成本机绝对路径（§31 前），字节已落盘。
    let data_dir = path.parent().unwrap().to_path_buf();
    std::fs::create_dir_all(data_dir.join("tweet_images/2099471972589576451")).unwrap();
    let bytes = vec![7u8; 33];
    std::fs::write(data_dir.join(rel), &bytes).unwrap();
    let legacy = format!("正文开头\n\n![]({})\n\n结尾", data_dir.join(rel).display());
    store
        .connection
        .execute(
            "INSERT INTO wiki_pages(id,slug,kind,title,summary,content_md,tags,source_event_ids,evidence_count,first_seen_at,last_seen_at,status,created_at,updated_at,area) \
             VALUES('wp-legacy','tweet-2099471972589576451','source','旧页','',?1,'[]','[]',1,'t1','t1','active','t1','t1','imported')",
            [&legacy],
        )
        .unwrap();
    drop(store);

    // 重新 open 触发启动回填：绝对路径折叠为占位符，字节收进资产表。
    let store = Store::open(&path).unwrap();
    let folded: String = store
        .connection
        .query_row(
            "SELECT content_md FROM wiki_pages WHERE id='wp-legacy'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        folded.contains(&format!("![]({{{{tweet-image:{rel}}}}})")),
        "应折叠为占位符：{folded}"
    );
    assert!(
        !folded.contains(&data_dir.display().to_string()),
        "不得残留本机路径"
    );
    let (sha, size): (String, i64) = store
        .connection
        .query_row(
            "SELECT sha256,size FROM wiki_assets WHERE rel=?1",
            [rel],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(size, 33);
    assert_eq!(sha.len(), 64);

    // 幂等：再 open 一次内容不变。
    drop(store);
    let store = Store::open(&path).unwrap();
    let again: String = store
        .connection
        .query_row(
            "SELECT content_md FROM wiki_pages WHERE id='wp-legacy'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(again, folded);
    let _ = std::fs::remove_file(path);
}
#[test]
fn network_proxy_save_load_roundtrip_and_validation() {
    let store = Store::open(&temporary_database()).unwrap();
    store
        .connection
        .execute(
            "INSERT INTO network_settings(singleton,doh_uri,doh_scope,doh_mode,proxy_mode,proxy_url,created_at,updated_at)
             VALUES(1,'','search_only','auto','auto','',datetime('now'),datetime('now'))",
            [],
        )
        .unwrap();

    // 默认 auto
    let s = store.network_settings().unwrap();
    assert_eq!(s.proxy_mode, "auto");
    assert_eq!(s.proxy_url, "");

    // custom 圆桌
    store
        .save_network_proxy("custom", "socks5://127.0.0.1:7890")
        .unwrap();
    let s = store.network_settings().unwrap();
    assert_eq!(s.proxy_mode, "custom");
    assert_eq!(s.proxy_url, "socks5://127.0.0.1:7890");

    // off
    store.save_network_proxy("off", "").unwrap();
    let s = store.network_settings().unwrap();
    assert_eq!(s.proxy_mode, "off");

    // 校验：非法 mode；custom 缺地址/地址不带 scheme
    assert!(store.save_network_proxy("weird", "").is_err());
    assert!(store.save_network_proxy("custom", "").is_err());
    assert!(
        store
            .save_network_proxy("custom", "127.0.0.1:7890")
            .is_err()
    );

    // 失败保存不落库（仍是 off）
    let s = store.network_settings().unwrap();
    assert_eq!(s.proxy_mode, "off");
}
