//! 页级 TL;DR 摘要索引的后台生成（P1 分层检索第一层）。
//!
//! 检索第一跳只需要「这页讲什么、适用什么」的目录级信息；摘要由后台
//! tick 为缺 TL;DR 的页逐一生成（每 tick 至多 2 页，失败静默顺延——
//! 索引是尽力而为的加速层，缺页只退回现状的正文摘录，不阻断检索）。
//! 账本 task='page-tldr'；summary 锚定页面 content_hash，页面更新后
//! 自动视为过期等待重建（`Store::page_tldr` 按 hash 命中才返回）。

use crate::ai::memory::ContextMessage;
use crate::ai::provider::AiProvider;
use crate::storage::Store;
use anyhow::Result;

/// 每 tick 最多生成的页数。索引重建是渐进的：10 页库几轮 tick 补完，
/// 1000 页库按 last_seen 优先级逐日覆盖，不挤占同一 tick 的其他任务。
const PAGES_PER_TICK: usize = 2;

/// TL;DR 目标长度（字符）。200-300 字符目录级信息：一页检索命中
/// 约 100 token，20 页命中也只占 ~2K token 输入预算。
const TLDR_CHARS: usize = 300;

pub(crate) fn run_page_tldr_backfill(store: &Store, provider: &dyn AiProvider) -> Result<i64> {
    if !store.background_processing_allowed()? {
        return Ok(0);
    }
    // 已有摘要但页面更新的过期行不算「缺失」——它们仍能被 page_tldr 的
    // hash 校验识别为过期并退回正文摘录；重建优先补完全缺失的页。
    let candidates = store.pages_missing_tldr(PAGES_PER_TICK)?;
    if candidates.is_empty() {
        return Ok(0);
    }
    let mut generated = 0i64;
    for (slug, total_chars) in candidates {
        let Ok(Some(page)) = store.get_wiki_page(&slug) else {
            continue;
        };
        // 长文只喂开头 + 小节骨架：TL;DR 是目录级信息，不需要全文精读。
        // 与 LLM wiki 全文阅读纪律一致——生成物标注为摘要，不冒充原文。
        let body: String = page.content_md.chars().take(6000).collect();
        let prompt = format!(
            "用不超过 {TLDR_CHARS} 个汉字概括这篇知识页的核心内容：它讲什么、\
             给出什么可执行的方法或结论。直接输出摘要正文，不要标题、前言或解释。\
             这是生成摘要，不是原文引用。\n\n\
             页面标题：{title}\n页面类型：{kind}\n全文约 {total} 字，以下为开头部分：\n\n{body}",
            title = page.title,
            kind = page.kind,
            total = total_chars,
            body = body,
        );
        let reply = match provider.generate_reply(vec![
            ContextMessage::new(
                "system",
                "你是个人知识库的索引员。输出紧凑的中文摘要，不加任何前后缀。".to_string(),
            ),
            ContextMessage::new("user", prompt),
        ]) {
            Ok(reply) => reply,
            Err(error) => {
                crate::logging::log_always(&format!(
                    "[page-tldr] 页面 {slug} 摘要生成失败，顺延到下轮：{error:#}"
                ));
                continue;
            }
        };
        let summary = reply.content.trim();
        if summary.is_empty() {
            continue;
        }
        if let Err(error) = store.commit_page_tldr(&slug, summary, reply.model.as_deref()) {
            crate::logging::log_always(&format!(
                "[page-tldr] 页面 {slug} 摘要保存失败：{error:#}"
            ));
            continue;
        }
        generated += 1;
    }
    if generated > 0 {
        crate::logging::log_always(&format!("[page-tldr] 本轮生成 {generated} 条页摘要"));
    }
    Ok(generated)
}
