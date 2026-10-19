//! bounded 检索契约：选页、摘录匹配与引用（citation）装配。

use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceReference {
    pub snapshot_id: String,
    pub title: String,
    pub locator: Option<String>,
    pub version: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeCitation {
    pub page_slug: String,
    #[serde(default)]
    pub content_hash: String,
    pub title: String,
    pub excerpt: String,
    pub reason: String,
    pub applicable_when: String,
    pub strength: String,
    pub category: String,
    pub sources: Vec<SourceReference>,
    pub event_ids: Vec<String>,
}

/// 孵化闸门（提案 2026-10-19-clip-incubation-and-negative-knowledge）：
/// `source`/`note` 页的 `opinion` 缺省语义由「NULL=缺省认可」收紧为
/// 「NULL=孵化中」。闸门只拦**自动投递**（素材编译、知识召回、引用挂载、
/// 依据校验）；**按需读取**（get_wiki_page 工具、用户显式 @wiki 引用、
/// 派生稿采用核对）豁免——用户显式意志高于孵化状态。
pub fn participating(page: &WikiPage) -> bool {
    page.status != "archived"
        && (page.kind != "source" && page.kind != "note"
            || page.opinion.as_deref() == Some("endorse"))
}

/// 同谓词的 SQL 形态（`alias` 为 wiki_pages 的表别名或表名）。
pub(crate) fn sql_participating(alias: &str) -> String {
    format!(
        "({alias}.status<>'archived' AND ({alias}.kind NOT IN ('source','note') \
         OR {alias}.opinion='endorse'))"
    )
}

pub(crate) fn shorten(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

/// 挂载摘要：AI 产出的 summary 优先（截到 240 字符上限内），空则退回
/// 正文前 120 字符预览。挂载与 select_knowledge 共用该字段。
pub(crate) fn mount_summary(summary: &str, content_md: &str) -> String {
    let trimmed = summary.trim();
    if trimmed.is_empty() {
        shorten(content_md, 120)
    } else {
        shorten(trimmed, 240)
    }
}

pub(crate) fn query_terms(query: &str) -> Vec<String> {
    let query = shorten(query, 3000).to_lowercase();
    let mut terms = Vec::new();
    for word in query.split(|c: char| !c.is_alphanumeric()) {
        let chars: Vec<char> = word.chars().collect();
        if chars.iter().all(char::is_ascii) {
            if (2..=60).contains(&chars.len()) {
                terms.push(word.to_string());
            }
        } else {
            for group in chars.windows(2) {
                let term: String = group.iter().collect();
                if ![
                    "今天", "什么", "怎么", "如何", "一下", "这个", "可以", "请问", "是否", "我的",
                    "有关", "帮我", "关于", "最近", "哪些", "事情", "记录",
                ]
                .contains(&term.as_str())
                {
                    terms.push(term);
                }
            }
        }
    }
    let mut seen = HashSet::new();
    terms.retain(|t| seen.insert(t.clone()));
    terms.truncate(40);
    // Small, transparent vocabulary expansion complements literal matching.
    // Keep originals first and avoid asking a model just to find candidates.
    for group in [
        &["受众", "人群", "目标用户", "audience"][..],
        &["流量", "曝光", "触达", "traffic"][..],
        &["花费", "开销", "费用", "成本", "cost"][..],
        &["重试", "再试", "retry"][..],
        &["事务", "transaction"][..],
        &["备份", "backup"][..],
        &["截止", "到期", "deadline"][..],
    ] {
        if group.iter().any(|word| query.contains(word)) {
            for word in group {
                if seen.insert((*word).to_owned()) {
                    terms.push((*word).to_owned());
                }
            }
        }
    }
    terms.truncate(64);
    terms
}

/// Select the most relevant full-text window instead of always citing the intro.
fn matching_excerpt(text: &str, terms: &[String], limit: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= limit {
        return text.into();
    }
    let width = limit.saturating_sub(2).max(1);
    let step = (width / 2).max(1);
    let mut best = (0usize, 0usize);
    for start in (0..chars.len()).step_by(step) {
        let window = chars[start..(start + width).min(chars.len())]
            .iter()
            .collect::<String>()
            .to_lowercase();
        let score = terms
            .iter()
            .filter(|term| window.contains(term.as_str()))
            .count();
        if score > best.0 {
            best = (score, start);
        }
    }
    let end = (best.1 + width).min(chars.len());
    format!(
        "{}{}{}",
        if best.1 > 0 { "…" } else { "" },
        chars[best.1..end].iter().collect::<String>(),
        if end < chars.len() { "…" } else { "" }
    )
}

/// Fetches at most 80 candidate rows. The serialized result, including metadata
/// and provenance, fits the caller's character budget; empty is a valid result.
pub fn select_knowledge(
    store: &Store,
    query: &str,
    _task: &str,
    budget: usize,
) -> Result<Vec<KnowledgeCitation>> {
    let terms = query_terms(query);
    if terms.is_empty() || budget < 300 {
        return Ok(vec![]);
    }
    let budget = budget.min(12000);
    let prominent = "lower(title||' '||tags||' '||COALESCE((SELECT applicable_when FROM knowledge_metadata WHERE page_id=wiki_pages.id),''))";
    let predicates=terms.iter().enumerate().map(|(i,_)|format!("instr(lower(title||' '||tags||' '||summary||' '||content_md||' '||COALESCE((SELECT applicable_when FROM knowledge_metadata WHERE page_id=wiki_pages.id),'')),?{})>0",i+1)).collect::<Vec<_>>().join(" OR ");
    let score = terms
        .iter()
        .enumerate()
        .map(|(i, _)| {
            format!(
                "4*(instr({prominent},?{n})>0)+(instr(lower(content_md),?{n})>0)",
                n = i + 1
            )
        })
        .collect::<Vec<_>>()
        .join("+");
    let sql = format!(
        "SELECT {} FROM wiki_pages WHERE status!='archived' AND (kind NOT IN ('source','note') OR opinion='endorse')
        AND ({predicates}) ORDER BY ({score}) DESC,last_seen_at DESC,id LIMIT 80",
        crate::storage::WIKI_PAGE_COLS
    );
    let pages = store
        .connection
        .prepare(&sql)?
        .query_map(
            rusqlite::params_from_iter(terms.iter()),
            crate::storage::map_wiki_page,
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut ranked = Vec::new();
    for page in pages {
        let metadata = store.knowledge_metadata(&page.slug)?;
        let indexed = format!(
            "{} {} {}",
            page.title,
            page.tags.join(" "),
            metadata.applicable_when
        )
        .to_lowercase();
        let body = page.content_md.to_lowercase();
        let prominent = terms
            .iter()
            .filter(|t| indexed.contains(t.as_str()))
            .count();
        let body_hits = terms.iter().filter(|t| body.contains(t.as_str())).count();
        if prominent == 0 && body_hits == 0 {
            continue;
        }
        if matches!(page.kind.as_str(), "method" | "case" | "principle")
            && (metadata.applicable_when.is_empty()
                || !terms
                    .iter()
                    .any(|t| metadata.applicable_when.to_lowercase().contains(t)))
        {
            continue;
        }
        if let Some(mut citation) = citation_for_page(
            store,
            &page,
            format!("问题与页面主题/正文匹配（{} 处主题线索）", prominent),
            1000,
        )? {
            citation.excerpt = matching_excerpt(&page.content_md, &terms, 1000);
            ranked.push((prominent * 4 + body_hits.min(10), citation));
        }
    }
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.page_slug.cmp(&b.1.page_slug)));
    let mut selected = Vec::new();
    for (_, citation) in ranked.into_iter().take(12) {
        let mut next = selected.clone();
        next.push(citation);
        if serde_json::to_string(&next)?.chars().count() <= budget {
            selected = next;
        }
        if selected.len() == 6 {
            break;
        }
    }
    Ok(selected)
}

/// 造引用的默认通道：孵化闸门生效（自动投递路径：召回、挂载、依赖核验、
/// 维护）。孵化中（source/note 页 opinion 为 NULL）不产出引用。
pub fn citation_for_page(
    store: &Store,
    page: &WikiPage,
    reason: String,
    excerpt_limit: usize,
) -> Result<Option<KnowledgeCitation>> {
    citation_for_page_inner(store, page, reason, excerpt_limit, false)
}

/// 造引用的显式意志通道：豁免孵化闸门（用户明确打开页面、派生稿采用前
/// 核对）。仍排除归档与否定页——显式意志不推翻已做的决定。
pub fn citation_for_page_explicit(
    store: &Store,
    page: &WikiPage,
    reason: String,
    excerpt_limit: usize,
) -> Result<Option<KnowledgeCitation>> {
    citation_for_page_inner(store, page, reason, excerpt_limit, true)
}

fn citation_for_page_inner(
    store: &Store,
    page: &WikiPage,
    reason: String,
    excerpt_limit: usize,
    allow_incubating: bool,
) -> Result<Option<KnowledgeCitation>> {
    if page.status == "archived"
        || page.opinion.as_deref() == Some("reject")
        || (!allow_incubating && !participating(page))
        || dependencies::stale(store, &page.id)?
    {
        return Ok(None);
    }
    let snapshots = store.page_source_snapshots(&page.slug)?;
    for snapshot in &snapshots {
        if !queue::usable(store, snapshot)? {
            return Ok(None);
        }
        let latest: i64 = store.connection.query_row(
            "SELECT MAX(version) FROM knowledge_snapshots WHERE source_id=?1",
            [&snapshot.source_id],
            |r| r.get(0),
        )?;
        if snapshot.version != latest {
            return Ok(None);
        }
    }
    let mut event_ids = Vec::new();
    for id in &page.source_event_ids {
        if !store.recordable_event(id)? {
            return Ok(None);
        }
        event_ids.push(id.clone());
    }
    event_ids.truncate(8);
    // Unattributed generated prose is not evidence. Explicit directory reports
    // retain their file:// origin without importing an entire directory.
    if snapshots.is_empty() && event_ids.is_empty() && page.source_url.is_none() {
        return Ok(None);
    }
    let metadata = store.knowledge_metadata(&page.slug)?;
    let sources = snapshots
        .into_iter()
        .take(8)
        .map(|s| SourceReference {
            snapshot_id: s.id,
            title: shorten(&s.title, 100),
            locator: s.locator,
            version: s.version,
        })
        .collect::<Vec<_>>();
    Ok(Some(KnowledgeCitation {
        page_slug: page.slug.clone(),
        content_hash: content_hash(&page.content_md),
        title: shorten(&page.title, 100),
        excerpt: shorten(&page.content_md, excerpt_limit.min(2500)),
        reason,
        applicable_when: shorten(&metadata.applicable_when, 600),
        strength: metadata.strength,
        category: if !sources.is_empty() {
            "外部材料或基于材料的推断"
        } else if page.source_url.is_some() {
            "项目或外部来源报告（AI 推断）"
        } else if matches!(
            page.kind.as_str(),
            "insight" | "method" | "case" | "principle"
        ) || page.tags.iter().any(|t| t == "topic-plan")
        {
            "AI 推断"
        } else {
            "用户记录"
        }
        .into(),
        sources,
        event_ids,
    }))
}

/// A model response may arrive after the user changes the page or its sources.
/// Only the exact material still eligible at that point can be a verified citation.
pub fn current_candidates(
    store: &Store,
    candidates: &[KnowledgeCitation],
) -> Result<Vec<KnowledgeCitation>> {
    let mut current = Vec::new();
    for candidate in candidates {
        let Some(page) = store.get_wiki_page(&candidate.page_slug)? else {
            continue;
        };
        let Some(fresh) = citation_for_page(store, &page, String::new(), 0)? else {
            continue;
        };
        if fresh.content_hash == candidate.content_hash
            && fresh.strength == candidate.strength
            && fresh.applicable_when == candidate.applicable_when
            && fresh.event_ids == candidate.event_ids
            && fresh
                .sources
                .iter()
                .map(|s| &s.snapshot_id)
                .eq(candidate.sources.iter().map(|s| &s.snapshot_id))
        {
            current.push(candidate.clone());
        }
    }
    Ok(current)
}

pub fn context_text(candidates: &[KnowledgeCitation]) -> Result<String> {
    if candidates.is_empty() {
        return Ok("没有适用的已核验知识材料；允许不引用，不得声称已找到依据。".into());
    }
    Ok(format!("知识选材：\n{}", tool_context(candidates)?))
}

/// 依据当前用户问题自动挂载少量已核验知识的摘要段。
/// 背景：2026-10 的速度优化硬掉了每轮知识预载（固定 3.2k tokens），把
/// 「用不用知识」完全赌在模型的 search_knowledge_base 工具调用上；
/// 2026-10-18/19 事故证明长对话里这正是知识投递断供的时刻。本函数
/// 是折中重建：只挂 AI 摘要（不在整包正文），上限 3 页、总字符封顶；
/// 沿用「知识选材：」JSON 前缀，引用校验与 usage 记录零改动。返回
/// 空串表示没有命中，调用方不挂空段。
/// 返回 (挂载文本, 命中页 slug 列表)：列表供 knowledge_mounted 事件
/// 与健康巡检统计投递面。
pub fn summary_mount(
    store: &Store,
    query: &str,
    max_pages: usize,
) -> Result<(String, Vec<String>)> {
    const MOUNT_CHAR_LIMIT: usize = 3000;
    let candidates = select_knowledge(store, query, "conversation", 2400)?;
    let mut slim = Vec::new();
    for citation in candidates.into_iter().take(max_pages) {
        let Some(page) = store.get_wiki_page(&citation.page_slug)? else {
            continue;
        };
        let mut mounted = citation;
        // current_candidates 只比对 content_hash/strength/applicable_when/
        // event_ids/sources，不比对 excerpt：把千字级正文摘录换成 AI 摘要
        // 是安全的，挂载体积从整包预载降一个量级。
        mounted.excerpt = mount_summary(&page.summary, &page.content_md);
        slim.push(mounted);
    }
    while !slim.is_empty() && serde_json::to_string(&slim)?.chars().count() > MOUNT_CHAR_LIMIT {
        slim.pop();
    }
    if slim.is_empty() {
        return Ok((String::new(), Vec::new()));
    }
    let slugs = slim.iter().map(|c| c.page_slug.clone()).collect();
    // 包装键与 tool_context 一致：candidates_in_context 只认
    // {"knowledge_candidates":[...]} 形态。
    let text = format!(
        "知识选材：\n{}",
        serde_json::to_string(&serde_json::json!({ "knowledge_candidates": slim }))?
    );
    Ok((text, slugs))
}
