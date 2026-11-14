// 合成工具（knowledge::synthesis）：选页、分块、分批提取、逐字校验与装配。

use crate::knowledge::synthesis::{Channel, Request, synthesize};

/// 铺垫句与标记句都是 10 个字符，6000 字符的分块边界正好落在句子之间。
const FILLER: &str = "这是一句铺垫的话呀。";
const LIMIT: &str = "限制甲：失败要回滚。";
const DATA: &str = "数据乙：重试三次整。";
const COUNTER: &str = "反例丙：批量不回滚。";

/// `sentences` 句的长文，把指定位置换成标记句。
fn manual(sentences: usize, marks: &[(usize, &str)]) -> String {
    (0..sentences)
        .map(|index| {
            marks
                .iter()
                .find(|(at, _)| *at == index)
                .map_or(FILLER, |(_, text)| *text)
        })
        .collect()
}

/// 按片段内容作答的提取模型：把片段里含指定标记的整句逐字摘出来。
#[derive(Default)]
struct Extractor {
    /// 额外附带一条原文里没有的引文。
    fabricate: bool,
    /// 第几次请求失败（从 1 数），0 = 不失败。
    fail_on: usize,
    /// 每次请求都失败。
    fail_all: bool,
    /// 第一次回复不是 JSON。
    malformed_first: bool,
    /// 第一次请求就报「已停止」。
    stopped: bool,
    calls: std::cell::Cell<usize>,
    prompts: std::cell::RefCell<Vec<String>>,
}

impl AiProvider for Extractor {
    fn generate_reply_with_tools(
        &self,
        messages: Vec<ContextMessage>,
        _: Option<&[ToolSpec]>,
    ) -> Result<AiReply> {
        self.calls.set(self.calls.get() + 1);
        if self.stopped {
            return Err(crate::storage::chat_runs::ChatRunStopped.into());
        }
        anyhow::ensure!(
            !self.fail_all && self.calls.get() != self.fail_on,
            "upstream unavailable"
        );
        if self.malformed_first && self.calls.get() == 1 {
            return Ok(AiReply::text("抱歉，我先说明一下思路。"));
        }
        let prompt = messages
            .iter()
            .find(|message| message.role == "user")
            .unwrap()
            .content
            .clone();
        let mut evidence = Vec::new();
        for (id, block) in prompt.split("<chunk id=\"").skip(1).enumerate() {
            let body = block.split_once(">\n").unwrap().1;
            let body = body.split("\n</chunk>").next().unwrap();
            for sentence in body.split_inclusive('。') {
                for (question, marker) in [(0, "限制甲"), (1, "数据乙"), (2, "反例丙")] {
                    if sentence.contains(marker) {
                        evidence.push(serde_json::json!({
                            "q":question,"chunk":id,"kind":"limitation",
                            "quote":sentence,"note":"与该维度相关"
                        }));
                    }
                }
            }
        }
        if self.fabricate {
            evidence.push(serde_json::json!({
                "q":0,"chunk":0,"kind":"fact",
                "quote":"这句话在任何片段里都不存在，是编造的。","note":"伪造"
            }));
        }
        self.prompts.borrow_mut().push(prompt);
        Ok(AiReply::text(
            serde_json::json!({"evidence":evidence}).to_string(),
        ))
    }
}

fn questions() -> Vec<String> {
    ["失败时怎么回滚", "重试次数的数据", "批量写入的反例"]
        .map(String::from)
        .to_vec()
}

fn run(
    store: &Store,
    provider: &Extractor,
    slugs: &[&str],
    budget: Option<(usize, &str)>,
) -> Result<serde_json::Value> {
    let conversation = store.create_conversation(Some("合成"), None)?;
    run_in(store, &conversation, provider, slugs, budget)
}

fn run_in(
    store: &Store,
    conversation: &str,
    provider: &Extractor,
    slugs: &[&str],
    budget: Option<(usize, &str)>,
) -> Result<serde_json::Value> {
    let result = synthesize(
        store,
        conversation,
        &Channel {
            provider,
            provider_name: "stub",
            result_budget: budget,
        },
        &Request {
            question: "写入失败、重试和批量写入分别有什么要求".into(),
            sub_questions: questions(),
            page_slugs: slugs.iter().map(|slug| slug.to_string()).collect(),
            queries: vec![],
        },
    )?;
    Ok(serde_json::from_str(&result)?)
}

#[test]
fn synthesis_returns_located_verbatim_evidence_and_keeps_source_text_out() {
    let db = Db::new();
    let s = &db.store;
    // 标记分别落在第 0 块、第 0/1 块的重叠区、第 2 块；第 3 块没有任何词项命中。
    let body = manual(2000, &[(100, LIMIT), (610, COUNTER), (1500, DATA)]);
    let page = source(s, "https://example.com/synthesis-manual", &body);
    let provider = Extractor {
        fabricate: true,
        ..Default::default()
    };
    let value = run(s, &provider, &[&page.slug], None).unwrap();

    // 每条证据都是页面原文，位置就是它在页内的字符区间。
    let evidence = value["evidence"].as_array().unwrap();
    let chars: Vec<char> = body.chars().collect();
    let mut found = Vec::new();
    for item in evidence {
        let (start, end) = (
            item["start_char"].as_u64().unwrap() as usize,
            item["end_char"].as_u64().unwrap() as usize,
        );
        let quote = item["quote"].as_str().unwrap();
        assert_eq!(chars[start..end].iter().collect::<String>(), quote);
        assert_eq!(item["page_slug"], page.slug.as_str());
        found.push((quote.to_string(), start, item["q"].clone()));
    }
    found.sort_by_key(|(_, start, _)| *start);
    // 重叠区里被两块各摘一次的那句只留一条。
    assert_eq!(
        found,
        vec![
            (LIMIT.to_string(), 1000, serde_json::json!([0])),
            (COUNTER.to_string(), 6100, serde_json::json!([2])),
            (DATA.to_string(), 15000, serde_json::json!([1])),
        ]
    );
    // 编造的引文（每次请求一条）被丢弃并计数。
    let coverage = &value["coverage"];
    assert_eq!(provider.calls.get(), 2);
    assert_eq!(coverage["requests"], 2);
    assert_eq!(coverage["rejected_quotes"], 2);
    // 显式指定的页通读：四块都读了，没有未读范围。
    assert_eq!(coverage["chunks_total"], 4);
    assert_eq!(coverage["chunks_read"], 4);
    assert_eq!(coverage["unread"], serde_json::json!([]));
    assert_eq!(
        value["knowledge_reading"][&page.slug],
        serde_json::json!({"coverage":"evidence","total_chars":20000,"read_ranges":[[0,20000]]})
    );
    // 有证据的页进引用白名单，但不带摘录；原文没有进入结果。
    assert_eq!(value["knowledge_candidates"][0]["page_slug"], page.slug.as_str());
    assert_eq!(value["knowledge_candidates"][0]["excerpt"], "");
    assert!(!value.to_string().contains(FILLER));
    // 提取模型确实读到了原文片段。
    assert!(provider.prompts.borrow()[0].contains(FILLER));
}

#[test]
fn synthesis_reads_only_matching_chunks_of_retrieved_pages_and_reports_the_rest() {
    let db = Db::new();
    let s = &db.store;
    let body = manual(2000, &[(100, LIMIT), (610, COUNTER), (1500, DATA)]);
    let relevant = source(s, "https://example.com/synthesis-relevant", &body);
    let unrelated = source(
        s,
        "https://example.com/synthesis-unrelated",
        &"今天散步时看到的风景。".repeat(300),
    );
    let provider = Extractor::default();
    let value = run(s, &provider, &[], None).unwrap();
    // 没指定页面时按子问题检索；检索到的页只读有词项命中的块。
    let coverage = &value["coverage"];
    assert_eq!(coverage["chunks_read"], 3);
    assert_eq!(
        coverage["unread"],
        serde_json::json!([{"page_slug":relevant.slug,"ranges":[[18300,20000]]}])
    );
    assert_eq!(value["evidence"].as_array().unwrap().len(), 3);
    assert!(!value.to_string().contains(&unrelated.slug));
    assert!(!provider.prompts.borrow().concat().contains("散步"));
}

#[test]
fn synthesis_keeps_partial_results_and_only_caches_complete_runs() {
    let db = Db::new();
    let s = &db.store;
    let body = manual(2000, &[(100, LIMIT), (610, COUNTER), (1500, DATA)]);
    let page = source(s, "https://example.com/synthesis-partial", &body);

    // 第二批失败：第一批的证据照常返回，失败的范围记为未读。
    let flaky = Extractor {
        fail_on: 2,
        ..Default::default()
    };
    let conversation = s.create_conversation(Some("合成"), None).unwrap();
    let value = run_in(s, &conversation, &flaky, &[&page.slug], None).unwrap();
    assert_eq!(value["evidence"].as_array().unwrap().len(), 3);
    assert_eq!(value["coverage"]["failed_batches"], 1);
    assert_eq!(
        value["coverage"]["unread"],
        serde_json::json!([{"page_slug":page.slug,"ranges":[[18300,20000]]}])
    );
    // 不完整的提取不缓存：同样的调用会重新请求。
    let healthy = Extractor::default();
    let again = run_in(s, &conversation, &healthy, &[&page.slug], None).unwrap();
    assert_eq!(healthy.calls.get(), 2);
    assert_eq!(again["coverage"]["failed_batches"], 0);
    // 完整的提取在同一会话里复用（换渠道重跑不重复付费）。
    let idle = Extractor::default();
    let cached = run_in(s, &conversation, &idle, &[&page.slug], None).unwrap();
    assert_eq!(idle.calls.get(), 0);
    assert_eq!(cached["evidence"], again["evidence"]);

    // 第一次回复不是 JSON：带着错误重试一次。
    let other = s.create_conversation(Some("合成"), None).unwrap();
    let confused = Extractor {
        malformed_first: true,
        ..Default::default()
    };
    let repaired = run_in(s, &other, &confused, &[&page.slug], None).unwrap();
    assert_eq!(confused.calls.get(), 3);
    assert_eq!(repaired["evidence"].as_array().unwrap().len(), 3);
    assert_eq!(repaired["coverage"]["failed_batches"], 0);
    // 修复重试也是一次模型请求，如实计数。
    assert_eq!(repaired["coverage"]["requests"], 3);

    // 每一批都失败：报错，而不是交回一份看似「没有相关证据」的空结果。
    let down = Extractor {
        fail_all: true,
        ..Default::default()
    };
    let error = run(s, &down, &[&page.slug], None).unwrap_err();
    assert!(error.to_string().contains("synthesis_extraction_failed"));
    assert_eq!(down.calls.get(), 2);

    // 运行被停止：立刻收手，不再发后面的批次。
    let halted = Extractor {
        stopped: true,
        ..Default::default()
    };
    assert!(run(s, &halted, &[&page.slug], None).is_err());
    assert_eq!(halted.calls.get(), 1);
}

#[test]
fn synthesis_fits_the_result_budget_and_skips_uncitable_pages() {
    let db = Db::new();
    let s = &db.store;
    // 80 条都与第一个子问题相关、文字各不相同的证据（整页只有一块）。
    let body: String = (0..600)
        .map(|index| {
            if index % 7 == 0 && index / 7 < 80 {
                format!("限制甲{:03}：要回滚。", index / 7)
            } else {
                FILLER.to_string()
            }
        })
        .collect();
    let page = source(s, "https://example.com/synthesis-budget", &body);
    let memo = s
        .create_wiki_page("memo", "私人记事", &"私人想法不外传。".repeat(200), &[])
        .unwrap();
    let cost = |value: &serde_json::Value| {
        crate::ai::budget::text_tokens(
            "fixture",
            &serde_json::Value::String(value.to_string()).to_string(),
        )
    };

    // 余量太小：一次子请求都不发。
    let provider = Extractor::default();
    let exhausted = run(s, &provider, &[&page.slug], Some((3900, "fixture")));
    assert!(
        exhausted
            .unwrap_err()
            .to_string()
            .contains("synthesis_budget_exhausted")
    );
    assert_eq!(provider.calls.get(), 0);

    // 余量有限：证据按预算截止，略去的条数如实报告。
    let budget = 4500;
    let value = run(s, &provider, &[&page.slug], Some((budget, "fixture"))).unwrap();
    let kept = value["evidence"].as_array().unwrap().len();
    let omitted = value["coverage"]["omitted"].as_u64().unwrap() as usize;
    assert!(kept > 0 && omitted > 0, "kept={kept} omitted={omitted}");
    assert_eq!(kept + omitted, 80);
    assert!(cost(&value) <= budget);

    // 不限预算时全部返回。
    let unlimited = Extractor::default();
    let all = run(s, &unlimited, &[&page.slug], None).unwrap();
    assert_eq!(all["evidence"].as_array().unwrap().len(), 80);
    assert_eq!(all["coverage"]["omitted"], 0);

    // 拿不到引用的页不读：记事本和不存在的页都列入 skipped，原文不送模型。
    let careful = Extractor::default();
    let value = run(
        s,
        &careful,
        &[&memo.slug, "missing/page", &page.slug],
        None,
    )
    .unwrap();
    assert_eq!(
        value["skipped"],
        serde_json::json!([
            {"page_slug":memo.slug,"reason":"not_citable"},
            {"page_slug":"missing/page","reason":"not_found"}
        ])
    );
    assert!(!careful.prompts.borrow().concat().contains("私人想法"));
    assert_eq!(value["evidence"].as_array().unwrap().len(), 80);
}

#[test]
fn synthesis_tool_needs_a_provider_and_its_pages_are_citable_in_the_text_protocol() {
    use crate::ai::tool::{ToolCall, ToolRegistry, ToolRuntime, dispatch, dispatch_with_knowledge};
    let db = Db::new();
    let s = &db.store;
    let page = source(
        s,
        "https://example.com/synthesis-tool",
        &manual(700, &[(100, LIMIT)]),
    );
    let conversation = s.create_conversation(Some("合成"), None).unwrap();
    let call = ToolCall::new(
        "synthesize_knowledge",
        serde_json::json!({
            "question":"写入失败有什么要求","sub_questions":["失败时怎么回滚"],
            "page_slugs":[page.slug]
        }),
    );
    // 只读、常驻：分发时不开写事务（子请求期间不占写锁），也不需要先启用。
    let registry = ToolRegistry::default();
    let tool = registry.get("synthesize_knowledge").unwrap();
    assert_eq!(tool.policy(), crate::ai::tool::ToolPolicy::Read);
    assert!(!tool.deferred());
    // 没有 provider 的场景（确认后执行、内部调用）明确报错。
    let missing = dispatch(&call, &registry, s, &conversation);
    assert!(!missing.success);
    assert!(missing.content.contains("synthesis_provider_unavailable"));

    let provider = Extractor::default();
    let result = dispatch_with_knowledge(
        &call,
        &ToolRegistry::default(),
        s,
        &conversation,
        &[],
        ToolRuntime {
            provider: Some((&provider, "stub")),
            result_budget: None,
        },
    );
    assert!(result.success, "{}", result.content);
    // 两种协议下，留下证据的页都能被后续回答引用。
    for message in [
        ContextMessage::tool_result("call-1".into(), result.content.clone()),
        ContextMessage::new(
            "system",
            format!("工具「synthesize_knowledge」执行结果：{}", result.content),
        ),
        ContextMessage::new(
            "system",
            format!(
                "Result of tool \"synthesize_knowledge\": {}",
                result.content
            ),
        ),
    ] {
        let candidates = candidates_in_context(&[message]);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].page_slug, page.slug);
    }
}
