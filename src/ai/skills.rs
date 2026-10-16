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

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_dir(tag: &str) -> (std::path::PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "elsewhen-skills-test-{}-{}",
            tag,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let skills = dir.join("skills");
        std::fs::create_dir_all(skills.join("proxy-pick")).unwrap();
        (dir, skills)
    }

    #[test]
    fn discovers_and_parses_frontmatter() {
        let (dir, skills) = fixture_dir("discover");
        std::fs::write(
            skills.join("proxy-pick/SKILL.md"),
            "---\nname: proxy-pick\ndescription: \"切换 Clash Verge 代理节点。\n测速与挑选。\"\n---\n\n# 指南\n步骤在此。",
        )
        .unwrap();
        let found = discover(&dir).unwrap();
        assert_eq!(found.len(), 1);
        let skill = &found[0];
        assert_eq!(skill.id, "proxy-pick");
        assert_eq!(skill.title, "proxy-pick");
        assert!(skill.brief.contains("切换 Clash"));
        let body = load_body(&dir, "proxy-pick").unwrap();
        assert!(body.contains("# 指南"));
        assert!(
            !body.contains("name: proxy-pick"),
            "frontmatter 不应进入正文"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn enable_toggle_roundtrip() {
        let (dir, skills) = fixture_dir("toggle");
        std::fs::write(skills.join("proxy-pick/SKILL.md"), "just body").unwrap();
        assert!(is_enabled(&dir, "proxy-pick"));
        set_enabled(&dir, "proxy-pick", false).unwrap();
        assert!(!is_enabled(&dir, "proxy-pick"));
        set_enabled(&dir, "proxy-pick", true).unwrap();
        assert!(is_enabled(&dir, "proxy-pick"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// 启动时目录视图与分发台账的对账（roadmap P3）。
///
/// 合并规则（"目录是用户此刻意图，目录获胜"）：
/// - 表有目录无（无本地墓碑）→ 远端分发到达，落盘；
/// - 目录有表无 → 本地新装，回写表分发出去；
/// - 两侧都有但内容不同 → 目录获胜，回写表；
/// - 目录消失（表有行且无本地墓碑）→ 记删除墓碑并清行；
/// - 目录在且有本地墓碑 → 用户重装覆盖远端删除，回写表（replace 顺带清墓碑）。
/// - 启停状态（skills_state.json）不参与对账：按设备独立。
#[derive(Debug)]
pub struct ReconcileReport {
    /// 远端分发到达、落盘到目录的技能。
    pub applied_to_dir: Vec<String>,
    /// 本地目录状态回写进表的技能（新装/更新）。
    pub published_to_table: Vec<String>,
    /// 记删除墓碑的技能。
    pub tombstoned: Vec<String>,
    /// 拒绝 apply 的技能与原因（如含越界路径、超上限）。
    pub rejected: Vec<(String, String)>,
}

/// 扫描单个技能目录的全部文件（相对路径 + 字节 + sha）。
/// 隐藏文件（.audit.log 等）不入台账——那是本设备元数据，不分发。
pub(crate) fn collect_bundle_files(skill_dir: &Path) -> Result<Vec<(String, Vec<u8>, String)>> {
    let mut out = Vec::new();
    fn walk(dir: &Path, prefix: &str, out: &mut Vec<(String, Vec<u8>, String)>) -> Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .context("技能文件名不可读")?
                .to_string();
            let rel = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            if name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                walk(&path, &rel, out)?;
            } else if path.is_file() {
                let bytes = std::fs::read(&path)?;
                let sha = {
                    use ring::digest::SHA256;
                    ring::digest::digest(&SHA256, &bytes)
                        .as_ref()
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>()
                };
                out.push((rel, bytes, sha));
            }
        }
        Ok(())
    }
    walk(skill_dir, "", &mut out)?;
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

fn bundle_fingerprint(files: &[(String, Vec<u8>, String)]) -> String {
    files
        .iter()
        .flat_map(|(rel, _, sha)| [rel.as_str(), sha.as_str()])
        .collect::<Vec<_>>()
        .join("|")
}

pub(crate) fn apply_bundle_to_dir(
    data_dir: &Path,
    skill_id: &str,
    rows: &[crate::storage::skills::SkillFileRow],
) -> Result<()> {
    let skill_dir = skills_dir(data_dir).join(skill_id);
    // rel 安全校验先用，全部通过才动盘
    for row in rows {
        anyhow::ensure!(
            !row.rel.contains("..") && !row.rel.starts_with('/') && !row.rel.is_empty(),
            "分发台账含越界路径 {}: {}",
            skill_id,
            row.rel
        );
    }
    std::fs::create_dir_all(&skill_dir)?;
    let canonical_skill_dir = skill_dir.canonicalize()?;
    for row in rows {
        let target = skill_dir.join(&row.rel);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // 双保险：rel 文本校验之外，落盘位置 canonical 必须仍在技能目录内
        if let Ok(canon) = target.parent().unwrap().canonicalize() {
            anyhow::ensure!(
                canon.starts_with(&canonical_skill_dir),
                "分发路径越界 {}: {}",
                skill_id,
                row.rel
            );
        }
        std::fs::write(&target, &row.bytes)?;
    }
    Ok(())
}

/// 对账主入口（init_bridge 启动时调用；db 中文件字节随整库同步自然送达）。
pub fn reconcile_with_store(
    data_dir: &Path,
    store: &crate::storage::Store,
) -> Result<ReconcileReport> {
    let mut report = ReconcileReport {
        applied_to_dir: Vec::new(),
        published_to_table: Vec::new(),
        tombstoned: Vec::new(),
        rejected: Vec::new(),
    };

    // 1) 扫目录视图：id → 文件集（合法技能 = 有 SKILL.md）
    let mut dir_bundles: std::collections::BTreeMap<String, Vec<(String, Vec<u8>, String)>> =
        std::collections::BTreeMap::new();
    for skill in discover(data_dir)? {
        let files = collect_bundle_files(&skills_dir(data_dir).join(&skill.id))?;
        dir_bundles.insert(skill.id, files);
    }

    // 2) 表视图与墓碑
    let table_ids = crate::storage::skills::distributed_skill_ids(store)?;
    let tombstones: std::collections::BTreeMap<String, String> =
        crate::storage::skills::skill_tombstones(store)?
            .into_iter()
            .collect();

    let mut all_ids: std::collections::BTreeSet<String> = dir_bundles.keys().cloned().collect();
    all_ids.extend(table_ids.iter().cloned());
    all_ids.extend(tombstones.keys().cloned());

    for id in all_ids {
        let dir_files = dir_bundles.get(&id);
        let table_rows = crate::storage::skills::skill_files(store, &id)?;
        let has_table = !table_rows.is_empty();
        let tombstoned = tombstones.contains_key(&id);
        let table_fingerprint = bundle_fingerprint(
            &table_rows
                .iter()
                .map(|r| (r.rel.clone(), r.bytes.clone(), r.sha256.clone()))
                .collect::<Vec<_>>(),
        );

        match (dir_files.is_some(), has_table) {
            // 目录领先：本地新装 / 重装（覆盖墓碑）/ 用户编辑了文件 → 回写表
            (true, _) => {
                let files = dir_files.unwrap();
                let dir_fingerprint = bundle_fingerprint(files);
                let table_matches =
                    has_table && !tombstoned && table_fingerprint == dir_fingerprint;
                if !table_matches {
                    let rows = files
                        .iter()
                        .map(|(rel, bytes, sha)| crate::storage::skills::SkillFileRow {
                            rel: rel.clone(),
                            bytes: bytes.clone(),
                            sha256: sha.clone(),
                            size: bytes.len() as i64,
                        })
                        .collect::<Vec<_>>();
                    match crate::storage::skills::replace_skill_files(store, &id, &rows) {
                        Ok(()) => report.published_to_table.push(id.clone()),
                        Err(e) => report
                            .rejected
                            .push((id.clone(), format!("写台账失败：{e}"))),
                    }
                }
            }
            // 表领先：远端分发到达 → 落盘。
            // 注意：不区分"新分发"与"本地手动 rm"——删除必须走卸载 API
            // （uninstall_user_skill，显式记墓碑）；手动删目录被视为落后视图，
            // 会被分发复活（分发语义优先；见 uninstall_user_skill 文档）。
            (false, true) if !tombstoned => {
                // 上限保护：本地目录已有技能数 + 到达数不越限
                let current = dir_bundles.len();
                if current >= USER_SKILL_LIMIT {
                    report.rejected.push((
                        id.clone(),
                        format!("本地技能数已达上限 {USER_SKILL_LIMIT}，未落盘"),
                    ));
                    continue;
                }
                match apply_bundle_to_dir(data_dir, &id, &table_rows) {
                    Ok(()) => report.applied_to_dir.push(id.clone()),
                    Err(e) => report.rejected.push((id.clone(), format!("{e}"))),
                }
            }
            // 有本地墓碑：删除在本设备已确认（卸载 API 记录），不分发复活
            (false, true) => {}
            // 两边都没有：幽灵墓碑，保留（防删除语义回潮），不动
            (false, false) => {}
        }
    }

    if !report.applied_to_dir.is_empty()
        || !report.published_to_table.is_empty()
        || !report.tombstoned.is_empty()
        || !report.rejected.is_empty()
    {
        record_audit(
            data_dir,
            "reconcile",
            &format!(
                "applied={:?} published={:?} tombstoned={:?} rejected={:?}",
                report.applied_to_dir,
                report.published_to_table,
                report.tombstoned,
                report.rejected
            ),
        );
    }
    Ok(report)
}

#[cfg(test)]
mod reconcile_tests {
    use super::*;
    use crate::storage::skills::{SkillFileRow, replace_skill_files};

    struct Env {
        data_dir: std::path::PathBuf,
        db_path: std::path::PathBuf,
    }

    impl Drop for Env {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.data_dir);
        }
    }

    fn env(tag: &str) -> Env {
        let dir =
            std::env::temp_dir().join(format!("elsewhen-reconcile-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("data")).unwrap();
        Env {
            db_path: dir.join("sync-test.db"),
            data_dir: dir.join("data"),
        }
    }

    fn open_store(env: &Env) -> crate::storage::Store {
        crate::storage::Store::open(&env.db_path).unwrap()
    }

    /// 模拟"另一台设备"的目录状态 → 文件集行。
    fn rows_of(skill_dir: &Path) -> Vec<SkillFileRow> {
        collect_bundle_files(skill_dir)
            .unwrap()
            .into_iter()
            .map(|(rel, bytes, sha)| SkillFileRow {
                size: bytes.len() as i64,
                rel,
                bytes,
                sha256: sha,
            })
            .collect()
    }

    fn touch_skill(data_dir: &Path, id: &str, body: &str) {
        let dir = skills_dir(data_dir).join(id);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {id}\ndescription: {body}\n---\n正文"),
        )
        .unwrap();
    }

    #[test]
    fn local_install_publishes_to_table_and_remote_dir_receives() {
        let env = env("pub-recv");
        let store = open_store(&env);

        // 设备 A：装了 pick；模拟同步后的 db → 设备 B 空目录接住
        touch_skill(&env.data_dir, "pick", "A 机安装");
        let report = reconcile_with_store(&env.data_dir, &store).unwrap();
        assert_eq!(report.published_to_table, ["pick"]);
        assert!(store_distributed(&store, "pick"), "A 机安装后应进台账");
        assert!(!read_audit(&env.data_dir, "pick").is_empty());

        // 换"设备 B 视角"：清空目录，重放 reconcile（同 db）→ 落盘分发
        std::fs::remove_dir_all(skills_dir(&env.data_dir)).unwrap();
        let report = reconcile_with_store(&env.data_dir, &store).unwrap();
        assert_eq!(report.applied_to_dir, ["pick"], "B 机应接住分发");
        assert!(
            skills_dir(&env.data_dir).join("pick/SKILL.md").is_file(),
            "分发的文件应完整落盘"
        );
        let _ = std::fs::remove_dir_all(std::env::temp_dir().join("elsewhen-reconcile-pub-recv-0"));
    }

    fn store_distributed(store: &crate::storage::Store, id: &str) -> bool {
        crate::storage::skills::distributed_skill_ids(store)
            .unwrap()
            .contains(&id.to_string())
    }

    fn read_audit(data_dir: &Path, needle: &str) -> String {
        std::fs::read_to_string(skills_dir(data_dir).join(".audit.log"))
            .unwrap_or_default()
            .contains(needle)
            .to_string()
            + &std::fs::read_to_string(skills_dir(data_dir).join(".audit.log")).unwrap_or_default()
    }

    #[test]
    fn deletion_propagates_and_reinstall_overrides_tombstone() {
        let env = env("delete");
        let store = open_store(&env);
        touch_skill(&env.data_dir, "pick", "v1");
        reconcile_with_store(&env.data_dir, &store).unwrap();

        // 本机卸载（显式 API）→ 墓碑 + 清行
        crate::api::user_skills::uninstall_user_skill_at(&env.data_dir, &store, "pick").unwrap();
        assert!(!store_distributed(&store, "pick"));

        // 模拟另一设备（fresh 目录 + 同 db）：分发不应复活已删除技能
        std::fs::remove_dir_all(skills_dir(&env.data_dir)).unwrap();
        let report = reconcile_with_store(&env.data_dir, &store).unwrap();
        assert!(report.applied_to_dir.is_empty(), "墓碑必须拦住删除回潮");
        assert!(!skills_dir(&env.data_dir).join("pick").exists());

        // 语义边界（文档化行为）：绕过 API 手动 rm 目录 → 视为落后视图，分发复活
        touch_skill(&env.data_dir, "pick", "v1");
        reconcile_with_store(&env.data_dir, &store).unwrap();
        std::fs::remove_dir_all(skills_dir(&env.data_dir).join("pick")).unwrap();
        let report = reconcile_with_store(&env.data_dir, &store).unwrap();
        assert_eq!(
            report.applied_to_dir,
            ["pick"],
            "手动删目录会被分发复活——删除必须走卸载 API"
        );

        // 本机重装同名 → 目录获胜，墓碑撤销，重新分发
        touch_skill(&env.data_dir, "pick", "v2 重装");
        let report = reconcile_with_store(&env.data_dir, &store).unwrap();
        assert_eq!(report.published_to_table, ["pick"]);
        assert!(store_distributed(&store, "pick"));
        let _ = std::fs::remove_dir_all(std::env::temp_dir().join("elsewhen-reconcile-delete-0"));
    }

    #[test]
    fn local_edit_wins_over_table() {
        let env = env("edit-win");
        let store = open_store(&env);
        touch_skill(&env.data_dir, "pick", "v1");
        reconcile_with_store(&env.data_dir, &store).unwrap();

        // 用户直接改目录文件（P1 手动放目录的方式）→ 目录获胜回写
        std::fs::write(
            skills_dir(&env.data_dir).join("pick/extra.md"),
            "用户手放的模板",
        )
        .unwrap();
        let report = reconcile_with_store(&env.data_dir, &store).unwrap();
        assert_eq!(report.published_to_table, ["pick"]);
        assert!(store_distributed(&store, "pick"));

        // sha 一致时不出现在 published（无谓写被抑制）
        let report = reconcile_with_store(&env.data_dir, &store).unwrap();
        assert!(report.published_to_table.is_empty());
        let _ = std::fs::remove_dir_all(std::env::temp_dir().join("elsewhen-reconcile-edit-win-0"));
    }

    #[test]
    fn traversal_in_bundle_is_rejected() {
        let env = env("traversal");
        let store = open_store(&env);
        let evil = vec![SkillFileRow {
            rel: "../outside.txt".into(),
            bytes: b"x".to_vec(),
            sha256: "0".repeat(64),
            size: 1,
        }];
        let err = apply_bundle_to_dir(&env.data_dir, "evil", &evil).unwrap_err();
        assert!(format!("{err:#}").contains("越界"));
        assert!(
            !env.data_dir.join("outside.txt").exists(),
            "越界文件落盘了！"
        );
        let _ =
            std::fs::remove_dir_all(std::env::temp_dir().join("elsewhen-reconcile-traversal-0"));
    }
}
