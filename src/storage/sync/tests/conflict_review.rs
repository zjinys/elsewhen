use super::capture_wiki;
use super::deliver;
use super::pair;
use super::resolve_kindbinding;
use super::seed_wiki_page;
use super::*;

#[test]
fn wiki_conflict_review_accept_remote_rename_migrates_from_current_slug() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    a.store
        .submit_input_as_event("证据事件文本", "capture", None)
        .unwrap();
    let event_id = a.store.list_events().unwrap()[0].id.clone();
    seed_wiki_page(&a, "page-1", "topic/项目", "项目", "正文", &[&event_id]);
    capture_wiki(&a, "page-1");
    deliver(&a, &b);
    // 双端离线各自改名；b 端建一条引用（本端 slug）。
    a.store.rename_wiki_page("topic/项目", "甲名", "a").unwrap();
    b.store.rename_wiki_page("topic/项目", "乙名", "b").unwrap();
    b.store
        .connection
        .execute(
            "INSERT INTO relations(id,from_slug,from_kind,to_slug,to_kind,relation,confidence,created_at,last_seen_at) \
             VALUES('rel-1','p','person','topic/乙名','topic','跟进',3,'2026-01-01T00:00:00+00:00','2026-01-01T00:00:00+00:00')",
            [],
        )
        .unwrap();
    deliver(&a, &b);
    deliver(&b, &a);
    // b 端审阅列表：rename_diverged，双端状态并列。
    let reviews = b.store.list_wiki_page_conflicts().unwrap();
    assert_eq!(reviews.len(), 1);
    let review = &reviews[0];
    assert_eq!(review.reason, "wiki_page_rename_diverged");
    assert_eq!(review.incoming.slug, "topic/甲名");
    assert_eq!(review.incoming.renamed_from.as_deref(), Some("topic/项目"));
    let local = review.local.as_ref().unwrap();
    assert_eq!(local.slug, "topic/乙名");
    // 绑定失败关闭：updated_at 编造回传必须被拒。
    let err = b
        .store
        .resolve_wiki_page_conflict(&review.op_id, Some("1999-01-01T00:00:00+00:00"), true)
        .unwrap_err()
        .to_string();
    assert!(err.contains("wiki_conflict_local_changed"), "实际: {err}");
    // 采纳远端：b 从**当前** slug 迁到 a 的 slug，引用跟着走。
    b.store
        .resolve_wiki_page_conflict(&review.op_id, Some(&local.updated_at), true)
        .unwrap();
    assert!(b.store.get_wiki_page("topic/乙名").unwrap().is_none());
    let moved = b.store.get_wiki_page("topic/甲名").unwrap().unwrap();
    assert_eq!(moved.title, "甲名");
    let rel_to: String = b
        .store
        .connection
        .query_row("SELECT to_slug FROM relations WHERE id='rel-1'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(rel_to, "topic/甲名");
    assert!(b.store.list_wiki_page_conflicts().unwrap().is_empty());
}

#[test]
fn wiki_conflict_accept_remote_delete_or_keep_local() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    a.store
        .submit_input_as_event("证据事件文本", "capture", None)
        .unwrap();
    let event_id = a.store.list_events().unwrap()[0].id.clone();
    seed_wiki_page(&a, "page-1", "topic/项目", "项目", "正文", &[&event_id]);
    capture_wiki(&a, "page-1");
    deliver(&a, &b);
    // b 捕获删除（head 已删、行仍在），a 离线编辑 → b 出删除候选。
    {
        let tx = b.store.begin_write_transaction().unwrap();
        b.store.sync_record_wiki_page("page-1", true).unwrap();
        if let Some(tx) = tx {
            tx.commit().unwrap();
        }
    }
    a.store
        .save_wiki_page_content("topic/项目", "离线改", "离线编辑", None)
        .unwrap();
    deliver(&a, &b);
    let reviews = b.store.list_wiki_page_conflicts().unwrap();
    assert_eq!(reviews.len(), 1);
    let review = &reviews[0];
    assert_eq!(review.reason, "wiki_page_deleted");
    assert!(review.local_head_deleted);
    // 保留本地 = 维持本端删除态：head 已删 → 行物理删除（与业务删除一致）。
    let updated = review.local.as_ref().unwrap().updated_at.clone();
    b.store
        .resolve_wiki_page_conflict(&review.op_id, Some(&updated), false)
        .unwrap();
    assert!(b.store.list_wiki_page_conflicts().unwrap().is_empty());
    assert!(b.store.get_wiki_page("topic/项目").unwrap().is_none());
    let head = b
        .store
        .sync_object_head("wiki_page", "page-1")
        .unwrap()
        .unwrap();
    assert!(head.1, "保留本地删除态后 head 必须仍是已删");
    // a 再编辑后投递：b 端再次出候选（本端行已物理删，local 为空）。
    a.store
        .save_wiki_page_content("topic/项目", "离线改 v2", "离线编辑", None)
        .unwrap();
    deliver(&a, &b);
    let reviews = b.store.list_wiki_page_conflicts().unwrap();
    assert_eq!(reviews.len(), 1);
    assert!(reviews[0].local.is_none());
    // 这次采纳远端：复活本端行并采纳离线编辑（整行替换语义）。
    b.store
        .resolve_wiki_page_conflict(&reviews[0].op_id, None, true)
        .unwrap();
    let revived = b.store.get_wiki_page("topic/项目").unwrap().unwrap();
    assert_eq!(revived.content_md, "离线改 v2");
    let head = b
        .store
        .sync_object_head("wiki_page", "page-1")
        .unwrap()
        .unwrap();
    assert!(!head.1, "采纳远端编辑后 head 必须回到未删");
    assert!(b.store.list_wiki_page_conflicts().unwrap().is_empty());
}

#[test]
fn object_conflict_review_keep_local_recaptures_and_accept_remote_adopts() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    let todo = a
        .store
        .create_todo("买牛奶", "high", None, None, None, None, false)
        .unwrap();
    deliver(&a, &b);
    // 双端离线编辑 → 各自立案 base_revision_changed。
    a.store
        .update_todo(&todo.id, "a 改的标题", None, Some("high"), None)
        .unwrap();
    b.store
        .update_todo(&todo.id, "b 改的标题", None, Some("high"), None)
        .unwrap();
    deliver(&a, &b);
    deliver(&b, &a);
    assert_eq!(a.store.list_object_conflicts().unwrap().len(), 1);
    let reviews = b.store.list_object_conflicts().unwrap();
    assert_eq!(reviews.len(), 1);
    let review = &reviews[0];
    assert_eq!(review.object_kind, "todo");
    assert_eq!(review.reason, "base_revision_changed");
    assert_eq!(review.incoming.summary, "a 改的标题");
    assert_eq!(review.local_summary.as_deref(), Some("b 改的标题"));

    // 绑定失败关闭：expected 与审阅时不一致必须报错。
    let op_id = review.op_id.clone();
    let err = b
        .store
        .resolve_object_conflict(&op_id, Some("2020-01-01T00:00:00+00:00"), true)
        .unwrap_err();
    assert!(
        format!("{err:#}").contains("object_conflict_local_changed"),
        "绑定不一致必须失败关闭: {err:#}"
    );

    // b 保留本端：重捕广播、清案；对端收到反向候选（对称收敛）。
    resolve_kindbinding(&b, &op_id, review.local_updated_at.as_deref(), false);
    assert_eq!(b.store.list_object_conflicts().unwrap().len(), 0);
    let title_b: String = b
        .store
        .connection
        .query_row("SELECT title FROM todos WHERE id=?1", [&todo.id], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(title_b, "b 改的标题", "保留本端不得覆盖本端现状");
    deliver(&b, &a);
    // a 端原候选未决，b 的保留本端重捕又到达：两条并列，都待人审。
    let reviews_a = a.store.list_object_conflicts().unwrap();
    assert_eq!(reviews_a.len(), 2, "原候选与反向候选并列");
    assert!(reviews_a.iter().all(|r| r.incoming.summary == "b 改的标题"));

    // a 端逐条采纳对端（b 的现状 = 保留本端后的版本）：整行采纳。首条
    // 采纳会改写本端行，次条的审阅绑定随之失效——必须重拉列表取新绑定
    // （失败关闭语义的必然闭环）。
    for _ in 0..2 {
        let Some(review) = a.store.list_object_conflicts().unwrap().last().cloned() else {
            break;
        };
        resolve_kindbinding(&a, &review.op_id, review.local_updated_at.as_deref(), true);
    }
    assert_eq!(a.store.list_object_conflicts().unwrap().len(), 0);
    let title_a: String = a
        .store
        .connection
        .query_row("SELECT title FROM todos WHERE id=?1", [&todo.id], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(title_a, "b 改的标题", "采纳远端整行落地");
}

#[test]
fn todo_deleted_candidate_keep_local_clears_and_accept_remote_revives() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    let todo = a
        .store
        .create_todo("待办", "normal", None, None, None, None, false)
        .unwrap();
    deliver(&a, &b);
    // b 删除（head 停在删除态）；a 离线编辑后送到 → b 立案 todo_deleted
    //（编辑 op 对着已删 head 到达，先于 base CAS 报告删除分歧）。
    b.store.delete_todo(&todo.id).unwrap();
    a.store
        .update_todo(&todo.id, "a 的离线编辑", None, Some("normal"), None)
        .unwrap();
    deliver(&a, &b);
    let reviews = b.store.list_object_conflicts().unwrap();
    assert_eq!(reviews.len(), 1);
    assert_eq!(reviews[0].reason, "todo_deleted");
    assert_eq!(reviews[0].incoming.summary, "a 的离线编辑");
    assert!(reviews[0].local_head_deleted, "本端 head 处于删除态");

    // b 保留本端 = 删除态即裁决：仅清案，不复活、不重捕。
    let op_keepsake = reviews[0].op_id.clone();
    resolve_kindbinding(&b, &op_keepsake, None, false);
    assert_eq!(b.store.list_object_conflicts().unwrap().len(), 0);
    assert_eq!(b.count("todos"), 0, "删除态是本端裁决，不得复活");
}

#[test]
fn goal_active_limit_accept_is_rejected_and_keep_local_clears() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    // b 本地已有 3 个活跃目标（本端触发器允许的满额）。
    b.store
        .create_goal("本端目标一", crate::storage::GoalPhase::Near)
        .unwrap();
    b.store
        .create_goal("本端目标二", crate::storage::GoalPhase::Near)
        .unwrap();
    b.store
        .create_goal("本端目标三", crate::storage::GoalPhase::Near)
        .unwrap();
    // a 建第 4 个活跃目标并送达：b 显式立案 goal_active_limit（远端写入
    // 不得静默顶掉本端目标）。
    a.store
        .create_goal("对端的第 4 个目标", crate::storage::GoalPhase::Near)
        .unwrap();
    deliver(&a, &b);
    let reviews = b.store.list_object_conflicts().unwrap();
    assert_eq!(reviews.len(), 1, "第 4 个活跃目标显式立案");
    assert_eq!(reviews[0].reason, "goal_active_limit");
    assert_eq!(reviews[0].incoming.summary, "对端的第 4 个目标");
    let op = reviews[0].op_id.clone();

    // 采纳被显式拒绝（业务不变式不旁路），候选保持待决。
    let err = b
        .store
        .resolve_object_conflict(&op, reviews[0].local_updated_at.as_deref(), true)
        .unwrap_err();
    assert!(
        format!("{err:#}").contains("goal_active_limit"),
        "采纳必须被拒绝: {err:#}"
    );
    assert_eq!(b.store.list_object_conflicts().unwrap().len(), 1);

    // 保留本端：清案，本端 3 个活跃目标不动，第 4 个不落行。
    resolve_kindbinding(&b, &op, reviews[0].local_updated_at.as_deref(), false);
    assert_eq!(b.store.list_object_conflicts().unwrap().len(), 0);
    assert_eq!(b.count("goals"), 3, "越限目标不得静默落行");
}
