//! Markdown 导出的文件名清洗与页面渲染。

use super::*;

impl Store {
    /// 把知识库主列表页面导出为独立 Markdown 文件（YAML frontmatter + 正文）。
    /// 与主列表同口径：排除派生产物（area=derivative）与已归档/拒绝页；导出的
    /// 是"用户名下的知识"，不是系统派生缓存。同名文件（slug 清洗后）覆盖，
    /// 可重复导出幂等。返回导出页数。
    pub fn export_wiki_markdown(&self, out_dir: &std::path::Path) -> Result<usize> {
        std::fs::create_dir_all(out_dir)?;
        let pages = self.list_wiki_pages(None, None)?;
        let mut exported = 0usize;
        for page in pages {
            if page.status == "archived" || page.opinion.as_deref() == Some("reject") {
                continue;
            }
            let filename = format!("{}.md", sanitize_wiki_filename(&page.slug));
            let document = render_wiki_markdown(&page)?;
            std::fs::write(out_dir.join(filename), document)?;
            exported += 1;
        }
        Ok(exported)
    }
}

pub(crate) fn sanitize_wiki_filename(slug: &str) -> String {
    slug.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => "__".to_string(),
            c if (c as u32) < 0x20 => "__".to_string(),
            c => c.to_string(),
        })
        .collect()
}

/// 渲染单页：前置 frontmatter（元数据，机器可回读）+ 原正文。字符串统一用
/// JSON 转义（双引号标量是合法 YAML），避免手写转义规则。
pub(crate) fn render_wiki_markdown(page: &crate::storage::WikiPage) -> Result<String> {
    let json = |v: &str| serde_json::to_string(v).unwrap_or_else(|_| "\"\"".into());
    let tags = serde_json::to_string(&page.tags).unwrap_or_else(|_| "[]".into());
    let mut document = String::with_capacity(page.content_md.len() + 512);
    document.push_str("---\n");
    document.push_str(&format!("title: {}\n", json(&page.title)));
    document.push_str(&format!("slug: {}\n", json(&page.slug)));
    document.push_str(&format!("kind: {}\n", json(&page.kind)));
    document.push_str(&format!("summary: {}\n", json(&page.summary)));
    document.push_str(&format!("tags: {}\n", tags));
    document.push_str(&format!("evidence_count: {}\n", page.evidence_count));
    document.push_str(&format!("updated_at: {}\n", json(&page.updated_at)));
    if let Some(url) = &page.source_url {
        document.push_str(&format!("source_url: {}\n", json(url)));
    }
    document.push_str("---\n\n");
    document.push_str(&page.content_md);
    if !document.ends_with('\n') {
        document.push('\n');
    }
    Ok(document)
}
