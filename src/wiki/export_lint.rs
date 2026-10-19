//! 自 wiki.rs 机械分段平移（行为零变化）。



use crate::storage::{Store};
use anyhow::{Context, Result};
use std::path::{Path};
use crate::wiki::{build_index_md, kind_dir};
// ── export：物化 markdown 树（可接 Obsidian）──────────────────────────────

pub struct ExportReport {
    pub files: Vec<String>,
    pub dir: String,
}

/// 把 wiki 物化为 markdown 目录：index.md + log.md + <kind>/<slug>.md
pub fn export_wiki(store: &Store, dir: &Path) -> Result<ExportReport> {
    std::fs::create_dir_all(dir).with_context(|| format!("创建导出目录 {}", dir.display()))?;

    let pages = store.list_wiki_pages(None, None)?;
    let mut files = Vec::new();

    let index = build_index_md(&pages);
    let index_path = dir.join("index.md");
    std::fs::write(&index_path, index)?;
    files.push(index_path.display().to_string());

    let log_path = dir.join("log.md");
    let mut log_content = String::from("# Wiki Log\n");
    let log = store.list_wiki_log(200)?;
    for (ts, entry) in log.iter().rev() {
        log_content.push_str(&format!("<!-- {} -->\n{}\n", ts, entry));
    }
    std::fs::write(&log_path, log_content)?;
    files.push(log_path.display().to_string());

    for page in pages {
        let sub = dir.join(kind_dir(&page.kind));
        std::fs::create_dir_all(&sub)?;
        // 完整 slug 转文件名（`/` → `-`）：slug 前缀不必等于 kind，
        // 只取最后一段会让 `person/a` 与 `misc/a` 落到同名文件互相覆盖。
        let file_name = page.slug.replace('/', "-");
        let path = sub.join(format!("{}.md", file_name));
        let sources: Vec<String> = page
            .source_event_ids
            .iter()
            .map(|s| format!("  - \"{}\"", s))
            .collect();
        let tags: Vec<String> = page.tags.iter().map(|t| format!("  - \"{}\"", t)).collect();
        let content = format!(
            "---\nkind: {}\nslug: {}\nevidence_count: {}\nstatus: {}\nupdated_at: {}\ntags:\n{}\nsources:\n{}\n---\n\n# {}\n\n{}",
            page.kind,
            page.slug,
            page.evidence_count,
            page.status,
            page.updated_at,
            if tags.is_empty() {
                "  - []".to_string()
            } else {
                tags.join("\n")
            },
            if sources.is_empty() {
                "  - []".to_string()
            } else {
                sources.join("\n")
            },
            page.title,
            page.content_md
        );
        std::fs::write(&path, content)?;
        files.push(path.display().to_string());
    }

    Ok(ExportReport {
        files,
        dir: dir.display().to_string(),
    })
}

// ── lint：确定性健康检查 ──────────────────────────────────────────────────

/// 确定性检查：孤儿页、无溯源页。返回问题描述列表。
/// insight 页是综合产物（依据是 wiki 页而非事件），天然无事件溯源，跳过。
pub fn lint_wiki(store: &Store) -> Result<Vec<String>> {
    let pages = store.list_wiki_pages(None, None)?;
    let mut issues = Vec::new();

    for p in &pages {
        if p.status == "archived" || p.kind == "insight" {
            continue;
        }
        if p.source_event_ids.is_empty() {
            issues.push(format!("无事件溯源: {} `{}`", p.kind, p.slug));
        }
        let referenced = pages
            .iter()
            .filter(|o| o.id != p.id)
            .any(|o| o.content_md.contains(&p.slug));
        if !referenced {
            issues.push(format!("无入链页面（orphan）: {}", p.slug));
        }
    }
    Ok(issues)
}
