use super::*;

/// R1 基准：非 GPT 模型下 34,204 字节工具 schema 估算落 [8K,16K]
/// （事故实况：字节退化估为 34,204；真实 ~9-12K）。
#[test]
fn gitea_schema_estimation_falls_in_calibrated_band() {
    let mut schema = String::from("{");
    for i in 0..53 {
        let desc = "Execute Gitea repository operation and return structured result ".repeat(8);
        schema.push_str(
            &format!(
                "\"mcp__gitea-mcp__tool_{i:02}\": {{\"name\": \"tool_{i:02}\", \"description\": \"{desc}\"}},"
            ),
        );
    }
    schema.push_str("\"dummy\": null}");
    let estimated = text_tokens("deepseek-flash", &schema);
    assert!(
        (8_000..=16_000).contains(&estimated),
        "估算 {estimated} 未落 [8K,16K]"
    );
}

/// 1000 汉字的估算：没有实测数据的模型保持 R1 基准 [1.8K,2.4K]（字节退化时是
/// 3,000）；有实测数据的族按下调后的系数落在 [1.2K,1.5K]（2026-11-17）。
#[test]
fn chinese_history_estimation_falls_in_calibrated_band() {
    let text = "用户记录了一段关于咖啡豆烘焙曲线的观察，浅烘带出果酸，深烘则厚重回甘。".repeat(28); // ~980 字
    for model in ["unknown", "llama3-8b", "qwen-max", "deepseek-r1:8b"] {
        let estimated = text_tokens(model, &text);
        assert!(
            (1_800..=2_400).contains(&estimated),
            "{model} 估算 {estimated} 未落 [1.8K,2.4K]"
        );
        assert_eq!(estimated, conservative_text_tokens(&text));
    }
    for model in [
        "deepseek-flash",
        "deepseek/deepseek-v4.1-flash-1017",
        "glm-5.3-flash",
        "zai-org/GLM-5.3-Flash",
        "nim/nvidia/glm-5.3",
        "moonshotai/kimi-k3",
        "moonshot-v1-128k",
        "gemini-3.8-flash",
        "gpt-oss:120b",
    ] {
        let estimated = text_tokens(model, &text);
        assert!(
            (1_200..=1_500).contains(&estimated),
            "{model} 估算 {estimated} 未落 [1.2K,1.5K]"
        );
    }
    // 不含数字串的 ASCII 文本，估算与模型族无关。
    let ascii = "Execute repository operation and return structured result. ".repeat(40);
    assert_eq!(
        text_tokens("deepseek-flash", &ascii),
        text_tokens("unknown", &ascii)
    );
    // 估算方法不同，预算报告里的口径标签也不同（实测工具按它分组）。
    assert_ne!(counting_label("deepseek-flash"), counting_label("unknown"));
    assert_eq!(counting_label("gpt-4o"), counting_label("unknown"));
}

/// 下调汉字系数的族必须同时按真实成本给标识符串计价。BPE 对哈希、UUID、
/// 时间戳几乎逐字符切分，「每 3.4 字符 1 token」只估出真实值的一半到三分之一；
/// 汉字系数是 2.0 时这个低估被汉字的高估盖住，降到 1.2 后带引用元数据的工具
/// 结果会低于真实值。这里用知识检索结果、原文片段、内嵌引用三种实际会发送的
/// 结构，对照 tiktoken 两套词表。没有实测数据的模型估算不变。
#[test]
fn calibrated_estimate_prices_identifier_runs_at_their_real_cost() {
    let hash = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";
    let uuid = "3f2b8c1e-7a4d-4e9b-b1c6-0d5e8f2a9c47";
    let time = "2026-11-14T06:00:31.684192473+00:00";
    let excerpt =
        "用户记录了一段关于咖啡豆烘焙曲线的观察，浅烘带出果酸，深烘则厚重回甘。".repeat(7);
    let citation = |excerpt: &str| {
        serde_json::json!({"page_slug":"source-1a2b3c4d","content_hash":hash,"title":"咖啡豆烘焙曲线",
            "excerpt":excerpt,"reason":"问题与页面主题/正文匹配（2 处主题线索）","applicable_when":"",
            "strength":"reference","category":"外部材料或基于材料的推断",
            "sources":[{"snapshot_id":uuid,"title":"咖啡豆烘焙曲线","locator":"https://example.com/notes/roast-curve","version":3}],
            "event_ids":[uuid,uuid]})
    };
    let search = serde_json::json!({"knowledge_candidates":[citation(&excerpt),citation(&excerpt),citation(&excerpt)],
        "knowledge_reading":{"source-1a2b3c4d":{"coverage":"excerpt","total_chars":5210,"hit_offset":1830}},
        "knowledge_warnings":[],"events":[{"event_id":uuid,"excerpt":&excerpt[..90],"category":"用户记录"}]}).to_string();
    let spans = serde_json::json!({"kind":"original_history_spans","parts":(0..4).map(|i| serde_json::json!({
        "message_id":uuid,"role":"user","created_at":time,"start_char":i*2048,"end_char":(i+1)*2048,
        "original_text":"好的"})).collect::<Vec<_>>()}).to_string();
    for (name, text) in [
        ("hash", hash.to_string()),
        ("uuid", uuid.to_string()),
        ("time", time.to_string()),
        ("search", search),
        ("spans", spans),
        (
            "embedded",
            serde_json::Value::String(citation("短").to_string()).to_string(),
        ),
    ] {
        let estimated = text_tokens("deepseek-flash", &text);
        for (vocabulary, bpe) in [
            ("o200k", tiktoken_rs::o200k_base_singleton()),
            ("cl100k", tiktoken_rs::cl100k_base_singleton()),
        ] {
            let real = bpe.encode_ordinary(&text).len();
            assert!(
                estimated >= real,
                "{name}：估算 {estimated} 低于 {vocabulary} 的真实计数 {real}"
            );
        }
        // 没有实测数据的模型仍是原来的估算，这一项不随本次调整变化。
        assert_eq!(
            text_tokens("unknown", &text),
            conservative_text_tokens(&text)
        );
    }
    // 标识符串的计价只在下调了汉字系数的族生效。
    assert!(text_tokens("deepseek-flash", hash) > 2 * text_tokens("unknown", hash));
    // 不含数字的英文单词不算标识符串。
    let words = "structured repository operation result";
    assert_eq!(
        text_tokens("deepseek-flash", words),
        text_tokens("unknown", words)
    );
}

/// 下调后的估算仍不低于两种真实 BPE 分词器的计数。应用自带的中英文提示词
/// 资源就是请求里实际发送的内容（中文说明、英文说明、JSON 结构都有）。
/// DeepSeek 等族的分词器不在本地，这里只能拿 tiktoken 的两套词表作旁证：
/// 它不证明对那些模型不会低估，只保证系数没有低到连通用 BPE 都盖不住。
#[test]
fn calibrated_estimate_stays_above_real_bpe_counts_on_shipped_prompts() {
    let resources = [
        include_str!("../../../resources/prompts/chat-policy.zh-Hans.txt"),
        include_str!("../../../resources/prompts/chat-contract.zh-Hans.txt"),
        include_str!("../../../resources/prompts/chat-context.zh-Hans.json"),
        include_str!("../../../resources/prompts/chat-context.en.json"),
        include_str!("../../../resources/prompts/tool-descriptions.zh-Hans.json"),
        include_str!("../../../resources/prompts/tool-descriptions.en.json"),
        include_str!("../../../resources/prompts/task-processing.zh-Hans.json"),
        include_str!("../../../resources/prompts/digest-policy.zh-Hans.txt"),
    ];
    for (index, text) in resources.into_iter().enumerate() {
        let estimated = text_tokens("deepseek-flash", text);
        for (vocabulary, bpe) in [
            ("o200k", tiktoken_rs::o200k_base_singleton()),
            ("cl100k", tiktoken_rs::cl100k_base_singleton()),
        ] {
            let real = bpe.encode_ordinary(text).len();
            assert!(
                estimated >= real,
                "第 {index} 份资源：估算 {estimated} 低于 {vocabulary} 的真实计数 {real}"
            );
        }
    }
}

/// 空文本不炸、非 CJK 非 ASCII 字符 1：1 保守计。
#[test]
fn heuristic_edge_cases() {
    assert_eq!(text_tokens("deepseek-flash", ""), 0);
    assert_eq!(text_tokens("gpt-4o", ""), 0);
    // 浮点 ceil 边界：100 × 1.12 = 112（允许 +1 误差）
    let emoji = "🎯".repeat(100);
    assert!((112..=113).contains(&text_tokens("glm-5.3-flash", &emoji)));
}

#[test]
fn time_hints_follow_removed_turns_and_reanchor_after_smaller_retry() {
    use crate::ai::memory::ContextMessage;
    use crate::ai::time_context::{SystemClock, enrich_history, reminder_for};
    let user = |time: &str, content: String| {
        let mut message = ContextMessage::new("user", content);
        message.message_time = Some(chrono::DateTime::parse_from_rfc3339(time).unwrap());
        message
    };
    let oldest = user("2026-09-30T10:00:00Z", "old".repeat(4000));
    let middle = user("2026-09-30T10:01:00Z", "middle".repeat(300));
    let latest = user("2026-09-30T10:02:00Z", "当前问题".into());
    let oldest_hint = reminder_for(&oldest, &SystemClock).unwrap().content;
    let middle_hint = reminder_for(&middle, &SystemClock).unwrap().content;
    let latest_hint = reminder_for(&latest, &SystemClock).unwrap().content;
    let context = enrich_history(
        vec![
            ContextMessage::new("system", "规则"),
            oldest,
            middle,
            latest,
            ContextMessage::new("system", "必要来源"),
            ContextMessage::assistant_with_tool_calls(
                String::new(),
                vec![super::super::tool::ToolCall {
                    id: "call_current".into(),
                    name: "search".into(),
                    arguments: serde_json::json!({}),
                    raw_arguments: Some("{}".into()),
                }],
                None,
            ),
            ContextMessage::tool_result("call_current".into(), "真实结果".repeat(240).into()),
        ],
        &SystemClock,
    );
    assert_eq!(context.iter().filter(|m| m.time_reminder).count(), 1);
    let mut policies = RequestPolicies::from_context(&context);
    let mut messages: Vec<_> = context
        .iter()
        .map(|m| serde_json::json!({"role":m.role,"content":m.content}))
        .collect();
    let n = messages.len();
    messages[n - 2] = serde_json::json!({"role":"assistant", "tool_calls":[{"id":"call_current","type":"function","function":{"name":"search","arguments":"{}"}}]});
    messages[n - 1] = serde_json::json!({"role":"tool", "tool_call_id":"call_current", "content":"真实结果".repeat(240)});
    let exchange = messages[n - 3..].to_vec();
    let mut request = serde_json::json!({"messages":messages});
    policies.fit("unknown", &mut request, 4096, 512).unwrap();
    assert!(!request.to_string().contains(&oldest_hint));
    assert!(
        request["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["content"] == middle_hint)
    );
    // 10-19 档案 P0-b：只剩两轮时退步重试不可再砍——省略会复现
    // 「invisible turn」失忆事故，此时必须诚实失败而不是交付幻觉。
    let error = policies
        .fit("unknown", &mut request, 2000, 256)
        .unwrap_err();
    assert!(
        error.to_string().contains("最近两轮对话不可省略"),
        "实际错误：{error}"
    );
}

#[test]
fn optional_removal_keeps_time_policy_indices_in_sync() {
    use crate::ai::memory::ContextMessage;
    let mut background = ContextMessage::new("system", "optional".repeat(2000));
    background.optional_background = true;
    let mut hint = ContextMessage::new("system", "<message_time>old hint</message_time>");
    hint.time_reminder = true;
    let context = vec![
        ContextMessage::new("system", "rules"),
        background,
        ContextMessage::new("user", "old".repeat(3000)),
        hint,
        // 10-19 P0-b：最近两轮不可砍，砍轮目标必须是 old 轮；垫一轮
        // 小的让省略后仍能回到预算内。垫轮放大至新估算口径下仍
        // 超最小窗口，保住第二段断言的「最近两轮不可省略」诚实失败。
        ContextMessage::new("user", "垫轮".repeat(300)),
        ContextMessage::new("assistant", "垫答"),
        ContextMessage::new("user", "latest"),
        ContextMessage::new("system", "evidence"),
    ];
    let mut policies = RequestPolicies::from_context(&context);
    let mut request = serde_json::json!({"messages":context.iter().map(|m| serde_json::json!({"role":m.role,"content":m.content})).collect::<Vec<_>>()});
    policies.fit("unknown", &mut request, 2600, 256).unwrap();
    assert!(!request.to_string().contains("old hint"));
    assert!(request.to_string().contains("evidence"));
    assert!(request.to_string().contains("latest"));
    let messages = request["messages"].as_array().unwrap();
    assert_eq!(
        messages
            .iter()
            .filter(|m| m["content"]
                .as_str()
                .is_some_and(|c| c.contains("较早对话因")))
            .count(),
        1
    );
    // 退步重试装不下剩余两轮（579 token > 预算）时砍轮撞上
    // 「最近两轮不可省略」→ 诚实失败，而不是把上一轮也吞掉。
    let error = policies.fit("unknown", &mut request, 512, 64).unwrap_err();
    assert!(error.to_string().contains("最近两轮对话不可省略"));
    assert_eq!(
        request["messages"].as_array().unwrap().len(),
        policies.messages.len()
    );
}

#[test]
fn default_window_recognizes_mainstream_model_families() {
    assert_eq!(default_window("gpt-4o"), 65536);
    assert_eq!(default_window("deepseek-chat"), 65536);
    assert_eq!(default_window("deepseek-v4.1-flash"), 65536);
    assert_eq!(default_window("glm-5.3-flash"), 131072);
    assert_eq!(default_window("qwen3-235b"), 131072);
    assert_eq!(default_window("kimi-k2"), 131072);
    assert_eq!(default_window("moonshot-v1-128k"), 131072);
    assert_eq!(default_window("doubao-seed-1.6"), 131072);
    assert_eq!(default_window("claude-opus-4-6"), 131072);
    assert_eq!(default_window("gemini-3.0-pro"), 131072);
    // openrouter 风格 vendor 前缀
    assert_eq!(default_window("zai-org/glm-5.3"), 131072);
    // 未知模型保守兜底
    assert_eq!(default_window("llama3-8b"), 32768);
    assert_eq!(default_window("my-finetune-v2"), 32768);
}

#[test]
fn default_output_recognizes_model_families() {
    assert_eq!(default_output("deepseek-chat"), 8192);
    assert_eq!(default_output("deepseek-coder"), 8192);
    assert_eq!(default_output("deepseek-reasoner"), 32768);
    assert_eq!(default_output("deepseek-v4.1-flash"), 32768);
    assert_eq!(default_output("deepseek/deepseek-v4.1-flash-1017"), 32768);
    assert_eq!(default_output("deepseek-v3.2-exp"), 32768);
    assert_eq!(default_output("gpt-4o"), 32768);
    assert_eq!(default_output("glm-5.3-flash"), 32768);
    assert_eq!(default_output("zai-org/glm-5.3"), 32768);
    assert_eq!(default_output("kimi-k2"), 32768);
    assert_eq!(default_output("qwen3-235b"), 32768);
    assert_eq!(default_output("claude-opus-4-6"), 32768);
    assert_eq!(default_output("gemini-3.0-pro"), 32768);
    // 未知模型仍保守，但不再回到老默认的 4K 档
    assert_eq!(default_output("llama3-8b"), 16384);
    assert_eq!(default_output("my-finetune-v2"), 16384);
}

#[test]
fn chat_output_is_lower_than_generation_reserve() {
    // 聊天档取生成档的一半，但老代 8K 硬上限不动；64K 窗口的输入预算
    // 因此回到 ~46K，多轮工具结果不再撞本地拒绝（run 8fd94ade）。
    assert_eq!(chat_output("deepseek-chat"), 8192);
    assert_eq!(chat_output("deepseek-v4.1-flash"), 16384);
    assert_eq!(chat_output("deepseek/deepseek-v4.1-flash-1017"), 16384);
    assert_eq!(chat_output("gpt-4o"), 16384);
    assert_eq!(chat_output("glm-5.3-flash"), 16384);
    assert_eq!(chat_output("kimi-k2"), 16384);
    // 未知模型生成档 16384 → 聊天档 8192（仍高于实测聊天输出 ≤2.5K）
    assert_eq!(chat_output("llama3-8b"), 8192);
    // 不变量：聊天档永远 ≤ 生成档
    for m in [
        "deepseek-chat",
        "deepseek-v4.1-flash",
        "gpt-4o",
        "llama3-8b",
    ] {
        assert!(chat_output(m) <= default_output(m));
    }
}

#[test]
fn optional_background_is_removed_without_touching_evidence_or_tool_exchange() {
    let evidence = "经核验的原文，不得裁剪。".repeat(15);
    let mut req = serde_json::json!({"messages":[
        {"role":"system","content":"不可变规则"},
        {"role":"system","content":"可省略的其他会话背景".repeat(500)},
        {"role":"user","content":"当前问题"},
        {"role":"system","content":evidence},
        {"role":"assistant","tool_calls":[{"id":"a","function":{"name":"search","arguments":"{}"}}]},
        {"role":"tool","content":"真实执行结果","tool_call_id":"a"}
    ]});
    let protected = req["messages"].as_array().unwrap()[2..].to_vec();
    let mut optional = vec![1];
    let count = fit_request_with_background("unknown", &mut req, 4096, 512, &mut optional).unwrap();
    assert!(count <= 4096 - 512 - 256);
    assert!(optional.is_empty());
    assert_eq!(
        &req["messages"].as_array().unwrap()[2..],
        protected.as_slice()
    );
    assert!(
        req["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains("已省略")
    );
}

#[test]
fn smaller_retry_removes_only_marked_background() {
    let mut req = serde_json::json!({"messages":[
        {"role":"system","content":"核心规则"},
        {"role":"system","content":"背景".repeat(1000)},
        {"role":"user","content":"当前问题"}
    ]});
    let mut optional = vec![1];
    fit_request_with_background("unknown", &mut req, 16384, 512, &mut optional).unwrap();
    assert_eq!(optional, vec![1]);
    fit_request_with_background("unknown", &mut req, 4096, 512, &mut optional).unwrap();
    assert!(optional.is_empty());
    assert_eq!(req["messages"][2]["content"], "当前问题");
    fit_request_with_background("unknown", &mut req, 2048, 512, &mut optional).unwrap();
    assert_eq!(req["messages"][2]["content"], "当前问题");
}

#[test]
fn budget_errors_are_typed_and_invalid_reserves_do_not_overflow() {
    let mut req = serde_json::json!({"messages":[{"role":"user","content":"原文".repeat(2000)}]});
    for (window, output) in [(4096, 512), (4096, 0), (4096, usize::MAX)] {
        let error = fit_request("unknown", &mut req, window, output).unwrap_err();
        assert!(error.downcast_ref::<RequestBudgetError>().is_some());
    }
}

#[test]
fn full_request_counts_tools_and_preserves_current_exchange() {
    // 10-19 P0-b 之后旧话题轮可被砍但最近两轮（空回答工具轮+当前）不可；
    // schema 缩到砍完旧轮后能回到预算内。
    let schema = "嵌套参数描述：日期、条件、原文。".repeat(48);
    // 三轮构造：更早轮可砍，最近两轮（旧一轮+当前）受 10-19 P0-b 保护。
    let mut req = serde_json::json!({"messages":[{"role":"system","content":"系统要求"},{"role":"user","content":"更早话题".repeat(400)},{"role":"assistant","content":"更早回答"},{"role":"user","content":"旧话题".repeat(400)},{"role":"assistant","content":"旧回答"},{"role":"user","content":"当前问题"},{"role":"assistant","content":"","tool_calls":[{"id":"a","function":{"name":"search","arguments":"{}"}}]},{"role":"tool","content":"结果","tool_call_id":"a"}],"tools":[{"parameters":{"description":schema}}]});
    let bare = input_tokens("gpt-4o", &req["messages"]);
    assert!(input_tokens("gpt-4o", &req) > bare);
    let count = fit_request("gpt-4o", &mut req, 4096, 512).unwrap();
    assert!(count + 512 + 256 <= 4096);
    assert!(req.to_string().contains("当前问题"));
    assert!(req.to_string().contains("tool_call_id"));
    assert!(req.to_string().contains("已省略"));
}
#[test]
fn oversized_current_evidence_errors_without_silent_truncation() {
    let mut req = serde_json::json!({"messages":[{"role":"system","content":"原料".repeat(3000)},{"role":"user","content":"请阅读全文"}]});
    let before = req.clone();
    assert!(fit_request("unknown", &mut req, 4096, 512).is_err());
    assert_eq!(req, before);
    assert!(text_tokens("unknown", "中文🙂") >= text_tokens("gpt-4o", "中文🙂"));
}

/// P1 验收的主场景演练：按真实形态构造一个超长会话（主闲聊轮次
/// + 密集工具调用的wiki搜索/编辑会话），窗口 32k，提出 fit。
/// 验收口径（roadmap §P1）：
/// 1. 超长请求在报错前完成降级（Ok 且装配后 tokens ≤ input_limit）；
/// 2. 工具结果占位化节省量可观测（pruned_tool_results ≥ 15）；
/// 3. 降级后消息仍闭合（fil 内部 exchanges::validate 不炸）；
/// 4. 最近 3 个工具结果保留原文，更早的替换为占位。
#[test]
fn long_session_32k_window_fully_degrades_before_erroring() {
    let tool_result_payload = || {
        // 仿 wiki 检索/页面编辑产出的结构化长文，非同一种字骑常量。
        format!(
            "{{\"snippets\":[{snippet}]}} 卜补：\n```rust\n{code}\n```\n小结：以上要点来自知识库最近的整理，请引用时注意段落的原始出处。",
            snippet = (0..8)
                .map(|j| format!(
                    "{{\"title\":\"要点{j}\",\"slug\":\"kb/entry-{j}\",\"text\":\"{}\"}}",
                    "知识库存档的段落原文，用于核对来源与出处。".repeat(20)
                ))
                .collect::<Vec<_>>()
                .join(","),
            code = "fn consolidate(items: &[Item]) -> Summary { /* 省略实现 */ }\n".repeat(30)
        )
    };

    let mut messages = vec![serde_json::json!({
        "role":"system",
        "content":"你是个人知识助手。回答要引用知识库来源，保持简洁。当前会话在进行一篇技术调研的整理，完成后会导出到 wiki。"
    })];
    let mut expected_tool_results: usize = 0;
    for round in 0..24usize {
        messages.push(serde_json::json!({
            "role":"user",
            "content": format!(
                "第{round}轮：请把下面的素材整理进知识库，并把改动作与已建立条目建立交叉引用。这个工作流之后还会继续补充素材，烦请保持上下文连贯。素材如下：\n{}",
                "批次素材内容，涉及术语定义、文献线索、与之前条目的差异对比。".repeat(60)
            )
        }));
        if round % 4 == 3 {
            // assistant 带 tool_call + tool result（保留最近的闭合单元形态）
            let call_id = format!("call_{round}");
            messages.push(serde_json::json!({
                "role":"assistant",
                "content": null,
                "tool_calls":[{"id":call_id,"type":"function","function":{
                    "name":"wiki_search",
                    "arguments": format!("{{\"query\":\"第{round}轮相关的既有条目检索\",\"limit\":8}}"),
                }}]
            }));
            messages.push(serde_json::json!({
                "role":"tool",
                "tool_call_id":call_id,
                "content": tool_result_payload()
            }));
            expected_tool_results += 1;
        } else {
            messages.push(serde_json::json!({
                "role":"assistant",
                "content": format!(
                    "已核对素材并更新交叉引用。本轮补充：术语定义与文献线索已按既有结构入条，差异对比器已替你标出与第{}轮的冲突点，导出版 wiki 草稿也一并更新。后续几轮我们会继续推进引用关系采集，稍后可要求预览汇总。",
                    round.saturating_sub(4)
                )
            }));
        }
    }
    messages.push(serde_json::json!({"role":"user","content":"现在问题：把前面所有轮次的要点收敛成一份导出大纲。"}));
    let mut request = serde_json::json!({ "messages": messages });
    let serialized_chars = request.to_string().chars().count();
    assert!(
        serialized_chars > 45_000,
        "构造负载应显著超长，实测 {serialized_chars} chars"
    );

    let window = 32_768usize;
    let output = 2_048usize;
    let tokens_before = input_tokens("gpt-4o", &request);
    let mut policies = RequestPolicies::plain(&request);
    let tokens_after = policies
        .fit("gpt-4o", &mut request, window, output)
        .expect("超长会话应在报错前完成降级");
    let limit = input_limit(window, output).unwrap();
    assert!(tokens_before > limit, "构造负载应确实触发降级路径");
    assert!(
        tokens_after <= limit,
        "降级后应入预算：{tokens_after} ≤ {limit}"
    );
    // 装配报告第一手值（非事后从 wire 反推）：占位化确实发生过。
    let first_hand_pruned = policies.pruned_tool_results;
    assert!(
        first_hand_pruned > 0,
        "占位化应参与降级，实现为 0 说明顺序回归"
    );

    let sent = &request["messages"];
    let arr = sent.as_array().unwrap();
    let tool_messages: Vec<(usize, &serde_json::Value)> = arr
        .iter()
        .enumerate()
        .filter(|(_, m)| m["role"] == "tool")
        .map(|(i, m)| (i, m))
        .collect();
    // 闭合单元铁律：每个存留的 tool 结果，其发起方 assistant tool_call
    // 必须同在（两者要么都留要么都摘，绝不孒儿）。
    for (idx, tool_msg) in &tool_messages {
        let call_id = tool_msg["tool_call_id"].as_str().unwrap_or("");
        let paired = arr[..*idx].iter().any(|m| {
            m["role"] == "assistant"
                && m["tool_calls"]
                    .as_array()
                    .is_some_and(|calls| calls.iter().any(|c| c["id"].as_str() == Some(call_id)))
        });
        assert!(
            paired,
            "孒儿 tool_result 出现：tool_call_id={call_id}（红线 2）"
        );
    }
    let kept_raw = tool_messages
        .iter()
        .filter(|(_, m)| {
            m["content"]
                .as_str()
                .is_some_and(|c| c.contains("知识库存档的段落"))
        })
        .count();
    let pruned = tool_messages
        .iter()
        .filter(|(_, m)| {
            m["content"].as_str() == Some(crate::ai::budget::PRUNED_TOOL_RESULT_PLACEHOLDER)
        })
        .count();
    // 降级构成（占位化只试一轮，不够则回落砍轮——设计如此）：
    let dropped_pairs = expected_tool_results.saturating_sub(tool_messages.len());
    println!(
        "[P1 验收] 原 {} 个工具结果 · 降级前 tokens {tokens_before} → 降级后 {tokens_after}（预算 {limit}）\n  占位化（第一手报告）{first_hand_pruned} · 随闭合单元整轮摘除 {} 个结果 · wire 带占位 {} · 原文保留 {}",
        expected_tool_results, dropped_pairs, pruned, kept_raw
    );
    assert!(
        first_hand_pruned + kept_raw <= expected_tool_results,
        "占位+保留合计不应超出原有工具结果数"
    );
    assert!(
        kept_raw <= 3,
        "「保留最近 3」的语义必须守住：kept_raw={kept_raw}"
    );
}

/// 2026-10-18 事故「预算前移」：超限的补充材料（TaskSupplement）应在
/// 砍历史轮之前先截断挂载，而不是拒绝整轮；截断后带如实声明，当前
/// 问题与历史轮完整保留。
#[test]
fn oversized_supplement_is_truncated_instead_of_rejecting_the_turn() {
    use crate::ai::memory::ContextMessage;
    let body = "用户带进来的素材段落，包含术语定义与出处。".repeat(4000);
    let context_messages = vec![
        ContextMessage::new("system", "基线规则"),
        ContextMessage::new("user", "第一问"),
        ContextMessage::new("assistant", "第一答"),
        ContextMessage::new("user", "当前问题：请基于补充材料继续"),
        ContextMessage::new("system", body).with_segment(
            SegmentPurpose::TaskSupplement,
            SegmentOrigin::StoredMessage,
            Some("input-1"),
            OmissionPolicy::Required,
        ),
    ];
    let original_chars = context_messages.last().unwrap().content.chars().count();
    let mut request = serde_json::json!({ "messages": context_messages.iter()
        .map(|m| serde_json::json!({"role": m.role, "content": m.content}))
        .collect::<Vec<_>>() });
    let mut policies = RequestPolicies::from_context(&context_messages);
    policies
        .fit("gpt-4o", &mut request, 6000, 512)
        .expect("截断挂载后应回到预算内，而不是拒绝整轮");
    let msgs = request["messages"].as_array().unwrap();
    // 截断先于砍轮发生：历史轮与当前问题未被損坏
    assert!(msgs.contains(&serde_json::json!({"role":"user","content":"第一问"})));
    assert!(msgs.iter().any(|m| {
        m["content"]
            .as_str()
            .is_some_and(|c| c.contains("当前问题：请基于补充材料继续"))
    }));
    // 补充材料被截断且带如实声明
    let mounted = msgs
        .iter()
        .find(|m| {
            m["role"] == "system"
                && m["content"]
                    .as_str()
                    .is_some_and(|c| c.contains("素材段落"))
        })
        .expect("补充材料应仍在（截断挂载而非丢弃）");
    let content = mounted["content"].as_str().unwrap();
    assert!(content.contains("因超过请求预算被截断"));
    let mounted_chars = content.chars().count();
    assert!(
        mounted_chars < original_chars,
        "截断必须实际发生：{mounted_chars} vs {original_chars}"
    );
}
