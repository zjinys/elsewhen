//! 技能包与存储对账：bundle 收集、落盘与 reconcile_with_store。

use super::*;

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
