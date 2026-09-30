#[test]
fn fulltext_reading_covers_tail_resumes_and_rejects_invented_quotes() {
    let db = Db::new();
    let s = &db.store;
    let text = format!("{}尾部限制：仅限离线备份时使用。", "前段介绍。".repeat(1400));
    let original = source(s, "https://example.com/long", &text);
    let snapshot = s.source_history(&original.slug).unwrap().remove(0);
    let first = Stub(r#"{"summary":"前半篇介绍备份步骤","quotes":[]}"#.into());
    reading::advance(s, &first).unwrap();
    assert!(!reading::ready(s, &snapshot).unwrap());
    // A reopened connection reuses the committed first chunk.
    let reopened = Store::open(&db.path).unwrap();
    struct TailReader;
    impl AiProvider for TailReader {
        fn generate_reply_with_tools(&self, context: Vec<ContextMessage>, _: Option<&[ToolSpec]>) -> Result<AiReply> {
            assert!(context.last().unwrap().content.contains("仅限离线备份时使用"));
            Ok(AiReply::text(r#"{"summary":"仅限离线备份时使用，在线状态不适用。","quotes":["仅限离线备份时使用。"]}"#))
        }
    }
    reading::advance(&reopened, &TailReader).unwrap();
    reading::advance(&reopened, &TailReader).unwrap();
    assert!(reading::ready(s, &snapshot).unwrap());
    let (summary, quotes) = reading::context(s, &snapshot).unwrap();
    assert!(summary.contains("仅限离线"));
    assert_eq!(quotes, ["仅限离线备份时使用。"]);
    let coverage: (i64,i64,i64) = s.connection.query_row("SELECT MIN(start_char),MAX(end_char),SUM(end_char-start_char) FROM knowledge_source_readings WHERE snapshot_id=?1 AND level=0",[&snapshot.id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
    assert_eq!(coverage, (0,text.chars().count() as i64,text.chars().count() as i64));
    reading::advance(s, &Replies(std::cell::RefCell::new(vec![]))).unwrap();
    let changed = source(s, "https://example.com/long", &format!("{text}新版本"));
    let latest = s.source_history(&changed.slug).unwrap().remove(0);
    reading::advance(s, &Stub(r#"{"summary":"归纳","quotes":["伪造的不存在原文摘录"]}"#.into())).unwrap();
    assert!(!reading::ready(s, &latest).unwrap());
    let nodes: i64=s.connection.query_row("SELECT COUNT(*) FROM knowledge_source_readings WHERE snapshot_id=?1",[latest.id],|r|r.get(0)).unwrap();
    assert_eq!(nodes,0);
    reading::advance(s, &Replies(std::cell::RefCell::new(vec![]))).unwrap();
}

#[test]
fn refresh_queue_covers_every_dependent_and_preserves_protected_mixed_evidence() {
    let db = Db::new();
    let s = &db.store;
    let original=source(s,"https://example.com/dependents","原料版本一");
    let snapshot=s.source_history(&original.slug).unwrap().remove(0);
    let event=s.insert_event(NewEvent::now("真实的事务观察")).unwrap();
    for n in 0..5 {
        let draft=WikiPageDraft {slug:format!("topic/refresh-{n}"),kind:if n==4 {"method"}else{"topic"}.into(),title:format!("主题 {n}"),summary:"旧".into(),content_md:"旧知识".into(),tags:vec![],source_event_ids:vec![event.clone()],status:"active".into(),reason:"test".into(),source_url:None};
        let page=s.upsert_wiki_page(&draft,ContentPolicy::Always).unwrap().page;
        s.bind_page_sources(&page.id,&[snapshot.id.clone()]).unwrap();
        if n==4 {s.set_knowledge_metadata(&page.slug,"人工条件","rule").unwrap();}
    }
    source(s,"https://example.com/dependents","原料版本二");
    for _ in 0..5 {assert_eq!(crate::knowledge_background::run_knowledge_refresh(s,&compile_stub()).unwrap(),1);}
    assert_eq!(crate::knowledge_background::run_knowledge_refresh(s,&Replies(std::cell::RefCell::new(vec![]))).unwrap(),0);
    let done:i64=s.connection.query_row("SELECT COUNT(*) FROM knowledge_refresh_jobs WHERE status='succeeded'",[],|r|r.get(0)).unwrap();
    assert_eq!(done,5);
    for n in 0..4 {
        let slug=format!("topic/refresh-{n}");
        assert_eq!(s.page_source_snapshots(&slug).unwrap()[0].version,2);
        assert_eq!(s.get_wiki_page(&slug).unwrap().unwrap().source_event_ids,[event.clone()]);
    }
    assert_eq!(s.get_wiki_page("topic/refresh-4").unwrap().unwrap().content_md,"旧知识");
    let pending=s.knowledge_proposals(Some("topic/refresh-4")).unwrap().into_iter().find(|p|p.status=="pending").unwrap();
    s.resolve_knowledge_proposal(&pending.id,true).unwrap();
    assert_eq!(s.knowledge_metadata("topic/refresh-4").unwrap().strength,"rule");
    assert_eq!(s.page_source_snapshots("topic/refresh-4").unwrap()[0].version,2);
}

#[test]
fn strategy_upgrade_revisits_successful_snapshot() {
    let db=Db::new(); let s=&db.store;
    let original=source(s,"https://example.com/strategy","原料");
    let id=s.source_history(&original.slug).unwrap()[0].id.clone();
    s.connection.execute("INSERT INTO knowledge_background_runs(id,task,input_key,status,started_at,finished_at) VALUES('legacy','source-compilation',?1,'succeeded','2020-01-01','2020-01-01')",[id]).unwrap();
    assert_eq!(crate::knowledge_background::run_automatic_source_compilation(s,&auto_stub()).unwrap(),1);
    assert_eq!(crate::knowledge_background::run_automatic_source_compilation(s,&Replies(std::cell::RefCell::new(vec![]))).unwrap(),0);
}

#[test]
fn retrieval_matches_tail_synonyms_and_ranks_before_recency_limit() {
    let db=Db::new();let s=&db.store;
    let event=s.insert_event(NewEvent::now("飞船试验观察")).unwrap();
    let draft=WikiPageDraft {slug:"topic/old-relevant".into(),kind:"topic".into(),title:"火星轨道燃料".into(),summary:"".into(),content_md:format!("{}\n\n火星轨道燃料计算要考虑变轨时间。明确受众之后再安排发布。", "开场介绍。".repeat(1000)),tags:vec![],source_event_ids:vec![event.clone()],status:"active".into(),reason:"test".into(),source_url:None};
    s.upsert_wiki_page(&draft,ContentPolicy::Always).unwrap();
    for n in 0..90 {let mut d=draft.clone();d.slug=format!("topic/noise-{n}");d.title="轨道日记".into();d.content_md="轨道普通片段".into();s.upsert_wiki_page(&d,ContentPolicy::Always).unwrap();}
    let hits=select_knowledge(s,"火星轨道燃料","test",5000).unwrap();
    assert_eq!(hits[0].page_slug,draft.slug);assert!(hits[0].excerpt.contains("变轨时间"));
    assert!(serde_json::to_string(&hits).unwrap().chars().count()<=5000);
    let synonyms=select_knowledge(s,"目标用户","test",5000).unwrap();
    assert!(synonyms.iter().any(|c|c.page_slug==draft.slug && c.excerpt.contains("明确受众")));
    s.connection.execute("UPDATE wiki_pages SET opinion='reject' WHERE slug=?1",[&draft.slug]).unwrap();
    assert!(select_knowledge(s,"目标用户","test",5000).unwrap().is_empty());
}

#[test]
fn review_applies_selected_paragraphs_links_issue_and_prepares_restoration() {
    let db=Db::new();let s=&db.store;
    let original=source(s,"https://example.com/review","原文");
    let id=propose_knowledge(s,&original.slug,"method",&compile_stub()).unwrap();
    let page=s.resolve_knowledge_proposal(&id,true).unwrap().unwrap();
    let original_revision:String=s.connection.query_row("SELECT id FROM wiki_revisions WHERE page_id=?1 ORDER BY rowid LIMIT 1",[&page.id],|r|r.get(0)).unwrap();
    s.save_wiki_page_content(&page.slug,"共同段落\n\n旧步骤\n\n分隔段落\n\n旧边界 [[missing]]","人工",None).unwrap();
    s.set_knowledge_metadata(&page.slug,"我确认的条件","rule").unwrap();
    let before=s.get_wiki_page(&page.slug).unwrap().unwrap();
    let draft=WikiPageDraft {slug:page.slug.clone(),kind:"method".into(),title:page.title.clone(),summary:"".into(),content_md:"共同段落\n\n新步骤\n\n分隔段落\n\n新边界".into(),tags:vec![],source_event_ids:vec![],status:"active".into(),reason:"review".into(),source_url:None};
    let snaps:Vec<_>=s.page_source_snapshots(&page.slug).unwrap().into_iter().map(|s|s.id).collect();
    let id=s.record_proposal(&draft,"建议条件",&snaps,Some(&before),"review").unwrap();
    let parts=review::proposal_diff(s,&id).unwrap();
    let boundary=parts.iter().position(|p|p.after=="新边界").unwrap() as i64;
    let issue=s.knowledge_issues(Some(&page.slug)).unwrap().into_iter().find(|i|i.kind=="broken_link").unwrap();
    let accepted=review::accept_parts(s,&id,&[boundary],false,&[issue.fingerprint.clone()]).unwrap();
    assert!(accepted.content_md.contains("旧步骤"));assert!(accepted.content_md.contains("新边界"));
    assert_eq!(s.knowledge_metadata(&page.slug).unwrap().strength,"rule");
    assert_eq!(s.knowledge_metadata(&page.slug).unwrap().applicable_when,"我确认的条件");
    let resolution:(String,String)=s.connection.query_row("SELECT resolution,revision_id FROM knowledge_maintenance_reviews WHERE fingerprint=?1",[issue.fingerprint],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
    assert_eq!(resolution.0,"resolved");
    assert!(review::accept_parts(s,&id,&[boundary],false,&[]).is_err());
    let restore=review::restore_proposal(s,&page.slug,&original_revision).unwrap();
    assert_eq!(s.get_wiki_page(&page.slug).unwrap().unwrap().content_md,accepted.content_md);
    s.resolve_knowledge_proposal(&restore,true).unwrap();
    assert_eq!(s.get_wiki_page(&page.slug).unwrap().unwrap().content_md,page.content_md);
    assert_eq!(s.knowledge_metadata(&page.slug).unwrap().strength,"rule");
    // A rejected/stale revision never overwrites later edits.
    let base=s.get_wiki_page(&page.slug).unwrap().unwrap();
    let pending=s.record_proposal(&draft,"新的条件",&snaps,Some(&base),"review").unwrap();
    s.set_knowledge_metadata(&page.slug,"并发条件","method").unwrap();
    assert!(review::accept_parts(s,&pending,&[0],true,&[]).is_err());
}

#[test]
fn topic_merge_and_split_preserve_all_evidence_and_block_concurrent_edits() {
    let db=Db::new();let s=&db.store;
    let mut slugs=vec![];let mut snapshots=vec![];
    for n in 0..2 {
        let original=source(s,&format!("https://example.com/organize/{n}"),&format!("原文 {n}"));
        let snapshot=s.source_history(&original.slug).unwrap()[0].id.clone();snapshots.push(snapshot.clone());
        let draft=WikiPageDraft{slug:format!("topic/input-{n}"),kind:"topic".into(),title:format!("主题 {n}"),summary:"".into(),content_md:format!("知识正文 {n}"),tags:vec![],source_event_ids:vec![],status:"active".into(),reason:"test".into(),source_url:None};
        let page=s.upsert_wiki_page(&draft,ContentPolicy::Always).unwrap().page;s.bind_page_sources(&page.id,&[snapshot]).unwrap();slugs.push(page.slug);
    }
    let merged=serde_json::json!([{"title":"合并后的事务主题","content_md":"两份知识的有效内容。","applicable_when":"多资料事务","snapshot_ids":snapshots,"event_ids":[]}]);
    let first=organization::prepare(s,&slugs,"merge",&Stub(merged.to_string())).unwrap();
    s.save_wiki_page_content(&slugs[0],"并发人工修改","manual",None).unwrap();
    assert!(organization::resolve(s,&first,true).is_err());
    assert_eq!(s.get_wiki_page(&slugs[1]).unwrap().unwrap().status,"active");
    organization::resolve(s,&first,false).unwrap();
    let id=organization::prepare(s,&slugs,"merge",&Stub(merged.to_string())).unwrap();
    let outputs=organization::resolve(s,&id,true).unwrap();assert_eq!(outputs.len(),1);
    assert_eq!(s.page_source_snapshots(&outputs[0]).unwrap().len(),2);
    assert_eq!(s.get_wiki_page(&slugs[0]).unwrap().unwrap().status,"archived");
    assert!(s.knowledge_output_pages(&slugs[0]).unwrap().iter().any(|p|p.slug==outputs[0]));
    assert_eq!(s.knowledge_origin_pages(&outputs[0]).unwrap().len(),4); // 2 originals + 2 archived topics
    let split=serde_json::json!([
        {"title":"拆分甲","content_md":"第一份内容","applicable_when":"甲","snapshot_ids":[snapshots[0]],"event_ids":[]},
        {"title":"拆分乙","content_md":"第二份内容","applicable_when":"乙","snapshot_ids":[snapshots[1]],"event_ids":[]}
    ]);
    let id=organization::prepare(s,&outputs,"split",&Stub(split.to_string())).unwrap();
    let result=organization::resolve(s,&id,true).unwrap();assert_eq!(result.len(),2);
    assert!(result.iter().all(|slug|s.page_source_snapshots(slug).unwrap().len()==1));
    let mut bad=split;bad[1]["snapshot_ids"]=serde_json::json!(["invented"]);
    assert!(organization::prepare(s,&[result[0].clone()],"split",&Stub(bad.to_string())).is_err());
}

#[test]
#[ignore = "requires an explicitly prepared disposable database copy"]
fn real_data_copy_upgrade_preserves_raw_evidence() {
    let path=std::path::PathBuf::from(std::env::var("ELSEWHEN_WIKI_UPGRADE_COPY").expect("provide a disposable copy"));
    let path=path.canonicalize().unwrap();
    assert!(path.starts_with(std::env::temp_dir().canonicalize().unwrap()),"only disposable copies under the temporary directory are allowed");
    fn evidence(conn:&rusqlite::Connection)->(Vec<(String,String)>,Vec<(String,String)>) {
        let events=conn.prepare("SELECT id,raw_text FROM events ORDER BY id").unwrap().query_map([],|r|Ok((r.get(0)?,r.get(1)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
        let snapshots=conn.prepare("SELECT id,content_md FROM knowledge_snapshots ORDER BY id").unwrap().query_map([],|r|Ok((r.get(0)?,r.get(1)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
        (events,snapshots)
    }
    let before=rusqlite::Connection::open_with_flags(&path,rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let baseline=evidence(&before);drop(before);
    let store=Store::open(&path).unwrap();
    let version:i64=store.connection.query_row("PRAGMA user_version",[],|r|r.get(0)).unwrap();assert_eq!(version,8);
    assert_eq!(evidence(&store.connection),baseline);
    assert_eq!(store.connection.query_row("PRAGMA integrity_check",[],|r|r.get::<_,String>(0)).unwrap(),"ok");
    for p in store.list_wiki_pages(None,None).unwrap() {
        store.knowledge_issues(Some(&p.slug)).unwrap();
        store.knowledge_origin_pages(&p.slug).unwrap();
        store.knowledge_output_pages(&p.slug).unwrap();
    }
    eprintln!("upgraded copy: {} immutable events, {} immutable snapshots retained",baseline.0.len(),baseline.1.len());
}
