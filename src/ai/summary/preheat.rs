//! 后台摘要预热：空闲时推进长会话摘要树，不与用户生成抢渠道。

use super::*;

/// P1 追赶修复（2026-10-19 摘要事故诊断，预案一 option b）：后台空闲时
/// 把长会话的摘要树向前推进一批，让「冷启动收敛」不再消耗用户的生成
/// 轮（诊断结论：340 条消息需 ~163 个节点，16 请求/轮 = 11 轮全程无
/// 可见收益）。返回本批落库的摘要节点数；0 表示本轮无事可做。
///
/// 触发判据：最近一次摘要事件所属的会话（无论成败——成功建出 mark 后
/// `apply_summary` 因边界 drain 自然退化为近尾稳态，额外调用无害）。
/// 前台有进行中的 run 时本轮完全跳过，绝不与用户生成并发抢渠道。
/// 供测试断言撞顶语义用（常量保持私有，避免被随意引用）。
pub(crate) const fn probe_max_requests() -> usize {
    MAX_REQUESTS
}

pub(crate) fn background_preheat(store: &Store) -> Result<i64> {
    // 会话：最近活跃的（生产事故会话即全库最近活跃）；小会话会在
    // apply_summary 的早退闸（user 轮不足 / selected 过小 / 未过触发
    // 线）直接返回，代价可忽略。
    let conversation: Option<String> = store
        .connection
        .query_row(
            "SELECT conversation_id FROM messages GROUP BY conversation_id
             ORDER BY MAX(created_at) DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()?;
    let Some(conversation) = conversation else {
        return Ok(0);
    };
    // 生产同参 provider/basis/model/budget；None（Ollama 主渠道或无可用
    // 配置）时静默跳过——预热是尽力而为的优化，不构成可报错路径。
    let Some((provider, name, basis, model, budget)) =
        crate::ai::conversation::primary_summary_provider(store)?
    else {
        return Ok(0);
    };
    // 渠道错配防线（P1 修复 c）：该会话最近一次真实生成用的渠道
    // （chat_run_requests.provider_name 是事实记录）必须与预热渠道同名，
    // 否则 provider_basis 错位、建出的节点生产命不中——白烧。无生成
    // 历史的会话无从判定，同样跳过（刚建的会话也不急着预热）。
    if !preheat_channel_matches(store, &conversation, &name)? {
        return Ok(0);
    }
    background_preheat_with(
        store,
        &conversation,
        provider.as_ref(),
        &name,
        &basis,
        &model,
        budget,
    )
}

/// 渠道匹配判定：独立成函数让测试可以注入历史 run 请求。
pub(crate) fn preheat_channel_matches(
    store: &Store,
    conversation: &str,
    provider_name: &str,
) -> Result<bool> {
    let last: Option<String> = store
        .connection
        .query_row(
            "SELECT req.provider_name FROM chat_run_requests req
             JOIN chat_runs r ON r.id=req.run_id
             WHERE r.conversation_id=?1
             ORDER BY req.started_at DESC, req.id DESC LIMIT 1",
            [conversation],
            |r| r.get(0),
        )
        .optional()?;
    Ok(last.as_deref() == Some(provider_name))
}

/// P1 修复(c) 的实现体：单 tick 内连续多批推进，直到根节点建成
/// （mark 落地后 apply_summary 走早退/稳态）、 tick 请求预算耗尽、
/// 批零进展（结构性失败，续批只会重复白烧）或前台开始生成。
/// 返回推进的摘要节点数。
pub(crate) fn background_preheat_with(
    store: &Store,
    conversation: &str,
    provider: &dyn AiProvider,
    name: &str,
    basis: &str,
    model: &str,
    budget: usize,
) -> Result<i64> {
    // 三个预算数当成一组调：MAX_REQUESTS（批内 64）×~2 ≈
    // MAX_TICK_REQUESTS（tick 128）保证冷启动树 2 批建完；6 是批数防御
    // 上限。三数一致由 pct 语义保证，单独动任何一个都会改变熔断行为。
    // 已知饿隙：预算检查在批前（不在批中），单批超发最多一个
    // MAX_REQUESTS——当前 64/128 最坏 192 请求/tick，可接受；若
    // MAX_REQUESTS 将来调大需重估。
    const MAX_TICK_REQUESTS: usize = 128;
    const MAX_BATCHES: usize = 6;
    let tick_started = chrono::Utc::now().to_rfc3339();
    let mut tick_requests = 0usize;
    let mut nodes_before_last: i64 =
        store
            .connection
            .query_row("SELECT COUNT(*) FROM chat_summary_nodes", [], |r| r.get(0))?;
    let mut nodes_total = 0i64;
    let mut failure_detail: Option<String> = None;
    for _ in 0..MAX_BATCHES {
        // 批间重查前台并发：用户开始生成即停（窗口内已并发烧的部分由
        // INSERT OR IGNORE 幂等兜底，浪费有限）。
        let active: bool = store.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM chat_runs WHERE conversation_id=?1 AND status IN ('pending','running'))",
            [conversation], |r| r.get(0))?;
        if active {
            failure_detail = Some("前台生成开始，预热让路".into());
            break;
        }
        if tick_requests >= MAX_TICK_REQUESTS {
            break;
        }
        let before = nodes_before_last;
        let mut context = crate::ai::memory::prepare_uncompressed_context(conversation, store)?;
        // 失败细节由 apply_summary 内部留痕（P0-a）；背景侧只计量。
        let progress = apply_summary(
            store,
            conversation,
            provider,
            name,
            basis,
            model,
            budget,
            &mut context,
        )
        .unwrap_or(SummaryProgress {
            requests: 0,
            cache_hits: 0,
            failed: true,
        });
        tick_requests += progress.requests;
        let after: i64 =
            store
                .connection
                .query_row("SELECT COUNT(*) FROM chat_summary_nodes", [], |r| r.get(0))?;
        let delta = (after - before).max(0);
        nodes_total += delta;
        nodes_before_last = after;
        if progress.failed && delta == 0 {
            // 本批零节点且失败：结构性错误（如「没有可摘要的历史」），
            // 继续只重复烧——停并记账。
            failure_detail = Some(format!(
                "批次零进展且失败（requests={} cache_hits={}，详见 summary_unavailable 事件）",
                progress.requests, progress.cache_hits
            ));
            break;
        }
        if progress.requests == 0 && delta == 0 {
            // 全缓存命中且树建成（早退或 mark 后稳态）——完成，正常出口。
            break;
        }
        if !progress.failed {
            // 树建成（mark 已落）：确认性早退批次即视为收敛。
            if progress.cache_hits > 0 && delta == 0 {
                break;
            }
        }
    }
    background_preheat_ledger(
        store,
        conversation,
        tick_started,
        nodes_total,
        failure_detail.as_deref(),
    )?;
    Ok(nodes_total)
}

/// 预热账本：复用 knowledge_background_runs（task='summary-preheat'），
/// 运行侧栏/报告可见，独立于知识任务计数（此前 tick 汇总混淆——
/// P1 诊断缺陷③/④）。
fn background_preheat_ledger(
    store: &Store,
    conversation: &str,
    started_at: String,
    nodes: i64,
    failure: Option<&str>,
) -> Result<()> {
    let id = uuid::Uuid::new_v4().to_string();
    store.connection.execute(
        "INSERT INTO knowledge_background_runs(id,task,input_key,status,started_at,finished_at,error,result_count)
         VALUES (?1,'summary-preheat',?2,?3,?4,?4,?5,?6)",
        rusqlite::params![
            id,
            conversation,
            if failure.is_some() { "failed" } else { "succeeded" },
            started_at,
            failure,
            nodes
        ],
    )?;
    Ok(())
}
