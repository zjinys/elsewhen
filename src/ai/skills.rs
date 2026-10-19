//! 用户技能：磁盘 skill 目录的发现、注册与加载。
//!
//! 格式兼容 Claude Code 的 Skill 目录：每个技能一个子目录，入口是 SKILL.md，
//! frontmatter（`---` 包围）里的 name/description 作为 L1 常驻清单，
//! frontmatter 之后的正文在 enable 时才注入（L2）。目录内其他文件（脚本/模板）
//! 原样保留，经由 `run_skill_script` 工具以用户确认为闸门执行。
//!
//! 信任边界（见 docs/roadmap/2026-10-16-user-skills-roadmap.md）：
//! - skill 目录只由用户放置/修改，AI 没有任何写这里的工具；
//! - 启停状态由设置页控制，默认启用；
//! - 未在此注册的任何外部内容不会作为技能被加载（防伪装注入）。

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// 单个已发现的用户技能。
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct UserSkill {
    /// 目录名，同时是技能 id（enable_tools 的 skills 参数用）。
    pub id: String,
    /// frontmatter 的 name，缺省用 id。
    pub title: String,
    /// description 单行、截断到 200 chars —— L1 清单用。
    pub brief: String,
    /// frontmatter 原始 description（完整），enable 回传时拼进文档。
    pub description_full: String,
    /// SKILL.md 路径。
    pub skill_md: PathBuf,
}

/// 用户技能数量上限：超过后安装拒绝、L1 清单截断（防 schema 描述随安装数无限膨胀）。
/// 上限本身也写进清单提示，便于模型建议用户清理。
pub const USER_SKILL_LIMIT: usize = 30;

/// skills 状态文件：记录被用户手动停用的技能 id（默认全部启用）。
fn state_file(data_dir: &Path) -> PathBuf {
    data_dir.join("skills_state.json")
}

/// 技能操作审计（信任墙 #4）：安装/启停追加到 skills/.audit.log（JSONL）。
/// 故意不进用户事件流——那是个人记录，不应混入系统操作。
pub fn record_audit(data_dir: &Path, action: &str, detail: &str) {
    use std::io::Write;
    let dir = skills_dir(data_dir);
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let line = serde_json::json!({
        "ts": chrono::Utc::now().to_rfc3339(),
        "action": action,
        "detail": detail,
    });
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(".audit.log"))
    {
        let _ = writeln!(f, "{line}");
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct SkillsState {
    disabled: Vec<String>,
    /// 免审批执行的技能 id（与 MCP trusted 同模型）：在列技能的
    /// run_skill_script 调用直接起进程，不再逐次弹审批卡。脚本可在本机
    /// 执行任意操作，开启即用户自担风险（设置页有明示）。
    #[serde(default)]
    trusted: Vec<String>,
}

fn load_state(data_dir: &Path) -> SkillsState {
    std::fs::read(state_file(data_dir))
        .ok()
        .and_then(|bytes| serde_json::from_str(&String::from_utf8_lossy(&bytes)).ok())
        .unwrap_or_default()
}

fn save_state(data_dir: &Path, state: &SkillsState) -> Result<()> {
    std::fs::create_dir_all(data_dir)?;
    let path = state_file(data_dir);
    std::fs::write(&path, serde_json::to_string_pretty(state)?)
        .with_context(|| format!("写入技能状态 {}", path.display()))?;
    Ok(())
}

pub fn is_enabled(data_dir: &Path, id: &str) -> bool {
    !load_state(data_dir).disabled.iter().any(|x| x == id)
}

pub fn set_enabled(data_dir: &Path, id: &str, enabled: bool) -> Result<()> {
    let mut state = load_state(data_dir);
    if enabled {
        state.disabled.retain(|x| x != id);
    } else if !state.disabled.iter().any(|x| x == id) {
        state.disabled.push(id.to_string());
    }
    save_state(data_dir, &state)?;
    record_audit(data_dir, if enabled { "enable" } else { "disable" }, id);
    Ok(())
}

pub fn is_trusted(data_dir: &Path, id: &str) -> bool {
    load_state(data_dir).trusted.iter().any(|x| x == id)
}

pub fn set_trusted(data_dir: &Path, id: &str, trusted: bool) -> Result<()> {
    let mut state = load_state(data_dir);
    if trusted {
        if !state.trusted.iter().any(|x| x == id) {
            state.trusted.push(id.to_string());
        }
    } else {
        state.trusted.retain(|x| x != id);
    }
    save_state(data_dir, &state)?;
    record_audit(data_dir, if trusted { "trust" } else { "untrust" }, id);
    Ok(())
}

/// 平台 skills 目录：`<data_dir>/skills`。
pub fn skills_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("skills")
}

/// 扫描 skills 目录，解析每个子目录的 SKILL.md frontmatter。
/// 目录存在但缺 SKILL.md 的按无效跳过（不报错，避免一个坏目录拖垮全部）。
pub fn discover(data_dir: &Path) -> Result<Vec<UserSkill>> {
    let dir = skills_dir(data_dir);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut skills = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(&dir)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .collect();
    // 目录名序，保证 L1 清单稳定（前缀缓存友好）
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let skill_md = entry.path().join("SKILL.md");
        if !skill_md.is_file() {
            continue;
        }
        let skill = parse_skill_md(&skill_md)
            .with_context(|| format!("解析技能 {}", skill_md.display()))?;
        skills.push(skill);
    }
    Ok(skills)
}

/// 解析 SKILL.md：frontmatter 的 name/description（discover 与安装流程共用）。
pub fn parse_skill_md(skill_md: &Path) -> Result<UserSkill> {
    let id = skill_md
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .context("技能目录缺少可读目录名")?
        .to_string();
    let raw = std::fs::read_to_string(skill_md)
        .with_context(|| format!("读取 {}", skill_md.display()))?;
    let (frontmatter, _body) = split_frontmatter(&raw);
    let mut title = id.clone();
    let mut description_full = String::new();
    if let Some(fm) = frontmatter {
        for line in fm.lines() {
            if let Some(value) = line.strip_prefix("name:") {
                title = clean_scalar(value).unwrap_or_else(|| id.clone());
            } else if let Some(value) = line.strip_prefix("description:") {
                description_full = clean_scalar(value).unwrap_or_default();
            }
        }
    }
    let brief: String = description_full
        .lines()
        .next()
        .unwrap_or("")
        .chars()
        .take(200)
        .collect();
    Ok(UserSkill {
        id,
        title,
        brief,
        description_full,
        skill_md: skill_md.to_path_buf(),
    })
}

/// 把 `---` frontmatter 与正文切开；无 frontmatter 时返回 (None, 全文)。
pub fn split_frontmatter(raw: &str) -> (Option<&str>, &str) {
    let rest = raw.strip_prefix("---\n").or(raw.strip_prefix("---\r\n"));
    let Some(rest) = rest else {
        return (None, raw);
    };
    if let Some(end) = rest.find("\n---").or_else(|| rest.find("\r\n---")) {
        let fm = &rest[..end];
        let body = &rest[(end + 1)..];
        // 跳过收尾的 "---" 一行
        let body = body.trim_start_matches(['-', '\r', '\n']);
        (Some(fm), body)
    } else {
        (None, raw)
    }
}

/// 去掉 yaml 标量两侧引号与空白。
fn clean_scalar(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let unquoted = trimmed
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .or_else(|| {
            trimmed
                .strip_prefix('\'')
                .and_then(|v| v.strip_suffix('\''))
        });
    Some(unquoted.unwrap_or(trimmed).trim().to_string())
}

/// 读取技能正文（frontmatter 之后）+ 目录内文件相对路径清单。
/// 目录遍历深度 3、文件数上限 64：skill 目录不该装入代码仓库那么大的东西。
pub fn load_body(data_dir: &Path, id: &str) -> Result<String> {
    let skill = discover(data_dir)?
        .into_iter()
        .find(|s| s.id == id)
        .with_context(|| format!("未找到用户技能 {id}"))?;
    let raw = std::fs::read_to_string(&skill.skill_md)?;
    let (_fm, body) = split_frontmatter(&raw);
    let mut text = body.trim().to_string();
    if !skill.description_full.is_empty() && !text.contains(&skill.description_full) {
        text = format!("描述：{}\n\n{}", skill.description_full, text);
    }
    let files = list_files(&skill.skill_md.parent().unwrap().to_path_buf(), 3)?;
    if !files.is_empty() {
        text.push_str("\n\n目录内其他文件（可经工具调用）：\n");
        text.push_str(&format!("- {}\n", serde_json::to_string(&files)?));
    }
    Ok(text)
}

/// 列出 skill 目录下的相对路径（浅树，跳过隐藏项与常见垃圾目录）。
fn list_files(dir: &Path, depth: u8) -> Result<Vec<String>> {
    let mut out = Vec::new();
    if depth == 0 {
        return Ok(out);
    }
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name();
        let name = name.to_string_lossy().to_string();
        if name.starts_with('.') || name == "node_modules" || name == "__pycache__" {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            for sub in list_files(&path, depth - 1)? {
                out.push(format!("{name}/{sub}"));
            }
        } else {
            out.push(name);
        }
        if out.len() >= 64 {
            break;
        }
    }
    Ok(out)
}

mod reconcile;

#[cfg(test)]
mod reconcile_tests;
#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use reconcile::apply_bundle_to_dir;
pub(crate) use reconcile::collect_bundle_files;
pub use reconcile::{ReconcileReport, reconcile_with_store};
