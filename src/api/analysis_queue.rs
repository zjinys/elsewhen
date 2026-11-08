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
    store: &Store,
    hit: &PendingChatHit,
) -> Result<Vec<ContextMessage>> {
    let _scope = store.prompt_scope("background_task", &format!("rule_judgement:{}", hit.id))?;
    let mut context = vec![
        ContextMessage::new(
            "system",
            store.render_processing_instruction("judge_system", &[])?,
        ),
        ContextMessage::new(
            "user",
            store.render_processing_instruction(
                "judge_input",
                &[
                    ("rule", hit.rule_content.clone()),
                    ("message", hit.message_text.clone()),
                ],
            )?,
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
        let bundle = store.analysis_prompt_bundle(&job.id)?;
        let _prompt_scope = store.use_prompt_bundle(&bundle);
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
            store.render_processing_instruction("analysis_rules", &[("rules", listed)])?
        };
        let prompt = store.render_processing_instruction(
            "analysis_input",
            &[
                ("context", context),
                ("record", job.raw_text.clone()),
                ("rules", rules_section),
            ],
        )?;
        let mut analysis_context = vec![
            ContextMessage::new(
                "system",
                format!(
                    "{}\n{}\n{}",
                    store.prompt_for_run("analysis.policy")?,
                    store.render_context_instruction("citation", &[])?,
                    store.output_language_directive(false)?
                ),
            ),
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
        lines.push(store.render_processing_instruction(
            "analysis_rule",
            &[("body", rule.content.chars().take(500).collect())],
        )?);
    }
    let candidates = crate::knowledge::select_knowledge(store, query, "analysis", 2500)?;
    lines.push(crate::knowledge::context_text_for_run(store, &candidates)?);
    // 用户模型关系边（D2 累积）：最多 10 条，条目本身短语级不截断。
    if let Ok(relations) = store.list_relations() {
        let edges = relations
            .iter()
            .take(10)
            .map(|r| {
                store.render_processing_instruction(
                    "analysis_relation",
                    &[
                        ("from", r.from_slug.clone()),
                        ("relation", r.relation.clone()),
                        ("to", r.to_slug.clone()),
                    ],
                )
            })
            .collect::<Result<Vec<_>>>()?;
        if !edges.is_empty() {
            lines.push(store.render_processing_instruction(
                "analysis_relations",
                &[("body", edges.join("\n"))],
            )?);
        }
    }
    // 近期记录条目：最多 5 条、每条 120 字，供「这和最近的事有什么关系」参照。
    if let Ok(recent) = store.recent_events(7, 5) {
        let entries = recent
            .iter()
            .map(|e| {
                store.render_processing_instruction(
                    "analysis_record",
                    &[
                        ("time", e.recorded_at.chars().take(16).collect()),
                        ("body", e.raw_text.chars().take(120).collect()),
                    ],
                )
            })
            .collect::<Result<Vec<_>>>()?;
        if !entries.is_empty() {
            lines.push(store.render_processing_instruction(
                "analysis_recent",
                &[("body", entries.join("\n"))],
            )?);
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
