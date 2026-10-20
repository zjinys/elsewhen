use super::capture_wiki;
use super::deliver;
use super::pair;
use super::seed_wiki_page;
use super::*;

#[test]
fn wiki_page_edits_replicate_and_stop_at_revision_cas() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    seed_wiki_page(&a, "page-1", "topic/p1", "页面一", "v1", &[]);
    capture_wiki(&a, "page-1");
    deliver(&a, &b);
    assert_eq!(
        b.store
            .get_wiki_page("topic/p1")
            .unwrap()
            .unwrap()
            .content_md,
        "v1"
    );
    // 双端离线并发编辑：两端从同一 base 出发的修改都到达对端时，
    // 各自至少落一条冲突候选，且没有任何一端被时间更新顺序覆盖丢失。
    a.store
        .save_wiki_page_content("topic/p1", "edit-from-a", "a 改", None)
        .unwrap();
    b.store
        .save_wiki_page_content("topic/p1", "edit-from-b", "b 改", None)
        .unwrap();
    deliver(&b, &a);
    deliver(&a, &b);
    assert!(a.count("sync_conflicts") >= 1, "A 端必须保留冲突候选");
    assert!(b.count("sync_conflicts") >= 1, "B 端必须保留冲突候选");
    let a_page = a.store.get_wiki_page("topic/p1").unwrap().unwrap();
    assert_eq!(a_page.content_md, "edit-from-a", "冲突版本不得被静默覆盖");
    let b_page = b.store.get_wiki_page("topic/p1").unwrap().unwrap();
    assert_eq!(b_page.content_md, "edit-from-b", "冲突版本不得被静默覆盖");
}

#[test]
fn wiki_page_same_slug_from_two_ids_is_an_explicit_conflict_not_a_remap() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    // 两个各自的 device stream 上各自建页：slug 撞车（同名不同 id）。
    seed_wiki_page(&a, "page-a", "topic/dup", "同名页 A", "from-a", &[]);
    seed_wiki_page(&b, "page-b", "topic/dup", "同名页 B", "from-b", &[]);
    capture_wiki(&a, "page-a");
    deliver(&a, &b);
    assert_eq!(b.count("sync_conflicts"), 1, "slug 撞车必须显式冲突");
    let winner = b.store.get_wiki_page("topic/dup").unwrap().unwrap();
    assert_eq!(winner.id, "page-b", "本地页引用不得被静默重挂到对端 id");
    let conflict: String = b
        .store
        .connection
        .query_row("SELECT reason FROM sync_conflicts LIMIT 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(conflict, "slug_owned_by_other_id");
}

#[test]
fn wiki_page_evidence_waits_for_missing_event_then_applies() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    // 先在 A 捕获一个事件，再给出一页把它当证据引用；但只把「页」送到 B。
    a.store
        .submit_input_as_event("证据事件文本", "capture", None)
        .unwrap();
    let event_id = a.store.list_events().unwrap()[0].id.clone();
    seed_wiki_page(&a, "page-ev", "topic/ev", "证据页", "正文", &[&event_id]);
    capture_wiki(&a, "page-ev");
    let ai = a.store.sync_identity().unwrap().unwrap();
    let bi = b.store.sync_identity().unwrap().unwrap();
    let ops = a.store.pending_sync_operations(&bi.device_id, 100).unwrap();
    // 只投递引用页面的 op（跳过事件 op），证据未到：等待，不落库、不推断。
    let wiki_idx = ops
        .iter()
        .position(|op| {
            matches!(
                op.mutation,
                crate::sync::protocol::SyncMutation::WikiPageChanged { .. }
            )
        })
        .unwrap();
    let report = b
        .store
        .receive_sync_operation(&ai.device_id, &serde_json::to_vec(&ops[wiki_idx]).unwrap())
        .unwrap();
    assert_eq!(report.waiting, 1, "证据缺失必须等待");
    assert_eq!(b.count("wiki_pages"), 0);
    // 事件随后补齐，容忍等待态重投：页成功落地且证据继续存在。
    for op in &ops {
        b.store
            .receive_sync_operation(&ai.device_id, &serde_json::to_vec(op).unwrap())
            .unwrap();
    }
    let page = b.store.get_wiki_page("topic/ev").unwrap().unwrap();
    assert_eq!(page.content_md, "正文");
    assert_eq!(b.count("events"), 1);
}

#[test]
fn wiki_page_edit_after_delete_keeps_candidate() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    seed_wiki_page(&b, "page-1", "topic/gone", "被删页", "旧正文", &[]);
    capture_wiki(&b, "page-1");
    deliver(&b, &a);
    // A 端离线编辑，B 端删除；A 的编辑到达已删除的页面时保留候选。
    a.store
        .save_wiki_page_content("topic/gone", "offline edit", "离线改", None)
        .unwrap();
    {
        let tx = b.store.begin_write_transaction().unwrap();
        b.store.sync_record_wiki_page("page-1", true).unwrap();
        if let Some(tx) = tx {
            tx.commit().unwrap();
        }
    }
    deliver(&a, &b);
    assert_eq!(b.count("sync_conflicts"), 1);
    let conflict: String = b
        .store
        .connection
        .query_row("SELECT reason FROM sync_conflicts LIMIT 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(conflict, "wiki_page_deleted");
    // 冲突版本不落库：b 的本地行保持原内容。物理删除只来自本端的业务删除
    // API（delete_wiki_page）；capture(deleted=true) 本身不改写本地行。
    let row = b.store.get_wiki_page("topic/gone").unwrap().unwrap();
    assert_eq!(row.content_md, "旧正文");
    let deleted: bool = b
        .store
        .connection
        .query_row("SELECT deleted FROM sync_object_heads WHERE object_kind='wiki_page' AND object_id='page-1'", [], |r| r.get(0))
        .unwrap();
    assert!(deleted, "删除捕获必须推进 head");
}

#[test]
fn wiki_page_business_delete_removes_row_and_replicates() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    a.store
        .submit_input_as_event("证据事件文本", "capture", None)
        .unwrap();
    let event_id = a.store.list_events().unwrap()[0].id.clone();
    seed_wiki_page(&a, "page-1", "topic/gone", "要走页", "正文", &[&event_id]);
    // 一条修订记录，验证级联清理。
    a.store
        .save_wiki_page_content("topic/gone", "正文 v2", "二次编辑", None)
        .unwrap();
    capture_wiki(&a, "page-1");
    deliver(&a, &b);
    assert!(b.store.get_wiki_page("topic/gone").unwrap().is_some());
    // 业务删除：本端行与修订级联消失，head 推进为已删。
    assert!(a.store.delete_wiki_page("topic/gone").unwrap());
    assert!(a.store.get_wiki_page("topic/gone").unwrap().is_none());
    let revisions: i64 = a
        .store
        .connection
        .query_row(
            "SELECT COUNT(*) FROM wiki_revisions WHERE page_id='page-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(revisions, 0, "修订必须随页面级联删除");
    assert!(
        a.store
            .sync_object_head("wiki_page", "page-1")
            .unwrap()
            .is_some()
    );
    // 幂等：不存在再删返回 false。
    assert!(!a.store.delete_wiki_page("topic/gone").unwrap());
    // 复制：B 端同样物理删除（删除状态是全端一致的，不是仅本端标记）。
    deliver(&a, &b);
    assert!(b.store.get_wiki_page("topic/gone").unwrap().is_none());
    assert!(
        b.store
            .sync_object_head("wiki_page", "page-1")
            .unwrap()
            .is_some()
    );
}

#[test]
fn wiki_page_rename_replicates_and_migrates_references() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    a.store
        .submit_input_as_event("证据事件文本", "capture", None)
        .unwrap();
    let event_id = a.store.list_events().unwrap()[0].id.clone();
    seed_wiki_page(
        &a,
        "page-1",
        "topic/网站改版",
        "网站改版",
        "正文",
        &[&event_id],
    );
    capture_wiki(&a, "page-1");
    deliver(&a, &b);
    // b 端建一条指向该页的关系与一个页内会话（rename 应用端要迁移它们）。
    b.store
        .connection
        .execute(
            "INSERT INTO relations(id,from_slug,from_kind,to_slug,to_kind,relation,confidence,created_at,last_seen_at) \
             VALUES('rel-1','person/张三','person','topic/网站改版','topic','跟进',3,'2026-01-01T00:00:00+00:00','2026-01-01T00:00:00+00:00')",
            [],
        )
        .unwrap();
    b.store
        .create_wiki_chat_conversation("topic/网站改版", "[知识页] 网站改版")
        .unwrap();
    // a 端业务重命名（捕获在 rename_wiki_page 内部）。
    let outcome = a
        .store
        .rename_wiki_page("topic/网站改版", "测试项目 尾款", "更正项目名")
        .unwrap();
    assert!(outcome.changed);
    assert_eq!(outcome.new_slug, "topic/测试项目-尾款");
    assert!(a.store.get_wiki_page("topic/网站改版").unwrap().is_none());
    deliver(&a, &b);
    // b 端：页换 slug，关系与会话同步迁移，旧 slug 释放。
    assert!(b.store.get_wiki_page("topic/网站改版").unwrap().is_none());
    let renamed = b
        .store
        .get_wiki_page("topic/测试项目-尾款")
        .unwrap()
        .unwrap();
    assert_eq!(renamed.title, "测试项目 尾款");
    let rel_to: String = b
        .store
        .connection
        .query_row(
            "SELECT to_slug FROM relations WHERE from_slug='person/张三'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(rel_to, "topic/测试项目-尾款");
    let chats: i64 = b
        .store
        .connection
        .query_row(
            "SELECT COUNT(*) FROM conversations WHERE wiki_page_slug='topic/测试项目-尾款'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(chats, 1);
    // 幂等重放：重复投递不产生新效果。
    deliver(&a, &b);
    assert!(
        b.store
            .get_wiki_page("topic/测试项目-尾款")
            .unwrap()
            .is_some()
    );
}

#[test]
fn offline_renames_on_both_sides_conflict_explicitly() {
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
    // 双端离线各自改名（不同标题/不同 slug）。
    a.store.rename_wiki_page("topic/项目", "甲名", "a").unwrap();
    b.store.rename_wiki_page("topic/项目", "乙名", "b").unwrap();
    // 交叉投递：两端各自保留本端改名，b→a 的到达 op 落显式冲突不选边。
    deliver(&a, &b);
    deliver(&b, &a);
    let diverged_a: i64 = a
        .store
        .connection
        .query_row(
            "SELECT COUNT(*) FROM sync_conflicts WHERE reason='wiki_page_rename_diverged'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(diverged_a > 0, "两端各自改名必须显式冲突");
    // 两端均保留本端改名结果，未被对端覆盖。
    assert!(b.store.get_wiki_page("topic/乙名").unwrap().is_some());
    assert!(a.store.get_wiki_page("topic/甲名").unwrap().is_some());
}

#[test]
fn rename_arriving_after_delete_stays_conflict_candidate() {
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
    // b 离线删除；a 端改名后再投递。
    assert!(b.store.delete_wiki_page("topic/项目").unwrap());
    a.store.rename_wiki_page("topic/项目", "新名", "a").unwrap();
    deliver(&a, &b);
    // b 端页已被业务删除物理移除：重命名 op 落冲突候选（等删除态对齐后人工审）。
    let pending: i64 = b
        .store
        .connection
        .query_row(
            "SELECT COUNT(*) FROM sync_conflicts WHERE reason='wiki_page_deleted'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(pending > 0, "删除后的迟到重命名应保留为候选");
}

#[test]
fn wiki_page_revisions_replicate_history() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    a.store
        .submit_input_as_event("证据事件文本", "capture", None)
        .unwrap();
    let event_id = a.store.list_events().unwrap()[0].id.clone();
    seed_wiki_page(&a, "page-1", "topic/项目", "项目", "初版正文", &[&event_id]);
    capture_wiki(&a, "page-1");
    // 一次人工编辑：save 同时产生页 op（先）与修订 op（后）。
    a.store
        .save_wiki_page_content("topic/项目", "二版正文", "人工修订", None)
        .unwrap();
    deliver(&a, &b);
    // b 端收到同一份修订历史（起源端权威：同 id 同内容）。
    let (a_rev, b_rev): (Vec<(String, String, String)>, Vec<(String, String, String)>) = {
        let q = "SELECT id, content_md, reason FROM wiki_revisions WHERE page_id='page-1' ORDER BY created_at";
        (
            a.store
                .connection
                .prepare(q)
                .unwrap()
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap(),
            b.store
                .connection
                .prepare(q)
                .unwrap()
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap(),
        )
    };
    assert_eq!(a_rev.len(), 1, "seed 无捕获，仅 save 一条修订");
    assert_eq!(a_rev, b_rev, "两端修订历史必须逐行一致");
    assert_eq!(b_rev[0].2, "[human] 人工修订");
    assert_eq!(b_rev[0].1, "二版正文");
    // 幂等：重复投递不产生重复行。
    deliver(&a, &b);
    assert_eq!(b.count("wiki_revisions"), 1);
}

#[test]
fn revision_arriving_on_deleted_head_stays_quiet() {
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
    // b 端业务删除（物理删），a 端离线编辑（页 op + 修订 op）后投递。
    assert!(b.store.delete_wiki_page("topic/项目").unwrap());
    a.store
        .save_wiki_page_content("topic/项目", "离线改", "离线编辑", None)
        .unwrap();
    deliver(&a, &b);
    // 页 op 报 wiki_page_deleted 候选一次；修订 op 不重复立案、不落行。
    let candidates: i64 = b
        .store
        .connection
        .query_row(
            "SELECT COUNT(*) FROM sync_conflicts WHERE reason='wiki_page_deleted'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(candidates, 1, "同一次编辑只在一个 op 上立案");
    assert_eq!(b.count("wiki_revisions"), 0, "已删页不复活历史行");
}

// 记事本建档即空正文（快记第一条之前），空 revision 无信息量且协议校验
// 拒收（sync_wiki_revision_empty）；修复后空页建档不得炸捕获事务，页面本体
// 仍随 WikiPageChanged 同步，快记追加后对端可见。
#[test]
fn memo_empty_content_creation_syncs_and_append_follows() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    // 修复前：空正文建档触发 sync_wiki_revision_empty，建档事务整体回滚。
    let page = a
        .store
        .create_wiki_page("memo", "记事本", "", &[])
        .expect("空正文建档必须成功");
    capture_wiki(&a, &page.id);
    deliver(&a, &b);
    let synced = b.store.get_wiki_page(&page.slug).unwrap();
    assert!(synced.is_some(), "空正文页本体必须随 WikiPageChanged 同步");
    assert_eq!(synced.unwrap().content_md, "");
    // 快记追加：非空 revision 正常捕获并到达对端。
    a.store
        .append_wiki_page_content(&page.slug, "买牛奶", "快记")
        .unwrap();
    capture_wiki(&a, &page.id);
    deliver(&a, &b);
    assert_eq!(
        b.store
            .get_wiki_page(&page.slug)
            .unwrap()
            .unwrap()
            .content_md
            .trim(),
        "买牛奶"
    );
}
