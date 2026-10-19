use super::*;

#[test]
fn frame_decoder_rejects_unknown_record_kinds_and_oversized_lengths() {
    let mut decoder = FrameDecoder::default();
    assert!(decoder.push(&u32::MAX.to_be_bytes(), |_| Ok(())).is_err());
    let bytes = br#"{"kind":"arbitrary_sql","statement":"not executable"}"#;
    let mut frame = (bytes.len() as u32).to_be_bytes().to_vec();
    frame.extend_from_slice(bytes);
    assert!(FrameDecoder::default().push(&frame, |_| Ok(())).is_err());
}

#[test]
fn snapshot_carries_every_table_or_explicitly_excludes_it() {
    let db = Database::new();
    let mut stmt = db
        .store
        .connection
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")
        .unwrap();
    let tables: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    drop(stmt);
    assert!(tables.len() > 20, "schema 必须真实加载");
    for table in &tables {
        let carried = SNAPSHOT_CARRIED_TABLES.contains(&table.as_str());
        let excluded = SNAPSHOT_EXCLUDED_TABLES.iter().any(|(n, _)| n == table);
        assert!(
            carried ^ excluded,
            "表 {table} 未在快照携带/排除清单登记：新增表必须显式归类，否则快照恢复断档"
        );
    }
    // 携带清单必须在 QUERIES 里被真实引用（provider bundle 承载的表除外）。
    for table in SNAPSHOT_CARRIED_TABLES {
        assert!(
            QUERIES.iter().any(|q| q.contains(table)) || *table == "ai_provider_configs",
            "表 {table} 在携带清单里但没有对应的导出查询"
        );
    }
    // 目录段同哲学：文件枚举只允许触达登记过的目录（§31b）。
    assert!(
        !SNAPSHOT_CARRIED_DIRS.is_empty(),
        "目录携带清单不得为空：新目录必须显式登记，否则快照文件段永不外发"
    );
    for dir in SNAPSHOT_CARRIED_DIRS {
        assert!(
            !dir.contains("..") && !dir.starts_with('/'),
            "目录携带清单必须是数据目录内的相对路径：{dir}"
        );
    }
}
