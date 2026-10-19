use super::*;

#[test]
fn proposals_parse_plain_and_fenced() {
    let reply = r#"[{"op":"create","kind":"recurring_cost","slug":"recurring-cost/test","title":"t","summary":"s","content":"- a","tags":["x"],"source_event_ids":["1"],"reason":"r"}]"#;
    let list = parse_proposals(reply).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].slug, "recurring-cost/test");

    let fenced = format!("```json\n{}\n```", reply);
    assert_eq!(parse_proposals(&fenced).unwrap().len(), 1);
}

#[test]
fn garbage_errors() {
    assert!(parse_proposals("不是 JSON").is_err());
}

#[test]
fn non_200_response_is_error() {
    let json = r#"{"code": 404, "message": "Tweet not found"}"#;
    let parsed: FxTwitterResponse = serde_json::from_str(json).unwrap();
    assert!(parse_tweet_content(parsed).is_err());
}
