//! reconcile 对账测试。

use super::*;

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
    let _ = std::fs::remove_dir_all(std::env::temp_dir().join("elsewhen-reconcile-traversal-0"));
}
