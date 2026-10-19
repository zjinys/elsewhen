#[allow(unused_imports)]
use super::*;

/// 目标与规则语义必须分离：规则是「对照检查」，目标是「仅作背景」。
#[test]
fn goals_and_rules_use_distinct_instructions() {
    use crate::storage::GoalPhase;
    let (store, conversation_id, path) = setup_store();
    store.create_goal("上线 v1", GoalPhase::Near).unwrap();
    // 规则库解耦后注入的是「规模播报」那句（不再全量搬规则原文），
    // 建一条生效规则让规模播报真实出现，两段都存在。
    store
        .add_rule("回复别用 emoji", crate::storage::RuleStatus::Active, None)
        .unwrap();

    let prompt = system_prompt_of(&store, &conversation_id);
    let goal_at = prompt.find("[近期]").expect("目标段应存在");
    let rule_at = prompt
        .find("个人规则库现有 1 条生效规则")
        .expect("规则段应存在");

    // 目标段在规则段之前，且各自措辞不同
    assert!(goal_at < rule_at, "目标段应与规则段分开");
    // 从目标段起点回看一整段，注意按字符边界切（提示词以中文为主，字节切分会切坏 UTF-8）
    // 用锚点取目标段，不按字节偏移回切：提示词以中文为主，按字节切会切坏
    // UTF-8 边界（`floor_char_boundary` 需 nightly，本仓库跑 stable）。
    let goal_block = prompt
        .split("用户当前的目标（仅作为理解其意图的背景）")
        .nth(1)
        .expect("目标段锚点应存在")
        .split('[')
        .next()
        .unwrap();
    assert!(
        !goal_block.contains("对照检查"),
        "目标不得继承规则的行为约束措辞：\n{goal_block}"
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn simple_memory_limits_message_count() {
    let memory = SimpleMemory::new(10);
    assert_eq!(memory.max_messages, 10);
}

#[test]
fn sliding_window_estimates_tokens() {
    let text = "This is a test message with several words";
    let tokens = SlidingWindowMemory::estimate_tokens(text);
    assert!(tokens > 0);
    assert!(tokens < text.len()); // Should be less than character count
}

#[test]
fn estimate_tokens_weights_cjk_more_than_ascii() {
    // 中文在主流 tokenizer 中约 1 字/token：同字符数的中文估算必须明显高于
    // ASCII（此前 chars()/4 对 CJK 低估 4 倍，请求体易硬超 provider 上限）。
    let cjk = "事件记录与任务跟进".repeat(10); // 90 个 CJK 字符
    let ascii = "a".repeat(90);
    assert!(cjk.chars().count() == ascii.chars().count());
    assert!(
        estimate_tokens(&cjk) > estimate_tokens(&ascii),
        "CJK 估算 {} 应高于 ASCII 估算 {}",
        estimate_tokens(&cjk),
        estimate_tokens(&ascii)
    );
    assert_eq!(
        estimate_tokens(&cjk),
        tiktoken_rs::cl100k_base_singleton()
            .encode_ordinary(&cjk)
            .len()
    );
    assert_eq!(
        estimate_tokens(&ascii),
        tiktoken_rs::cl100k_base_singleton()
            .encode_ordinary(&ascii)
            .len()
    );
}

#[test]
fn sliding_window_creates_with_limit() {
    let memory = SlidingWindowMemory::new(4096);
    assert_eq!(memory.max_tokens, 4096);
}

#[test]
fn simple_memory_prepends_system_prompt() {
    let (store, conversation_id, path) = setup_store();
    let context = SimpleMemory::new(10)
        .prepare_context(&conversation_id, &store)
        .unwrap();

    assert!(context[0].role == "system", "第一帧必须是 system 角色设定");
    assert!(
        context[0].content.contains("Elsewhen"),
        "system 提示词应定义本应用角色，实际: {}",
        context[0].content
    );
    assert!(
        context[0].content.contains("铁律"),
        "提示词应包含对工具零评价等硬性约束"
    );
    assert!(
        system_text(&context).contains("今日记录"),
        "system 提示词应注入当前对话标题"
    );
    // 原始消息应保留在 system 之后
    let user = context
        .iter()
        .find(|message| message.role == "user")
        .unwrap();
    assert!(user.content.contains("opencode2"));
    assert!(
        context[..context
            .iter()
            .position(|message| message.role == "user")
            .unwrap()]
            .iter()
            .all(|message| message.role == "system")
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn sliding_window_prepends_system_prompt() {
    let (store, conversation_id, path) = setup_store();
    let context = SlidingWindowMemory::new(4096)
        .prepare_context(&conversation_id, &store)
        .unwrap();

    assert!(context[0].role == "system");
    assert!(context[0].content.contains("个人知识库"));
    assert!(system_text(&context).contains("今日记录"));

    let _ = std::fs::remove_file(path);
}
