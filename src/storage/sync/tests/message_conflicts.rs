use super::deliver;
use super::pair;
use super::*;

#[test]
fn conversation_conflict_never_hides_the_new_immutable_fact() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    let c = a.store.create_conversation(None, None).unwrap();
    deliver(&a, &b);
    let captured = b
        .store
        .submit_conversation_input(&c, "offline fact survives", None)
        .unwrap();
    a.store.set_conversation_archived(&c, true).unwrap();
    a.store.delete_archived_conversation(&c).unwrap();
    deliver(&b, &a);
    assert_eq!(a.count("events"), 1);
    assert_eq!(a.count("messages"), 0);
    assert_eq!(a.count("sync_conflicts"), 1);
    let projected = a.store.get_input_record(&captured.id).unwrap().unwrap();
    assert_eq!(projected.event_id, captured.event_id);
    assert!(projected.message_id.is_none());
    assert_eq!(
        a.store.list_events().unwrap()[0].raw_text,
        "offline fact survives"
    );
}

#[test]
fn cross_origin_missing_base_waits_instead_of_creating_a_false_conflict() {
    let a = Database::new("a");
    let b = Database::new("b");
    let c = Database::new("c");
    pair(&a, &b);
    pair(&b, &c);
    let conversation = a.store.create_conversation(Some("initial"), None).unwrap();
    deliver(&a, &b);
    b.store
        .rename_conversation(&conversation, "b update")
        .unwrap();
    deliver(&b, &c);
    assert_eq!(c.count("sync_conflicts"), 0);
    assert!(c.store.get_conversation(&conversation).unwrap().is_none());
    pair(&a, &c);
    deliver(&a, &c);
    assert_eq!(
        c.store
            .get_conversation(&conversation)
            .unwrap()
            .unwrap()
            .title
            .as_deref(),
        Some("b update")
    );
    assert_eq!(c.count("sync_conflicts"), 0);
}

#[test]
fn logical_message_order_is_consistent_after_concurrent_appends() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    let c = a.store.create_conversation(None, None).unwrap();
    deliver(&a, &b);
    let first = a.store.send_message(&c, "user", "first", None).unwrap();
    deliver(&a, &b);
    let reply = b
        .store
        .send_message(&c, "assistant", "reply", Some(&first))
        .unwrap();
    let independent = a
        .store
        .send_message(&c, "user", "independent", None)
        .unwrap();
    deliver(&a, &b);
    deliver(&b, &a);
    let ids = |db: &Database| {
        db.store
            .list_messages(&c)
            .unwrap()
            .into_iter()
            .map(|m| m.id)
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&a), ids(&b));
    let order = ids(&a);
    assert!(order.iter().position(|id| id == &first) < order.iter().position(|id| id == &reply));
    assert!(order.contains(&independent));
}

#[test]
fn concurrent_title_changes_are_preserved_as_conflicts() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    let c = a.store.create_conversation(Some("initial"), None).unwrap();
    deliver(&a, &b);
    a.store.rename_conversation(&c, "a edit").unwrap();
    b.store.rename_conversation(&c, "b edit").unwrap();
    deliver(&a, &b);
    deliver(&b, &a);
    assert_eq!(a.count("sync_conflicts"), 1);
    assert_eq!(b.count("sync_conflicts"), 1);
    assert_eq!(
        a.store
            .get_conversation(&c)
            .unwrap()
            .unwrap()
            .title
            .as_deref(),
        Some("a edit")
    );
    assert_eq!(
        b.store
            .get_conversation(&c)
            .unwrap()
            .unwrap()
            .title
            .as_deref(),
        Some("b edit")
    );
}
