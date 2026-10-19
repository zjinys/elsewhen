//! Local directory ingestion and topic planning.
//! Project mode persists a bounded assessment; explicit file mode persists
//! per-file source pages. The original directory remains the source of truth.

use crate::ai::memory::ContextMessage;
use crate::ai::provider::{OpenAiCompatibleConfig, OpenAiCompatibleProvider};
use crate::storage::{ContentPolicy, Store, WikiPage, WikiPageDraft};
use crate::wiki::{slugify, unique_slug};
use anyhow::{Context, Result};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

const TEXT_EXTENSIONS: &[&str] = &[
    "md", "txt", "rst", "json", "yaml", "yml", "toml", "rs", "dart", "ts", "tsx", "js", "jsx",
    "py", "go", "java", "kt", "sql", "html", "css",
];
const MAX_PROJECT_MAP_FILES: usize = 2000;
/// Project mode bounds traversal; explicit per-file import retains its full scan.
const MAX_PROJECT_SCAN_FILES: usize = 20_000;
const MAX_PROJECT_SCAN_ENTRIES: usize = 100_000;
const MAX_SUMMARY_CHARS: usize = 18_000;
const MAX_EVIDENCE_FILES: usize = 96;
const MAX_AI_EVIDENCE_FILES: usize = 24;
const MAX_AI_EVIDENCE_CHARS: usize = 30_000;
const MAX_AI_FILE_CHARS: usize = 4_000;
const MAX_SCAN_FILE_CHARS: usize = 12_000;
const MAX_MANIFEST_CHARS: usize = 64_000;

/// 语言信号：manifest 文件名（小写）→ 语言标签。
const LANG_MANIFESTS: &[(&str, &str)] = &[
    ("cargo.toml", "Rust"),
    ("pubspec.yaml", "Dart/Flutter"),
    ("package.json", "Node.js"),
    ("go.mod", "Go"),
    ("pyproject.toml", "Python"),
    ("requirements.txt", "Python"),
    ("setup.py", "Python"),
    ("pom.xml", "JVM (Maven)"),
    ("build.gradle", "JVM (Gradle)"),
    ("build.gradle.kts", "JVM (Gradle Kotlin)"),
    ("package.swift", "Swift"),
    ("composer.json", "PHP"),
    ("mix.exs", "Elixir"),
];

/// 框架 / 工具链信号：相对路径（小写）包含该片段 → 标签。
const FRAMEWORK_SIGNALS: &[(&str, &str)] = &[
    ("next.config.", "Next.js"),
    ("vite.config.", "Vite"),
    ("tsconfig.json", "TypeScript"),
    ("tailwind.config.", "Tailwind CSS"),
    ("dockerfile", "Docker"),
    (".github/workflows/", "GitHub Actions CI"),
    (".gitlab-ci", "GitLab CI"),
    ("android/", "Android"),
    ("ios/", "iOS"),
    ("electron", "Electron"),
];

#[derive(Debug, Clone)]
pub struct IngestReport {
    pub files: usize,
    pub pages: Vec<String>,
    pub skipped: Vec<String>,
}

/// A bounded, read-only project assessment.  The directory remains the source
/// of truth; this report is the only part that is persisted to the wiki.
#[derive(Debug, Clone)]
pub struct ProjectSummary {
    pub project_name: String,
    /// Number of candidate text files in the bounded scan, not necessarily the directory total.
    pub files: usize,
    pub skipped: usize,
    pub scan_truncated: bool,
    pub content_md: String,
}

mod analyze;
mod ingest;
mod scan;

#[cfg(test)]
pub(crate) use analyze::collect_project_evidence;
pub use analyze::{analyze_project, analyze_project_with_ai};
#[cfg(test)]
pub(crate) use analyze::{is_sensitive_evidence_path, sanitize_evidence};
pub(crate) use ingest::{FileImportPreview, confirm_file_import, preview_file_import};
pub use ingest::{
    compact_project_page, ingest_directory, ingest_directory_files, ingest_project_summary,
    plan_topic, refresh_project_page,
};
pub(crate) use scan::{
    collect_files, collect_project_files, detect_tech_stack, git_snapshot, read_bounded_text,
};

#[cfg(test)]
#[path = "local_sources_tests/mod.rs"]
mod tests;
