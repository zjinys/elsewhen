use super::*;
use crate::knowledge::select::{query_terms, scan_candidates};
use crate::storage::{ContentPolicy, WikiPageDraft};

struct Db {
    store: Store,
    path: std::path::PathBuf,
}

impl Db {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("wiki-search-{}.db", uuid::Uuid::new_v4()));
        Self {
            store: Store::open(&path).unwrap(),
            path,
        }
    }

    fn page(&self, slug: &str, kind: &str, title: &str, body: &str) -> WikiPage {
        self.page_with(slug, kind, title, body, &[], "")
    }

    fn page_with(
        &self,
        slug: &str,
        kind: &str,
        title: &str,
        body: &str,
        tags: &[&str],
        summary: &str,
    ) -> WikiPage {
        self.store
            .upsert_wiki_page(
                &WikiPageDraft {
                    slug: slug.into(),
                    kind: kind.into(),
                    title: title.into(),
                    summary: summary.into(),
                    content_md: body.into(),
                    tags: tags.iter().map(|tag| tag.to_string()).collect(),
                    source_event_ids: vec![],
                    status: "active".into(),
                    reason: "test".into(),
                    source_url: None,
                },
                ContentPolicy::Always,
            )
            .unwrap()
            .page
    }

    fn dirty(&self) -> i64 {
        self.count("wiki_search_dirty")
    }

    fn count(&self, table: &str) -> i64 {
        self.store
            .connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap()
    }

    /// 把索引完全追平（后台 tick 做的事）。
    fn catch_up(&self) {
        for _ in 0..1000 {
            if self.store.refresh_wiki_search_batch().unwrap() {
                return;
            }
        }
        panic!("索引没有追平");
    }

    /// 走索引的候选页 slug；索引不可用时是 None。
    fn indexed(&self, query: &str) -> Option<Vec<String>> {
        self.store
            .wiki_search_candidates(&query_terms(query))
            .unwrap()
            .map(|pages| pages.into_iter().map(|page| page.slug).collect())
    }

    /// 扫描路径的候选页 slug。扫描的 WHERE 还匹配摘要，但摘要不计分，这类页
    /// 排在最后并在后续打分时被丢弃；这里按同样的规则去掉它们。
    fn scanned(&self, query: &str) -> Vec<String> {
        let terms = query_terms(query);
        scan_candidates(&self.store, &terms)
            .unwrap()
            .into_iter()
            .filter(|page| {
                let (prominent, body): (String, String) = self
                    .store
                    .connection
                    .query_row(
                        "SELECT lower(p.title||' '||p.tags||' '||COALESCE((SELECT applicable_when
                                FROM knowledge_metadata WHERE page_id=p.id),'')), lower(p.content_md)
                           FROM wiki_pages p WHERE p.id=?1",
                        [&page.id],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .unwrap();
                terms
                    .iter()
                    .any(|term| prominent.contains(term.as_str()) || body.contains(term.as_str()))
            })
            .map(|page| page.slug)
            .collect()
    }
}

impl Drop for Db {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// 打包的 SQLite 必须带 FTS5，且支持 contentless_delete（3.43+）。迁移里的
/// CREATE VIRTUAL TABLE 依赖它：缺了应用起不来，所以单独钉住。
#[test]
fn bundled_sqlite_supports_the_fts5_features_the_index_relies_on() {
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    let enabled: bool = connection
        .query_row("SELECT sqlite_compileoption_used('ENABLE_FTS5')", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(enabled, "打包的 SQLite 没有启用 FTS5");
    let version: String = connection
        .query_row("SELECT sqlite_version()", [], |r| r.get(0))
        .unwrap();
    let parts: Vec<u32> = version.split('.').map(|p| p.parse().unwrap()).collect();
    assert!(
        (parts[0], parts[1]) >= (3, 43),
        "contentless_delete 需要 SQLite 3.43+，当前 {version}"
    );
    connection
        .execute_batch(
            "CREATE VIRTUAL TABLE ft USING fts5(prominent, body, content='',
                 contentless_delete=1, tokenize='ascii');",
        )
        .unwrap();
    // 快照门禁按表名登记影子表：这里钉住它们的名字。
    let tables: Vec<String> = connection
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(
        tables,
        ["ft", "ft_config", "ft_data", "ft_docsize", "ft_idx"]
    );
}

/// 索引语义的核心：词项的二元组短语命中 ⇔ 词项是小写后文本的子串。用真实的
/// FTS5 引擎逐条比对，覆盖段边界、单字符段、大小写、中英文混排和重叠二元组。
#[test]
fn bigram_phrases_match_exactly_when_the_term_is_a_substring() {
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    connection
        .execute_batch(
            "CREATE VIRTUAL TABLE ft USING fts5(body, content='', contentless_delete=1,
                 tokenize='ascii');",
        )
        .unwrap();
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = |bound: usize| {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((state >> 33) as usize) % bound
    };
    let alphabet = [
        'a', 'b', 'A', 'B', '1', '事', '务', '回', 'é', 'É', ' ', '，', '-', '_', '\n', '。',
    ];
    let docs: Vec<String> = (0..400)
        .map(|_| {
            (0..next(30))
                .map(|_| alphabet[next(alphabet.len())])
                .collect()
        })
        .collect();
    for (index, doc) in docs.iter().enumerate() {
        connection
            .execute(
                "INSERT INTO ft(rowid,body) VALUES(?1,?2)",
                params![index as i64, tokens(doc)],
            )
            .unwrap();
    }
    // 2 到 4 个字符的全部词项。
    let letters = ['a', 'b', '1', '事', '务', 'é'];
    let mut terms: Vec<String> = letters.iter().map(|ch| ch.to_string()).collect();
    let mut all = Vec::new();
    for _ in 0..3 {
        terms = terms
            .iter()
            .flat_map(|prefix| letters.iter().map(move |ch| format!("{prefix}{ch}")))
            .collect();
        all.extend(terms.iter().cloned());
    }
    assert_eq!(all.len(), 36 + 216 + 1296);
    let mut statement = connection
        .prepare("SELECT rowid FROM ft WHERE ft MATCH ?1 ORDER BY rowid")
        .unwrap();
    let mut matched_terms = 0;
    for term in &all {
        let expected: Vec<i64> = docs
            .iter()
            .enumerate()
            .filter(|(_, doc)| doc.to_lowercase().contains(term.as_str()))
            .map(|(index, _)| index as i64)
            .collect();
        let found: Vec<i64> = statement
            .query_map([format!("body : {}", phrase(term).unwrap())], |row| {
                row.get(0)
            })
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(found, expected, "词项 {term:?}");
        matched_terms += usize::from(!expected.is_empty());
    }
    // 语料确实让相当一部分词项有命中，比对不是空对空。
    assert!(matched_terms > 300, "{matched_terms}");
    // 不能表达成短语的词项交给调用方回落扫描。
    assert_eq!(phrase("事"), None);
    assert_eq!(phrase("a b"), None);
    assert_eq!(tokens("a 事 1"), "");
    assert_eq!(tokens("Ab，事务"), "6162 z e4ba8be58aa1 z ");
}

#[test]
fn index_candidates_equal_the_scan_in_content_and_order() {
    let db = Db::new();
    let s = &db.store;
    // 90 个正文都含「事务」的页：超过 80 的候选上限，靠分数、时间和 id 决定去留。
    for index in 0..90 {
        db.page(
            &format!("method/bulk-{index:02}"),
            "method",
            &format!("批量页{index:02}"),
            "这里讲数据库事务的一般做法。",
        );
    }
    // 让时间有并列也有先后：排序的第二、第三关键字都被用到。
    s.connection
        .execute(
            "UPDATE wiki_pages SET last_seen_at=CASE WHEN CAST(substr(slug,13) AS INTEGER)%3=0
                 THEN '2026-01-02T00:00:00Z' ELSE '2026-01-01T00:00:00Z' END
               WHERE slug LIKE 'method/bulk-%'",
            [],
        )
        .unwrap();
    db.page(
        "method/title-hit",
        "method",
        "事务回滚手册",
        "正文不含那个词。",
    );
    db.page_with(
        "method/tag-hit",
        "method",
        "标签命中",
        "正文也不含。",
        &["事务", "SQLite"],
        "",
    );
    db.page("method/condition-hit", "method", "条件命中", "正文无关。");
    s.set_knowledge_metadata("method/condition-hit", "处理数据库事务失败时", "reference")
        .unwrap();
    db.page_with(
        "method/summary-only",
        "method",
        "只在摘要里",
        "正文无关。",
        &[],
        "摘要提到事务",
    );
    db.page(
        "method/both",
        "method",
        "事务与重试",
        "事务失败后重试三次。NoRetrying 策略另述，SQLite Transaction 亦然。",
    );
    db.page("method/archived", "method", "事务旧页", "事务旧做法。");
    s.connection
        .execute(
            "UPDATE wiki_pages SET status='archived' WHERE slug='method/archived'",
            [],
        )
        .unwrap();
    db.page("source/incubating", "source", "事务素材", "事务素材正文。");
    db.page(
        "source/endorsed",
        "source",
        "认可的事务素材",
        "事务素材正文。",
    );
    s.connection
        .execute(
            "UPDATE wiki_pages SET opinion='endorse' WHERE slug='source/endorsed'",
            [],
        )
        .unwrap();
    db.catch_up();
    assert_eq!(db.dirty(), 0);

    for query in [
        "事务",
        "数据库事务失败怎么重试",
        "retry",
        "SQLite transaction",
        "回滚手册",
        "不存在的词项组合",
        "批量页07",
        "sqlite事务",
    ] {
        let indexed = db.indexed(query).expect("索引已追平");
        assert_eq!(indexed, db.scanned(query), "查询 {query:?}");
    }
    let hits = db.indexed("事务").unwrap();
    assert_eq!(hits.len(), 80);
    // 主题文本命中排在只有正文命中的前面；资格过滤与扫描一致。
    assert_eq!(
        hits[..4],
        [
            "method/both",
            "method/condition-hit",
            "method/tag-hit",
            "method/title-hit"
        ]
        .map(String::from)
    );
    for excluded in [
        "method/summary-only",
        "method/archived",
        "source/incubating",
    ] {
        assert!(!hits.contains(&excluded.to_string()), "{excluded}");
    }
    // ASCII 词按子串匹配，词中间的命中也算（与扫描相同）。
    assert_eq!(db.indexed("retry").unwrap(), ["method/both"]);
}

#[test]
fn index_follows_every_kind_of_page_change() {
    let db = Db::new();
    let s = &db.store;
    db.page("method/a", "method", "甲页", "正文讲事务。");
    // 检索路径自己把少量脏页追平。
    assert_eq!(db.dirty(), 1);
    assert_eq!(db.indexed("事务").unwrap(), ["method/a"]);
    assert_eq!(db.dirty(), 0);

    // 正文改了：旧词不再命中，新词命中。
    db.page("method/a", "method", "甲页", "正文改讲备份。");
    assert_eq!(db.indexed("事务").unwrap(), Vec::<String>::new());
    assert_eq!(db.indexed("备份").unwrap(), ["method/a"]);

    // 标签、适用条件、标题各自进主题文本。
    s.update_wiki_tags("method/a", &["回滚".to_string()])
        .unwrap();
    assert_eq!(db.indexed("回滚").unwrap(), ["method/a"]);
    s.set_knowledge_metadata("method/a", "发布窗口之前", "reference")
        .unwrap();
    assert_eq!(db.indexed("发布窗口").unwrap(), ["method/a"]);
    let renamed = s.rename_wiki_page("method/a", "乙页新名", "test").unwrap();
    assert!(renamed.changed);
    let slug = renamed.new_slug;
    assert_eq!(db.indexed("新名").unwrap(), [slug.clone()]);
    assert_eq!(db.indexed("甲页").unwrap(), Vec::<String>::new());

    // 归档不需要重建索引：资格在查询时按页面当前状态判断。
    s.connection
        .execute(
            "UPDATE wiki_pages SET status='archived' WHERE slug=?1",
            [&slug],
        )
        .unwrap();
    assert_eq!(db.dirty(), 0);
    assert_eq!(db.indexed("备份").unwrap(), Vec::<String>::new());
    s.connection
        .execute(
            "UPDATE wiki_pages SET status='active' WHERE slug=?1",
            [&slug],
        )
        .unwrap();
    assert_eq!(db.indexed("备份").unwrap(), [slug.clone()]);

    // 删除：索引项和文档号一起清掉。
    assert_eq!(db.count("wiki_search_docs"), 1);
    assert!(s.delete_wiki_page(&slug).unwrap());
    assert_eq!(db.indexed("备份").unwrap(), Vec::<String>::new());
    assert_eq!(db.count("wiki_search_docs"), 0);
    assert_eq!(db.dirty(), 0);
}

#[test]
fn stale_index_is_never_used_and_never_written_inside_a_callers_transaction() {
    let db = Db::new();
    let s = &db.store;
    for index in 0..70 {
        db.page(
            &format!("method/p{index:02}"),
            "method",
            &format!("页{index:02}"),
            "正文讲事务。",
        );
    }
    // 70 个脏页超过检索路径一次追平的量：这次不用索引，但追了一批。
    assert_eq!(db.dirty(), 70);
    assert_eq!(db.indexed("事务"), None);
    assert_eq!(db.dirty(), 70 - QUERY_REFRESH_PAGES as i64);
    // 再来一次就追平了，结果与扫描一致。
    assert_eq!(db.indexed("事务").unwrap(), db.scanned("事务"));
    assert_eq!(db.dirty(), 0);

    // 身处调用方的事务：不写索引、不用它，事务提交后才追平。
    let transaction = s.begin_write_transaction().unwrap().unwrap();
    db.page("method/in-tx", "method", "事务内新页", "正文讲回滚。");
    assert_eq!(db.dirty(), 1);
    assert_eq!(db.indexed("回滚"), None);
    assert_eq!(db.dirty(), 1);
    assert_eq!(db.scanned("回滚"), ["method/in-tx"]);
    transaction.commit().unwrap();
    assert_eq!(db.indexed("回滚").unwrap(), ["method/in-tx"]);

    // 大页按字符数截批：一批只处理到上限为止，但至少一页。
    let big = "长".repeat(QUERY_REFRESH_CHARS);
    db.page("method/big-1", "method", "大页一", &big);
    db.page("method/big-2", "method", "大页二", &big);
    assert_eq!(db.indexed("大页"), None);
    assert_eq!(db.dirty(), 1);
    assert_eq!(db.indexed("大页").unwrap().len(), 2);
}

#[test]
fn query_path_does_not_wait_for_another_connections_write_lock() {
    let db = Db::new();
    db.page("method/a", "method", "甲页", "正文讲事务。");
    assert_eq!(db.dirty(), 1);
    let other = Store::open(&db.path).unwrap();
    let writing = other.begin_write_transaction().unwrap().unwrap();
    let started = std::time::Instant::now();
    assert_eq!(db.indexed("事务"), None);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "检索路径不应排队等写锁：{:?}",
        started.elapsed()
    );
    assert_eq!(db.dirty(), 1);
    // 扫描照常可用；连接原来的等待时间已恢复。
    assert_eq!(db.scanned("事务"), ["method/a"]);
    let timeout: i64 = db
        .store
        .connection
        .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
        .unwrap();
    assert_eq!(timeout, 5000);
    drop(writing);
    assert_eq!(db.indexed("事务").unwrap(), ["method/a"]);
}

#[test]
fn index_rebuilds_after_a_scheme_change_and_survives_vacuum() {
    let db = Db::new();
    let s = &db.store;
    for index in 0..12 {
        db.page(
            &format!("method/p{index:02}"),
            "method",
            &format!("页{index:02}"),
            &format!("正文讲事务，第{index:02}份。"),
        );
    }
    db.catch_up();
    let before = db.indexed("事务").unwrap();
    assert_eq!(before.len(), 12);

    // 分词方案变了：旧索引整体作废并重建。
    s.set_meta(SCHEME_KEY, "older-scheme").unwrap();
    assert_eq!(db.indexed("事务").unwrap(), before);
    assert_eq!(s.get_meta(SCHEME_KEY).unwrap().as_deref(), Some(SCHEME));
    assert_eq!(db.count("wiki_search_docs"), 12);

    // 删掉一些页再整理数据库：页面表的隐式 rowid 会重排，索引用的文档号不受影响。
    for index in [0, 3, 7] {
        assert!(s.delete_wiki_page(&format!("method/p{index:02}")).unwrap());
    }
    db.catch_up();
    s.connection.execute_batch("VACUUM").unwrap();
    assert_eq!(db.dirty(), 0);
    let after = db.indexed("事务").unwrap();
    assert_eq!(after.len(), 9);
    assert_eq!(after, db.scanned("事务"));
    // 查询按两字词项「任一命中」取候选，命中词项最多的排最前。
    let ranked = db.indexed("第05份").unwrap();
    assert_eq!(ranked[0], "method/p05");
    assert_eq!(ranked, db.scanned("第05份"));
}

/// 升级：迁移前已有的页面全部排队等建索引；之后的写入无论外层语句带什么
/// 冲突子句，重复标脏都不会让页面写入本身失败。
#[test]
fn migration_queues_existing_pages_and_dirty_marking_never_fails_a_page_write() {
    let mut connection = rusqlite::Connection::open_in_memory().unwrap();
    crate::storage::new_migrations::initialize_test_version(&mut connection, 83).unwrap();
    let insert = |connection: &rusqlite::Connection, verb: &str, id: &str| {
        connection.execute(
            &format!(
                "{verb} INTO wiki_pages(id,slug,kind,title,content_md,first_seen_at,
                     last_seen_at,created_at,updated_at)
                 VALUES(?1,?1,'method',?1,'正文','t','t','t','t')"
            ),
            [id],
        )
    };
    insert(&connection, "INSERT", "old-1").unwrap();
    insert(&connection, "INSERT", "old-2").unwrap();
    crate::storage::new_migrations::initialize(&mut connection).unwrap();
    let dirty = |connection: &rusqlite::Connection| -> Vec<String> {
        connection
            .prepare("SELECT page_id FROM wiki_search_dirty ORDER BY page_id")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    };
    assert_eq!(dirty(&connection), ["old-1", "old-2"]);

    // 已经在队里的页再被改：外层语句带 OR ROLLBACK / OR FAIL / OR ABORT 都照常成功。
    for verb in ["UPDATE OR ROLLBACK", "UPDATE OR FAIL", "UPDATE OR ABORT"] {
        connection
            .execute(
                &format!("{verb} wiki_pages SET content_md=content_md||'x' WHERE id='old-1'"),
                [],
            )
            .unwrap();
    }
    for (verb, id) in [
        ("INSERT OR ROLLBACK", "new-1"),
        ("INSERT OR REPLACE", "new-1"),
    ] {
        insert(&connection, verb, id).unwrap();
    }
    connection
        .execute(
            "INSERT OR REPLACE INTO knowledge_metadata(page_id,applicable_when) VALUES('new-1','条件')",
            [],
        )
        .unwrap();
    connection
        .execute("DELETE FROM wiki_pages WHERE id='old-2'", [])
        .unwrap();
    assert_eq!(dirty(&connection), ["new-1", "old-1", "old-2"]);

    // 与检索无关的列变化不标脏。
    connection
        .execute("DELETE FROM wiki_search_dirty", [])
        .unwrap();
    connection
        .execute(
            "UPDATE wiki_pages SET status='archived',summary='s',last_seen_at='u',opinion='endorse'
              WHERE id='old-1'",
            [],
        )
        .unwrap();
    connection
        .execute(
            "UPDATE knowledge_metadata SET strength='method' WHERE page_id='new-1'",
            [],
        )
        .unwrap();
    assert_eq!(dirty(&connection), Vec::<String>::new());
}

/// 规模基准：索引与扫描在同一份合成语料上的耗时、索引体积和单页更新成本。
///   cargo test --lib synthetic_search_scale -- --ignored --nocapture
/// 页数可用 ELSEWHEN_SCALE_PAGES 指定（100 到 100000），默认一万。
#[test]
#[ignore = "synthetic scale benchmark; run explicitly with --ignored --nocapture"]
fn synthetic_search_scale() {
    measure_scale(
        std::env::var("ELSEWHEN_SCALE_PAGES")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(10_000),
    );
}

/// 十万页的同一基准（库文件约 600 MB，建索引要几十秒）。
///   cargo test --lib synthetic_search_at_100k_pages -- --ignored --nocapture
#[test]
#[ignore = "synthetic scale benchmark at 100k pages; slow, run explicitly"]
fn synthetic_search_at_100k_pages() {
    measure_scale(100_000);
}

fn measure_scale(pages: usize) {
    use std::time::Instant;
    assert!((100..=100_000).contains(&pages));
    let db = Db::new();
    let s = &db.store;
    // 每页约 1700 个字符的共同正文，加上各页不同的一句；千分之一的页带一个
    // 罕见词。共同正文让「常见词」命中全部页面——这是索引最不利的情形。
    let filler = "这是一段用于测量词法索引的合成资料，包含适用条件和反例。".repeat(60);
    let transaction = s.connection.unchecked_transaction().unwrap();
    for index in 0..pages {
        let id = format!("scale-{index:06}");
        let rare = if index % 1000 == 7 {
            "罕见标记甲乙丙。"
        } else {
            ""
        };
        s.connection
            .execute(
                "INSERT INTO wiki_pages(id,slug,kind,title,content_md,first_seen_at,last_seen_at,
                     created_at,updated_at,area)
                 VALUES(?1,?1,'method',?1,?2,'2026-09-29','2026-09-29','2026-09-29','2026-09-29',
                     'insight')",
                params![id, format!("{filler}第{index}号资料。{rare}")],
            )
            .unwrap();
    }
    transaction.commit().unwrap();
    let size = || std::fs::metadata(&db.path).unwrap().len();
    let before = size();
    let start = Instant::now();
    db.catch_up();
    let build_ms = start.elapsed().as_millis();
    let after = size();
    eprintln!(
        "scale pages={pages} chars_per_page={} db_bytes_before={before} index_bytes={} build_ms={build_ms}",
        filler.chars().count(),
        after - before
    );
    for (label, query) in [
        ("common", "适用条件 反例"),
        ("rare", "罕见标记甲乙丙"),
        ("missing", "不存在的词项"),
    ] {
        let terms = query_terms(query);
        let start = Instant::now();
        let scanned = scan_candidates(s, &terms).unwrap();
        let scan_ms = start.elapsed().as_millis();
        let start = Instant::now();
        let indexed = s.wiki_search_candidates(&terms).unwrap().unwrap();
        let index_ms = start.elapsed().as_millis();
        eprintln!(
            "  {label}: terms={} scan_ms={scan_ms} index_ms={index_ms} candidates={}",
            terms.len(),
            indexed.len()
        );
        let slugs = |pages: &[WikiPage]| -> Vec<String> {
            pages.iter().map(|page| page.slug.clone()).collect()
        };
        assert_eq!(slugs(&indexed), slugs(&scanned), "{label}");
    }
    let start = Instant::now();
    s.connection
        .execute(
            "UPDATE wiki_pages SET content_md=content_md||'新增一句。' WHERE id='scale-000000'",
            [],
        )
        .unwrap();
    assert!(s.refresh_wiki_search_batch().unwrap());
    eprintln!("  single_page_update_ms={}", start.elapsed().as_millis());
    let plan: Vec<String> = s
        .connection
        .prepare(&format!("EXPLAIN QUERY PLAN {}", candidates_sql()))
        .unwrap()
        .query_map(["[]"], |row| row.get(3))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    for line in &plan {
        eprintln!("  plan: {line}");
    }
}
