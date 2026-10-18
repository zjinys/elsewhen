//! 用户技能分发台账（roadmap P3）：技能文件字节入 db，随整库同步跨设备流动。
//!
//! 语义（见 migrations/41-skill-bundles/up.sql）：
//! - `skill_files` 是分发事实源；skills 目录是设备本地视图（用户主权）。
//! - 启停状态 `skills_state.json` 按设备独立，**不**进同步。
//! - 同名技能冲突时目录获胜（本地目录是用户此刻的意图），并回写表。

use super::Store;
use anyhow::{Context, Result};
use serde_json::json;

/// 技能内一个文件的台账行。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillFileRow {
    pub rel: String,
    pub bytes: Vec<u8>,
    pub sha256: String,
    pub size: i64,
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn sha256_hex(bytes: &[u8]) -> String {
    use ring::digest::SHA256;
    ring::digest::digest(&SHA256, bytes)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// 整技能覆盖写表（安装 / 目录回写）。同一事务内先清后插，
/// 保证表里永远是这个技能完整、一致的文件集。
pub fn replace_skill_files(store: &Store, skill_id: &str, files: &[SkillFileRow]) -> Result<()> {
    let tx = store
        .connection
        .unchecked_transaction()
        .context("开 skill_files 写事务")?;
    let now = now_rfc3339();
    tx.execute("DELETE FROM skill_files WHERE skill_id=?1", [skill_id])?;
    for f in files {
        tx.execute(
            "INSERT INTO skill_files(skill_id, rel, bytes, sha256, size, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![skill_id, f.rel, f.bytes, f.sha256, f.size, now],
        )?;
    }
    // 目录视图重新出现即视为本地重新安装，撤销远端删除墓碑
    tx.execute("DELETE FROM skill_tombstones WHERE skill_id=?1", [skill_id])?;
    tx.commit()?;
    Ok(())
}

/// 表中某技能的文件集（无行返回空集=未分发）。
pub fn skill_files(store: &Store, skill_id: &str) -> Result<Vec<SkillFileRow>> {
    let mut stmt = store.connection.prepare(
        "SELECT rel, bytes, sha256, size FROM skill_files WHERE skill_id=?1 ORDER BY rel",
    )?;
    let rows = stmt
        .query_map([skill_id], |r| {
            Ok(SkillFileRow {
                rel: r.get(0)?,
                bytes: r.get(1)?,
                sha256: r.get(2)?,
                size: r.get(3)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// 表中全部已分发技能 id（剔墓碑后）。
pub fn distributed_skill_ids(store: &Store) -> Result<Vec<String>> {
    let mut stmt = store.connection.prepare(
        "SELECT DISTINCT f.skill_id FROM skill_files f
         WHERE NOT EXISTS(SELECT 1 FROM skill_tombstones t WHERE t.skill_id = f.skill_id)
         ORDER BY f.skill_id",
    )?;
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// 记删除墓碑并清文件行（目录消失时由 reconcile 调）。
pub fn tombstone_skill(store: &Store, skill_id: &str) -> Result<()> {
    store.connection.execute(
        "INSERT OR REPLACE INTO skill_tombstones(skill_id, updated_at) VALUES(?1, ?2)",
        rusqlite::params![skill_id, now_rfc3339()],
    )?;
    store
        .connection
        .execute("DELETE FROM skill_files WHERE skill_id=?1", [skill_id])?;
    Ok(())
}

/// 墓碑及时间（id → updated_at）。
pub fn skill_tombstones(store: &Store) -> Result<Vec<(String, String)>> {
    let mut stmt = store
        .connection
        .prepare("SELECT skill_id, updated_at FROM skill_tombstones ORDER BY skill_id")?;
    let rows = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// 台账摘要（维护面板/诊断用，只读）。
pub fn skill_bundle_summary(store: &Store) -> Result<serde_json::Value> {
    let skills: i64 = store.connection.query_row(
        "SELECT COUNT(DISTINCT skill_id) FROM skill_files",
        [],
        |r| r.get(0),
    )?;
    let (files, total): (i64, i64) = store.connection.query_row(
        "SELECT COUNT(*), COALESCE(SUM(size),0) FROM skill_files",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let tombstones: i64 =
        store
            .connection
            .query_row("SELECT COUNT(*) FROM skill_tombstones", [], |r| r.get(0))?;
    Ok(json!({
        "skills": skills,
        "files": files,
        "bytes": total,
        "tombstones": tombstones,
    }))
}
