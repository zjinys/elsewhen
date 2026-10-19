use super::*;

use super::*;

#[test]
fn wash_emotion_drops_empty_merges_duplicates_and_caps() {
    let mut value =
        EventAnalysisV1::parse(r#"{"schema_version":"event-analysis-v3","recordable":true,"kind":"event","event_type":"note","confidence":0.9,"summary":"s","clarifications":[],"people":[],"projects":[],"activities":[],"follow_ups":[],"emotion":[{"target":"甲","feeling":"焦虑"},{"target":"甲","feeling":"焦虑"},{"feeling":"挫败"},{"target":"乙","feeling":"被理解"}]}"#).unwrap();
    wash_emotion(&mut value);
    let tags = value.emotion.unwrap();
    assert_eq!(tags.len(), 3, "重复归并、其余保留");
    assert_eq!(
        tags[1].target.as_deref(),
        None,
        "缺 target 保留为对事件本身"
    );
    assert_eq!(tags[1].feeling, "挫败");
    // 全空 → 整字段清空
    // 条数封顶 8
    let mut many_json = String::from(
        r#"{"schema_version":"event-analysis-v3","recordable":true,"kind":"event","event_type":"note","confidence":0.9,"summary":"s","clarifications":[],"people":[],"projects":[],"activities":[],"follow_ups":[],"emotion":["#,
    );
    for i in 0..10 {
        if i > 0 {
            many_json.push(',');
        }
        many_json.push_str(&format!(r#"{{"target":"person{i}","feeling":"感受{i}"}}"#));
    }
    many_json.push_str("]}");
    let mut many = EventAnalysisV1::parse(&many_json).unwrap();
    wash_emotion(&mut many);
    assert_eq!(many.emotion.unwrap().len(), 8, "条数封顶 8");
}
