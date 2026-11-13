//! 页级 TL;DR 摘要索引的后台生成（分层检索第一层）。
//!
//! 检索第一跳只需要「这页讲什么、适用什么」的目录级信息；摘要由后台 tick
//! 为缺 TL;DR 的长页逐一生成（每 tick 至多 2 页）。索引是尽力而为的加速层，
//! 缺页只退回原文词窗摘录，不阻断检索。
//!
//! 边界：
//! - 只为**可引用**的页生成（`citation_for_page` 通过）。TL;DR 只在检索命中
//!   可引用页之后才被读取；记事本、孵化中、已否定、来源失效的页既不需要
//!   摘要，也不应被后台送给模型。
//! - 退避账本 `page_tldr_backoff`：生成失败的页按次数拉长退避，并且最近一次
//!   失败后整个回填冷却一段时间——不在每个 tick 重复请求。当前不可引用的页
//!   延后一小时再核对，不反复占用候选名额。
//! - 摘要锚定页面 content_hash；正文变化由迁移 83 的触发器删行，回填重建。
//! - 摘要是生成物：记录生成时实际读到的范围（basis），检索侧单独标注，
//!   不进入原文摘录。

use crate::ai::memory::ContextMessage;
use crate::ai::provider::AiProvider;
use crate::knowledge::select::TLDR_MIN_CHARS;
use crate::storage::{Store, WikiPage};
use anyhow::Result;

/// 每 tick 最多生成的页数：渐进补完，不挤占同一 tick 的其他任务。
const PAGES_PER_TICK: usize = 2;
/// 每 tick 最多核对多少个候选的引用资格（每次核对要读整页）。
const MAX_ELIGIBILITY_CHECKS: usize = 100;
/// 一次生成失败后，整个回填停多久。
const FAILURE_COOLDOWN_SECS: i64 = 600;
/// TL;DR 目标长度（字符），写进提示词。
const TLDR_CHARS: usize = 300;
/// 存储上限；超出的回复截到句末保存，而不是当作失败反复重试。
const TLDR_MAX_CHARS: usize = 600;
/// 不超过这个长度的页整页送入；更长的页送开头加标题骨架。
const FULL_INPUT_CHARS: usize = 6000;
const HEAD_CHARS: usize = 4000;
const OUTLINE_LINES: usize = 60;

pub(crate) fn run_page_tldr_backfill(store: &Store, provider: &dyn AiProvider) -> Result<i64> {
    if !store.background_processing_allowed()?
        || store.page_tldr_recently_failed(FAILURE_COOLDOWN_SECS)?
    {
        return Ok(0);
    }
    let mut generated = 0i64;
    for (page, hash) in eligible_pages(store)? {
        match generate(store, provider, &page, &hash) {
            Ok(()) => generated += 1,
            // 生成期间页面被改：这份摘要属于旧正文，丢弃即可，不算失败；
            // 新版本仍是候选，下个 tick 再生成。
            Err(error) if error.to_string().contains("page_tldr_version_changed") => {}
            Err(error) => {
                // 不把回复正文或页面原文写进日志。
                crate::logging::log_always(&format!(
                    "[page-tldr] 页面 {} 摘要未生成，退避后重试：{error:#}",
                    page.slug
                ));
                store.record_page_tldr_failure(&page.id, &hash)?;
                // 多半是上游故障：本 tick 不再对下一页重复请求。
                break;
            }
        }
    }
    if generated > 0 {
        crate::logging::log_always(&format!("[page-tldr] 本轮生成 {generated} 条页摘要"));
    }
    Ok(generated)
}

/// 预筛候选里真正可引用的前几页，连同各自的正文哈希。核对过但不可引用的页
/// 记入退避账本，下次查询不再出现——既不重复核对，也不挡住排在后面的页。
fn eligible_pages(store: &Store) -> Result<Vec<(WikiPage, String)>> {
    const BATCH: usize = 50;
    let mut eligible = Vec::new();
    let mut checked = 0;
    while eligible.len() < PAGES_PER_TICK && checked < MAX_ELIGIBILITY_CHECKS {
        // 已选中的页仍在候选里且排在最前，用 offset 跳过。
        let slugs = store.page_tldr_candidates(TLDR_MIN_CHARS, BATCH, eligible.len())?;
        if slugs.is_empty() {
            break;
        }
        for slug in slugs {
            checked += 1;
            let Some(page) = store.get_wiki_page(&slug)? else {
                continue;
            };
            let hash = crate::storage::knowledge::content_hash(&page.content_md);
            if crate::knowledge::citation_for_page(store, &page, String::new(), 0)?.is_some() {
                eligible.push((page, hash));
                if eligible.len() == PAGES_PER_TICK {
                    break;
                }
            } else {
                store.defer_page_tldr(&page.id, &hash)?;
            }
        }
    }
    Ok(eligible)
}

fn generate(store: &Store, provider: &dyn AiProvider, page: &WikiPage, hash: &str) -> Result<()> {
    let (basis, body) = input(store, page, hash)?;
    // 指令语言与知识输出语言按数据集策略固定在 bundle 上。同一套提示词与
    // 语言策略共用一个 bundle；提示词或策略变化后依赖哈希变化，自动换新。
    let _scope = store.prompt_scope(
        "background_task",
        &format!(
            "page-tldr:{}",
            store.prompt_dependency(&["task.processing.fixed_contract"])?
        ),
    )?;
    let system = format!(
        "{}{}",
        store.render_processing_instruction("tldr_system", &[])?,
        store.output_language_directive(false)?
    );
    let prompt = store.render_processing_instruction(
        "tldr_input",
        &[
            ("basis", basis.to_string()),
            ("body", body),
            ("kind", page.kind.clone()),
            ("max", TLDR_CHARS.to_string()),
            ("title", page.title.clone()),
            ("total", page.content_md.chars().count().to_string()),
        ],
    )?;
    let reply = provider.generate_reply(vec![
        ContextMessage::new("system", system),
        ContextMessage::new("user", prompt),
    ])?;
    if let Some(usage) = &reply.usage {
        store.record_token_usage(
            None,
            usage.prompt_tokens.min(i64::MAX as u64) as i64,
            usage.completion_tokens.min(i64::MAX as u64) as i64,
            (usage.prompt_tokens + usage.completion_tokens).min(i64::MAX as u64) as i64,
            reply.model.as_deref(),
        )?;
    }
    let summary = bounded(reply.content.trim());
    anyhow::ensure!(!summary.is_empty(), "模型返回了空摘要");
    store.commit_page_tldr(&page.slug, hash, &summary, basis, reply.model.as_deref())
}

/// 摘要的输入与它覆盖的范围。
/// - `full`：整页原文；
/// - `reading-root`：本页就是来源原文、且后台已读完全文——用全文阅读的根归纳
///   （生成摘要，但覆盖全文，不只是开头）；
/// - `head-outline`：开头原文加其余部分的标题骨架。
fn input(store: &Store, page: &WikiPage, hash: &str) -> Result<(&'static str, String)> {
    let total = page.content_md.chars().count();
    if total <= FULL_INPUT_CHARS {
        return Ok(("full", page.content_md.clone()));
    }
    for snapshot in store.page_source_snapshots(&page.slug)? {
        if snapshot.content_hash == hash {
            if let Some(summary) = store.source_reading_root_summary(&snapshot.id)? {
                return Ok(("reading-root", summary));
            }
        }
    }
    let head: String = page.content_md.chars().take(HEAD_CHARS).collect();
    let rest: String = page.content_md.chars().skip(HEAD_CHARS).collect();
    let outline = rest
        .lines()
        .filter(|line| line.trim_start().starts_with('#'))
        .take(OUTLINE_LINES)
        .map(|line| line.trim().chars().take(80).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");
    Ok(("head-outline", format!("{head}\n\n…\n\n{outline}")))
}

/// 超长回复截到上限内最后一个句末；找不到句末就硬截并标省略。
fn bounded(summary: &str) -> String {
    if summary.chars().count() <= TLDR_MAX_CHARS {
        return summary.to_string();
    }
    let cut: String = summary.chars().take(TLDR_MAX_CHARS - 1).collect();
    match cut.rfind(['。', '！', '？', '.', '!', '?', '\n']) {
        Some(index) if cut[..index].chars().count() >= TLDR_CHARS / 2 => {
            let end = index + cut[index..].chars().next().map_or(0, char::len_utf8);
            cut[..end].trim_end().to_string()
        }
        _ => format!("{cut}…"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlong_reply_is_cut_at_a_sentence_end_within_the_limit() {
        let reply = format!("{}。{}", "甲".repeat(400), "乙".repeat(400));
        let cut = bounded(&reply);
        assert_eq!(cut, format!("{}。", "甲".repeat(400)));
        let hard = bounded(&"丙".repeat(900));
        assert_eq!(hard.chars().count(), TLDR_MAX_CHARS);
        assert!(hard.ends_with('…'));
        assert_eq!(bounded("短摘要。"), "短摘要。");
    }
}
