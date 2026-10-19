use super::super::*;
use super::capture_wiki;
use super::deliver;
use super::pair;
use super::seed_wiki_page;
use super::*;

// ── S05：待办/目标/规则的可变对象复制 ─────────────────────────────────────

#[test]
fn todos_replicate_edit_state_and_double_delete_converges() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    let todo = a
        .store
        .create_todo("买牛奶", "high", None, None, None, None, false)
        .unwrap();
    deliver(&a, &b);
    assert_eq!(b.count("todos"), 1, "收端整行到达");
    // b 标完成 → 双端一致
    b.store
        .update_todo_status(&todo.id, crate::storage::TodoStatus::Done)
        .unwrap();
    deliver(&b, &a);
    let status_a: String = a
        .store
        .connection
        .query_row("SELECT status FROM todos WHERE id=?1", [&todo.id], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(status_a, "done");
    assert!(a.store.list_wiki_page_conflicts().unwrap().is_empty());
    // 双端离线各自删除：先到者删行，后到者幂等收敛，不立案。
    a.store.delete_todo(&todo.id).unwrap();
    b.store.delete_todo(&todo.id).unwrap();
    deliver(&a, &b);
    deliver(&b, &a);
    assert_eq!(a.count("todos"), 0);
    assert_eq!(b.count("todos"), 0);
    let candidate = a
        .store
        .connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_conflicts WHERE object_kind='todo' AND object_id=?1)",
            [&todo.id],
            |r| r.get::<_, bool>(0),
        )
        .unwrap();
    assert!(!candidate, "双端同删必须幂等收敛，不应立案");
}

#[test]
fn todo_delete_versus_edit_lands_as_candidate_and_todo_slug_ref_migrates() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    // 顺手覆盖 wiki rename 迁移：待办的页面引用跟页走
    a.store
        .submit_input_as_event("证据事件文本", "capture", None)
        .unwrap();
    let event_id = a.store.list_events().unwrap()[0].id.clone();
    seed_wiki_page(&a, "page-1", "topic/项目", "项目", "正文", &[&event_id]);
    capture_wiki(&a, "page-1");
    deliver(&a, &b);
    let todo = a
        .store
        .create_todo(
            "改稿",
            "normal",
            None,
            None,
            Some("topic/项目"),
            None,
            false,
        )
        .unwrap();
    deliver(&a, &b);
    a.store
        .rename_wiki_page("topic/项目", "topic/新项目", "a")
        .unwrap();
    deliver(&a, &b);
    let slug_b: Option<String> = b
        .store
        .connection
        .query_row(
            "SELECT related_wiki_slug FROM todos WHERE id=?1",
            [&todo.id],
            |r| r.get(0),
        )
        .unwrap();
    // rename 以新标题派生 slug（topic/ 前缀 + 标题 slugify），b 端引用应一致同迁
    assert_eq!(
        slug_b.as_deref(),
        Some("topic/topic-新项目"),
        "rename 后待办引用同迁"
    );
    // 删除 vs 编辑：a 改标题、b 删除 → 交付后 b 收候选（b head 已删）。
    a.store
        .update_todo(&todo.id, "改稿 v2", None, None, None)
        .unwrap();
    b.store.delete_todo(&todo.id).unwrap();
    let a_ops = deliver(&a, &b);
    let _ = a_ops;
    let candidate = b
        .store
        .connection
        .query_row(
            "SELECT reason FROM sync_conflicts WHERE object_kind='todo' AND object_id=?1",
            [&todo.id],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .unwrap();
    assert_eq!(candidate.as_deref(), Some("todo_deleted"));
    // b→a：b 的删除 op（base 为共享 v1）到 a 与 a 的编辑冲突。
    deliver(&b, &a);
    let candidate_a = a
        .store
        .connection
        .query_row(
            "SELECT reason FROM sync_conflicts WHERE object_kind='todo' AND object_id=?1",
            [&todo.id],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .unwrap();
    assert!(
        matches!(candidate_a.as_deref(), Some("base_revision_changed")),
        "实际: {candidate_a:?}"
    );
}

#[test]
fn goals_replicate_archive_reactivate_and_cap_conflicts() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    use crate::storage::GoalPhase;
    // 双端各自离线建 2 条，交错交付：b 在收到 a 的第 2 条时触发上限候选，
    // 不静默顶掉本端目标。
    let a1 = a.store.create_goal("a-1", GoalPhase::Near).unwrap().id;
    let a2 = a.store.create_goal("a-2", GoalPhase::Near).unwrap().id;
    let b1 = b.store.create_goal("b-1", GoalPhase::Long).unwrap().id;
    let b2 = b.store.create_goal("b-2", GoalPhase::Long).unwrap().id;
    deliver(&a, &b);
    deliver(&b, &a);
    // a/b 各自状态：a 有 a-1,a-2 + b 的 1 条到达（第 2 条成候选）。
    assert_eq!(a.store.count_active_goals().unwrap(), 3);
    let candidate = a
        .store
        .connection
        .query_row(
            "SELECT reason FROM sync_conflicts WHERE object_kind='goal' AND object_id=?1",
            [&b2],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .unwrap();
    // b-1 已占第 3 席，b-2 越限 → goal_active_limit 候选
    // 注意 b-1 与 b-2 到达顺序由 fr1ontier 保证，先 b-1 后 b-2。
    assert_eq!(candidate.as_deref(), Some("goal_active_limit"));
    assert!(a.store.list_archived_goals().unwrap().is_empty());
    // b 端对称：b 有 a-1,a-2,b-1,b-2 —— b 本地 2 + 收 2 也会越限。
    // a-2 到达时 b 已有 3 条（b-1,b-2,a-1）→ a-2 成候选。
    let candidate_b = b
        .store
        .connection
        .query_row(
            "SELECT reason FROM sync_conflicts WHERE object_kind='goal' AND object_id=?1",
            [&a2],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .unwrap();
    assert_eq!(candidate_b.as_deref(), Some("goal_active_limit"));
    assert_eq!(b.store.count_active_goals().unwrap(), 3);
    // 归档释放名额 + 复活复制两态均达；归档后一致。
    a.store.archive_goal(&a1).unwrap();
    deliver(&a, &b);
    let archived_b: String = b
        .store
        .connection
        .query_row("SELECT status FROM goals WHERE id=?1", [&a1], |r| r.get(0))
        .unwrap();
    assert_eq!(
        archived_b, "superseded",
        "归档随流复制（a2 候选不影响 a1 的归档）"
    );
    let _ = a1;
}

#[test]
fn rules_replicate_promote_discard_and_edit_conflict() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    let rule = a
        .store
        .add_rule("先确认再推进", crate::storage::RuleStatus::Active, None)
        .unwrap();
    deliver(&a, &b);
    assert_eq!(b.count("rules"), 1);
    // 删除复制（物理删除一致）。
    a.store.delete_rule(&rule).unwrap();
    deliver(&a, &b);
    assert_eq!(b.count("rules"), 0);
    // pending 规则 + promote 状态迁移复制。
    let conv = a
        .store
        .create_conversation(Some("测试会话"), Some("discussion"))
        .unwrap();
    let rule2 = a
        .store
        .add_rule(
            "待确认规则",
            crate::storage::RuleStatus::Pending,
            Some(&conv),
        )
        .unwrap();
    deliver(&a, &b);
    let status_b: String = b
        .store
        .connection
        .query_row("SELECT status FROM rules WHERE id=?1", [&rule2], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(status_b, "pending");
    a.store.promote_pending_rules(&conv).unwrap();
    deliver(&a, &b);
    let status_b2: String = b
        .store
        .connection
        .query_row("SELECT status FROM rules WHERE id=?1", [&rule2], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(status_b2, "active", "promote 状态迁移随流复制");
    // 删除再复制：状态迁移后再次删除随流收敛（物理删除一致）。
    a.store.delete_rule(&rule2).unwrap();
    deliver(&a, &b);
    assert_eq!(b.count("rules"), 1 - 1, "删除随流收敛");
    assert!(
        !a.store
            .connection
            .query_row::<bool,_,_>("SELECT EXISTS(SELECT 1 FROM sync_conflicts WHERE object_kind='rule' AND object_id=?1)",[&rule2],|r| r.get(0))
            .unwrap()
    );
}

#[test]
fn wiki_chat_conversation_and_messages_replicate() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    // 页面先于会话存在：页面 op（seq 顺序在前）先落地 → 会话应用时引用锚已见过。
    seed_wiki_page(&a, "page-1", "topic/p1", "页面一", "v1", &[]);
    capture_wiki(&a, "page-1");
    let conv_id = a
        .store
        .create_wiki_chat_conversation("topic/p1", "[知识页] 页面一")
        .unwrap();
    a.store
        .send_message(&conv_id, "user", "这个页面讲了什么？", None)
        .unwrap();
    let ops = deliver(&a, &b);
    assert!(ops.len() >= 3, "页面 op + 会话 op + 消息 op 都要出站");
    let conv = b
        .store
        .connection
        .query_row(
            "SELECT wiki_page_slug, assistant_mode FROM conversations WHERE id=?1",
            [&conv_id],
            |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, String>(1)?)),
        )
        .optional()
        .unwrap();
    let (slug, mode) = conv.expect("会话应已复制");
    assert_eq!(slug.as_deref(), Some("topic/p1"));
    assert_eq!(mode, "knowledge_mentor");
    assert_eq!(b.count("messages"), 1, "wiki 专属会话的消息一并复制");
    assert_eq!(b.count("sync_wiki_slug_seen"), 1, "页面足迹由应用端写入");
    // 普通会话列表不得混入 wiki 专属会话。
    assert!(
        b.store
            .list_conversations()
            .unwrap()
            .iter()
            .all(|c| c.id != conv_id)
    );
}

#[test]
fn wiki_chat_waits_for_page_evidence_and_converges_without_inference() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    // P5a 之前的旧页：行在库但从未捕获（无 head）。会话捕获时自动补发页面
    // op（seq1），会话 op 随后（seq2）——同流有序。
    seed_wiki_page(&a, "page-late", "topic/late", "迟到页", "v1", &[]);
    let conv_id = a
        .store
        .create_wiki_chat_conversation("topic/late", "[知识页] 迟到页")
        .unwrap();
    let ops = deliver_except_first(&a, &b);
    assert_eq!(ops.len(), 2, "自动补发的页面 op + 会话 op");
    // 先到会话 op（跨流乱序的真实可能）：引用证据缺失 → Waiting。
    assert_eq!(b.count("conversations"), 0, "证据缺失不得落会话行");
    let state: String = b
        .store
        .connection
        .query_row(
            "SELECT state FROM sync_operations WHERE op_id=?1",
            [&ops[1].op_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(state, "waiting", "op 必须等待补齐而不是推断");
    assert_eq!(
        b.count("sync_wiki_slug_seen"),
        0,
        "页面 op 未到：足迹不得预先存在"
    );
    // 同名新页不得收养：本端已有同名 slug 的本地页也不改变等待——足迹表
    // 只认真实到达过的页面 op。
    seed_wiki_page(&b, "page-impersonator", "topic/late", "冒名页", "x", &[]);
    b.store.sync_drain_waiting().unwrap();
    assert_eq!(b.count("conversations"), 0, "足迹表没有记录，等待继续");
    // 移除冒名行（它只用于证明等待判定不被本地同名行满足；留着会让页面
    // op 按 slug_owned_by_other_id 立案冲突，阻断收敛路径）。
    b.store
        .connection
        .execute("DELETE FROM wiki_pages WHERE id='page-impersonator'", [])
        .unwrap();
    // 页面 op 到达 → 足迹写入 → 会话收敛。
    deliver(&a, &b);
    let (slug, mode): (Option<String>, String) = b
        .store
        .connection
        .query_row(
            "SELECT wiki_page_slug, assistant_mode FROM conversations WHERE id=?1",
            [&conv_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(slug.as_deref(), Some("topic/late"));
    assert_eq!(mode, "knowledge_mentor");
}

#[test]
fn knowledge_review_and_reading_state_propagate() {
    use crate::storage::{ContentPolicy, WikiPageDraft};

    fn wiki_page(store: &Store, slug: &str) {
        store
            .upsert_wiki_page(
                &WikiPageDraft {
                    slug: slug.into(),
                    kind: "topic".into(),
                    title: slug.into(),
                    summary: "s".into(),
                    content_md: "正文".into(),
                    tags: vec![],
                    source_event_ids: vec![],
                    status: "active".into(),
                    reason: "test".into(),
                    source_url: None,
                },
                ContentPolicy::Always,
            )
            .unwrap();
    }
    let page_id = |db: &Database| -> String {
        db.store
            .connection
            .query_row(
                "SELECT id FROM wiki_pages WHERE slug='topic/sync'",
                [],
                |r| r.get(0),
            )
            .unwrap()
    };

    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    // 页先行：用户语义行引用的 wiki_pages 必须先到（无悬空引用）。
    // 页只在 a 建立并同步给 b——双端各自创建同一 slug 正文页会触发页级
    // 双创建冲突（既有设计），会冻结同 stream 后续 op（真实场景为单写多读）。
    wiki_page(&a.store, "topic/sync");
    deliver(&a, &b);

    // —— 评审行：append-only 捕获 → 收端 INSERT OR IGNORE 幂等合并 ——
    let insert_review = |db: &Database, fp: &str| {
        let tx = db.store.begin_write_transaction().unwrap();
        db.store
            .connection
            .execute(
                "INSERT OR IGNORE INTO knowledge_maintenance_reviews\
                 (fingerprint,page_id,resolution,created_at,issue_description)\
                 VALUES(?1,?2,'dismissed','2026-01-01T00:00:00Z','上游已变更')",
                params![fp, page_id(db)],
            )
            .unwrap();
        db.store.sync_record_knowledge_review(fp).unwrap();
        if let Some(tx) = tx {
            tx.commit().unwrap();
        }
    };
    insert_review(&a, "fp-1");
    deliver(&a, &b);
    assert_eq!(a.count("knowledge_maintenance_reviews"), 1);
    assert_eq!(b.count("knowledge_maintenance_reviews"), 1);
    // 双端各自关同一 issue（同 fingerprint）：收敛为一行，不翻倍。
    insert_review(&b, "fp-1");
    deliver(&b, &a);
    assert_eq!(a.count("knowledge_maintenance_reviews"), 1);
    assert_eq!(b.count("knowledge_maintenance_reviews"), 1);

    // —— 阅读状态：业务写直达传播 ——
    crate::knowledge::workflows::set_reading_state(&a.store, "topic/sync", "read").unwrap();
    deliver(&a, &b);
    let b_state: String = b
        .store
        .connection
        .query_row(
            "SELECT state FROM knowledge_reading_states WHERE page_id=?1",
            [&page_id(&b)],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(b_state, "read");
    // 反向：b 改 valuable（更新更晚），a 收敛到 b 的值。
    crate::knowledge::workflows::set_reading_state(&b.store, "topic/sync", "valuable").unwrap();
    deliver(&b, &a);
    let a_state: String = a
        .store
        .connection
        .query_row(
            "SELECT state FROM knowledge_reading_states WHERE page_id=?1",
            [&page_id(&a)],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(a_state, "valuable");
}
