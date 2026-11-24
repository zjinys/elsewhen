//! 合成型问题的证据提取：在工具内部分批阅读原文，只把校验过的逐字证据
//! 交回主对话上下文。
//!
//! 流程：选页 → 6000 字符分块 → 按子问题的词项选块 → 分批子请求提取 →
//! 逐字校验并定位 → 确定性装配。原文不进入主上下文。
//!
//! 边界：
//! - 每条证据的 quote 都从页面正文按字符区间重新切出，一定是原文；kind 与
//!   note 是提取模型的归类，返回时照此标注，系统不为它们作保。
//! - 读哪些页只由现有的引用资格决定（显式指定走显式通道，检索到的走默认
//!   通道）；拿不到引用的页不读，列入 skipped。
//! - 没读到的范围写进 coverage.unread，不把「读了一部分」当成「读了全部」。
//! - 只读：不写知识页、不建提案。子请求只经 `generation::tool_request`，
//!   共享取消、租约、预算与记账。

use super::*;
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

/// 与全文阅读（reading.rs）同一分块宽度；块尾多带一段，避免句子被切断。
const CHUNK: usize = 6000;
const OVERLAP: usize = 300;
/// 一次合成至多发起的子请求数。
const MAX_REQUESTS: usize = 6;
/// 每个子请求至多放几块。
const MAX_CHUNKS_PER_REQUEST: usize = 3;
const MAX_PAGES: usize = 8;
const MAX_SUB_QUESTIONS: usize = 6;
const MAX_QUERIES: usize = 4;
/// 交回主上下文的证据总量上限（quote 与 note 的字符数之和）。
const MAX_EVIDENCE_CHARS: usize = 6000;
/// 结果预算低于这个数也允许开始：至少返回 coverage、来源和部分证据，
/// 不把“部分可用结果”误报成整轮失败。最终证据数量由 assemble 再按预算裁剪。
const MIN_RESULT_TOKENS: usize = 800;
/// 连续这么多批失败就收手：多半是渠道故障，不逐批耗到超时。
const MAX_CONSECUTIVE_FAILURES: usize = 2;
/// 装批时给子请求的输出与估算误差留的余量（估算 token）。
const REQUEST_SLACK_TOKENS: usize = 1024;
/// 超过这个时长不再发新的子请求；已得到的证据照常返回。
const SOFT_DEADLINE: Duration = Duration::from_secs(240);
const QUOTE_CHARS: std::ops::RangeInclusive<usize> = 8..=300;
const KINDS: [&str; 6] = [
    "fact",
    "opinion",
    "data",
    "conclusion",
    "limitation",
    "counter",
];
/// 换渠道重跑时复用刚做完的提取，不重复付费。只在进程内、只留十分钟。
const CACHE_TTL: Duration = Duration::from_secs(600);

pub(crate) struct Request {
    pub question: String,
    pub sub_questions: Vec<String>,
    pub page_slugs: Vec<String>,
    pub queries: Vec<String>,
}

/// 子请求走的渠道，以及结果最多可占用的输入预算（估算 token）与计数所用模型。
pub(crate) struct Channel<'a> {
    pub provider: &'a dyn AiProvider,
    pub provider_name: &'a str,
    pub result_budget: Option<(usize, &'a str)>,
}

struct Page {
    page: WikiPage,
    citation: KnowledgeCitation,
    /// 模型明确指定的页：没有词项命中的块也读（通读长页的场景）。
    explicit: bool,
    chars: Vec<char>,
}

struct Chunk {
    page: usize,
    start: usize,
    end: usize,
    text: String,
    /// 每个子问题在这一块里命中的词项数。
    scores: Vec<usize>,
}

#[derive(Clone)]
struct Evidence {
    questions: BTreeSet<usize>,
    kind: String,
    page: usize,
    start: usize,
    end: usize,
    note: String,
}

#[derive(Clone, Default)]
struct Extracted {
    evidence: Vec<Evidence>,
    /// 成功提取过的块（chunks 的下标）。
    read: Vec<usize>,
    requests: usize,
    rejected: usize,
    failed_batches: usize,
    stopped_early: bool,
}

#[derive(Deserialize)]
struct Reply {
    evidence: Vec<RawEvidence>,
}

#[derive(Deserialize)]
struct RawEvidence {
    q: usize,
    chunk: usize,
    #[serde(default)]
    kind: String,
    quote: String,
    #[serde(default)]
    note: String,
}

static CACHE: LazyLock<Mutex<HashMap<String, (Instant, Extracted)>>> =
    LazyLock::new(Default::default);

pub(crate) fn synthesize(
    store: &Store,
    conversation_id: &str,
    channel: &Channel<'_>,
    request: &Request,
) -> Result<String> {
    validate(request)?;
    // 低预算仍执行提取；assemble 会优先保留 coverage 与最少证据，
    // 让上层可以继续分批或提示缺失页面，而不是丢掉已完成的读取。
    let started = Instant::now();
    let (pages, skipped) = select_pages(store, request)?;
    let terms: Vec<Vec<String>> = request
        .sub_questions
        .iter()
        .map(|question| query_terms(question))
        .collect();
    let chunks = chunk_pages(&pages, &terms);
    // 同一会话、同样的问题与页面版本、同样的读法（通读还是只读命中块）。
    let key = content_hash(
        &serde_json::json!([
            conversation_id,
            request.question,
            request.sub_questions,
            pages
                .iter()
                .map(|page| serde_json::json!([
                    page.page.slug,
                    page.citation.content_hash,
                    page.explicit
                ]))
                .collect::<Vec<_>>()
        ])
        .to_string(),
    );
    let cached = {
        let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        cache.retain(|_, (at, _)| at.elapsed() < CACHE_TTL);
        cache.get(&key).map(|(_, extracted)| extracted.clone())
    };
    let extracted = match cached {
        Some(extracted) => extracted,
        None => {
            let order = selection_order(&chunks, &pages, terms.len());
            let extracted = extract(store, channel, request, &pages, &chunks, order, started)?;
            // 一批都没读成：报错，而不是交回一份看起来「没有相关证据」的空结果。
            anyhow::ensure!(
                !extracted.read.is_empty() || extracted.failed_batches == 0,
                "synthesis_extraction_failed"
            );
            // 不完整的提取不缓存：重跑时应当再试一次。
            if extracted.failed_batches == 0 && !extracted.stopped_early {
                CACHE
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(key, (Instant::now(), extracted.clone()));
            }
            extracted
        }
    };
    let (result, kept, omitted) = assemble(
        store,
        request,
        &pages,
        &chunks,
        &extracted,
        &skipped,
        channel.result_budget,
    )?;
    // 运行记录只留计数，不留原文。
    crate::ai::generation::event(
        store,
        "knowledge_synthesis",
        serde_json::json!({
            "pages":pages.len(),"chunks_total":chunks.len(),"chunks_read":extracted.read.len(),
            "requests":extracted.requests,"evidence":kept,"omitted":omitted,
            "rejected_quotes":extracted.rejected,"failed_batches":extracted.failed_batches,
            "stopped_early":extracted.stopped_early,"elapsed_ms":started.elapsed().as_millis() as u64
        }),
    )?;
    Ok(result)
}

fn validate(request: &Request) -> Result<()> {
    let text = |value: &str, max: usize| !value.trim().is_empty() && value.chars().count() <= max;
    anyhow::ensure!(text(&request.question, 2000), "synthesis_question_invalid");
    anyhow::ensure!(
        (1..=MAX_SUB_QUESTIONS).contains(&request.sub_questions.len())
            && request.sub_questions.iter().all(|q| text(q, 300)),
        "synthesis_sub_questions_invalid"
    );
    anyhow::ensure!(
        request.page_slugs.len() <= MAX_PAGES && request.page_slugs.iter().all(|s| text(s, 300)),
        "synthesis_page_slugs_invalid"
    );
    anyhow::ensure!(
        request.queries.len() <= MAX_QUERIES && request.queries.iter().all(|q| text(q, 300)),
        "synthesis_queries_invalid"
    );
    Ok(())
}

/// 显式指定的页在前（显式通道），其后是检索到的页（默认通道，孵化闸门
/// 生效），合计至多 MAX_PAGES。只给了 page_slugs 时不再另行检索。
fn select_pages(
    store: &Store,
    request: &Request,
) -> Result<(Vec<Page>, Vec<(String, &'static str)>)> {
    let loaded = |page: WikiPage, citation: KnowledgeCitation, explicit: bool| Page {
        chars: page.content_md.chars().collect(),
        page,
        citation,
        explicit,
    };
    let mut pages = Vec::new();
    let mut skipped = Vec::new();
    let mut seen = HashSet::new();
    for slug in &request.page_slugs {
        if !seen.insert(slug.clone()) {
            continue;
        }
        let Some(page) = store.get_wiki_page(slug)? else {
            skipped.push((slug.clone(), "not_found"));
            continue;
        };
        match citation_for_page_explicit(store, &page, "合成时明确指定".into(), 0)? {
            Some(citation) => pages.push(loaded(page, citation, true)),
            None => skipped.push((slug.clone(), "not_citable")),
        }
    }
    let queries: &[String] = if !request.queries.is_empty() {
        &request.queries
    } else if request.page_slugs.is_empty() {
        &request.sub_questions
    } else {
        &[]
    };
    // 每个问法各自检索，按倒数排名融合。
    let mut ranked = HashMap::<String, (f64, KnowledgeCitation)>::new();
    for query in queries {
        for (index, citation) in select_knowledge(store, query, "synthesis", 12000)?
            .into_iter()
            .enumerate()
        {
            let entry = ranked
                .entry(citation.page_slug.clone())
                .or_insert((0.0, citation));
            entry.0 += 1.0 / (60.0 + index as f64);
        }
    }
    let mut ranked: Vec<_> = ranked.into_values().collect();
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.page_slug.cmp(&b.1.page_slug)));
    for (_, hit) in ranked {
        if pages.len() >= MAX_PAGES {
            break;
        }
        if !seen.insert(hit.page_slug.clone()) {
            continue;
        }
        let Some(page) = store.get_wiki_page(&hit.page_slug)? else {
            continue;
        };
        // 引用从刚读到的正文重新生成：内容哈希与要定位的正文是同一版。
        if let Some(citation) = citation_for_page(store, &page, hit.reason, 0)? {
            pages.push(loaded(page, citation, false));
        }
    }
    Ok((pages, skipped))
}

fn chunk_pages(pages: &[Page], terms: &[Vec<String>]) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    for (index, page) in pages.iter().enumerate() {
        let total = page.chars.len();
        let mut start = 0;
        while start < total {
            let end = (start + CHUNK + OVERLAP).min(total);
            let text: String = page.chars[start..end].iter().collect();
            let lowered = text.to_lowercase();
            chunks.push(Chunk {
                page: index,
                start,
                end,
                scores: terms
                    .iter()
                    .map(|terms| {
                        terms
                            .iter()
                            .filter(|term| lowered.contains(term.as_str()))
                            .count()
                    })
                    .collect(),
                text,
            });
            if end == total {
                break;
            }
            start += CHUNK;
        }
    }
    chunks
}

/// 读块的先后：先保证每个子问题得分最高的那一块，其余按总分从高到低；
/// 没有任何词项命中的块只在显式指定的页里保留，排在最后、按原文顺序。
fn selection_order(chunks: &[Chunk], pages: &[Page], questions: usize) -> Vec<usize> {
    let total = |index: usize| chunks[index].scores.iter().sum::<usize>();
    let mut order: Vec<usize> = (0..chunks.len())
        .filter(|&index| total(index) > 0 || pages[chunks[index].page].explicit)
        .collect();
    order.sort_by(|&a, &b| total(b).cmp(&total(a)).then(a.cmp(&b)));
    let mut front = Vec::new();
    for question in 0..questions {
        let best = order
            .iter()
            .copied()
            .filter(|&index| chunks[index].scores[question] > 0)
            .max_by(|&a, &b| {
                chunks[a].scores[question]
                    .cmp(&chunks[b].scores[question])
                    .then(b.cmp(&a))
            });
        if let Some(best) = best.filter(|best| !front.contains(best)) {
            front.push(best);
        }
    }
    order.retain(|index| !front.contains(index));
    front.extend(order);
    front
}

fn batch_messages(
    store: &Store,
    request: &Request,
    pages: &[Page],
    chunks: &[Chunk],
    batch: &[usize],
) -> Result<Vec<ContextMessage>> {
    let sub_questions = request
        .sub_questions
        .iter()
        .enumerate()
        .map(|(index, question)| format!("{index}. {}", question.trim()))
        .collect::<Vec<_>>()
        .join("\n");
    let body = batch
        .iter()
        .enumerate()
        .map(|(id, &index)| {
            let chunk = &chunks[index];
            format!(
                "<chunk id=\"{id}\" page=\"{}\" chars=\"{}-{}\">\n{}\n</chunk>",
                shorten(&pages[chunk.page].page.title, 100),
                chunk.start,
                chunk.end,
                chunk.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(vec![
        ContextMessage::new(
            "system",
            format!(
                "{}{}",
                store.render_processing_instruction("synthesis_system", &[])?,
                store.output_language_directive(true)?
            ),
        ),
        ContextMessage::new(
            "user",
            store.render_processing_instruction(
                "synthesis_input",
                &[
                    ("chunks", body),
                    ("question", request.question.trim().to_string()),
                    ("sub_questions", sub_questions),
                ],
            )?,
        ),
    ])
}

fn stopped(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<crate::storage::chat_runs::ChatRunStopped>()
        .is_some()
}

fn extract(
    store: &Store,
    channel: &Channel<'_>,
    request: &Request,
    pages: &[Page],
    chunks: &[Chunk],
    order: Vec<usize>,
    started: Instant,
) -> Result<Extracted> {
    let mut extracted = Extracted::default();
    let mut queue: VecDeque<usize> = order.into();
    let mut consecutive_failures = 0;
    while extracted.requests < MAX_REQUESTS {
        let Some(first) = queue.pop_front() else {
            break;
        };
        if started.elapsed() > SOFT_DEADLINE {
            extracted.stopped_early = true;
            break;
        }
        crate::ai::generation::check(store)?;
        // 贪心装批：放得进子请求的预算就继续加，至多 MAX_CHUNKS_PER_REQUEST 块。
        // provider 不报预算时按上限装。
        let mut batch = vec![first];
        while batch.len() < MAX_CHUNKS_PER_REQUEST {
            let Some(&next) = queue.front() else {
                break;
            };
            batch.push(next);
            let fits = channel
                .provider
                .input_headroom(
                    &batch_messages(store, request, pages, chunks, &batch)?,
                    None,
                )?
                .is_none_or(|(headroom, _)| headroom >= REQUEST_SLACK_TOKENS);
            if !fits {
                batch.pop();
                break;
            }
            queue.pop_front();
        }
        let outcome = extract_batch(
            store,
            channel,
            request,
            pages,
            chunks,
            &batch,
            &mut extracted.requests,
        );
        match outcome {
            Ok((evidence, rejected)) => {
                consecutive_failures = 0;
                extracted.evidence.extend(evidence);
                extracted.rejected += rejected;
                extracted.read.extend(batch);
            }
            // 用户停止或租约失效：立刻收手，不再发请求。
            Err(error) if stopped(&error) => return Err(error),
            // 单批失败（上游错误、两次都解析不了、本地预算拒绝）：其余批照常。
            Err(error) => {
                crate::logging::log_always(&format!("[synthesis] 一批提取失败：{error:#}"));
                extracted.failed_batches += 1;
                consecutive_failures += 1;
                if consecutive_failures == MAX_CONSECUTIVE_FAILURES {
                    extracted.stopped_early = true;
                    break;
                }
            }
        }
    }
    Ok(extracted)
}

/// 提取一批。`requests` 是本次合成已发出的模型请求数：每发一次加一，输出
/// 无法解析时的那次修复重试同样计入，并且只在还没到上限时才重试。
fn extract_batch(
    store: &Store,
    channel: &Channel<'_>,
    request: &Request,
    pages: &[Page],
    chunks: &[Chunk],
    batch: &[usize],
    requests: &mut usize,
) -> Result<(Vec<Evidence>, usize)> {
    let mut messages = batch_messages(store, request, pages, chunks, batch)?;
    let mut repaired = false;
    loop {
        *requests += 1;
        let reply = crate::ai::generation::tool_request(
            store,
            channel.provider,
            channel.provider_name,
            messages.clone(),
            "synthesizing",
        )?;
        let parsed = structured_text(&reply, "合成").and_then(|text| {
            let raw = text
                .trim_start_matches("```json")
                .trim_start_matches("```")
                .trim_end_matches("```")
                .trim();
            serde_json::from_str::<Reply>(raw).context("合成结果不是约定的 JSON")
        });
        match parsed {
            Ok(reply) => {
                return Ok(verify(
                    reply.evidence,
                    request.sub_questions.len(),
                    chunks,
                    batch,
                ));
            }
            Err(error) if !repaired && *requests < MAX_REQUESTS => {
                repaired = true;
                messages.push(ContextMessage::new(
                    "system",
                    store.render_processing_instruction(
                        "synthesis_repair",
                        &[("error", error.to_string())],
                    )?,
                ));
            }
            Err(error) => return Err(error),
        }
    }
}

/// 只留下能在原文里逐字找到的引文，并换算成页内字符区间。先在模型标注的
/// 块里找，找不到再看同批的其他块：标错块号不影响它是不是原文。
fn verify(
    items: Vec<RawEvidence>,
    questions: usize,
    chunks: &[Chunk],
    batch: &[usize],
) -> (Vec<Evidence>, usize) {
    let mut kept = Vec::new();
    let mut rejected = 0;
    for item in items {
        let quote = item.quote.trim();
        let length = quote.chars().count();
        let located = (item.q < questions && QUOTE_CHARS.contains(&length))
            .then(|| {
                batch
                    .get(item.chunk)
                    .into_iter()
                    .chain(batch.iter())
                    .find_map(|&index| chunks[index].text.find(quote).map(|bytes| (index, bytes)))
            })
            .flatten();
        let Some((index, bytes)) = located else {
            rejected += 1;
            continue;
        };
        let chunk = &chunks[index];
        let start = chunk.start + chunk.text[..bytes].chars().count();
        kept.push(Evidence {
            questions: BTreeSet::from([item.q]),
            kind: if KINDS.contains(&item.kind.as_str()) {
                item.kind
            } else {
                "other".into()
            },
            page: chunk.page,
            start,
            end: start + length,
            note: shorten(item.note.trim(), 120),
        });
    }
    (kept, rejected)
}

/// 把一组 [start,end) 区间并成不相交的有序区间。
fn union(mut ranges: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    ranges.sort();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in ranges {
        match merged.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    merged
}

/// 确定性装配，不调用模型：同页重叠的引文合并，按子问题轮转取用，总量按
/// 字符上限与结果预算封顶。返回 (结果 JSON, 入选条数, 因封顶略去的条数)。
fn assemble(
    store: &Store,
    request: &Request,
    pages: &[Page],
    chunks: &[Chunk],
    extracted: &Extracted,
    skipped: &[(String, &'static str)],
    budget: Option<(usize, &str)>,
) -> Result<(String, usize, usize)> {
    // 同页区间重叠的引文合并（块尾的重叠区会让同一句被摘两次）。
    let mut sorted: Vec<(usize, &Evidence)> = extracted.evidence.iter().enumerate().collect();
    sorted.sort_by_key(|(_, e)| (e.page, e.start, e.end));
    let mut merged: Vec<(usize, Evidence)> = Vec::new();
    for (order, evidence) in sorted {
        match merged.last_mut() {
            Some((first, last)) if last.page == evidence.page && evidence.start < last.end => {
                last.end = last.end.max(evidence.end);
                last.questions.extend(evidence.questions.iter().copied());
                *first = (*first).min(order);
            }
            _ => merged.push((order, evidence.clone())),
        }
    }
    // 按提取的先后（即选块的优先级）排，再按子问题轮转，保持各维度均衡。
    merged.sort_by_key(|(order, _)| *order);
    let mut used = vec![false; merged.len()];
    let mut ordered: Vec<usize> = Vec::new();
    loop {
        let before = ordered.len();
        for question in 0..request.sub_questions.len() {
            let next = (0..merged.len())
                .find(|&index| !used[index] && merged[index].1.questions.contains(&question));
            if let Some(index) = next {
                used[index] = true;
                ordered.push(index);
            }
        }
        if ordered.len() == before {
            break;
        }
    }

    let warnings = {
        let mut queries = vec![request.question.clone()];
        queries.extend(request.sub_questions.iter().take(3).cloned());
        super::negative::select_warnings(store, &queries, 800)?
    };
    let read_ranges = |page: usize| {
        union(
            extracted
                .read
                .iter()
                .map(|&index| &chunks[index])
                .filter(|chunk| chunk.page == page)
                .map(|chunk| (chunk.start, chunk.end))
                .collect(),
        )
    };
    let pairs = |ranges: &[(usize, usize)]| -> Vec<[usize; 2]> {
        ranges.iter().map(|&(start, end)| [start, end]).collect()
    };
    let mut reading = serde_json::Map::new();
    let mut unread = Vec::new();
    for (index, page) in pages.iter().enumerate() {
        let read = read_ranges(index);
        let mut gaps = Vec::new();
        let mut cursor = 0;
        for &(start, end) in &read {
            if start > cursor {
                gaps.push((cursor, start));
            }
            cursor = end;
        }
        if cursor < page.chars.len() {
            gaps.push((cursor, page.chars.len()));
        }
        if !gaps.is_empty() {
            unread.push(serde_json::json!({"page_slug":page.page.slug,"ranges":pairs(&gaps)}));
        }
        if !read.is_empty() {
            reading.insert(
                page.page.slug.clone(),
                serde_json::json!({"coverage":"evidence","total_chars":page.chars.len(),"read_ranges":pairs(&read)}),
            );
        }
    }
    let render = |count: usize, warnings: Option<&[serde_json::Value]>| -> String {
        let chosen: Vec<&Evidence> = ordered[..count].iter().map(|&i| &merged[i].1).collect();
        // 只有留下了已校验引文的页才进引用白名单（引用本身不带摘录）。
        let candidates: Vec<&KnowledgeCitation> = pages
            .iter()
            .enumerate()
            .filter(|(index, _)| chosen.iter().any(|e| e.page == *index))
            .map(|(_, page)| &page.citation)
            .collect();
        let evidence: Vec<_> = chosen
            .iter()
            .map(|e| {
                serde_json::json!({
                    "q":e.questions,"kind":e.kind,"page_slug":pages[e.page].page.slug,
                    "start_char":e.start,"end_char":e.end,
                    "quote":pages[e.page].chars[e.start..e.end].iter().collect::<String>(),
                    "note":e.note
                })
            })
            .collect();
        let mut coverage = serde_json::json!({
            "pages_read":reading.len(),"chunks_read":extracted.read.len(),"chunks_total":chunks.len(),
            "requests":extracted.requests,"unread":unread,"rejected_quotes":extracted.rejected,
            "omitted":ordered.len() - count,"failed_batches":extracted.failed_batches
        });
        if extracted.stopped_early {
            coverage["stopped_early"] = true.into();
        }
        if warnings.is_none() {
            coverage["warnings_omitted"] = true.into();
        }
        serde_json::json!({
            "knowledge_candidates":candidates,"knowledge_reading":reading,"evidence":evidence,
            "coverage":coverage,
            "skipped":skipped.iter().map(|(slug, reason)| serde_json::json!({"page_slug":slug,"reason":reason})).collect::<Vec<_>>(),
            "knowledge_warnings":warnings.unwrap_or_default()
        })
        .to_string()
    };
    // 结果作为 JSON 字符串嵌进请求，按嵌入后的形态计数。
    let fits = |count: usize, warnings: Option<&[serde_json::Value]>| {
        budget.is_none_or(|(tokens, model)| {
            crate::ai::budget::text_tokens(
                model,
                &serde_json::Value::String(render(count, warnings)).to_string(),
            ) <= tokens
        })
    };
    // 历史否定提醒连同覆盖情况都放不下时先舍掉提醒并注明，证据优先；
    // 搜索和自动挂载仍会各自带上它。
    let warnings = Some(warnings.as_slice()).filter(|warnings| fits(0, Some(warnings)));
    anyhow::ensure!(fits(0, warnings), "synthesis_budget_exhausted");
    let mut count = 0;
    let mut chars = 0;
    while count < ordered.len() {
        let evidence = &merged[ordered[count]].1;
        chars += evidence.end - evidence.start + evidence.note.chars().count();
        if chars > MAX_EVIDENCE_CHARS || !fits(count + 1, warnings) {
            break;
        }
        count += 1;
    }
    Ok((render(count, warnings), count, ordered.len() - count))
}
