//! 工具调用 agent 循环：多轮调用/协议修补（实体平移）。
use super::continuity::{FINAL_ANSWER_NUDGE, FollowUp, progress_fallback, unhelpful_failure_reply};
use super::*;
use crate::ai::provider::{AiProvider, AiReply};
use crate::ai::tool::{ToolPolicy, ToolRegistry, ToolResultMsg};
use crate::storage::Store;
use anyhow::Result;

/// Agent 循环结果
pub(crate) struct AgentOutcome {
    pub(crate) content: String,
    pub(crate) prompt_tokens: u64,
    pub(crate) completion_tokens: u64,
    pub(crate) model: Option<String>,
    /// 诊断：本次循环实际消耗的轮数（1..=MAX_TOOL_ROUNDS），供空回复兜底文案说明重试过几次。
    pub(crate) rounds_used: usize,
    /// 诊断：因上游返回空 content 而额外重发的次数。
    pub(crate) empty_retries: usize,
}
/// Agent 循环：最多 MAX_TOOL_ROUNDS 轮。
/// - 首轮尝试原生 tool-calling（带 tools 清单）；失败则回落文本协议重试一次。
/// - 原生返回 tool_calls → 回传 assistant(tool_calls) + 追加 tool 结果，进入下一轮。
/// - 纯文本但含可解析调用 → 执行并进入下一轮。
/// - 疑似想调但格式未知 → 回炉重发（最多 MAX_PROTOCOL_REPAIRS 次），绝不静默吞掉。
/// - 无工具调用 → 写声明核验（标题→查库→本轮时间窗）→ 收敛，返回最终文本。
/// Provider failover never removes an already committed event. Fixed input
/// identities and durable results make retries idempotent instead of destructive.
pub(crate) fn run_agent_loop(
    provider: &dyn AiProvider,
    provider_name: &str,
    context: &mut Vec<ContextMessage>,
    registry: &ToolRegistry,
    store: &Store,
    conversation_id: &str,
    turn_start: chrono::DateTime<chrono::Utc>,
) -> Result<AgentOutcome> {
    run_agent_loop_inner(
        provider,
        provider_name,
        context,
        registry,
        store,
        conversation_id,
        turn_start,
    )
}
pub(crate) fn assistant_continuation(reply: AiReply) -> ContextMessage {
    let mut message = ContextMessage::new("assistant", reply.content);
    message.reasoning_content = reply.reasoning_content;
    message
}

/// 外网搜索意图接线（激活 `web_search::intent` 的启发式）：仅当**本轮最新
/// 用户文本**命中搜索信号时，push 一条 system 提示；其余情况零干扰——纪律
/// 仍是「不自行断定用户要搜」。
///
/// 提示按信号强度取两档：
/// * `Direct`（搜一下/网上查…）：强提示。未启用 → 指向「设置 → 外网搜索」
///   且明确要求不装作已搜、不编造；已启用但延迟未登记 → 指向 enable_tools；
///   已启用且常驻注入（defer_tools=false 旧协议）→ 模型本可直接调，不注入。
/// * 仅 `Timely`（最新/今天/新闻…）：弱提示——「整理今天的事件」这类本地
///   时间任务同样命中这些词，只提示先判本地/外网，不得断言要联网。
///
/// 注入点在进入多轮循环之前，每轮 agent 循环只注一次（failover 重试各带
/// 各的 attempt_context，克隆自同一基底，不会重复叠加）。
pub(crate) fn note_web_search_intent(
    context: &mut Vec<ContextMessage>,
    registry: &ToolRegistry,
    store: &Store,
) {
    let Some(last_user) = context.iter().rev().find(|m| m.role == "user") else {
        return;
    };
    let Some(signal) = crate::web_search::search_signal(&last_user.content) else {
        return;
    };
    use crate::web_search::SearchSignal;
    let note = match signal {
        SearchSignal::Direct => match store.web_search_config() {
            Ok(cfg) if !cfg.enabled => Some(
                "用户本轮输入显式要求外网搜索，但外网搜索尚未启用。请如实告知：需在「设置 → 外网搜索」开启并配置引擎（SearxNG 免 API key）后才能联网搜；不要装作已搜索，更不要编造搜索结果。"
                    .to_string(),
            ),
            Ok(cfg) if cfg.enabled && registry.is_deferred("web_search") && registry.defer_tools() => Some(
                "用户本轮输入显式要求外网搜索。外网搜索已启用但 web_search 工具尚未登记：请先调用 enable_tools（skills 传入 [\"web_search\"]），下一轮即可调用 web_search 获取结果，并按来源如实引用。"
                    .to_string(),
            ),
            // 已启用且非延迟协议：模型本可直接调用，无需额外提示。
            _ => None,
        },
        SearchSignal::Timely => Some(
            "用户文本含时效词（如最新/今天），先判断这是查本地记录任务还是需要外网最新信息：本地内容（事件/待办/对话/知识页）优先用本地工具；仅当确需外网信息时才使用 web_search（未启用会得到如实说明，届时向用户传达设置路径），不要为本地任务联网，也不要编造检索结果。"
                .to_string(),
        ),
    };
    if let Some(note) = note {
        debug_eprintln!("[agent] web_search 意图命中（{signal:?}）→ 注入前置提示");
        context.push(ContextMessage::new("system", note));
    }
}
pub(crate) fn run_agent_loop_inner(
    provider: &dyn AiProvider,
    provider_name: &str,
    context: &mut Vec<ContextMessage>,
    registry: &ToolRegistry,
    store: &Store,
    conversation_id: &str,
    turn_start: chrono::DateTime<chrono::Utc>,
) -> Result<AgentOutcome> {
    let conversation = store.get_conversation(conversation_id)?;
    let is_knowledge_mentor = conversation.as_ref().is_some_and(|conversation| {
        conversation.assistant_mode == "knowledge_mentor" || conversation.wiki_page_slug.is_some()
    });
    let input_already_recorded = store
        .latest_event_id_for_conversation(conversation_id)?
        .is_some();
    let followup = FollowUp::load(store, conversation_id)?;
    let has_pending = !store
        .pending_actions_for_conversation(conversation_id)?
        .is_empty();
    let status_only = followup
        .as_ref()
        .is_some_and(|f| !f.unresolved || has_pending);
    let verification_start = followup
        .as_ref()
        .and_then(|f| f.request_started_at)
        .unwrap_or(turn_start);
    let allow_record_event = !is_knowledge_mentor && !input_already_recorded && followup.is_none();
    let mut protocol: Option<ToolProtocol> = None;
    let mut content = String::new();
    let mut prompt_tokens: u64 = 0;
    let mut completion_tokens: u64 = 0;
    let mut model: Option<String> = None;
    let mut results: Vec<crate::ai::tool::ToolResultMsg> = Vec::new();
    // 延迟加载：本轮 agent 循环内已启用完整 schema 的工具名（enable_tools 登记）。
    let mut enabled_tools: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut must_finalize = false;
    // 协议修复预算：疑似调用但未知格式时的回炉次数（与轮数上限独立，双保险防死循环）
    let mut repairs_used = 0usize;
    // 2026-10-08：写声明核验不再要求「用户先说写意图」前置，
    // writes_requested 变量与 user_requested_write 函数随之删除。
    let mut claim_repaired = false;
    // 收敛前接地重发的一次性闸门（防 finalizing 空转，见收敛段注释）。
    let mut grounding_retried = false;
    // 第二次空头预告直接进入最终答复阶段；不能因修复预算用完而放行预告。
    let mut promise_repaired = false;
    // 空回复重发：上游返回空 content 时消耗一轮预算重试，而不是把空串当答案收敛。
    let mut empty_retries_used = 0usize;
    let mut rounds_used = 0usize;
    let tool_names = registry.all_names();
    note_web_search_intent(context, registry, store);

    for _round in 0..MAX_TOOL_ROUNDS {
        super::generation::check(store)?;
        if context.iter().any(|m| m.sampled_time.is_some()) {
            super::time_context::refresh_current_time(context, &super::time_context::SystemClock);
        }
        rounds_used = _round + 1;
        let finalizing = must_finalize || _round == MAX_TOOL_ROUNDS - 1;
        if finalizing {
            context.push(ContextMessage::new("system", FINAL_ANSWER_NUDGE));
            // 接地注入（治本）：把本轮行为事实（工具成败 + 库内新增）摆进上下文，
            // 模型看着清单复述，无需靠词表猜它是否撒谎。
            context.push(ContextMessage::new(
                "system",
                turn_grounding(
                    store,
                    registry,
                    conversation_id,
                    &results,
                    verification_start,
                )?,
            ));
        }
        if protocol == Some(ToolProtocol::Text) {
            text_protocol_history(context);
        }
        // The provider checks the full serialized request, including tools, below.
        let wants_native = match &protocol {
            Some(ToolProtocol::Native) => true,
            Some(ToolProtocol::Text) => false,
            None => true,
        };
        let tools = if wants_native && !finalizing {
            let mut specs = registry.provider_specs_partitioned(allow_record_event, &enabled_tools);
            if let Some(meta) = registry.meta_spec() {
                specs.push(meta);
            }
            if status_only {
                specs.retain(|spec| {
                    registry
                        .get(&spec.name)
                        .is_some_and(|t| t.policy() == ToolPolicy::Read)
                });
            }
            Some(specs)
        } else {
            None
        };

        let mut request_context = context.clone();
        if protocol == Some(ToolProtocol::Text) && !finalizing {
            let mut meta_specs =
                registry.provider_specs_partitioned(allow_record_event, &enabled_tools);
            if let Some(meta) = registry.meta_spec() {
                meta_specs.push(meta);
            }
            let specs: Vec<_> = meta_specs
                .into_iter()
                .filter(|spec| {
                    !status_only
                        || registry
                            .get(&spec.name)
                            .is_some_and(|t| t.policy() == ToolPolicy::Read)
                })
                .collect();
            request_context.push(ContextMessage::new(
                "system",
                ToolRegistry::prompt_for_specs(&specs),
            ));
        }
        // Only a completed tool exchange creates a steering boundary. Inputs
        // arriving during a plain answer remain queued, never silently appended.
        if _round > 0 && !results.is_empty() {
            if let Some(supplement) = super::supplements::inject_next(
                store,
                conversation_id,
                provider,
                &request_context,
                tools.as_deref(),
            )? {
                request_context.push(supplement.clone());
                context.push(supplement);
            }
        }
        let reply = match super::generation::request(
            store,
            provider,
            provider_name,
            request_context,
            tools.as_deref(),
        ) {
            Ok(r) => r,
            Err(e) => {
                super::generation::check(store)?;
                // Local budget failures are not evidence of protocol incompatibility.
                if e.downcast_ref::<super::budget::RequestBudgetError>()
                    .is_some()
                    || e.downcast_ref::<super::provider::ObservedRequestFailure>()
                        .is_some()
                {
                    return Err(e);
                }
                if finalizing {
                    debug_eprintln!(
                        "[agent] provider「{provider_name}」final response failed, report known progress: {e}"
                    );
                    break;
                }
                // 网关/上游暂时失败时不要把同一上下文改成纯文本协议；
                // 直接交给外层 provider 轮换，避免重复请求和协议状态污染。
                if is_transient_provider_error(&e) {
                    debug_eprintln!(
                        "[agent] provider「{provider_name}」上游暂时失败，交给下一个 provider：{e:#}"
                    );
                    return Err(e);
                }
                // 原生协议失败（首次尝试 or 已锁定原生）→ 回落到纯文本协议重试一次：
                // 首轮可能是模型/网关不支持 tools 字段；后续轮可能是原生 tool_calls
                // 回传校验失败（如 arguments 字节不一致）。历史里已有上下文，文本模式仍能组织最终回答。
                if !matches!(protocol, Some(ToolProtocol::Text)) {
                    protocol = Some(ToolProtocol::Text);
                    context.push(ContextMessage::new("system", TEXT_PROTOCOL_NUDGE));
                    debug_eprintln!(
                        "[agent] provider「{provider_name}」round={_round} 请求失败({e:#})，下一轮使用文本工具协议"
                    );
                    // 回退前小退避：原生协议刚软失败，紧随的文本重试可能命中
                    // 同一瞬限流；仅首次硬失败后触发，成功路径零开销（P2 backoff）。
                    super::generation::pause(
                        store,
                        std::time::Duration::from_millis(PROTOCOL_FALLBACK_SLEEP_MS),
                    )?;
                    continue;
                } else {
                    debug_eprintln!(
                        "[agent] provider「{provider_name}」纯文本协议请求也失败：{e:#}"
                    );
                    return Err(e);
                }
            }
        };
        // Account and diagnose every HTTP response before choosing a retry.
        if let Some(u) = &reply.usage {
            prompt_tokens += u.prompt_tokens;
            completion_tokens += u.completion_tokens;
        }
        if reply.model.is_some() {
            model = reply.model.clone();
        }
        debug_eprintln!(
            "[agent] provider「{provider_name}」round={_round} protocol={:?} offered_tools={} finalizing={finalizing} content_len={} reasoning_len={} tool_calls={} finish_reason={:?} model={:?} prompt_tokens={} completion_tokens={}",
            protocol,
            tools.as_ref().map_or(0, Vec::len),
            reply.content.chars().count(),
            reply
                .reasoning_content
                .as_ref()
                .map_or(0, |r| r.chars().count()),
            reply.tool_calls.len(),
            reply.finish_reason,
            reply.model,
            reply.usage.as_ref().map_or(0, |u| u.prompt_tokens),
            reply.usage.as_ref().map_or(0, |u| u.completion_tokens)
        );
        if missing_tool_call_response(&reply) {
            debug_eprintln!(
                "[agent] round={_round} 响应声明调用工具但缺少调用内容，本次未执行工具"
            );
            if finalizing {
                break;
            }
            if protocol != Some(ToolProtocol::Text) {
                protocol = Some(ToolProtocol::Text);
                context.push(ContextMessage::new("system", TEXT_PROTOCOL_NUDGE));
                debug_eprintln!("[agent] 下一轮切换文本工具协议，本次循环不再启用原生 tools");
            } else {
                must_finalize = true;
                debug_eprintln!("[agent] 文本工具协议仍返回不完整响应，下一轮只报告实际结果");
            }
            continue;
        }

        // 原生 tool-calls：回传 assistant(tool_calls) + 工具结果
        if !reply.tool_calls.is_empty() {
            // Even a model ignoring the finalization instruction cannot start
            // another action, through either protocol.
            if finalizing {
                break;
            }
            if protocol != Some(ToolProtocol::Text) {
                protocol = Some(ToolProtocol::Native);
            }
            context.push(ContextMessage::assistant_with_tool_calls(
                reply.content.clone(),
                reply.tool_calls.clone(),
                reply.reasoning_content.clone(),
            ));
            for call in &reply.tool_calls {
                if let Some(body) = call.arguments.get("content_md").and_then(|v| v.as_str()) {
                    crate::knowledge::workflows::record_generation(
                        store,
                        conversation_id,
                        body,
                        reply.model.as_deref(),
                    )?;
                }
                if call.name == "enable_tools" {
                    let names: Vec<String> = call
                        .arguments
                        .get("skills")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|x| x.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();
                    let text = registry.enable_result(&names, &mut enabled_tools);
                    context.push(ContextMessage::tool_result(call.id.clone(), text.clone()));
                    results.push(ToolResultMsg::ok(call, text));
                    continue;
                }
                // R2A 守卫：MCP 工具默认延迟加载，未启用（enable_tools 登记）
                // 不执行，回错指路（与内置延迟工具对称）。
                if call.name.starts_with(super::mcp::MCP_PREFIX)
                    && !enabled_tools.contains(&call.name)
                {
                    let text = format!(
                        "MCP 工具 {} 未启用（第三方工具默认延迟加载）。请先调用 enable_tools，参数 skills 传入 [\"{}\"] 登记后再调用。",
                        call.name, call.name
                    );
                    context.push(ContextMessage::tool_result(call.id.clone(), text.clone()));
                    results.push(ToolResultMsg::err(call, text));
                    continue;
                }
                let result = dispatch_for_turn(
                    call,
                    registry,
                    store,
                    conversation_id,
                    status_only,
                    allow_record_event,
                    &crate::knowledge::candidates_in_context(context),
                    &enabled_tools,
                )?;
                debug_eprintln!(
                    "[agent] dispatch tool={} -> {}",
                    call.name,
                    &result.content.chars().take(80).collect::<String>()
                );
                context.push(ContextMessage::tool_result(
                    call.id.clone(),
                    result.content.clone(),
                ));
                results.push(result);
            }
            continue;
        }

        // 文本信封：assistant 原文 + 工具结果（走 system 角色，避免原生协议对 tool 角色的约束）
        let (clean, calls) = parse_tool_call_envelope(&reply.content);
        if !calls.is_empty() {
            if finalizing {
                break;
            }
            protocol = Some(ToolProtocol::Text);
            let reply_model = reply.model.clone();
            context.push(assistant_continuation(reply));
            for call in &calls {
                if let Some(body) = call.arguments.get("content_md").and_then(|v| v.as_str()) {
                    crate::knowledge::workflows::record_generation(
                        store,
                        conversation_id,
                        body,
                        reply_model.as_deref(),
                    )?;
                }
                if call.name == "enable_tools" {
                    let names: Vec<String> = call
                        .arguments
                        .get("skills")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|x| x.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();
                    let text = registry.enable_result(&names, &mut enabled_tools);
                    context.push(ContextMessage::new(
                        "system",
                        format!("工具「enable_tools」执行结果：{}", text),
                    ));
                    results.push(ToolResultMsg::ok(&call, text));
                    continue;
                }
                // R2A 守卫（文本信封路径，同上）。
                if call.name.starts_with(super::mcp::MCP_PREFIX)
                    && !enabled_tools.contains(&call.name)
                {
                    let text = format!(
                        "MCP 工具 {} 未启用（第三方工具默认延迟加载）。请先调用 enable_tools，参数 skills 传入 [\"{}\"] 登记后再调用。",
                        call.name, call.name
                    );
                    context.push(ContextMessage::new(
                        "system",
                        format!("工具「{}」执行结果：{}", call.name, text),
                    ));
                    results.push(ToolResultMsg::err(&call, text));
                    continue;
                }
                let result = dispatch_for_turn(
                    &call,
                    registry,
                    store,
                    conversation_id,
                    status_only,
                    allow_record_event,
                    &crate::knowledge::candidates_in_context(context),
                    &enabled_tools,
                )?;
                debug_eprintln!(
                    "[agent] text-protocol tool={} -> {}",
                    call.name,
                    &result.content.chars().take(80).collect::<String>()
                );
                context.push(ContextMessage::new(
                    "system",
                    format!("工具「{}」执行结果：{}", call.name, result.content),
                ));
                results.push(result);
            }
            continue;
        }

        // 疑似想调工具但严格解析器没认出来 → 回炉重发，而不是静默吞掉。
        // 原则1：该调的必须调。误报最多浪费一轮（有次数上限），漏报则工具永远不执行。
        if tool_call_attempt_detected(&reply.content, &tool_names) {
            if finalizing || repairs_used >= MAX_PROTOCOL_REPAIRS {
                break;
            }
            repairs_used += 1;
            debug_eprintln!("[agent] 疑似工具调用但无法解析，回炉重发 (repair {repairs_used})");
            context.push(assistant_continuation(reply));
            context.push(ContextMessage::new("system", PROTOCOL_REPAIR_NUDGE));
            continue;
        }

        // 写声明核验（辅助层）：回复命中声明词 → 查库验真。治本靠 finalizing 的
        // 接地注入（turn_grounding）；这层只兜「一轮收敛且措辞命中词表」的裸谎。
        // 教学/引用语境（无标题 + 零工具调用 + 无待办）不走回炉，减少误报。
        if write_claim_detected(&clean) {
            let titles = extract_claimed_titles(&clean);
            match verify_claimed_writes(store, conversation_id, &titles, verification_start)? {
                ClaimVerdict::Clean => {}
                verdict => {
                    if finalizing {
                        break;
                    }
                    must_finalize = claim_repaired;
                    claim_repaired = true;
                    debug_eprintln!("[agent] 写声明与库内事实不符，回炉纠正 ({verdict:?})");
                    context.push(assistant_continuation(reply));
                    context.push(ContextMessage::new(
                        "system",
                        write_claim_repair_nudge(store, conversation_id, &verdict)?,
                    ));
                    continue;
                }
            }
        }

        // 承诺回炉：只预告「接下来要做」就收敛 = 用户等一场空。逼它现在动手或直接给结果。
        // 与写声明核验同源不同向：那边抓「谎报已做完」，这边抓「预告还没做」。
        if future_promise_detected(&clean) {
            if finalizing {
                break;
            }
            must_finalize = promise_repaired;
            promise_repaired = true;
            debug_eprintln!("[agent] 只预告未动手，回炉逼它执行 (repair)");
            context.push(assistant_continuation(reply));
            context.push(ContextMessage::new("system", PROMISE_REPAIR_NUDGE));
            continue;
        }

        // 空回复重发：上游给了空 content（200 但无正文，常见于推理模型烧光输出预算）。
        // 原来这里直接 break，空串被当成答案收敛，用户只看到一句无信息兜底。
        if clean.trim().is_empty() || unhelpful_failure_reply(&clean) {
            if finalizing {
                break;
            }
            must_finalize = empty_retries_used >= MAX_EMPTY_RETRIES;
            if !must_finalize {
                empty_retries_used += 1;
            }
            debug_eprintln!(
                "[agent] empty reply; retry={empty_retries_used} finalize_next={must_finalize}"
            );
            context.push(ContextMessage::new("system", EMPTY_REPLY_NUDGE));
            continue;
        }

        // 收敛前最后一道接地（治本版）：判定依据从「AI 说了什么」换成
        // 「用户要什么 + AI 做了什么」——用户明确要求写 + 本轮零工具调用 = 强制重发，
        // 不看 AI 回复措辞。2026-10-08 kimi 模式：用户说「保存为topic」，AI 贴出正文
        // 说「现在重新提交」但 tool_calls=0——它把「展示内容」当成了「执行保存」，
        // 任何词表都拦不住这种认知错误，只能从行为（零工具）+ 意图（用户要写）判定。
        // finalizing 轮不重发：轮首已注入过接地清单，再 continue 会因条件反复命中空转
        // 到轮数耗尽。grounding_retried 一次性闸门：重发后仍不动手就放行，事实留在上下文。
        let user_wants_write = context
            .iter()
            .rev()
            .find(|m| m.role == "user")
            .is_some_and(|m| user_text_wants_write(&m.content));
        if !finalizing && !grounding_retried && results.is_empty() && user_wants_write {
            grounding_retried = true;
            debug_eprintln!("[agent] 用户要求写但本轮零工具调用，强制接地重发");
            context.push(assistant_continuation(reply));
            context.push(ContextMessage::new(
                "system",
                format!(
                    "{}\n用户明确要求你执行写操作（保存/提交/整理），但你本轮没有调用任何工具。\
                     把内容贴给用户看不等于执行——必须调用对应工具（如 save_knowledge_draft），\
                     或明确告诉用户你无法执行及原因。",
                    turn_grounding(
                        store,
                        registry,
                        conversation_id,
                        &results,
                        verification_start
                    )?
                ),
            ));
            must_finalize = true;
            continue;
        }

        // 收敛
        content = clean;
        break;
    }

    if content.trim().is_empty() {
        content = progress_fallback(
            store,
            conversation_id,
            context,
            &results,
            Some(
                "连续几轮都没有产出可用内容，本轮到此为止；你的问题还挂着，说「继续」或换个问法即可，不需要重新输入原话。",
            ),
        )?;
    }

    Ok(AgentOutcome {
        content,
        prompt_tokens,
        completion_tokens,
        model,
        rounds_used,
        empty_retries: empty_retries_used,
    })
}
