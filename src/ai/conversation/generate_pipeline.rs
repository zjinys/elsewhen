//! 会话生成主干：多 provider 重试管线（原 conversation.rs 实体平移，行为零变化）。
use super::continuity::{FollowUp, progress_fallback};
use super::*;
use crate::ai::provider::{OllamaProvider, OpenAiCompatibleProvider};
use crate::storage::{RuleStatus, Store};
use anyhow::{Context, Result};

/// provider failover 轮换的 jittered backoff：第 n 次失败约 400·n ms，
/// 叠加 ±30% 抖动。瞬时全挂时把连环重试拉开，避免 thundering herd。
pub(crate) fn jitter_backoff_ms(attempt: usize) -> u64 {
    let base = 400u64.saturating_mul(attempt as u64);
    // 用单调时间低位做伪随机源，不进 rand 依赖；jitter ∈ [0, 0.3·base]
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0);
    let jitter = nanos % 100 * base / 100 * 3 / 10;
    if nanos % 2 == 0 {
        base.saturating_add(jitter)
    } else {
        base.saturating_sub(jitter)
    }
}
pub(crate) fn generate_with_provider_configs(
    conversation_id: &str,
    store: &Store,
    config: Option<ConversationConfig>,
    providers: Vec<crate::storage::AiProviderConfig>,
) -> Result<String> {
    super::generation::check(store)?;
    super::generation::event(store, "phase", serde_json::json!({"phase":"preparing"}))?;
    let mut config = config.unwrap_or_default();
    let selection_model = providers
        .first()
        .map(|p| p.model.clone())
        .unwrap_or_else(|| "unknown".into());
    // 生产与测试统一走延迟加载：同一套链路，测试直接守护生产行为，
    // 包括 enable_tools 与请求预算的相互作用（零余量窗口不能被工具化挤爆）。
    let mut registry = ToolRegistry::deferred_runtime();
    // MCP sidecar：拉取启用 server 的工具清单拼进注册表；单个 server 失败
    // 不拖垮生成，错误记入事件供设置页/报表观测（红线上抛不适用于插件缺失）。
    let (mcp_tools, mcp_errors) = super::mcp::available_tools(store);
    if !mcp_errors.is_empty() {
        super::generation::event(store, "mcp_errors", serde_json::json!(mcp_errors))?;
    }
    registry.set_mcp_tools(mcp_tools);
    let tool_definitions = registry.eager_specs(true).into_iter().map(|spec| serde_json::json!({
        "type":"function", "function":{"name":spec.name,"description":spec.description,"parameters":spec.parameters}
    })).collect::<Vec<_>>();
    let tool_reserve =
        super::budget::text_tokens(&selection_model, &serde_json::to_string(&tool_definitions)?);
    if matches!(
        config.memory_type,
        MemoryType::SlidingWindow { max_tokens: 0 }
    ) {
        let provider = providers.first();
        let window = provider
            .as_ref()
            .map(|p| {
                p.context_window
                    .map(|v| v as usize)
                    .unwrap_or_else(|| super::budget::default_window(&p.model))
            })
            .unwrap_or(32768);
        let output = provider.as_ref().and_then(|p| p.max_tokens).unwrap_or(4096) as usize;
        // 注意：不要在此处加“总预算上限”——历史预算 = 上限 - tool_reserve，
        // 在 21 个工具定义 ≈16k tokens 的现状下，20k 上限会把历史预算挤到
        // 负值，记忆整体失效（实测：全部历史被“较早对话已省略”替代）。
        // 提速走：时间戳尾部化吃 provider 前缀缓存 + 工具瘦身腾预算。
        config.memory_type = MemoryType::SlidingWindow {
            max_tokens: window.saturating_sub(output + window / 20),
        };
    }

    // 本轮时间窗起点：写声明核验以库内时间为准（标题→查库→卡时间窗），
    // failover 换 provider 重试也不重置、不丢失，比内存 flag 稳定。
    let turn_start = chrono::Utc::now();

    // 本轮只读取提议/回执；自然语言不构成业务写入授权。

    // 2) 构建上下文
    let selection_budget = match config.memory_type {
        MemoryType::SlidingWindow { max_tokens } => {
            Some(max_tokens.saturating_sub(tool_reserve).max(1))
        }
        MemoryType::Simple { .. } => None,
    };
    // Keep full originals until the provider is available for cancellable derived
    // summaries. Final request budgeting remains in the provider in all paths.
    let mut context = match config.memory_type {
        MemoryType::Simple { max_messages } => {
            SimpleMemory::new(max_messages).prepare_context(conversation_id, store)?
        }
        MemoryType::SlidingWindow { .. } => {
            super::memory::prepare_uncompressed_context(conversation_id, store)?
        }
    };
    super::supplements::restore(store, conversation_id, &mut context)?;
    let feedback = crate::knowledge::workflows::feedback_context(store, conversation_id)?;
    if !feedback.is_empty() {
        context.push(ContextMessage::new("system", feedback).with_segment(
            SegmentPurpose::ReviewState,
            SegmentOrigin::Review,
            Some(conversation_id),
            OmissionPolicy::Required,
        ));
    }
    let followup = FollowUp::load(store, conversation_id)?;

    if let Some(review) = pending_review_context(store, conversation_id)? {
        context.push(review.with_segment(
            SegmentPurpose::ReviewState,
            SegmentOrigin::Review,
            Some(conversation_id),
            OmissionPolicy::Required,
        ));
    }

    // 本机数据直查：统计/清单类问题（token 用量、规则等）直接在本地查库注入，
    // 让 AI 直接引用真实数据作答，不依赖模型的 tool-calling 能力。
    let last_user = store
        .list_messages(conversation_id)?
        .into_iter()
        .rev()
        .find(|m| m.role == "user");
    if let Some(message) = &last_user {
        let annotations = crate::event::parse_annotations(&message.content);
        if !annotations.people.is_empty() && !annotations.targets.is_empty() {
            let pending = store.pending_actions_for_conversation(conversation_id)?;
            let already_pending = pending
                .iter()
                .any(|p| p.action == "propose_people_relations");
            if !already_pending {
                let people: Vec<_> = annotations
                    .people
                    .iter()
                    .map(|name| serde_json::json!({"name": name}))
                    .collect();
                let relations: Vec<_> = annotations.people.iter().flat_map(|person| {
                    annotations.targets.iter().map(move |target| serde_json::json!({"person": person, "target": target, "relation": "参与"}))
                }).collect();
                let args = serde_json::json!({
                    "people": people,
                    "relations": relations,
                    "source_event_id": store.latest_event_id_for_conversation(conversation_id)?
                });
                let transaction = store.begin_write_transaction()?;
                super::generation::check(store)?;
                store.create_pending_action(
                    conversation_id,
                    "propose_people_relations",
                    &args.to_string(),
                )?;
                super::generation::event(
                    store,
                    "proposal_ready",
                    serde_json::json!({"kind":"action"}),
                )?;
                if let Some(transaction) = transaction {
                    transaction.commit()?;
                }
                return Ok(format!(
                    "已识别联系人 {} 和事项 {}，并草拟了关系保存内容。在审阅卡点击接受确认保存，在审阅卡点击拒绝取消。",
                    annotations.people.join("、"),
                    annotations.targets.join("、")
                ));
            }
        }
    }
    let query = followup
        .as_ref()
        .filter(|f| f.unresolved)
        .map(|f| f.request.as_str())
        .or_else(|| last_user.as_ref().map(|m| m.content.as_str()));
    let direct_query_result: Option<(String, String)> = match query {
        Some(query) => direct_query(store, query)?,
        None => None,
    };
    if let Some((label, data)) = &direct_query_result {
        context.push(ContextMessage::new(
            "system",
            format!(
                "（本机数据直查：系统已直接查好「{label}」的真实数据，请直接引用回答，无需再调用工具）\n{data}"
            ),
        ).with_segment(SegmentPurpose::SelectedEvidence, SegmentOrigin::ToolExecution, None, OmissionPolicy::Required));
    }

    let page_task = super::page_tasks::task_for_run(store, conversation_id)?;
    if let (Some(task), Some(input)) = (&page_task, &last_user) {
        // Legacy callers have no display-time task, but even their tools must
        // retain the material version captured at this generation boundary.
        store
            .captured_page_task
            .replace(Some((input.id.clone(), task.clone())));
    }

    // 速度优化（2026-10）：知识不再每轮预载（Candidates 曾固定占 3.2k
    // tokens，对闲聊纯浪费），按需检索的引导写进 search_knowledge_base
    // 的工具描述（消除单独 system 段的 token 开销）；引用规范仍常驻，
    // 保证检索结果可溯源。
    context.push(ContextMessage::new(
        "system",
        crate::knowledge::CITATION_INSTRUCTIONS,
    ));

    // 相关知识的确定性摘要挂载（2026-10-18/19 事故档案「投递侧不设防」
    // 结论的落地）：不赌模型自觉检索，也不回到 3.2k 整包预载——只挂
    // 少量 AI 摘要（OptionalBackground，预算紧张时最先让路，且不会像
    // 工具结果那样变成顶包素材）。
    if let Some(mounted) = relevant_knowledge_mount(
        store,
        last_user.as_ref().map(|m| m.content.as_str()).unwrap_or(""),
        3,
    ) {
        context.push(mounted);
    }

    // The provider budgets the complete request, including dynamic context and tools.
    // Recover the original task from persisted history even if the memory window
    // only retained the short status question. The per-round bound still applies.
    if let Some(followup) = &followup {
        let has_pending = !store
            .pending_actions_for_conversation(conversation_id)?
            .is_empty();
        context.push(followup.context(has_pending));
    }

    super::time_context::refresh_current_time(&mut context, &super::time_context::SystemClock);

    super::generation::event(store, "provider_basis", serde_json::json!(providers.iter().map(|p| serde_json::json!({
        "id":p.id,"model":p.model,"context_window":p.context_window,
        "version":crate::storage::knowledge::content_hash(&format!("{}|{}|{}|{:?}|{:?}|{}",p.id,p.base_url,p.model,p.max_tokens,p.context_window,p.temperature))
    })).collect::<Vec<_>>()))?;
    let summary_configs: std::collections::HashMap<_, _> = providers
        .iter()
        .map(|p| {
            (
                p.id.clone(),
                (
                    p.model.clone(),
                    crate::storage::knowledge::content_hash(&format!(
                        "{}|{}|{}|{:?}|{:?}|{}",
                        p.id, p.base_url, p.model, p.max_tokens, p.context_window, p.temperature
                    )),
                ),
            )
        })
        .collect();
    // 3) 创建 AI provider
    let ai_providers: Vec<(Option<String>, String, Box<dyn AiProvider>)> =
        match config.provider_type {
            ProviderType::OpenAiCompatible => {
                let configs = providers;
                if configs.is_empty() {
                    anyhow::bail!("No active AI provider configuration");
                }
                configs
                    .into_iter()
                    .map(|ai_config| {
                        let id = ai_config.id.clone();
                        let provider_config = super::provider::OpenAiCompatibleConfig {
                            base_url: ai_config.base_url,
                            api_key: ai_config.api_key,
                            model: ai_config.model,
                            temperature: ai_config.temperature as f32,
                            max_tokens: ai_config.max_tokens.map(|v| v as u32),
                            context_window: ai_config.context_window.map(|v| v as u32),
                        };
                        Ok((
                            Some(id),
                            ai_config.name,
                            Box::new(OpenAiCompatibleProvider::new(provider_config)?)
                                as Box<dyn AiProvider>,
                        ))
                    })
                    .collect::<Result<Vec<_>>>()?
            }
            ProviderType::Ollama => {
                // For Ollama, use environment variables or defaults
                let base_url = std::env::var("OLLAMA_BASE_URL")
                    .unwrap_or_else(|_| "http://localhost:11434".to_string());
                let model = std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama2".to_string());

                let provider_config = super::provider::OllamaConfig {
                    context_window: None,
                    max_tokens: None,
                    base_url,
                    model,
                    temperature: 0.7,
                };
                vec![(
                    None,
                    "Ollama".to_string(),
                    Box::new(OllamaProvider::new(provider_config)?),
                )]
            }
        };

    // 4) Agent 循环（可调工具）
    let mut errors = Vec::new();
    let mut outcome = None;
    for (provider_id, provider_name, provider) in ai_providers {
        let mut attempt_context = context.clone();
        if let Some(budget) = selection_budget {
            let fallback_model = std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama2".into());
            let fallback_basis = crate::storage::knowledge::content_hash(&format!(
                "ollama|{}|{}",
                fallback_model,
                std::env::var("OLLAMA_BASE_URL")
                    .unwrap_or_else(|_| "http://localhost:11434".into())
            ));
            let (model, basis) = provider_id
                .as_ref()
                .and_then(|id| summary_configs.get(id))
                .map(|(model, basis)| (model.as_str(), basis.as_str()))
                .unwrap_or((&fallback_model, &fallback_basis));
            // usage 锚定（R1 调序项）：本会话最近一次「真实 prompt_tokens /
            // 当时启发式估」>1 说明启发式对该 provider 低估，按比率收紧
            // 有效预算，宁高不低；无锚定记录时比率为 1.0，行为不变。
            let budget = {
                let underestimate =
                    super::generation::usage_underestimate_ratio(conversation_id, model);
                ((budget as f64 / underestimate) as usize).max(1)
            };
            super::summary::apply_summary(
                store,
                conversation_id,
                &*provider,
                &provider_name,
                basis,
                model,
                budget,
                &mut attempt_context,
            )?;
            super::memory::compress_context_for_model(&mut attempt_context, budget, model);
        }
        if let Some(task) = &page_task {
            if task.kind == "full_revision" {
                return super::page_tasks::revise_full(
                    store,
                    conversation_id,
                    task,
                    query.unwrap_or(""),
                    &*provider,
                    &provider_name,
                    &attempt_context,
                );
            }
            attempt_context.push(super::page_tasks::prepare(
                store,
                conversation_id,
                task,
                query.unwrap_or(""),
                &*provider,
                &provider_name,
            )?);
        }
        match run_agent_loop(
            &*provider,
            &provider_name,
            &mut attempt_context,
            &registry,
            store,
            conversation_id,
            turn_start,
        ) {
            Ok(result) => {
                if let Some(id) = provider_id {
                    // failover 落点不进同步域：active 是本机运行时事实，
                    // 不跨设备——否则两端对话各自改 active，provider bundle
                    // 永久分叉，同步冲突审阅永远无法收敛。
                    store.set_active_ai_provider_config_for_sync(&id, false)?;
                }
                context = attempt_context;
                outcome = Some(result);
                break;
            }
            Err(error) => {
                super::generation::check(store)?;
                // Consumed supplements belong to this run, not to one provider
                // attempt. A failover must not lose or consume them a second time.
                for supplement in attempt_context
                    .iter()
                    .filter(|message| message.segment.purpose == SegmentPurpose::TaskSupplement)
                {
                    if !context.iter().any(|message| {
                        message.segment.purpose == SegmentPurpose::TaskSupplement
                            && message.segment.identity_hash == supplement.segment.identity_hash
                    }) {
                        context.push(supplement.clone());
                    }
                }
                if error
                    .downcast_ref::<super::budget::RequestBudgetError>()
                    .is_some()
                {
                    return Err(error);
                }
                errors.push(format!("[{provider_name}] {error}"));
                debug_eprintln!("[agent] provider「{provider_name}」失败：{error:#}");
                // 全部 provider 可能临时不可用（网关抖动/限流）：轮换前加
                // jittered backoff，避免每次会话请求都同一瞬间锤向这批端点
                // （P2 retry backoff；Dart 侧 5s timer 只控制 tick，拦不住
                // 同 tick 内的连环 failover）。
                super::generation::pause(
                    store,
                    std::time::Duration::from_millis(jitter_backoff_ms(errors.len())),
                )?;
            }
        }
    }
    let outcome = outcome.context(format!("所有 AI provider 均失败：{}", errors.join(" | ")))?;
    if let Some(task) = &page_task {
        task.validate(store)?;
    }
    let transaction = store.begin_write_transaction()?;
    super::generation::check(store)?;
    crate::knowledge::workflows::record_generation(
        store,
        conversation_id,
        &outcome.content,
        outcome.model.as_deref(),
    )?;
    let raw = outcome.content;
    debug_eprintln!(
        "[agent] finished rounds={} empty_retries={}",
        outcome.rounds_used,
        outcome.empty_retries
    );

    // 5) 解析规则提议：若 AI 在末尾提交了一条规则，剥离标记转为友好提示展示，
    //    并把规则文本暂存为「待确认」，等待用户下一条消息确认后入库生效
    let (mut content, proposed_rule) = parse_rule_proposal(&raw);
    if let Some(rule) = proposed_rule {
        let id = store.add_rule(&rule, RuleStatus::Pending, Some(conversation_id))?;
        super::generation::event(
            store,
            "proposal_ready",
            serde_json::json!({"kind":"rule","proposal_id":id}),
        )?;
    }

    // Report the task and real results; transport diagnostics belong in logs.
    if content.trim().is_empty() {
        content = match &direct_query_result {
            Some((label, data)) => {
                format!("查到的「{label}」数据如下：\n{data}")
            }
            // 事故 2026-10-18 P1-A：自动兜底只产诚实失败消息，工具结果不进消息流。
            None => progress_fallback(
                store,
                conversation_id,
                &context,
                &[],
                Some("生成没有产出可用答复；你的问题还挂着，可以重试一次。"),
            )?,
        };
    }

    let knowledge_candidates = crate::knowledge::candidates_in_context(&context);
    let current = crate::knowledge::current_candidates(store, &knowledge_candidates)?;
    let (validated_content, cited) =
        crate::knowledge::validate_answer_citations(&content, &current);
    content = validated_content;
    let owner = format!(
        "{conversation_id}:{}",
        crate::storage::knowledge::content_hash(&content)
    );
    crate::knowledge::record_usage(store, "conversation", &owner, &knowledge_candidates, &cited)?;

    // 6) 记录 token 用量：优先 provider 返回的 usage（累计），缺失则本地估算
    let prompt_est: u64 = context
        .iter()
        .map(|m| estimate_tokens(&m.content) as u64)
        .sum();
    let completion_est = estimate_tokens(&content) as u64;
    let prompt = if outcome.prompt_tokens > 0 {
        outcome.prompt_tokens
    } else {
        prompt_est
    };
    let completion = if outcome.completion_tokens > 0 {
        outcome.completion_tokens
    } else {
        completion_est
    };
    store.record_token_usage(
        Some(conversation_id),
        prompt as i64,
        completion as i64,
        (prompt + completion) as i64,
        outcome.model.as_deref(),
    )?;

    super::generation::check(store)?;
    if let Some(transaction) = transaction {
        transaction.commit()?;
    }
    // The runtime commits the verified reply and terminal state together.
    Ok(content)
}
