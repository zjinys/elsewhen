//! 分析队列与投喂（chat rule check / shadow judgement / analysis queue / 决策支持材料 / 情绪洗涤）。

use super::*;

/// 与 process_analysis_queue 同形态：封顶 20 条/轮、单条失败记更新不阻塞。
/// 注：confidence 解析失败按 None（字段可选）。
pub(crate) fn process_chat_rule_check_queue(
    store: &Store,
    provider: &dyn AiProvider,
) -> Result<i64> {
    let Ok(hits) = store.pending_chat_hits(20) else {
        return Ok(0);
    };
    let mut judged = 0i64;
    for hit in hits {
        let (applicable, confidence) = match build_judge_context(store, &hit)
            .and_then(|context| {
                provider
                    .generate_reply(context)
                    .map_err(|e| anyhow::anyhow!("影子判定调用失败：{e}"))
            })
            .and_then(|reply| parse_shadow_judgement(&reply.content))
        {
            Ok(pair) => pair,
            Err(_error) => continue, // 下轮重试：pending 不动，不毒化其它轮次。
        };
        if store
            .resolve_chat_hit(&hit.id, applicable, confidence)
            .is_ok()
        {
            judged += 1;
        }
    }
    Ok(judged)
}

/// 组装判定上下文（极简：系统纪律 + 规则 + 消息原文，封顶 4K tokens）。
pub(crate) fn build_judge_context(
    _store: &Store,
    hit: &PendingChatHit,
) -> Result<Vec<ContextMessage>> {
    let mut context = vec![
        ContextMessage::new(
            "system",
            "你只输出严格 JSON 对象（含 applicable 布尔与 confidence 数字 0..1），不要 Markdown 围栏。判定不确定时宁可 applicable=false（宁缺毋滥）。",
        ),
        ContextMessage::new(
            "user",
            format!(
                "判定下面的对话消息是否需要遵守这条个人经验规则。需要遵守/适用为 true；无关、纯属闲聊或规则不适用为 false。\n\n规则：{}\n\n消息：{}",
                hit.rule_content, hit.message_text
            ),
        ),
    ];
    crate::knowledge::bound_model_context(&mut context, 4000)?;
    Ok(context)
}

/// 解析影子判定回包（宽松：confidence 缺失按 None；applicable 缺失报错重试）。
pub(crate) fn parse_shadow_judgement(content: &str) -> Result<(bool, Option<f64>)> {
    let value: serde_json::Value = serde_json::from_str(content.trim())?;
    let applicable = value
        .get("applicable")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| anyhow::anyhow!("影子判定缺少 applicable 字段"))?;
    let confidence = value.get("confidence").and_then(serde_json::Value::as_f64);
    Ok((applicable, confidence))
}

pub(crate) fn process_analysis_queue(store: &Store, provider: &dyn AiProvider) -> Result<i64> {
    let mut processed = 0;
    // Bound each invocation even when producers keep adding work or retries
    // become available while a slow provider is processing other records.
    let stats = store.analysis_job_stats()?;
    // 总墙钟时间上限：provider 单次超时可到 60s，若一批 50 条全部超时，
    // 无上限会占住 worker 约 50 分钟且中途无中断（P2 worker 限时）。
    // 每完成一条记录后检查，超限即停，余量留待下次 tick。
    let batch_start = std::time::Instant::now();
    for _ in 0..(stats.pending + stats.retry).min(50) {
        if batch_start.elapsed().as_secs() >= ANALYSIS_BATCH_MAX_SECS {
            break;
        }
        let Some(job) = crate::storage::with_busy_retry(|| {
            store
                .claim_analysis_job()
                .context("analysis worker: claim job")
        })?
        else {
            break;
        };
        // 上下文构建属于分析任务的一部分：失败不能把已 claim 的 job
        // 孤悬为 running（否则要等下次启动才 recover），记入 last_error 放回重试。
        let (context, candidates) = match decision_support_material(store, &job.raw_text) {
            Ok(context) => context,
            Err(error) => {
                store.fail_analysis(&job, &format!("决策上下文构建失败：{error}"))?;
                continue;
            }
        };
        // 规则命中判定段（D1）：pending 命中随本次分析一并判定，不新增调用面。
        let pending_hits = store.pending_rule_hits_for_event(&job.event_id)?;
        let rules_section = if pending_hits.is_empty() {
            String::new()
        } else {
            let listed = pending_hits
                .iter()
                .map(|hit| format!("- rule_id={}：{}", hit.rule_id, hit.rule_content))
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                "\n\n以下规则在入库预筛时命中本记录，请逐条判定其是否真正适用本记录，并在输出的 rule_confirmations 数组中逐条回填（rule_id 原样返回；confidence 0..1 表示适用置信度）：\n{listed}\n"
            )
        };
        let prompt = format!(
            "分析以下个人记录，只返回 JSON 对象，不要 Markdown，也不要增加字段。schema_version 固定为 event-analysis-v3。字段必须包含 schema_version、recordable(boolean)、kind(event/discussion/chitchat/meta)、event_type(string)、confidence(number 0..1)、summary(string)、clarifications(array of strings)、people(array of strings)、projects(array of strings)、activities(array of strings)、follow_ups(array of strings)；仅当下方附有规则命中时才包含 rule_confirmations(array of {{rule_id: string, applicable: boolean, confidence: number 0..1}})。情绪走红线：仅当记录客观存在对具体人或事的情绪反应时才可选包含 emotion 数组：[{{\"target\": string, \"feeling\": string}}]——feeling 用简短文本描述感受（如「被理解」「焦虑」「挫败」），绝对禁止打分或生成情绪分析报告；target 填相关的人或事（与 people/projects 一致，对事件本身的感受填空字符串）；没有则整个省略 emotion 字段。只有当决策辅助上下文与你确有关联时才可选包含 guidance 对象：{{\"related\":[{{\"slug\":string,\"note\":string}}],\"lessons\":[string],\"follow_ups_refined\":[string]}}：related[].slug 只能取自决策辅助上下文中出现的知识库页 slug，note 写为什么相关/可复用什么（一句话）；lessons 写可复用经验（不是重复记录内容）；follow_ups_refined 是对 follow_ups 的精炼合并；无关联时整个省略 guidance 字段，不得杜撰 slug。projects 只填写明确的长期项目/产品/组织；网站改版、联调、任务、沟通、会议等动作或事项必须放入 activities，不要放入 projects。activities 提取简短常见的动词短语（如 散步、跑步、阅读、写代码），不要用描述性长句——统计按词匹配，词汇不一致就数不到。只有客观经历、决定、行动或进展 recordable=true/kind=event；对 AI 回复评价、闲聊、纯提问或元对话 recordable=false，并保留简短 summary。\n\n决策辅助上下文（只作参考，不能据此臆测新事实）：\n{}\n\n记录：{}{}",
            context, job.raw_text, rules_section
        );
        let mut analysis_context = vec![
            ContextMessage::new("system", crate::knowledge::CITATION_INSTRUCTIONS),
            ContextMessage::new("user", prompt),
        ];
        if let Err(error) = crate::knowledge::bound_model_context(&mut analysis_context, 24000) {
            store.fail_analysis(&job, &error.to_string())?;
            continue;
        }
        match provider.generate_reply(analysis_context) {
            Ok(reply) => match EventAnalysisV1::parse(&reply.content) {
                Ok(mut value) => {
                    // guidance 护栏（D4）：杜撰 slug 丢弃、条数与文本封顶；
                    // 清洗后为空则整字段省略——派生层不存垃圾。
                    wash_guidance(store, &mut value);
                    wash_emotion(&mut value);
                    let current = crate::knowledge::current_candidates(store, &candidates)?;
                    let mut cited = Vec::new();
                    for field in std::iter::once(&mut value.summary)
                        .chain(value.clarifications.iter_mut())
                        .chain(value.follow_ups.iter_mut())
                    {
                        let (text, used) =
                            crate::knowledge::validate_answer_citations(field, &current);
                        *field = text;
                        for citation in used {
                            if !cited.iter().any(|c: &crate::knowledge::KnowledgeCitation| {
                                c.page_slug == citation.page_slug
                            }) {
                                cited.push(citation);
                            }
                        }
                    }
                    store.complete_analysis(
                        &job,
                        &value.schema_version,
                        &serde_json::to_string(&value)?,
                    )?;
                    // 规则判定回填（D1）：失败不影响已入库分析；pending 命中留待
                    // 下次 reanalyze 补判（幂等——只处理 pending 行）。
                    if !value.rule_confirmations.is_empty() {
                        let confirmations: Vec<crate::storage::rule_triggers::RuleConfirmation> =
                            value
                                .rule_confirmations
                                .iter()
                                .map(|c| crate::storage::rule_triggers::RuleConfirmation {
                                    rule_id: c.rule_id.clone(),
                                    applicable: c.applicable,
                                    confidence: c.confidence,
                                })
                                .collect();
                        let _ = store.apply_rule_confirmations(&job.event_id, &confirmations);
                    }
                    crate::knowledge::record_usage(
                        store,
                        "analysis",
                        &job.event_id,
                        &candidates,
                        &cited,
                    )?;
                    // 用户模型增量维护（D2）：@/# 双向标注的组合是用户写死的
                    // 权威实体，双方页面都已存在时直接增量入库（不用等确认）；
                    // 其余组合保留草拟确认卡（低置信才草拟，防打扰）。
                    if value.recordable && !value.people.is_empty() && !value.projects.is_empty() {
                        let annotations = crate::event::parse_annotations_many(std::iter::once(
                            job.raw_text.as_str(),
                        ));
                        let mut draft_pairs: Vec<(String, String)> = Vec::new();
                        for person in &value.people {
                            let person_authoritative =
                                annotations.people.iter().any(|p| p == person);
                            for target in &value.projects {
                                let target_authoritative =
                                    annotations.targets.iter().any(|t| t == target);
                                let authoritative = person_authoritative && target_authoritative;
                                if authoritative
                                    && try_auto_upsert_annotated_relation(
                                        store,
                                        person,
                                        target,
                                        &job.event_id,
                                    )
                                    .unwrap_or(false)
                                {
                                    continue; // 自动入库成功，不再出草拟卡
                                }
                                draft_pairs.push((person.clone(), target.clone()));
                            }
                        }
                        if !draft_pairs.is_empty() {
                            if let Some(conversation_id) =
                                store.conversation_id_for_event(&job.event_id)?
                            {
                                let exists = store.action_exists_for_event(
                                    &conversation_id,
                                    "propose_people_relations",
                                    &job.event_id,
                                )?;
                                if !exists {
                                    let people: Vec<_> = draft_pairs
                                        .iter()
                                        .map(|(person, _)| serde_json::json!({"name": person}))
                                        .collect();
                                    let relations: Vec<_> = draft_pairs
                                        .iter()
                                        .map(|(person, target)| {
                                            serde_json::json!({
                                                "person": person,
                                                "target": target,
                                                "relation": "参与"
                                            })
                                        })
                                        .collect();
                                    let args = serde_json::json!({
                                        "people": people,
                                        "relations": relations,
                                        "source_event_id": job.event_id
                                    });
                                    store.create_pending_action(
                                        &conversation_id,
                                        "propose_people_relations",
                                        &args.to_string(),
                                    )?;
                                }
                            }
                        }
                    }
                    processed += 1;
                }
                Err(error) => store.fail_analysis(
                    &job,
                    &format!("AI 返回不符合 {EVENT_ANALYSIS_VERSION}: {error}"),
                )?,
            },
            Err(error) => store.fail_analysis(&job, &error.to_string())?,
        }
    }
    Ok(processed as i64)
}

/// D2 增量合并的自动入库窄集：person/target 双双来自 @/# 标注（用户写死的
/// 权威实体），且对应页面都已存在时，直接以「参与」关系入库（confidence 4、
/// 记 source_event_id）。任何前置不满足（页面缺失等）都返回 false，走草拟卡。
/// 返回 Ok(false) 不算错误——只是「这组关系不适合自动入库」。
pub(crate) fn try_auto_upsert_annotated_relation(
    store: &Store,
    person: &str,
    target: &str,
    event_id: &str,
) -> Result<bool> {
    // D3 归一管道：标题/别名命中即写 canonical slug；查不到或多义则降级草拟。绝
    // 不盲写 slugify 直译 slug——那正是重复实体的成因。
    let (Some(from_slug), Some(to_slug)) = (
        store.resolve_entity_slug("person", person)?,
        store.resolve_entity_slug("topic", target)?,
    ) else {
        return Ok(false);
    };
    let from = store.get_wiki_page(&from_slug)?;
    let to = store.get_wiki_page(&to_slug)?;
    let (Some(from), Some(to)) = (from, to) else {
        return Ok(false);
    };
    store.upsert_relation(&crate::storage::RelationDraft {
        from_slug,
        from_kind: from.kind,
        to_slug,
        to_kind: to.kind,
        relation: "参与".to_string(),
        note: None,
        confidence: 4,
        source_conversation_id: None,
        source_event_id: Some(event_id.to_string()),
    })?;
    Ok(true)
}

/// Build bounded, read-only context for Phase 4C. This makes existing rules and
/// knowledge visible at the decision point while leaving all writes behind the
/// existing confirmation gates.
/// 决策辅助上下文（D4 扩展：用户模型关系边 + 近期记录条目）。
/// 段级 fail-soft：任何一段检索失败只省略该段，绝不把已入库记录拒之分析门外；
/// 知识库主材料组装失败（select_knowledge 本身）仍向上报错。
pub(crate) fn decision_support_material(
    store: &Store,
    query: &str,
) -> Result<(String, Vec<crate::knowledge::KnowledgeCitation>)> {
    let mut lines = Vec::new();
    for rule in store.list_active_rules()?.into_iter().take(8) {
        lines.push(format!(
            "- 已确认规则：{}",
            rule.content.chars().take(500).collect::<String>()
        ));
    }
    let candidates = crate::knowledge::select_knowledge(store, query, "analysis", 2500)?;
    lines.push(crate::knowledge::context_text(&candidates)?);
    // 用户模型关系边（D2 累积）：最多 10 条，条目本身短语级不截断。
    if let Ok(relations) = store.list_relations() {
        let edges = relations
            .iter()
            .take(10)
            .map(|r| format!("- 关系：{} ——{}—— {}", r.from_slug, r.relation, r.to_slug))
            .collect::<Vec<_>>();
        if !edges.is_empty() {
            lines.push(format!("你的既有关系账本：\n{}", edges.join("\n")));
        }
    }
    // 近期记录条目：最多 5 条、每条 120 字，供「这和最近的事有什么关系」参照。
    if let Ok(recent) = store.recent_events(7, 5) {
        let entries = recent
            .iter()
            .map(|e| {
                format!(
                    "- 近期记录（{}）：{}",
                    e.recorded_at.chars().take(16).collect::<String>(),
                    e.raw_text.chars().take(120).collect::<String>()
                )
            })
            .collect::<Vec<_>>();
        if !entries.is_empty() {
            lines.push(format!("近期记录：\n{}", entries.join("\n")));
        }
    }
    Ok((lines.join("\n\n"), candidates))
}

/// guidance 清洗与护栏（D4）：相关页 slug 必须真实存在，否则丢弃该项；
/// 每类条数封顶 10、每条文本 500 字——分析输出不得无限膨胀。
/// 情绪标注护栏（D5）：只有文本无数值；空 feeling 丢弃、重复归并、
/// 每条 200 字封顶、条数封 8。清洗后为空则整字段省略。
pub(crate) fn wash_emotion(value: &mut EventAnalysisV1) {
    let Some(tags) = value.emotion.as_mut() else {
        return;
    };
    tags.retain(|tag| !tag.feeling.trim().is_empty());
    let mut seen: Vec<(Option<String>, String)> = Vec::new();
    tags.retain(|tag| {
        let feeling: String = tag.feeling.trim().chars().take(200).collect();
        let target = tag
            .target
            .as_ref()
            .map(|t| t.trim().to_owned())
            .filter(|t| !t.is_empty());
        if seen.iter().any(|(t, f)| *t == target && f == &feeling) {
            return false;
        }
        seen.push((target.clone(), feeling.clone()));
        true
    });
    for tag in tags.iter_mut() {
        tag.feeling = tag.feeling.trim().chars().take(200).collect();
        tag.target = tag
            .target
            .as_ref()
            .map(|t| t.trim().chars().take(200).collect::<String>())
            .filter(|t| !t.is_empty());
    }
    tags.truncate(8);
    if tags.is_empty() {
        value.emotion = None;
    }
}

pub(crate) fn wash_guidance(store: &Store, value: &mut EventAnalysisV1) {
    let Some(guidance) = value.guidance.as_mut() else {
        return;
    };
    guidance.related.retain(|r| {
        store
            .get_wiki_page(r.slug.trim())
            .map(|page| page.is_some())
            .unwrap_or(false)
    });
    guidance.related.truncate(10);
    for r in &mut guidance.related {
        r.note = r.note.chars().take(500).collect();
    }
    guidance.lessons = guidance
        .lessons
        .iter()
        .take(10)
        .map(|l| l.chars().take(500).collect())
        .collect();
    guidance.follow_ups_refined = guidance
        .follow_ups_refined
        .iter()
        .take(10)
        .map(|f| f.chars().take(500).collect())
        .collect();
    if guidance.related.is_empty()
        && guidance.lessons.is_empty()
        && guidance.follow_ups_refined.is_empty()
    {
        value.guidance = None;
    }
}
