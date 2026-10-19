use super::super::*;
use super::add_history;
use super::context;
use super::fixture;
use super::good_reply;
use super::*;

// ---------------------------------------------------------------------------
// P1 诊断（2026-10-19 摘要事故预案一）：离线重放生产库上的摘要失败。
// 错误是确定性的（归并根节点建不出来），不需要等真实流量——用 P0-a 的
// detail 留痕直接拿真实错误。运行方式：
//   cp ~/.local/share/elsewhen/elsewhen.db /tmp/p1-replay.db
//   P1_REPLAY_DB=/tmp/p1-replay.db cargo test --lib p1_replay -- --ignored --nocapture
// 会真实调用 provider（小请求、最多 16 次：上限与生产一致），用于复现根因。
#[test]
#[ignore = "P1 诊断：真实流量复现，需 P1_REPLAY_DB 指向带真实 key 的库副本"]
fn p1_replay_production_summary_failure() {
    let db_path = std::env::var("P1_REPLAY_DB")
        .expect("请设置 P1_REPLAY_DB 指向库副本（cp 主库到 /tmp 后传入）");
    let store = match Store::open(std::path::Path::new(&db_path)) {
        Ok(store) => store,
        Err(error) => panic!("打开副本库失败：{error:#}"),
    };
    let providers = store.list_ai_provider_configs_for_runtime().unwrap();
    let config = providers
        .first()
        .cloned()
        .expect("副本库里没有可用的 provider 配置");
    println!(
        "provider：{}（{}，窗口 {:?}）",
        config.name, config.model, config.context_window
    );
    // 会话取消息最多的（2026-10-19 出事会话即全库最大）。
    let (conversation, message_count) = store
        .connection
        .query_row(
            "SELECT id, (SELECT COUNT(*) FROM messages WHERE conversation_id=conversations.id) AS n
             FROM conversations ORDER BY n DESC LIMIT 1",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .unwrap();
    println!("出事会话：{conversation}（{message_count} 条消息）");

    // 生产同款装配：无延展 prompt 的纯摘要前置（provider 实例化照抄
    // generate_with_provider_configs 的 OpenAI-compatible 分支）。
    let provider_config = crate::ai::provider::OpenAiCompatibleConfig {
        base_url: config.base_url.clone(),
        api_key: config.api_key.clone(),
        model: config.model.clone(),
        temperature: config.temperature as f32,
        max_tokens: config.max_tokens.map(|v| v as u32),
        context_window: config.context_window.map(|v| v as u32),
    };
    let provider = crate::ai::provider::OpenAiCompatibleProvider::new(provider_config).unwrap();
    let mut context =
        crate::ai::memory::prepare_uncompressed_context(&conversation, &store).unwrap();
    let window = config.context_window.unwrap_or(200_000) as usize;
    let budget = window.saturating_sub(40_000).max(1);
    let error = super::apply_summary(
        &store,
        &conversation,
        &provider,
        &config.name,
        &format!("{}|{}", config.base_url, config.model),
        &config.model,
        budget,
        &mut context,
    )
    .err()
    .expect("摘要竟然成功了——根因已被后续修复或环境差异，把输出贴回事故档案");
    println!("真实失败链：{error:#}");
    panic!("P1 复现成功，见上一行");
}

// ---------------------------------------------------------------------------
// 零成本机理实验（P1 配套）：mock provider 下模拟「用户连续聊天、摘要管
// 线逐轮追赶」的动力学，判定 340 条消息会话「根节点永远建不出来」的根因
// 属于哪一类：
//   (a) MAX_REQUESTS=16 单轮配额太小 → 纯粹收敛慢，轮间 cache_hits 线性
//       增长、节点无废料；
//   (b) 分组边界漂移 → 缓存命中率回落或多代 leaf 废料堆积。
// 运行：cargo test --lib p1_chase -- --ignored --nocapture
#[test]
#[ignore = "P1 机理实验：观察逐轮追赶动力学"]
fn p1_chase_dynamics_experiment() {
    let (_db, store, conversation) = fixture();
    let body = serde_json::to_string(&json!({
        "summary": "mock 摘要文本，覆盖本轮输入的主要事实、决定与未解决问题。",
        "coverage_notes": "零成本实验",
        "open_questions": []
    }))
    .unwrap();
    let provider = FakeProvider::new(AiReply {
        content: body,
        tool_calls: Vec::new(),
        model: Some("mock".into()),
        usage: None,
        reasoning_content: None,
        finish_reason: None,
    });
    let pad = "对话与决定占位：血糖记录复盘、饮食调整与随访安排，细节保持稳定。".repeat(4);
    let rounds = 40;
    for round in 0..rounds {
        store
            .send_message(
                &conversation,
                "user",
                &format!("第{round}轮提问：{pad}"),
                None,
            )
            .unwrap();
        store
            .send_message(
                &conversation,
                "assistant",
                &format!("第{round}轮回答：{pad}"),
                None,
            )
            .unwrap();
        let mut context =
            crate::ai::memory::prepare_uncompressed_context(&conversation, &store).unwrap();
        let before = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM chat_summary_nodes WHERE conversation_id=?1",
                [&conversation],
                |r| r.get::<_, i64>(0),
            )
            .unwrap();
        let provider_calls_before = provider.requests.get();
        let result = super::apply_summary(
            &store,
            &conversation,
            &provider,
            "mock",
            "mock-basis",
            "mock",
            8_000,
            &mut context,
        );
        let after = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM chat_summary_nodes WHERE conversation_id=?1",
                [&conversation],
                |r| r.get::<_, i64>(0),
            )
            .unwrap();
        if round < 6 || round == 39 {
            println!(
                "轮 {round:>2}：摘要调用 {}，节点 {before:>3}→{after:>3}，结果 {}",
                provider.requests.get() - provider_calls_before,
                if result.is_ok() { "OK" } else { "ERR" },
            );
        }
        if round == 39 {
            let (hits, reqs) = store
                .connection
                .query_row(
                    "SELECT payload_json FROM chat_run_events WHERE kind='summary_unavailable'
                     ORDER BY created_at DESC LIMIT 1",
                    [],
                    |r| r.get::<_, String>(0),
                )
                .map(|payload| {
                    let value: serde_json::Value = serde_json::from_str(&payload).unwrap();
                    (
                        value["cache_hits"].as_i64().unwrap_or_default(),
                        value["requests"].as_i64().unwrap_or_default(),
                    )
                })
                .unwrap_or((0, 0));
            println!("最后一条 summary_unavailable：cache_hits={hits} requests={reqs}");
            let total: i64 = store
                .connection
                .query_row("SELECT COUNT(*) FROM chat_summary_nodes", [], |r| r.get(0))
                .unwrap();
            println!("全库节点总数：{total}");
        }
    }
}
