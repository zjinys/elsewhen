//! 技术栈识别、依赖提取、git 快照与文件收集（尊重 gitignore 的边界扫描）。

use super::*;

/// 识别技术栈：语言（manifest）+ 框架/工具链信号 + 关键依赖。
/// 同语言多个 manifest 去重，优先保留根目录（路径最短）的那个。
pub(crate) fn detect_tech_stack(files: &[PathBuf], directory: &Path) -> String {
    let mut langs: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    let mut frameworks: Vec<String> = Vec::new();
    let mut deps: Vec<String> = Vec::new();
    let mut weak_c = false;

    for path in files {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let relative = path
            .strip_prefix(directory)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let rel_lower = relative.to_ascii_lowercase();

        // 弱 C/C++ 信号：CMake/Makefile 在很多项目里只是构建脚手架（如 Flutter 的
        // linux/ 目录），仅在没有任何其他语言 manifest 时才作为语言结论。
        if name == "cmakelists.txt" || name == "makefile" {
            weak_c = true;
        }

        for &(manifest, lang) in LANG_MANIFESTS {
            if name != manifest {
                continue;
            }
            let label = match read_bounded_text(path, MAX_MANIFEST_CHARS) {
                Ok(raw) => {
                    extract_key_deps(manifest, &raw, &mut deps);
                    refine_lang_label(manifest, &raw, lang)
                }
                Err(_) => lang.to_string(),
            };
            langs
                .entry(label.clone())
                .and_modify(|existing| {
                    if relative.len() < existing.len() {
                        *existing = relative.clone();
                    }
                })
                .or_insert_with(|| relative.clone());
            break;
        }

        for &(sig, label) in FRAMEWORK_SIGNALS {
            if rel_lower.contains(sig) && !frameworks.iter().any(|f| f.as_str() == label) {
                frameworks.push(label.to_string());
            }
        }
    }

    let mut out = String::new();
    if langs.is_empty() {
        if weak_c {
            out.push_str("- C/C++（CMake/Makefile 构建）\n");
        } else {
            out.push_str("- 未识别到明确的 manifest（可能为纯文本/数据或非代码项目）。\n");
        }
    } else {
        for (lang, rel) in &langs {
            out.push_str(&format!("- {lang}（`{rel}`）\n"));
        }
    }
    if !frameworks.is_empty() {
        out.push_str(&format!("- 框架/工具链：{}\n", frameworks.join("、")));
    }
    if !deps.is_empty() {
        out.push_str(&format!(
            "- 关键依赖：{}\n",
            deps.iter().take(12).cloned().collect::<Vec<_>>().join("、")
        ));
    }
    out.trim_end().to_string()
}

/// 根据 manifest 内容细化语言标签（如 pubspec 是否 Flutter）。
fn refine_lang_label(manifest: &str, raw: &str, fallback: &str) -> String {
    match manifest {
        "pubspec.yaml" => {
            if raw.lines().any(|l| {
                let t = l.trim();
                t == "flutter:" || t.starts_with("flutter:")
            }) {
                "Flutter (Dart)".to_string()
            } else {
                "Dart".to_string()
            }
        }
        _ => fallback.to_string(),
    }
}

fn extract_key_deps(manifest: &str, raw: &str, out: &mut Vec<String>) {
    let keys = match manifest {
        "cargo.toml" => extract_cargo_deps(raw),
        "pubspec.yaml" => extract_pubspec_deps(raw),
        "package.json" => extract_package_json_deps(raw),
        _ => Vec::new(),
    };
    // 每个 manifest 最多取 6 个，避免单一 manifest 占满列表，让多语言依赖都能露头。
    for k in keys.into_iter().take(6) {
        if !out.iter().any(|d| d == &k) {
            out.push(k);
        }
    }
}

fn extract_cargo_deps(raw: &str) -> Vec<String> {
    let mut deps = Vec::new();
    let mut in_deps = false;
    for line in raw.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_deps = t == "[dependencies]";
            continue;
        }
        if !in_deps || t.starts_with('#') {
            continue;
        }
        let Some(eq) = t.find('=') else { continue };
        let name = t[..eq].trim();
        let name = name.split('.').next().unwrap_or(name).trim();
        if !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
            && !deps.contains(&name.to_string())
        {
            deps.push(name.to_string());
        }
    }
    deps
}

fn extract_pubspec_deps(raw: &str) -> Vec<String> {
    let mut deps = Vec::new();
    let mut in_deps = false;
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let has_indent = line.starts_with(' ') || line.starts_with('\t');
        if !has_indent {
            in_deps = trimmed == "dependencies:" || trimmed == "dev_dependencies:";
            continue;
        }
        if !in_deps {
            continue;
        }
        // 顶层依赖是 2 空格缩进；其下的 `sdk:` 等子属性是 4 空格缩进。
        let indent = line.len() - line.trim_start().len();
        if indent != 2 {
            continue;
        }
        let name = trimmed.split(':').next().unwrap_or("").trim();
        if !name.is_empty()
            && name != "sdk"
            && name != "flutter"
            && name.chars().all(|c| c.is_alphanumeric() || c == '_')
            && !deps.contains(&name.to_string())
        {
            deps.push(name.to_string());
        }
    }
    deps
}

fn extract_package_json_deps(raw: &str) -> Vec<String> {
    let mut deps = Vec::new();
    let Ok(json) = serde_json::from_str::<serde_json::Value>(raw) else {
        return deps;
    };
    for section in ["dependencies", "devDependencies"] {
        if let Some(obj) = json.get(section).and_then(|v| v.as_object()) {
            for key in obj.keys() {
                if !deps.contains(key) {
                    deps.push(key.clone());
                }
            }
        }
    }
    deps
}

pub(crate) fn git_snapshot(directory: &Path) -> String {
    let status = std::process::Command::new("git")
        .args(["-C", &directory.to_string_lossy(), "status", "--short"])
        .output();
    let changes = match status {
        Ok(out) if out.status.success() => {
            Some(String::from_utf8_lossy(&out.stdout).lines().count())
        }
        _ => None,
    };
    let log = std::process::Command::new("git")
        .args(["-C", &directory.to_string_lossy(), "log", "-5", "--oneline"])
        .output();
    let recent = match log {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            if text.trim().is_empty() {
                None
            } else {
                Some(
                    text.lines()
                        .map(|l| l.trim().to_string())
                        .collect::<Vec<_>>()
                        .join("；"),
                )
            }
        }
        _ => None,
    };
    match (changes, recent) {
        (Some(c), Some(r)) => {
            format!("已发现 Git 工作树（{c} 条未提交变更）。最近提交：{r}")
        }
        (Some(c), None) => format!("已发现 Git 工作树（{c} 条未提交变更，无提交历史）"),
        (None, Some(r)) => format!("已发现 Git 仓库。最近提交：{r}"),
        (None, None) => "未发现可读取的 Git 工作树".to_string(),
    }
}

pub(crate) fn read_bounded_text(path: &Path, max_chars: usize) -> Result<String> {
    let max_bytes = max_chars.saturating_mul(4).saturating_add(4);
    let mut file =
        fs::File::open(path).with_context(|| format!("无法读取文件: {}", path.display()))?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(max_bytes as u64)
        .read_to_end(&mut bytes)
        .with_context(|| format!("无法读取文件: {}", path.display()))?;
    Ok(String::from_utf8_lossy(&bytes)
        .chars()
        .take(max_chars)
        .collect())
}

pub(crate) fn collect_files(root: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let ignored = load_gitignore_dirs(root);
    collect_files_inner(root, out, &ignored)
}

pub(crate) fn collect_project_files(root: &Path) -> Result<(Vec<PathBuf>, bool)> {
    let ignored = load_gitignore_dirs(root);
    let mut files = Vec::new();
    let mut truncated = false;
    let mut entries_seen = 0usize;
    collect_project_files_inner(
        root,
        &ignored,
        &mut files,
        &mut truncated,
        &mut entries_seen,
    )?;
    Ok((files, truncated))
}

fn collect_project_files_inner(
    dir: &Path,
    ignored: &std::collections::HashSet<String>,
    files: &mut Vec<PathBuf>,
    truncated: &mut bool,
    entries_seen: &mut usize,
) -> Result<()> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("无法读取目录: {}", dir.display()))?
    {
        *entries_seen += 1;
        if *entries_seen > MAX_PROJECT_SCAN_ENTRIES {
            *truncated = true;
            break;
        }
        entries.push(entry?);
    }
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with('.')
            || matches!(
                name,
                "target"
                    | "node_modules"
                    | "third_party"
                    | "thirdparty"
                    | "vendor"
                    | "build"
                    | "dist"
            )
            || ignored.contains(name)
        {
            continue;
        }
        if path.is_dir() {
            collect_project_files_inner(&path, ignored, files, truncated, entries_seen)?;
            if *truncated {
                return Ok(());
            }
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| TEXT_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
            .unwrap_or(false)
        {
            if files.len() == MAX_PROJECT_SCAN_FILES {
                *truncated = true;
                return Ok(());
            }
            files.push(path);
        }
    }
    Ok(())
}

fn collect_files_inner(
    dir: &Path,
    out: &mut Vec<PathBuf>,
    ignored: &std::collections::HashSet<String>,
) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("无法读取目录: {}", dir.display()))?
    {
        let path = entry?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with('.')
            || name == "target"
            || name == "node_modules"
            || name == "third_party"
            || name == "thirdparty"
            || name == "vendor"
            || name == "build"
            || name == "dist"
            || ignored.contains(name)
        {
            continue;
        }
        if path.is_dir() {
            collect_files_inner(&path, out, ignored)?;
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| TEXT_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
            .unwrap_or(false)
        {
            out.push(path);
        }
    }
    Ok(())
}

/// 读取根目录 .gitignore，提取被忽略的顶层目录名。
/// 只处理 `xxx/` 或 `/xxx/` 这类字面目录，忽略 glob/通配/取反，解析失败静默降级。
fn load_gitignore_dirs(root: &Path) -> std::collections::HashSet<String> {
    let mut dirs = std::collections::HashSet::new();
    let Ok(raw) = fs::read_to_string(root.join(".gitignore")) else {
        return dirs;
    };
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') || t.starts_with('!') {
            continue;
        }
        let t = t.trim_start_matches('/');
        if let Some(dir) = t.strip_suffix('/') {
            if !dir.is_empty() && !dir.contains('/') && !dir.contains('*') && !dir.contains('?') {
                dirs.insert(dir.to_string());
            }
        }
    }
    dirs
}
