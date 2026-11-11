//! Automatic cognitive insights, with a persisted run ledger and bounded input.
mod integration;
mod reconsolidation;
mod refresh;
pub(crate) use reconsolidation::run_knowledge_reconsolidation;
pub(crate) use refresh::run_knowledge_refresh;
mod sources;
use crate::ai::{insight::parse_insights, memory::ContextMessage, provider::AiProvider};
use crate::knowledge::{record_usage, select_knowledge};
use crate::storage::{ContentPolicy, Store, WikiPageDraft};
use anyhow::{Context, Result, bail};
pub(crate) use integration::run_automatic_integration;
use rusqlite::{Transaction, TransactionBehavior, params};
pub(crate) use sources::run_automatic_source_compilation;
use std::sync::Mutex;

static RUNNING: Mutex<()> = Mutex::new(());

pub fn run_automatic_insights(store: &Store, provider: &dyn AiProvider) -> Result<i64> {
    let Ok(_guard) = RUNNING.try_lock() else {
        return Ok(0);
    };
    let now = chrono::Utc::now();
    // A killed process leaves a running record. The normal provider timeout is
    // 60 seconds; a ten-minute lease also prevents cross-process double work.
    let tx = Transaction::new_unchecked(&store.connection, TransactionBehavior::Immediate)?;
    if !store.background_processing_allowed()? {
        tx.commit()?;
        return Ok(0);
    }
    store.connection.execute("UPDATE knowledge_background_runs SET status='failed',finished_at=?1,error='上次运行中断，等待重试'
        WHERE status='running' AND task='insight' AND started_at<?2",params![now.to_rfc3339(),(now-chrono::Duration::minutes(10)).to_rfc3339()])?;
    let active:bool=store.connection.query_row("SELECT EXISTS(SELECT 1 FROM knowledge_background_runs WHERE task='insight' AND status='running')",[],|r|r.get(0))?;
    if active {
        tx.commit()?;
        return Ok(0);
    }
    let recent = store
        .connection
        .prepare(
            "SELECT status,started_at,finished_at,input_key FROM knowledge_background_runs
        WHERE task='insight' ORDER BY started_at DESC,rowid DESC LIMIT 8",
        )?
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if let Some((status, started, finished, _)) = recent.first() {
        let previous =
            chrono::DateTime::parse_from_rfc3339(finished.as_deref().unwrap_or(started))?;
        let failures = recent.iter().take_while(|r| r.0 == "failed").count();
        let seconds = if status == "succeeded" {
            86400
        } else if failures >= 5 {
            21600
        } else {
            60 * (1_i64 << failures.min(4))
        };
        if now.signed_duration_since(previous).num_seconds() < seconds {
            tx.commit()?;
            return Ok(0);
        }
    }
    let mut events = Vec::new();
    let mut chars = 0;
    for event in store.recent_events(14, 60)? {
        if !store.recordable_event(&event.id)? {
            continue;
        }
        let cost = event.raw_text.chars().count();
        if cost > 1500 || chars + cost > 6000 {
            continue;
        }
        chars += cost;
        events.push(event);
    }
    if events.is_empty() {
        tx.commit()?;
        return Ok(0);
    }
    let input = events
        .iter()
        .map(|e| format!("{} {}", e.id, e.raw_text))
        .collect::<Vec<_>>()
        .join("\n");
    let key = crate::storage::knowledge::content_hash(&input);
    let done: bool = store.connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM knowledge_background_runs WHERE task='insight'
        AND input_key=?1 AND status='succeeded')",
        [&key],
        |r| r.get(0),
    )?;
    if done {
        tx.commit()?;
        return Ok(0);
    }
    let run_id = uuid::Uuid::new_v4().to_string();
    store.connection.execute(
        "INSERT INTO knowledge_background_runs(id,task,input_key,status,started_at)
        VALUES (?1,'insight',?2,'running',?3)",
        params![run_id, key, now.to_rfc3339()],
    )?;
    tx.commit()?;
    let result = generate(store, provider, &events, &input, &run_id);
    match result {
        Ok(count) => Ok(count),
        Err(error) => {
            store.connection.execute("UPDATE knowledge_background_runs SET status='failed',finished_at=?1,error=?2 WHERE id=?3",
                params![chrono::Utc::now().to_rfc3339(),error.to_string().chars().take(400).collect::<String>(),run_id])?;
            Ok(0)
        }
    }
}

/// 近期事件分析给出的指导要点（罗盘衔接素材）：去重取 10 条；任何
/// 读取/解析失败静默返回空——缺失素材不能阻断洞察生成。
fn recent_guidance_lessons(store: &Store) -> String {
    let Ok(mut stmt) = store.connection.prepare(
        "SELECT result_json FROM event_analyses WHERE result_json IS NOT NULL ORDER BY created_at DESC LIMIT 30",
    ) else {
        return String::new();
    };
    let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) else {
        return String::new();
    };
    let mut lessons: Vec<String> = Vec::new();
    for raw in rows.flatten() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
            continue;
        };
        let Some(items) = value
            .pointer("/guidance/lessons")
            .and_then(|v| v.as_array())
        else {
            continue;
        };
        for item in items {
            if let Some(text) = item.as_str() {
                if !lessons.iter().any(|e| e == text) {
                    lessons.push(text.to_owned());
                }
            }
        }
    }
    lessons.truncate(10);
    if lessons.is_empty() {
        return String::new();
    }
    lessons
        .iter()
        .map(|l| format!("- {l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn generate(
    store: &Store,
    provider: &dyn AiProvider,
    events: &[crate::event::EventSummary],
    input: &str,
    run_id: &str,
) -> Result<i64> {
    let _scope = store.prompt_scope("background_task", &format!("insight:{run_id}"))?;
    store.bind_background_prompt_bundle(run_id)?;
    let candidates = select_knowledge(store, input, "insight", 5000)?;
    let history = store.list_insights()?;
    // 提示词只挂最近 20 条既有标题约束生成；去重窗口放宽到 300 条——
    // 换措辞复述同一主题的洞察完全可能落在 20 条之外。
    let past = history.iter().take(20).map(|i| i.title.clone()).collect::<Vec<_>>();
    let mut seen_titles = history
        .iter()
        .take(300)
        .map(|i| i.title.clone())
        .collect::<Vec<_>>();
    // 罗盘衔接（D4）：把近期事件分析给出的指导要点作为素材挂进洞察窗口；
    // 只挂接不重写——insight 生成路径本身不变，空段整体省略。
    let guidance = recent_guidance_lessons(store);
    let guidance_material = if guidance.is_empty() {
        String::new()
    } else {
        store.render_knowledge_instruction("insight_guidance", &[("body", guidance)])?
    };
    let prompt = store.render_knowledge_instruction(
        "insight_input",
        &[
            ("past", serde_json::to_string(&past)?),
            ("events", input.into()),
            (
                "knowledge",
                crate::knowledge::context_text_for_run(store, &candidates)?,
            ),
            ("guidance_material", guidance_material),
        ],
    )?;
    let reply = provider.generate_reply(vec![
        ContextMessage::new(
            "system",
            format!(
                "{}\n{}",
                store.render_knowledge_instruction("insight_system", &[])?,
                store.output_language_directive(false)?
            ),
        ),
        ContextMessage::new("user", prompt),
    ])?;
    let insights = parse_insights(&reply.content).context("洞察返回格式不合法")?;
    if insights.len() > 3 {
        bail!("洞察超出数量上限");
    }
    for insight in &insights {
        if insight.title.trim().is_empty()
            || insight.title.chars().count() > 150
            || insight.observation.trim().is_empty()
            || insight.observation.chars().count() > 3000
            || !matches!(insight.lens.as_str(), "1" | "2" | "3" | "4")
            || (insight.source_slugs.is_empty() && insight.related_events.is_empty())
            || insight
                .source_slugs
                .iter()
                .any(|s| !candidates.iter().any(|c| &c.page_slug == s))
            || insight
                .related_events
                .iter()
                .any(|id| !events.iter().any(|e| &e.id == id))
        {
            bail!("洞察包含无效来源或内容，整批未写入");
        }
    }
    let tx = Transaction::new_unchecked(&store.connection, TransactionBehavior::Immediate)?;
    store.require_background_processor()?;
    let current = crate::knowledge::current_candidates(store, &candidates)?;
    // Recheck sources at commit: users may reject material while the model runs.
    for event in events {
        if !store.recordable_event(&event.id)? {
            bail!("事件记录性已变化，等待重新评估");
        }
    }
    let mut count = 0;
    for insight in insights {
        // 主题级去重：精确匹配 + 换措辞同主题（含本批刚写入的）一律跳过，
        // 不建页也不入池——这是「每天启动都弹同主题卡片」的根因堵点。
        if seen_titles
            .iter()
            .any(|t| duplicate_insight_title(t, &insight.title))
        {
            continue;
        }
        let used = candidates
            .iter()
            .filter(|c| insight.source_slugs.contains(&c.page_slug))
            .cloned()
            .collect::<Vec<_>>();
        let mut snapshots = Vec::new();
        let mut ids = insight.related_events.clone();
        for citation in &used {
            if !current.iter().any(|c| c.page_slug == citation.page_slug) {
                bail!("引用来源已变化");
            }
            snapshots.extend(citation.sources.iter().map(|s| s.snapshot_id.clone()));
            ids.extend(citation.event_ids.clone());
        }
        ids.sort();
        ids.dedup();
        snapshots.sort();
        snapshots.dedup();
        let links = used
            .iter()
            .map(|c| format!("[[{}]]", c.page_slug))
            .collect::<Vec<_>>()
            .join("、");
        let content = format!(
            "{}\n\n{}\n\n来源：{}\n\n*AI 推断，供审阅。*",
            insight.observation,
            insight.action.clone().unwrap_or_default(),
            links
        );
        let draft = WikiPageDraft {
            slug: format!("insight/{}", crate::wiki::slugify(&insight.title)),
            kind: "insight".into(),
            title: insight.title.clone(),
            summary: insight.observation.chars().take(120).collect(),
            content_md: content,
            tags: vec![format!("lens-{}", insight.lens)],
            source_event_ids: ids,
            status: "active".into(),
            reason: "后台认知洞察（有来源的 AI 推断）".into(),
            source_url: None,
        };
        let outcome = store.upsert_wiki_page_in_tx(&draft, ContentPolicy::PreserveHumanEdits)?;
        store.bind_page_sources(&outcome.page.id, &snapshots)?;
        store.insert_insight(
            14,
            "insight-v3-sourced",
            &insight.lens,
            &insight.title,
            &insight.observation,
            &insight.related_events,
            insight.action.as_deref(),
        )?;
        record_usage(store, "insight", &outcome.page.slug, &candidates, &used)?;
        // 洞见浮现候选（2026-11-05 提案 §5/§7-P1）：带行动建议的洞察登记为
        // 浮现候选；引用页里有活跃习惯页时挂到该习惯页（「开启习惯问候」
        // 出口需要 habit 页 slug），否则挂洞察页自身。同页终身一次由存储层
        // 台账去重。无行动建议的洞察不入池（无法形成闭环行动，守住信噪比）。
        if let Some(action) = insight
            .action
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let mut surfacing_slug: Option<String> = None;
            for citation in &used {
                let is_habit: bool = store.connection.query_row(
                    "SELECT EXISTS(SELECT 1 FROM wiki_pages
                     WHERE slug=?1 AND status='active' AND kind='habit')",
                    [&citation.page_slug],
                    |row| row.get(0),
                )?;
                if is_habit {
                    surfacing_slug = Some(citation.page_slug.clone());
                    break;
                }
            }
            store.record_digest_insight_candidate(
                "insight",
                surfacing_slug.as_deref().unwrap_or(&outcome.page.slug),
                &insight.title,
                &insight.observation,
                Some(action),
                None,
            )?;
        }
        seen_titles.push(insight.title.clone());
        count += 1;
    }
    store.append_wiki_log(&format!("后台认知洞察 {run_id}：{count} 条"))?;
    store.connection.execute("UPDATE knowledge_background_runs SET status='succeeded',finished_at=?1,result_count=?2 WHERE id=?3",
        params![chrono::Utc::now().to_rfc3339(),count,run_id])?;
    tx.commit()?;
    Ok(count)
}

/// 主题级去重：模型每天会换措辞复述同一主题，标题逐字不同但意思相同，
/// 精确匹配拦不住。标题归一化（仅字母数字、小写）后取二元组（bigram）
/// Jaccard 相似度，≥0.45 判为同一主题。
///
/// 阈值用 2026-11 本地全量 47 条洞察校准：唯一真重复对（闲置AI算力→跨
/// 平台内容生成服务，两次生成措辞不同）0.52，其余不同主题对全部 <0.2。
/// 观察正文不参与比对——实测真重复对的观察相似度仅 0.04，比标题噪声大。
fn duplicate_insight_title(existing: &str, candidate: &str) -> bool {
    fn normalized(text: &str) -> String {
        text.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    }
    fn bigrams(text: &str) -> std::collections::HashSet<String> {
        let text = normalized(text);
        let chars: Vec<_> = text.chars().collect();
        if chars.len() < 2 {
            return std::iter::once(text).collect();
        }
        chars.windows(2).map(|w| w.iter().collect()).collect()
    }
    let (a, b) = (bigrams(existing), bigrams(candidate));
    let overlap = a.intersection(&b).count();
    let union = a.union(&b).count().max(1);
    overlap as f64 / union as f64 >= 0.45
}

#[cfg(test)]
mod insight_dedup_tests {
    use super::duplicate_insight_title;

    #[test]
    fn reworded_same_theme_is_caught_but_distinct_themes_pass() {
        // 真实重复对：两次生成换措辞复述同一主题（2026-11 实测相似度 0.52）。
        assert!(duplicate_insight_title(
            "闲置AI算力转化为跨平台内容生成服务",
            "将闲置AI算力包装为跨平台内容生成服务提升发布收益"
        ));
        // 不同主题必须放行（实测分布 <0.2）。
        assert!(!duplicate_insight_title(
            "闲置AI算力转化为跨平台内容生成服务",
            "竞品出现只证明品类被展示，不等于付费需求成立"
        ));
        assert!(!duplicate_insight_title(
            "深度AI实操内容的互动效率高于浅教程",
            "技术写作Skill附带网盘精选资料作为拉新承接"
        ));
    }
}
