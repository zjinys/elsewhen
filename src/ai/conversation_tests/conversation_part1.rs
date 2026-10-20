use super::super::content_chat::{
    is_confirmation, is_declination, parse_rule_proposal, trim_noise,
};
use super::super::parse_tools::{extract_claimed_titles, parse_tool_call_envelope};
use super::super::*;
use super::*;
use crate::ai::conversation::{ConversationConfig, MemoryType, ProviderType};

#[test]
fn default_config_uses_sliding_window() {
    let config = ConversationConfig::default();
    matches!(config.memory_type, MemoryType::SlidingWindow { .. });
}

#[test]
fn default_config_uses_openai_compatible() {
    let config = ConversationConfig::default();
    matches!(config.provider_type, ProviderType::OpenAiCompatible);
}

#[test]
fn confirmation_words() {
    for yes in [
        "好",
        "好的",
        "好呀",
        "嗯嗯",
        "可以",
        "没问题",
        "行吧",
        "记下",
        "ok",
        "OK",
        "好的！",
        "就这么办",
        "确认",
        "记下来吧",
        "好，",
        "好。",
        "「好」",
        "“好”",
        "好的呀",
        "好嘞",
        "嗯！",
    ] {
        assert!(is_confirmation(yes), "应识别为确认: {yes}");
    }
    for no in [
        "不好",
        "确认一下",
        "好的，别保存",
        "可以不保存",
        "确认不要存",
        "再看看",
        "这条规则不太好",
        "好烦啊",
        "讲到另外一件事了",
        "不用",
        "先别记",
        "不行就打电话",
        "好的句子咋写啊",
    ] {
        assert!(!is_confirmation(no), "不应识别为确认: {no}");
    }
}

#[test]
fn declination_words() {
    for yes in [
        "不用",
        "不用了",
        "算了",
        "不要",
        "不存",
        "先不用",
        "以后再说",
        "不用记了",
        "暂不",
        "取消",
        "先放着",
    ] {
        assert!(is_declination(yes), "应识别为拒绝: {yes}");
    }
    for no in ["好", "好的", "不行就再打电话", "我先看看", "明天再说吧"] {
        assert!(!is_declination(no), "不应识别为拒绝: {no}");
    }
}

#[test]
fn parse_rule_proposal_extracts_and_cleans() {
    let reply = "这口气确实难咽，明明是对方的问题却让你兜底 💢\n\n[规则提议]和大型企业的人沟通重要事项必须留痕（文字或邮件记录）";
    let (shown, proposal) = parse_rule_proposal(reply);
    let proposal = proposal.expect("应有提议规则");
    assert!(proposal.rule.starts_with("和大型企业的人沟通"), "规则文本: {}", proposal.rule);
    assert!(!shown.contains("[规则提议]"), "展示文本不应含原始标记");
    assert!(
        shown.contains("建议沉淀成一条个人规则"),
        "展示文本应含友好提示: {shown}"
    );
}

#[test]
fn parse_rule_proposal_none_when_absent() {
    let (shown, rule) = parse_rule_proposal("普通回复，没有提议");
    assert!(rule.is_none());
    assert_eq!(shown, "普通回复，没有提议");
}

#[test]
fn parse_tool_call_envelope_single_call() {
    let content = r#"我帮你查一下规则库。
[工具调用]{"name":"list_rules","arguments":{}}"#;
    let (clean, calls) = parse_tool_call_envelope(content);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "list_rules");
    assert!(!clean.contains("[工具调用]"), "信封应被剥离: {clean}");
    assert!(
        clean.contains("我帮你查一下规则库"),
        "其余文本应保留: {clean}"
    );
}

#[test]
fn parse_tool_call_envelope_broken_line_is_skipped() {
    // 标记行不是合法 JSON（被换行打断）→ 不解析、不 panic；
    // 残留标记行按协议兜底规则剥掉，不见用户。
    let content = "[工具调用]{\"name\":\"get_wiki_page\"";
    let (clean, calls) = parse_tool_call_envelope(content);
    assert!(calls.is_empty(), "非法 JSON 不应被解析");
    assert!(!clean.contains("[工具调用]"), "残留标记应被剥离: {clean}");
}

#[test]
fn parse_tool_call_envelope_no_marker() {
    let (clean, calls) = parse_tool_call_envelope("随便聊聊");
    assert!(calls.is_empty());
    assert_eq!(clean, "随便聊聊");
}

#[test]
fn parse_tool_call_envelope_strips_dsml_protocol_output() {
    let content = "我已经记录好了。\n<|DSML|>\n<|invoke name=\"record_event\">\n<|parameter name=\"content\">今天完成了\n</|parameter>\n</|invoke>\n<|DSML|>\n";
    let (clean, calls) = parse_tool_call_envelope(content);
    assert_eq!(clean, "我已经记录好了。");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "record_event");
    assert_eq!(calls[0].arguments["raw_text"], "今天完成了");
}

#[test]
fn parse_tool_call_envelope_xml_dialect() {
    // 线上真实漏出：模型无视文本协议，按自带习惯吐 XML 方言。
    // 必须解析执行，且正文不留残留。
    let content = "关于小生意和需求的关系说的很透彻。\n<tool_call>search_knowledge_base<arg_key>query</arg_key><arg_value>感悟</arg_value></tool_call>";
    let (clean, calls) = parse_tool_call_envelope(content);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "search_knowledge_base");
    assert_eq!(calls[0].arguments["query"], "感悟");
    assert!(
        !clean.contains("tool_call") && !clean.contains("arg_"),
        "调用块应被整体剥离: {clean}"
    );
    assert!(clean.contains("关于小生意"), "其余文本应保留: {clean}");
}

#[test]
fn parse_tool_call_envelope_xml_multi_args_and_blocks() {
    let content = "先搜再看。\n<tool_call>search_knowledge_base<arg_key>query</arg_key><arg_value>咖啡</arg_value><arg_key>limit</arg_key><arg_value>3</arg_value></tool_call>\n<tool_call>list_rules<arg_key>keyword</arg_key><arg_value>归档</arg_value></tool_call>";
    let (clean, calls) = parse_tool_call_envelope(content);
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].name, "search_knowledge_base");
    assert_eq!(calls[0].arguments["query"], "咖啡");
    assert_eq!(calls[0].arguments["limit"], "3");
    assert_eq!(calls[1].name, "list_rules");
    assert_eq!(clean, "先搜再看。");
}

#[test]
fn parse_tool_call_envelope_xml_garbage_is_scrubbed() {
    // 结构残缺（无闭合/无参数）：不解析，但也不许漏给用户。
    for content in [
        "正文\n<tool_call>search_knowledge_base<arg_key>query</arg_key>",
        "正文\n<tool_call></tool_call>尾巴",
        "正文\n<tool_call>list_rules",
    ] {
        let (clean, calls) = parse_tool_call_envelope(content);
        assert!(calls.is_empty(), "残缺块不应被解析: {content}");
        assert!(!clean.contains("tool_call"), "残留标记应被剥离: {clean}");
    }
}

#[test]
fn tool_call_attempt_detected_structural_markers() {
    let names = vec!["search_knowledge_base".to_string()];
    // 各家方言的结构标记：见一个即命中，与严格解析器能否解析无关
    for content in [
        "<tool_call>search_knowledge_base<arg_key>query</arg_key><arg_value>x</arg_value></tool_call>",
        "[工具调用]{\"name\":\"search_knowledge_base\",\"arguments\":{}}",
        "<function name=\"search_knowledge_base\"><param>x</param></function>",
        "{\"tool_calls\":[{\"name\":\"search_knowledge_base\"}]}",
    ] {
        assert!(
            tool_call_attempt_detected(content, &names),
            "应命中: {content}"
        );
    }
}

#[test]
fn tool_call_attempt_detected_tool_name_plus_shape() {
    let names = vec![
        "search_knowledge_base".to_string(),
        "list_rules".to_string(),
    ];
    // 未知方言但含工具名 + 调用形态 → 命中（回炉），不能静默吞掉
    assert!(tool_call_attempt_detected(
        "[CALL] search_knowledge_base with arguments {\"query\": \"x\"}",
        &names
    ));
    // 只提到工具名、没有调用形态 → 不命中（正常讲解文字，放行）
    assert!(!tool_call_attempt_detected(
        "你可以用 search_knowledge_base 搜一下知识库，挺方便的",
        &names
    ));
    // 普通正文 → 不命中
    assert!(!tool_call_attempt_detected(
        "今天天气不错，适合出门",
        &names
    ));
}

#[test]
fn agent_loop_unknown_dialect_is_repaired_not_swallowed() {
    // 原则1的端到端：未知方言（严格解析器不认、意图检测认）→ 回炉 →
    // 模型按唯一格式重发 → 工具必须执行，最终回答干净。
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("t"), None).unwrap();
    let mut context = vec![ContextMessage::new("user", "帮我搜点感悟")];
    let registry = ToolRegistry::default();
    let provider = ScriptedProvider::new(vec![
        // 第一轮：未知方言（<function> 不在严格解析器覆盖内）
        Ok(AiReply::text(
            "我来搜一下。<function name=\"list_rules\"><param>归档</param></function>",
        )),
        // 回炉后：按唯一格式重发
        Ok(AiReply::text(
            "[工具调用]{\"name\":\"list_rules\",\"arguments\":{}}",
        )),
        Ok(AiReply::text("规则库查好了。")),
    ]);
    let outcome = run_agent_loop(
        &provider,
        "test-provider",
        &mut context,
        &registry,
        &store,
        &conv,
        chrono::Utc::now(),
    )
    .unwrap();
    assert_eq!(outcome.content, "规则库查好了。");
    // 回炉提示确已下发（不是静默吞掉）
    assert!(
        context.iter().any(|m| m.content.contains("格式无法识别")),
        "应有回炉提示"
    );
    // 工具确已执行（不是跳过）
    assert!(
        context
            .iter()
            .any(|m| m.content.contains("工具「list_rules」执行结果")),
        "回炉后工具必须执行"
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn write_claim_detected_forensic_corpus() {
    // 真实法医语料：命中 = 当初撒谎的那几句；不命中 = 诚实/检索/草拟语境。
    let claims = [
        "存好了 ✅ 「感悟」这页已经在知识库里了，以后这类零散想法都往里扔。",
        "存好了 ✅ 「感悟」这页现在是两条都在里面，往后有新的接着往里攒。",
        "已保存 ✅ 在知识库里就能看到《视频二创搬运 YouTube：Skill + Codex 工作流》。",
        "已保存 ✅ 「售卖伪解决方案：关键信息提炼」现在挂在本页下的派生产物里，原页正文一字未动。",
    ];
    for c in claims {
        assert!(write_claim_detected(c), "应识别为写声明: {c}");
    }
    let non_claims = [
        "搜了一下已保存的事件和知识库，确实没有「张三」「赵六」的记录。",
        "草拟好了，你看下：\n\n**标题**：一个月赚一万元（主题）",
        "在「今天」栏或审阅卡确认后保存 ✅",
        "翻了下知识库，「感悟」那页其实还没真正建起来（我之前说存好了，说早了，抱歉）。",
        "草稿这次是真的登记进系统了（状态：待确认）——之前几轮是我光说没执行。",
        "知识库里还没有这一页，我草拟好了。",
        "今天天气不错，适合出门",
    ];
    for c in non_claims {
        // 注意：第4条含“说存好了”但被“还没”守卫拦下——它是在认错，不是在声明完成。
        assert!(!write_claim_detected(c), "不应识别为写声明: {c}");
    }
}

#[test]
fn agent_loop_false_save_claim_is_repaired_not_stored() {
    // 接地关端到端：用户要求保存 → 模型撒谎“存好了”（无任何工具调用）→
    // 回炉纠正 → 模型实际调用 save_knowledge_draft → 诚实回复收敛，谎言不入库。
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("t"), None).unwrap();
    let mut context = vec![ContextMessage::new("user", "把这两条感悟存一下")];
    let registry = ToolRegistry::default();
    let provider = ScriptedProvider::new(vec![
        // 第一轮：撒谎（感悟事件的原文句式），无工具调用
        Ok(AiReply::text(
            "存好了 ✅ 「感悟」这页已经在知识库里了，以后这类想法都往里扔。",
        )),
        // 回炉后：实际调用写工具（只登记草稿，不算写）
        Ok(AiReply::text(
            "[工具调用]{\"name\":\"save_knowledge_draft\",\"arguments\":{\"title\":\"感悟\",\"content_md\":\"两条感悟正文\"}}",
        )),
        // 落定：诚实回复（无完成声明，直接收敛）
        Ok(AiReply::text(
            "草拟好了，标题「感悟」，在审阅卡点击接受保存。",
        )),
    ]);
    let outcome = run_agent_loop(
        &provider,
        "test-provider",
        &mut context,
        &registry,
        &store,
        &conv,
        chrono::Utc::now(),
    )
    .unwrap();
    assert_eq!(
        outcome.content,
        "草拟好了，标题「感悟」，在审阅卡点击接受保存。"
    );
    // 纠正提示确已下发
    assert!(
        context.iter().any(|m| m.content.contains("系统核实")),
        "应有写声明纠正"
    );
    // 草稿确已登记（纠正后真的调了工具，不是换一句谎言）
    assert_eq!(
        store.pending_actions_for_conversation(&conv).unwrap().len(),
        1
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn future_promise_detected_catches_preamble_without_action() {
    // 线上真实回复（2026-09-29 知识页「Omni flash 视频 prompt」第 4 条）：
    // 用户贴入新 prompt 要求合并保存，模型只回一句预告，零工具调用零写入。
    let real = "由于新的 prompt 与现有页面相关且有重叠的内容，我们可以选择将这两个 prompt 结合到一个页面中，以便更好地管理和查阅。\n\n我将为你草拟更新后的版本，稍等片刻。";
    assert!(future_promise_detected(real), "应识别为空头预告");
    // 关键：写声明检测对它完全无感——这正是它当初能收敛溜过去的原因。
    assert!(
        !write_claim_detected(real),
        "预告不是完成声明，两个检测器必须各管一头"
    );

    for p in [
        "稍等片刻",
        "请稍等",
        "我这就处理",
        "接下来我先整理一下",
        "我准备一个更新版本",
        "我来帮你看",
    ] {
        assert!(future_promise_detected(p), "应识别为预告: {p}");
    }

    // 正常答复不得误伤：解释、提问、道歉、以及「已交付内容 + 求确认」。
    for ok in [
        "这两条 prompt 的差别主要在镜头语言上，我列三点给你看。",
        "你是想把两条合并，还是各自建一页？",
        "抱歉，刚才没能生成回复，请再发一次。",
        "草拟好了，标题「感悟」，在审阅卡点击接受保存。",
        "已经整理成草稿了，请确认要不要入库。",
    ] {
        assert!(!future_promise_detected(ok), "不应识别为预告: {ok}");
    }
}

#[test]
fn agent_loop_promise_without_tool_call_is_repaired() {
    // 承诺回炉端到端：用户要保存 → 模型只回「稍等片刻」（零工具调用）→
    // 回炉逼它动手 → 模型真的调 save_knowledge_draft → 诚实回复收敛。
    // 改之前第一轮就 break，用户永远等不到草稿。
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("t"), None).unwrap();
    let mut context = vec![ContextMessage::new("user", "把这条新的 prompt 也存进来")];
    let registry = ToolRegistry::default();
    let provider = ScriptedProvider::new(vec![
        // 第一轮：空头预告，没有工具调用
        Ok(AiReply::text(
            "我准备一个更新版本，把新 prompt 整合进去。\n\n我将为你草拟更新后的版本，稍等片刻。",
        )),
        // 回炉后：真的调写工具
        Ok(AiReply::text(
            "[工具调用]{\"name\":\"save_knowledge_draft\",\"arguments\":{\"title\":\"视频 prompt 优化版\",\"content_md\":\"优化后的 prompt 正文\"}}",
        )),
        // 落定：诚实回复
        Ok(AiReply::text(
            "草拟好了，标题「视频 prompt 优化版」，在审阅卡点击接受保存。",
        )),
    ]);
    let outcome = run_agent_loop(
        &provider,
        "test-provider",
        &mut context,
        &registry,
        &store,
        &conv,
        chrono::Utc::now(),
    )
    .unwrap();
    assert_eq!(
        outcome.content,
        "草拟好了，标题「视频 prompt 优化版」，在审阅卡点击接受保存。"
    );
    assert!(
        context.iter().any(|m| m.content.contains("承诺不是答案")),
        "应下发承诺回炉提示"
    );
    assert_eq!(
        store.pending_actions_for_conversation(&conv).unwrap().len(),
        1,
        "回炉后应真的登记了草稿，而不是又回一句承诺"
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn agent_loop_retries_empty_reply_instead_of_converging_empty() {
    // 普通空回复保留协议重试一次；再次为空则进入只答复的收尾轮。
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("t"), None).unwrap();
    let mut context = vec![ContextMessage::new("user", "继续")];
    let registry = ToolRegistry::default();
    // 两次普通空回复后，第三次请求只组织最终答复。
    let provider = ScriptedProvider::new(vec![
        Ok(AiReply::text("")),
        Ok(AiReply::text("")),
        Ok(AiReply::text("这是更新后的版本：…")),
    ]);
    let outcome = run_agent_loop(
        &provider,
        "test-provider",
        &mut context,
        &registry,
        &store,
        &conv,
        chrono::Utc::now(),
    )
    .unwrap();
    assert_eq!(outcome.content, "这是更新后的版本：…");
    assert_eq!(outcome.empty_retries, 1, "应恰好重发一次");
    assert!(
        context.iter().any(|m| m.content.contains("返回了空内容")),
        "应下发空回复回炉提示"
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn agent_loop_persistent_empty_terminates_within_budget() {
    // 持续空回复不能变成死循环：空回复预算用尽即收敛，轮数有界。
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("t"), None).unwrap();
    let mut context = vec![ContextMessage::new("user", "继续")];
    let registry = ToolRegistry::default();
    let provider = ScriptedProvider::new(vec![
        Ok(AiReply::text("")),
        Ok(AiReply::text("")),
        Ok(AiReply::text("")),
        Ok(AiReply::text("")),
        Ok(AiReply::text("")),
    ]);
    let outcome = run_agent_loop(
        &provider,
        "test-provider",
        &mut context,
        &registry,
        &store,
        &conv,
        chrono::Utc::now(),
    )
    .unwrap();
    assert!(outcome.content.contains("还没有交付可用结果"));
    assert!(!outcome.content.contains("模型"));
    assert_eq!(outcome.empty_retries, MAX_EMPTY_RETRIES);
    assert!(
        outcome.rounds_used <= MAX_TOOL_ROUNDS,
        "轮数必须有界: {}",
        outcome.rounds_used
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn extract_claimed_titles_picks_quoted_entities() {
    let titles =
        extract_claimed_titles("存好了 ✅ 「感悟」这页已经在知识库里了，《视频二创》也能看到。");
    assert!(titles.contains(&"感悟".to_string()), "{titles:?}");
    assert!(titles.contains(&"视频二创".to_string()), "{titles:?}");
    // 单字虚词引用不是实体
    assert!(extract_claimed_titles("在审阅卡确认").is_empty());
    assert!(extract_claimed_titles("今天天气不错").is_empty());
}
