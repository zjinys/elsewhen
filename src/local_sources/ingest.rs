//! ingest.rs: 目录/文件导入到 wiki 页：项目摘要入库、页面刷新压缩、显式文件模式、主题规划。

use super::*;

pub fn ingest_directory(store: &Store, directory: &Path) -> Result<IngestReport> {
    // CLI 的显式 sources project 命令等同于用户确认后的直接导入，
    // 但仍使用与聊天项目导入相同的 AI/确定性评估流程。
    let summary = analyze_project_with_ai(store, directory)?;
    ingest_project_summary(store, directory, summary)
}

/// 保存已经完成预览的项目摘要，避免用户确认后再次调用 AI 或重新生成报告。
pub fn ingest_project_summary(
    store: &Store,
    directory: &Path,
    summary: ProjectSummary,
) -> Result<IngestReport> {
    let mut report = IngestReport {
        files: summary.files,
        pages: Vec::new(),
        skipped: Vec::new(),
    };
    let project_name = summary.project_name;
    let project_slug = format!("project/{}", slugify(&project_name));
    let content = summary.content_md;
    let scan_truncated = summary.scan_truncated;
    let page_summary = if scan_truncated {
        format!(
            "本地项目摘要，共收集 {} 个有界候选文件（已达到扫描上限）",
            report.files
        )
    } else {
        format!("本地项目摘要，共收集 {} 个有界候选文件", report.files)
    };
    store.upsert_wiki_page(
        &WikiPageDraft {
            slug: project_slug.clone(),
            kind: "project".into(),
            title: project_name,
            summary: page_summary,
            content_md: content,
            tags: vec!["local-project".into(), "project-source".into()],
            source_event_ids: vec![],
            status: "active".into(),
            reason: format!("扫描本地项目目录：{}", directory.display()),
            // 本地目录也是 URL（file://）：路径作为结构化来源记进 source_url，
            // 后续刷新重扫、知识页改路径都以它为准；正文里的「项目目录」只是快照文本。
            source_url: Some(crate::wiki::path_to_file_url(directory)),
        },
        ContentPolicy::Always,
    )?;
    report.pages.push(project_slug);
    store.append_wiki_log(&format!(
        "本地目录导入：{}，读取 {} 个文件",
        directory.display(),
        report.files
    ))?;
    Ok(report)
}

/// 刷新一张目录导入的项目页：按页面的 `source_url`（file://）重新扫描目录并整篇更新。
/// 用户在知识页点的「刷新」与 AI 说的“重新扫描这个项目目录”都走这里（显式授权，Always）。
pub fn refresh_project_page(store: &Store, slug: &str) -> Result<WikiPage> {
    let page = store
        .get_wiki_page(slug)?
        .with_context(|| format!("知识页不存在：{slug}"))?;
    if page.kind != "project" {
        anyhow::bail!(
            "只有项目页（kind=project）支持刷新重扫，当前 kind={}",
            page.kind
        );
    }
    let url = page.source_url.clone().unwrap_or_default();
    let Some(directory) = crate::wiki::file_url_to_path(&url) else {
        anyhow::bail!("该项目页没有关联本地目录（来源不是 file:// 路径），无法刷新")
    };
    if !directory.is_dir() {
        anyhow::bail!(
            "本地目录不存在或不可访问：{}（目录搬家/删除后请先在知识页修改路径）",
            directory.display()
        );
    }
    let summary = analyze_project_with_ai(store, &directory)?;
    let page_summary = if summary.scan_truncated {
        format!(
            "本地项目摘要，共收集 {} 个有界候选文件（已达到扫描上限）",
            summary.files
        )
    } else {
        format!("本地项目摘要，共收集 {} 个有界候选文件", summary.files)
    };
    let files = summary.files;
    let outcome = store.upsert_wiki_page(
        &WikiPageDraft {
            slug: page.slug.clone(),
            kind: page.kind.clone(),
            title: summary.project_name,
            summary: page_summary,
            content_md: summary.content_md,
            tags: page.tags.clone(),
            source_event_ids: vec![],
            status: page.status.clone(),
            reason: format!("刷新重扫本地项目目录：{}", directory.display()),
            // 路径本身不变，回写同一 file://（规范形式），防止漂移。
            source_url: Some(crate::wiki::path_to_file_url(&directory)),
        },
        ContentPolicy::Always,
    )?;
    store.append_wiki_log(&format!(
        "项目页刷新：{}（{} 个文件）",
        outcome.page.slug, files
    ))?;
    Ok(outcome.page)
}

/// 为已导入的超大项目页生成受限版本，避免编辑器一次解析数万行文件地图。
pub fn compact_project_page(store: &Store, slug: &str) -> Result<bool> {
    let Some(page) = store.get_wiki_page(slug)? else {
        return Ok(false);
    };
    if page.kind != "project" || page.content_md.chars().count() < 500_000 {
        return Ok(false);
    }
    let mut lines = page.content_md.lines();
    let mut kept = Vec::new();
    let mut table_rows = 0usize;
    while let Some(line) = lines.next() {
        if line.starts_with("| `") && line.ends_with(" |") {
            table_rows += 1;
            if table_rows > MAX_PROJECT_MAP_FILES {
                continue;
            }
        }
        kept.push(line);
    }
    let content = format!(
        "{}\n\n> 文件地图已限制为前 {MAX_PROJECT_MAP_FILES} 项，原目录仍保留在页面路径中。",
        kept.join("\n")
    );
    let draft = WikiPageDraft {
        slug: page.slug,
        kind: page.kind,
        title: page.title,
        summary: page.summary,
        content_md: content,
        tags: page.tags,
        source_event_ids: page.source_event_ids,
        status: page.status,
        reason: "压缩过大的项目文件地图，保护阅读性能".into(),
        source_url: page.source_url,
    };
    store.upsert_wiki_page(&draft, ContentPolicy::Always)?;
    Ok(true)
}

/// 文件模式：目录中的每个可读文件独立成为一张素材知识页。
pub fn ingest_directory_files(store: &Store, directory: &Path) -> Result<IngestReport> {
    confirm_file_import(store, &preview_file_import(store, directory)?)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct FileImportEntry {
    pub slug: String,
    pub title: String,
    pub content: String,
    pub locator: String,
    pub expected_snapshot_id: Option<String>,
    pub previous_excerpt: Option<String>,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct FileImportPreview {
    pub directory: String,
    pub scanned: usize,
    pub entries: Vec<FileImportEntry>,
    pub skipped: Vec<String>,
}

/// Capture exactly what the user will confirm, with bounded pending payloads.
pub(crate) fn preview_file_import(store: &Store, directory: &Path) -> Result<FileImportPreview> {
    if !directory.is_dir() {
        anyhow::bail!("目录不存在或不可访问: {}", directory.display());
    }
    let directory = directory.canonicalize()?;
    let mut files = Vec::new();
    collect_files(&directory, &mut files)?;
    files.sort();
    let mut report = FileImportPreview {
        directory: directory.display().to_string(),
        scanned: files.len(),
        entries: Vec::new(),
        skipped: Vec::new(),
    };
    let mut total_chars = 0;
    for path in files {
        // 先看字节数，超出 bounded 读者上限的直接跳过并说明——不整文件读入内存
        // （本地目录可能包含数百 MB 的 .txt/.json/.log，fs::read_to_string 会撑爆内存）。
        let file_size = match std::fs::metadata(&path) {
            Ok(meta) => meta.len(),
            Err(_) => {
                report.skipped.push(path.display().to_string());
                continue;
            }
        };
        const INGEST_MAX_BYTES: u64 = MAX_SCAN_FILE_CHARS as u64 * 4 + 4;
        if file_size > INGEST_MAX_BYTES {
            report.skipped.push(format!(
                "{}（{} 字节，超过单文件导入上限）",
                path.display(),
                file_size
            ));
            continue;
        }
        let raw = match fs::File::open(&path).and_then(|file| {
            let mut raw = String::new();
            file.take(INGEST_MAX_BYTES + 1).read_to_string(&mut raw)?;
            Ok(raw)
        }) {
            Ok(value) => value,
            Err(_) => {
                report.skipped.push(path.display().to_string());
                continue;
            }
        };
        let chars = raw.chars().count();
        if chars > MAX_SCAN_FILE_CHARS
            || total_chars + chars > 240_000
            || report.entries.len() >= 100
        {
            report.skipped.push(format!(
                "{}（超出本批原文预算，未截断保存）",
                path.display()
            ));
            continue;
        }
        total_chars += chars;
        let relative = path
            .strip_prefix(&directory)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("source")
            .to_string();
        let slug = unique_slug(store, &format!("source/{}", slugify(&relative)), &title)?;
        let locator = crate::wiki::path_to_file_url(&path);
        let existing = store.matching_source_page(Some(&locator), &raw)?;
        let snapshot = existing
            .as_ref()
            .map(|p| store.source_history(&p.slug))
            .transpose()?
            .and_then(|h| h.into_iter().next());
        let previous_excerpt = snapshot
            .as_ref()
            .map(|s| crate::storage::knowledge::source_change_preview(&s.content_md, &raw));
        report.entries.push(FileImportEntry {
            slug: existing.map(|p| p.slug).unwrap_or(slug),
            title,
            content: raw,
            locator,
            expected_snapshot_id: snapshot.as_ref().map(|s| s.id.clone()),
            previous_excerpt,
        });
    }
    Ok(report)
}

pub(crate) fn confirm_file_import(
    store: &Store,
    preview: &FileImportPreview,
) -> Result<IngestReport> {
    let tx = rusqlite::Transaction::new_unchecked(
        &store.connection,
        rusqlite::TransactionBehavior::Immediate,
    )?;
    let mut report = IngestReport {
        files: preview.scanned,
        pages: Vec::new(),
        skipped: preview.skipped.clone(),
    };
    for entry in &preview.entries {
        store.check_source_preview(
            &entry.locator,
            &entry.content,
            entry.expected_snapshot_id.as_deref(),
        )?;
        let saved = store.upsert_wiki_page_in_tx(
            &WikiPageDraft {
                slug: entry.slug.clone(),
                kind: "source".into(),
                title: entry.title.clone(),
                summary: entry.content.chars().take(240).collect(),
                content_md: entry.content.clone(),
                tags: vec!["local-source".into()],
                source_event_ids: vec![],
                status: "active".into(),
                reason: format!("确认文件原文：{}", preview.directory),
                source_url: Some(entry.locator.clone()),
            },
            ContentPolicy::Always,
        )?;
        report.pages.push(saved.page.slug);
    }
    store.append_wiki_log(&format!(
        "按文件导入本地目录：{}，读取 {} 个文件",
        preview.directory,
        report.pages.len()
    ))?;
    tx.commit()?;
    Ok(report)
}

pub fn plan_topic(store: &Store, topic: &str) -> Result<String> {
    let topic = topic.trim();
    if topic.is_empty() {
        anyhow::bail!("话题不能为空");
    }
    let hits = crate::knowledge::select_knowledge(store, topic, "topic", 4000)?;
    let evidence = if hits.is_empty() {
        "暂无匹配的知识页或事件。".to_string()
    } else {
        hits.iter()
            .map(|h| {
                format!(
                    "- [[{}|{}]]（{}）：{}",
                    h.page_slug,
                    h.title,
                    h.category,
                    h.excerpt.replace('\n', " ")
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let content = format!(
        "# {}\n\n## 目标解释\n\n将“{}”拆解为可衡量的结果、期限、资源与约束；当前需要先确认成功标准和可投入时间。\n\n## 初步实现方案\n\n1. 明确结果指标与截止日期。\n2. 基于已有资产选择一条最短验证路径，先做小规模实验。\n3. 每周复盘投入、产出和阻塞，保留有效动作并停止无效动作。\n\n## 现有知识库依据\n\n{}\n\n## 需要明确的问题\n\n- 目标中的金额/结果是收入、利润还是流水？\n- 截止日期、每周可投入时间和预算分别是多少？\n- 哪些能力、项目或资产可以直接复用？\n- 是否存在不能接受的风险或合规边界？\n\n> 这是基于当前知识库的第一版计划，补充答案后可再次生成。",
        topic, topic, evidence
    );
    let slug = unique_slug(store, &format!("topic/{}", slugify(topic)), topic)?;
    let mut events = hits
        .iter()
        .flat_map(|h| h.event_ids.clone())
        .collect::<Vec<_>>();
    events.sort();
    events.dedup();
    let snapshots = hits
        .iter()
        .flat_map(|h| h.sources.iter().map(|s| s.snapshot_id.clone()))
        .collect::<Vec<_>>();
    let tx = rusqlite::Transaction::new_unchecked(
        &store.connection,
        rusqlite::TransactionBehavior::Immediate,
    )?;
    let outcome = store.upsert_wiki_page_in_tx(
        &WikiPageDraft {
            slug: slug.clone(),
            kind: "topic".into(),
            title: topic.into(),
            summary: "基于个人知识库生成的目标分析与行动方案".into(),
            content_md: content,
            tags: vec!["topic-plan".into()],
            source_event_ids: events,
            status: "active".into(),
            reason: "创建话题分析页".into(),
            source_url: None,
        },
        ContentPolicy::PreserveHumanEdits,
    )?;
    if !outcome.protected {
        store.connection.execute(
            "DELETE FROM knowledge_page_sources WHERE page_id=?1",
            [&outcome.page.id],
        )?;
        store.bind_page_sources(&outcome.page.id, &snapshots)?;
        crate::knowledge::record_usage(store, "topic", &slug, &hits, &hits)?;
    }
    tx.commit()?;
    Ok(slug)
}
