//! LLM wiki：个人知识库层。
//!
//! 思想来自 Karpathy 的 "LLM Wiki" 模式（gist 442a6bf555914893e9891c11519de94f）：
//!
//! - 三层架构：原始事件（不可变，事实来源） / wiki 页（LLM 维护、核心合并的 markdown） / schema（约束 LLM 的规则）
//! - "编译一次，持续更新"：知识不是每次查询时从原始文档重推导，而是持续复利的产物
//! - 三个操作：ingest（写回） / query（导航 + 引用） / lint（健康检查）
//! - 好答案归档回 wiki 成为新页面，探索也复利
//!
//! 本模块：确定性合并、校验、索引与导出。LLM 只提供"提议"，本模块按规则落地。

use crate::ai::memory::ContextMessage;
use crate::ai::provider::{AiProvider, OpenAiCompatibleConfig, OpenAiCompatibleProvider};

use crate::storage::{EventRecord, Store, WikiPage};
use anyhow::{Context, Result};

pub const WIKI_PROMPT_VERSION: &str = "wiki-digest-v1";

/// 允许的页面类型
pub const WIKI_KINDS: &[&str] = &[
    "profile",
    "person",
    "recurring_cost",
    "capability",
    "asset",
    "project",
    "relationship",
    "decision",
    "habit",
    "constraint",
    "insight",
    "topic",
    "source",
    // 用户粘贴笔记（M1 kind 拆分：从 topic 独立为素材档 note）
    "note",
    "method",
    "case",
    "principle",
    // 记事本（人类专属，无 migration）：memo 只能经 create_wiki_page 建档；
    // 正文追加必须走单事务 append_wiki_page_content，不走读-改-写乐观锁路径
    // （评审已通过，见 notes/proposed/product/2026-10-19-notepad-memo-pages.md §8）。
    "memo",
];

/// AI 消化（digest）可写的 kind。memo 被明确排除：它是人类专属的
/// 记事本，digest 的整篇覆盖写回不能碰（评审 D1）。
pub fn validate_digest_kind(kind: &str) -> bool {
    WIKI_KINDS.contains(&kind) && kind != "memo"
}

/// 认知推微生成时要导航的页面类型（镜像四透镜的取材范围）
pub const INSIGHT_KINDS: &[&str] = &[
    "profile",
    "recurring_cost",
    "capability",
    "asset",
    "habit",
    "constraint",
    "decision",
    "relationship",
];

pub fn validate_kind(kind: &str) -> bool {
    WIKI_KINDS.contains(&kind)
}

pub fn validate_slug(slug: &str) -> bool {
    let seg_ok = |s: &str| {
        !s.is_empty()
            && !s.starts_with('-')
            && !s.ends_with('-')
            && !s.contains(' ')
            && s.chars().all(|c| c.is_alphanumeric() || c == '-')
    };
    let parts: Vec<&str> = slug.split('/').collect();
    parts.len() <= 2 && parts.iter().all(|p| seg_ok(p))
}

/// 任意字符串 → 小写 kebab-case（用于 insight 归档页 slug）
pub fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for c in s.chars() {
        let keep = if c.is_alphanumeric() {
            Some(c.to_ascii_lowercase())
        } else {
            None
        };
        if let Some(k) = keep {
            out.push(k);
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        out = "page".to_string();
    }
    out
}

/// 生成不冲突的 slug：若 `{base}` 已被「不同标题」的页面占用（同名 slug 撞车，
/// 如历史页标题不同但 slugify 后相同），则追加 -2、-3… 后缀避让。
/// 同名同人走 find_wiki_page_by_title 合并，这里只兜底冲突。
pub(crate) fn unique_slug(store: &Store, base: &str, title: &str) -> Result<String> {
    let mut slug = base.to_string();
    let mut n = 2usize;
    loop {
        match store.get_wiki_page(&slug)? {
            None => return Ok(slug),
            Some(page) if page.title.trim() == title => return Ok(slug),
            Some(_) => {
                slug = format!("{base}-{n}");
                n += 1;
            }
        }
    }
}

fn kind_dir(kind: &str) -> String {
    kind.replace('_', "-")
}

/// 生成 wiki 索引 markdown（导航用，对应 LLM wiki 的 index.md）。按 kind 分组，一行摘要 + 证据数。
pub fn build_index_md(pages: &[WikiPage]) -> String {
    let mut lines = vec!["# 知识库索引".to_string()];
    let mut by_kind: Vec<(String, Vec<&WikiPage>)> = Vec::new();
    for page in pages {
        if let Some(entry) = by_kind.iter_mut().find(|(k, _)| *k == page.kind) {
            entry.1.push(page);
        } else {
            by_kind.push((page.kind.clone(), vec![page]));
        }
    }
    by_kind.sort_by(|a, b| a.0.cmp(&b.0));
    for (kind, mut pages) in by_kind {
        pages.sort_by(|a, b| b.evidence_count.cmp(&a.evidence_count));
        lines.push(format!("\n### {}", kind));
        for p in pages {
            let e = if p.evidence_count > 0 {
                format!("（{} 条事件支持）", p.evidence_count)
            } else {
                String::new()
            };
            lines.push(format!("- `{}` — {}{}", p.slug, p.summary, e));
        }
    }
    lines.join("\n")
}

/// 选取用于 prompt 上下文的页面（数量与字符上限）
pub fn select_context_pages(
    pages: Vec<WikiPage>,
    kinds: &[&str],
    max_chars: usize,
) -> Vec<WikiPage> {
    let mut picked: Vec<WikiPage> = pages
        .into_iter()
        .filter(|p| p.status != "archived" && kinds.contains(&p.kind.as_str()))
        .collect();
    picked.sort_by(|a, b| {
        b.evidence_count
            .cmp(&a.evidence_count)
            .then_with(|| b.last_seen_at.cmp(&a.last_seen_at))
    });
    let mut total = 0usize;
    picked.retain(|p| {
        let cost = p.content_md.len().min(4000);
        if total + cost <= max_chars {
            total += cost;
            true
        } else {
            false
        }
    });
    picked
}

/// 事件驱动的相关性选页（知识页持续更新提案 A，2026-10-20）。
///
/// 解决缺陷①：旧选页只看证据数/时间，本批事件提到的低证据页只以一行索引出现，
/// 模型拿不到正文所以永远不会 update 它。改为：从本批事件抽词项，标题/tags/
/// summary 命中权重高、正文次之，evidence/时间只做平票裁定；命中页给全文
/// （单页 ≤4000 字），总预算 max_chars 维持。零命中时返回空——此时索引行已足够，
/// 也顺着提案 B 的「宁缺毋滥」减少提示词噪声。
/// 1-hop 关系邻居加分留到提案 D 落地后接入（图不存在时跳过，不阻塞本函数）。
const CONTEXT_MIN_SCORE: i64 = 6;
const CONTEXT_PAGE_MAX_CHARS: usize = 4000;

pub fn select_context_pages_for_events(
    events: &[EventRecord],
    pages: Vec<WikiPage>,
    kinds: &[&str],
    max_chars: usize,
) -> Vec<WikiPage> {
    // 逐事件抽词项合并（单事件内复用 knowledge 的词项提取），避免拼接后
    // 超过词项提取的内部截断导致尾部事件被忽略。
    let mut terms = Vec::new();
    for event in events {
        for term in crate::knowledge::query_terms(&event.raw_text) {
            if !terms.contains(&term) {
                terms.push(term);
            }
        }
    }
    if terms.is_empty() {
        return Vec::new();
    }
    let mut scored: Vec<(i64, WikiPage)> = pages
        .into_iter()
        .filter(|p| p.status != "archived" && kinds.contains(&p.kind.as_str()))
        .map(|p| {
            let mut score = 0i64;
            let title = p.title.to_lowercase();
            let summary = p.summary.to_lowercase();
            let body = p.content_md.to_lowercase();
            for term in &terms {
                if title.contains(term.as_str()) {
                    score += 8;
                }
                if p.tags
                    .iter()
                    .any(|t| t.to_lowercase().contains(term.as_str()))
                {
                    score += 6;
                }
                if summary.contains(term.as_str()) {
                    score += 4;
                }
                if body.contains(term.as_str()) {
                    score += 2;
                }
            }
            (score, p)
        })
        .collect();
    scored.retain(|(score, _)| *score >= CONTEXT_MIN_SCORE);
    scored.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| b.1.evidence_count.cmp(&a.1.evidence_count))
            .then_with(|| b.1.last_seen_at.cmp(&a.1.last_seen_at))
    });
    let mut total = 0usize;
    scored
        .into_iter()
        .filter_map(|(_, p)| {
            let cost = p.content_md.len().min(CONTEXT_PAGE_MAX_CHARS);
            if total + cost <= max_chars {
                total += cost;
                Some(p)
            } else {
                None
            }
        })
        .collect()
}

/// 标题归一化：去空白与常见连接符后小写比较（近似 slug 检查用）。
fn normalize_page_title(title: &str) -> String {
    title
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace() && !c.is_ascii_punctuation() && !"·—–_".contains(*c))
        .collect()
}

/// 提案 B（2026-10-20）：create 前的近似检查。与既有页 kind 相同、归一化
/// 标题逐字相等且至少 1 个 tag 重合的新建提议 → 改写为对既有页的 update
/// （下游按 slug 是否已存在推断 create/update，重写 slug 即可）。判定条件
/// 刻意收窄：宁可漏判走 create（有 wiki_revisions 可撤销），不可错合并
/// 「同标题不同实体」的页。
fn redirect_synonym_creates(
    store: &Store,
    mut proposals: Vec<DigestProposal>,
) -> Result<Vec<DigestProposal>> {
    let pages = store.list_wiki_pages(None, None)?;
    for p in &mut proposals {
        if p.op != "create" {
            continue;
        }
        let normalized = normalize_page_title(&p.title);
        if normalized.is_empty() {
            continue;
        }
        if let Some(existing) = pages.iter().find(|page| {
            page.kind == p.kind
                && normalize_page_title(&page.title) == normalized
                && p.tags.iter().any(|t| page.tags.contains(t))
        }) {
            p.reason = Some(format!(
                "与既有页 {} 同主题，按纪律并入更新：{}",
                existing.slug,
                p.reason
                    .clone()
                    .unwrap_or_else(|| "digest 消化新事件".into())
            ));
            p.slug = existing.slug.clone();
            p.op = "update".into();
        }
    }
    Ok(proposals)
}

#[cfg(not(test))]
fn call_provider(store: &Store, system: &str, user: &str, max_tokens: u32) -> Result<String> {
    let ai_config = store
        .active_ai_provider_config()?
        .context("没有可用的 AI Provider 配置，请先 `elsewhen settings` 配置")?;
    let provider_config = OpenAiCompatibleConfig {
        base_url: ai_config.base_url,
        api_key: ai_config.api_key,
        model: ai_config.model,
        temperature: 0.3,
        max_tokens: Some(max_tokens),
        context_window: ai_config.context_window.map(|v| v as u32),
    };
    let provider = OpenAiCompatibleProvider::new(provider_config)?;
    provider
        .generate_reply(vec![
            ContextMessage::new("system", system.to_string()),
            ContextMessage::new("user", user.to_string()),
        ])
        .map(|r| r.content)
}

#[cfg(test)]
fn call_provider(_store: &Store, _system: &str, user: &str, _max_tokens: u32) -> Result<String> {
    Ok(user.to_string())
}

pub mod digest;
pub mod export_lint;
pub mod media_download;
pub mod people_relations;
pub mod reconsolidate;
pub mod tweet_import;

// 路径兼容转发：原平铺符号继续可经 crate::wiki:: 直达。
pub use digest::*;
pub use export_lint::*;
pub use people_relations::*;
pub use reconsolidate::*;
pub use tweet_import::*;

#[cfg(test)]
#[path = "wiki_tests/mod.rs"]
mod tests;
