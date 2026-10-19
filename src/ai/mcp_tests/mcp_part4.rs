use super::*;

#[test]
fn oversized_tool_result_is_truncated_with_marker() {
    let big = "好".repeat(MAX_RESULT_CHARS + 10);
    let truncated = truncate_result(big);
    assert!(truncated.chars().count() < MAX_RESULT_CHARS + 100);
    assert!(truncated.contains("已截断"));
}
