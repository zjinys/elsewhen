//! 用户技能管理：列表、启停、打开目录。
//!
//! 技能文件只由用户放置/修改（AI 无写技能目录的工具，见
//! docs/roadmap/2026-10-16-user-skills-roadmap.md 的信任边界）；这里的启停
//! 状态写在 `skills_state.json`，不影响磁盘上的技能文件。

use super::*;
use std::path::{Path, PathBuf};

pub struct UserSkillDto {
    /// 目录名，同时是 enable_tools 的技能 id。
    pub id: String,
    /// frontmatter name，缺省用 id。
    pub title: String,
    /// description 单行截断到 200 chars。
    pub brief: String,
    pub enabled: bool,
    /// 免审批执行（skills_state.json trusted 列表）：true 时该技能的
    /// run_skill_script 调用直接起进程，不再逐次弹审批卡。
    pub trusted: bool,
    /// SKILL.md 是否存在（目录无 SKILL.md 的不会出现在列表，此字段保留给
    /// 未来"无效技能"提示用）。
    pub has_skill_md: bool,
}

pub fn list_user_skills() -> Result<Vec<UserSkillDto>> {
    let cfg = crate::config::AppConfig::load()?;
    let skills = crate::ai::skills::discover(&cfg.data_dir)?;
    Ok(skills
        .into_iter()
        .map(|skill| UserSkillDto {
            enabled: crate::ai::skills::is_enabled(&cfg.data_dir, &skill.id),
            trusted: crate::ai::skills::is_trusted(&cfg.data_dir, &skill.id),
            has_skill_md: skill.skill_md.is_file(),
            id: skill.id,
            title: skill.title,
            brief: skill.brief,
        })
        .collect())
}

pub fn set_user_skill_enabled(id: String, enabled: bool) -> Result<()> {
    let cfg = crate::config::AppConfig::load()?;
    crate::ai::skills::set_enabled(&cfg.data_dir, &id, enabled)
}

/// 免审批执行开关（与 MCP trusted 同模型）。开启后该技能的脚本调用直接执行。
pub fn set_user_skill_trusted(id: String, trusted: bool) -> Result<()> {
    let cfg = crate::config::AppConfig::load()?;
    crate::ai::skills::set_trusted(&cfg.data_dir, &id, trusted)
}

/// 卸载用户技能：删目录 + 记删除墓碑（同步语义：其他设备收到墓碑后删除）。
/// 这是删除的唯一合法入口——手动 rm 目录会被分发复活。
pub fn uninstall_user_skill(id: String) -> Result<()> {
    let cfg = crate::config::AppConfig::load()?;
    let store = Store::open(&cfg.database_path)?;
    uninstall_user_skill_at(&cfg.data_dir, &store, &id)
}

#[flutter_rust_bridge::frb(ignore)]
pub fn uninstall_user_skill_at(data_dir: &std::path::Path, store: &Store, id: &str) -> Result<()> {
    let skill_dir = crate::ai::skills::skills_dir(data_dir).join(id);
    anyhow::ensure!(
        !id.contains('/') && !id.contains('\\') && id != "." && id != "..",
        "非法技能名：{id}"
    );
    if skill_dir.exists() {
        std::fs::remove_dir_all(&skill_dir)
            .with_context(|| format!("删除技能目录 {skill_dir:?}"))?;
    }
    crate::storage::skills::tombstone_skill(store, id)?;
    // 启停状态一并清掉（本地文件，不在同步范围）
    crate::ai::skills::set_enabled(data_dir, id, true)?;
    crate::ai::skills::record_audit(data_dir, "uninstall", id);
    Ok(())
}

/// 技能目录绝对路径（供前端"打开技能目录"），目录不存在则创建。
pub fn user_skills_directory_path() -> Result<String> {
    let cfg = crate::config::AppConfig::load()?;
    let dir = crate::ai::skills::skills_dir(&cfg.data_dir);
    std::fs::create_dir_all(&dir)?;
    Ok(dir.to_string_lossy().to_string())
}

/// 技能安装：把来源（本地目录 / zip / git 仓库）收敛到 skills/<id>。
/// 三种来源最终都走同一条校验管道：必须含 SKILL.md、id 合法且不重复、
/// 装完能被 discover 解析；失败即回滚，不留半成品。

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &to)?;
        } else if ty.is_file() {
            std::fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}

fn staging_dir(data_dir: &Path) -> Result<PathBuf> {
    let base = data_dir.join(".skill-staging");
    std::fs::create_dir_all(&base)?;
    let dir = base.join(format!("stage-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// 安装公共收尾：从 staged 目录收敛到 skills/<id>，校验，失败清理。
/// id 取 frontmatter name，且必须是合法目录名；重名拒绝（需用户手动删旧版）。
fn install_staged(data_dir: &Path, staged: &Path, staging_root: &Path) -> Result<UserSkillDto> {
    let staged = staged
        .canonicalize()
        .unwrap_or_else(|_| staged.to_path_buf());
    let skill_md = staged.join("SKILL.md");
    let staged_ref = staged.as_path();
    anyhow::ensure!(
        skill_md.is_file(),
        "所选内容里没有 SKILL.md（技能必须包含 SKILL.md 入口）"
    );
    let parsed = crate::ai::skills::parse_skill_md(&skill_md)?;
    let id = parsed.title;
    anyhow::ensure!(
        !id.is_empty()
            && !id.starts_with('.')
            && !id.contains(['/', '\\'])
            && id != "."
            && id != ".."
            && id.chars().all(|c| !c.is_control()),
        "技能名不适合作为目录：{id}"
    );
    let target = crate::ai::skills::skills_dir(data_dir).join(&id);
    anyhow::ensure!(
        !target.exists(),
        "技能 {id} 已安装；如需覆盖请先在技能目录里删除旧版"
    );
    let existing = crate::ai::skills::discover(data_dir)?;
    anyhow::ensure!(
        existing.len() < crate::ai::skills::USER_SKILL_LIMIT,
        "用户技能已达上限 {} 个；请先在技能目录里移除不用的技能再安装",
        crate::ai::skills::USER_SKILL_LIMIT
    );
    std::fs::create_dir_all(target.parent().unwrap())?;
    std::fs::rename(staged_ref, &target).or_else(|_| {
        // rename 跨文件系统会失败（staging 与数据目录可能不同挂载点）
        copy_dir_recursive(staged_ref, &target)?;
        Ok::<(), anyhow::Error>(())
    })?;
    let reload = || -> Result<UserSkillDto> {
        let skill = crate::ai::skills::discover(data_dir)?
            .into_iter()
            .find(|s| s.id == id)
            .with_context(|| format!("安装后未发现技能 {id}"))?;
        Ok(UserSkillDto {
            id: skill.id,
            title: skill.title,
            brief: skill.brief,
            enabled: true,
            trusted: false,
            has_skill_md: skill.skill_md.is_file(),
        })
    };
    match reload() {
        Ok(dto) => {
            crate::ai::skills::record_audit(data_dir, "install", &dto.id);
            // 即时写分发台账（P3）：安装即全屋可见（其他设备下次启动对账接住）。
            // 失败不回滚安装——本地已可用，下次启动 reconcile 会补上行。
            let db_path = crate::config::AppConfig::load()
                .map(|c| c.database_path)
                .ok();
            if let Some(db_path) = db_path {
                let bundle = crate::ai::skills::collect_bundle_files(&target);
                if let Ok(files) = bundle {
                    if let Ok(store) = Store::open(&db_path) {
                        let rows = files
                            .iter()
                            .map(|(rel, bytes, sha)| crate::storage::skills::SkillFileRow {
                                rel: rel.clone(),
                                bytes: bytes.clone(),
                                sha256: sha.clone(),
                                size: bytes.len() as i64,
                            })
                            .collect::<Vec<_>>();
                        crate::storage::skills::replace_skill_files(&store, &dto.id, &rows)
                            .unwrap_or_else(|e| eprintln!("[skills] bundle write failed: {e:#}"));
                    }
                }
            }
            std::fs::remove_dir_all(staging_root).ok();
            Ok(dto)
        }
        Err(e) => {
            std::fs::remove_dir_all(&target).ok();
            std::fs::remove_dir_all(staging_root).ok();
            Err(e)
        }
    }
}

/// 安装本地目录：FilePicker 选目录后传绝对路径。
/// 所选目录无 SKILL.md 但一级子目录里恰好唯一一个含 SKILL.md 的，自动下钻
/// （clone 的仓库外壳 / 用户多选了上层目录都很常见）。
pub fn install_skill_from_directory(source: String) -> Result<UserSkillDto> {
    install_skill_from_directory_at(&crate::config::AppConfig::load()?.data_dir, source)
}

/// 测试可注入 data_dir 的真实现——绝不静默落到真实用户目录。
#[flutter_rust_bridge::frb(ignore)]
pub fn install_skill_from_directory_at(data_dir: &Path, source: String) -> Result<UserSkillDto> {
    let source = PathBuf::from(source)
        .canonicalize()
        .context("解析所选目录")?;
    anyhow::ensure!(source.is_dir(), "所选路径不是目录");
    let skill_root = if source.join("SKILL.md").is_file() {
        source.clone()
    } else if let Some(inner) = unique_skill_subdir(&source)? {
        inner
    } else {
        anyhow::bail!("所选目录及其一级子目录里没有找到 SKILL.md，无法确定技能入口");
    };
    let staging_root = staging_dir(data_dir)?;
    let staged = staging_root.join("skill");
    copy_dir_recursive(&skill_root, &staged)?;
    install_staged(data_dir, &staged, &staging_root)
}

fn unique_skill_subdir(source: &Path) -> Result<Option<PathBuf>> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(source)? {
        let path = entry?.path();
        if path.is_dir() && path.join("SKILL.md").is_file() {
            found.push(path);
        }
    }
    Ok(match found.len() {
        1 => Some(found.remove(0)),
        _ => None,
    })
}

/// 安装 zip：内部做 zip-slip 防御；根有 SKILL.md 视为技能本体，
/// 否则下钻唯一含 SKILL.md 的顶层子目录。
pub fn install_skill_from_zip(zip_path: String) -> Result<UserSkillDto> {
    install_skill_from_zip_at(&crate::config::AppConfig::load()?.data_dir, zip_path)
}

/// 测试可注入 data_dir 的真实现。
#[flutter_rust_bridge::frb(ignore)]
pub fn install_skill_from_zip_at(data_dir: &Path, zip_path: String) -> Result<UserSkillDto> {
    let file = std::fs::File::open(&zip_path).with_context(|| format!("打开 {zip_path}"))?;
    let mut archive = zip::ZipArchive::new(file).context("不是有效的 zip 包")?;
    let staging_root = staging_dir(data_dir)?;
    let staged = staging_root.join("skill");
    std::fs::create_dir_all(&staged)?;
    let outcome = (|| -> Result<()> {
        let canonical_root = staged.canonicalize()?;
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let raw = entry.name().context("读取 zip 条目名")?.to_string();
            anyhow::ensure!(
                !raw.contains("..") && !raw.starts_with('/') && !raw.contains(':'),
                "zip 内含非法路径条目：{raw}"
            );
            let rel = raw.trim_end_matches('/');
            if rel.is_empty() {
                continue;
            }
            let target = staged.join(rel);
            if entry.is_dir() {
                std::fs::create_dir_all(&target)?;
                continue;
            }
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            anyhow::ensure!(
                target
                    .parent()
                    .unwrap()
                    .canonicalize()?
                    .starts_with(&canonical_root),
                "zip 路径越界：{rel}"
            );
            let mut out = std::fs::File::create(&target)?;
            std::io::copy(&mut entry, &mut out)?;
        }
        Ok(())
    })();
    if let Err(e) = outcome {
        std::fs::remove_dir_all(&staging_root).ok();
        return Err(e);
    }
    // 根有 SKILL.md → 本体；否则下钻唯一含 SKILL.md 的顶层子目录
    let skill_root = if staged.join("SKILL.md").is_file() {
        staged.clone()
    } else if let Some(inner) = unique_skill_subdir(&staged)? {
        inner
    } else {
        std::fs::remove_dir_all(&staging_root).ok();
        anyhow::bail!("zip 内没有找到 SKILL.md");
    };
    install_staged(data_dir, &skill_root, &staging_root)
}

/// 从 git 仓库安装（`git clone --depth 1`）。桌面端依赖系统 git；手机端报错。
/// url 仅接受 https:// 或本地路径，禁止 file: 与:`其它 scheme 的超级歧义形式。
pub fn install_skill_from_git(url: String) -> Result<UserSkillDto> {
    install_skill_from_git_at(&crate::config::AppConfig::load()?.data_dir, url)
}

/// 测试可注入 data_dir 的真实现。
#[flutter_rust_bridge::frb(ignore)]
pub fn install_skill_from_git_at(data_dir: &Path, url: String) -> Result<UserSkillDto> {
    anyhow::ensure!(
        url.starts_with("https://") || url.starts_with("http://") || !url.contains("://"),
        "仅支持 https 仓库地址或本地路径；不支持的协议：{url}"
    );
    let staging_root = staging_dir(data_dir)?;
    let clone_dir = staging_root.join("skill");
    let output = std::process::Command::new("git")
        .args(["clone", "--depth", "1", &url])
        .arg(&clone_dir)
        .output()
        .context("启动 git 失败（桌面端需要安装 git；手机端暂不支持 git 安装）")?;
    if !output.status.success() {
        std::fs::remove_dir_all(&staging_root).ok();
        // stderr 可能带 URL 凭证，只取首行与状态码，不透传全文
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let summary = stderr
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("未知错误")
            .to_string();
        anyhow::bail!(
            "git clone 失败（exit {:?}）：{}",
            output.status.code(),
            summary
        );
    }
    install_staged(data_dir, &clone_dir, &staging_root)
}

#[cfg(test)]
mod install_tests {
    use super::*;
    use std::path::PathBuf;

    fn workdir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "elsewhen-skill-install-{}-{}",
            tag,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_skill(root: &Path, name: &str, skill_name: &str, description: &str) -> PathBuf {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {skill_name}\ndescription: {description}\n---\n\n# 用法\n正文。"),
        )
        .unwrap();
        dir
    }

    #[test]
    fn directory_install_with_shell_drilldown() {
        let dir = workdir("dir-install");
        let cfg_data = dir.join("data");
        std::fs::create_dir_all(&cfg_data).unwrap();
        // 仓库外壳：repo/proxy-pick/SKILL.md
        let repo = dir.join("repo");
        write_skill(&repo, "proxy-pick", "proxy-pick", "切换 Clash 代理节点");
        let dto =
            install_skill_from_directory_at(&cfg_data, repo.to_string_lossy().to_string()).unwrap();
        assert_eq!(dto.id, "proxy-pick");
        assert!(cfg_data.join("skills/proxy-pick/SKILL.md").is_file());
        // staging 已清空
        assert!(
            std::fs::read_dir(cfg_data.join(".skill-staging"))
                .unwrap()
                .count()
                == 0
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn duplicate_install_is_rejected() {
        let dir = workdir("dup");
        let cfg_data = dir.join("data");
        std::fs::create_dir_all(&cfg_data).unwrap();
        let repo = dir.join("repo");
        write_skill(&repo, "proxy-pick", "proxy-pick", "第一次安装");
        install_skill_from_directory_at(&cfg_data, repo.to_string_lossy().to_string()).unwrap();
        let again = install_skill_from_directory_at(&cfg_data, repo.to_string_lossy().to_string())
            .err()
            .expect("第二次安装应当失败");
        let err = format!("{again:#}");
        assert!(err.contains("已安装"), "意外错误：{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn zip_install_rejects_slip_and_installs_nested() {
        use std::io::Write;
        let dir = workdir("zip");
        let cfg_data = dir.join("data");
        std::fs::create_dir_all(&cfg_data).unwrap();

        let make_zip = |path: &Path, entries: Vec<(&str, Vec<u8>)>| {
            let file = std::fs::File::create(path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let opts = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            for (name, body) in entries {
                zip.start_file(name, opts).unwrap();
                zip.write_all(&body).unwrap();
            }
            zip.finish().unwrap();
        };

        // 正常包：顶层单目录 skill-demo/
        let good = dir.join("good.zip");
        make_zip(
            &good,
            vec![
                (
                    "skill-demo/SKILL.md",
                    b"---\nname: skill-demo\ndescription: zip \xe5\xae\x89\xe8\xa3\x85\n---\nbody"
                        .to_vec(),
                ),
                ("skill-demo/templates/run.sh", b"#!/bin/sh\n".to_vec()),
            ],
        );
        let dto = install_skill_from_zip_at(&cfg_data, good.to_string_lossy().to_string()).unwrap();
        assert_eq!(dto.id, "skill-demo");
        assert!(
            cfg_data
                .join("skills/skill-demo/templates/run.sh")
                .is_file()
        );

        // zip-slip 包：条目逃逸出根
        let evil = dir.join("evil.zip");
        make_zip(&evil, vec![("../pwned.txt", b"x".to_vec())]);
        let err = install_skill_from_zip_at(&cfg_data, evil.to_string_lossy().to_string())
            .err()
            .expect("zip-slip 应当失败");
        assert!(format!("{err:#}").contains("非法路径"));
        assert!(!cfg_data.join("pwned.txt").exists(), "zip-slip 得手了！");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_limit_is_enforced() {
        let dir = workdir("limit30");
        let cfg_data = dir.join("data");
        std::fs::create_dir_all(&cfg_data).unwrap();
        let mut last = None;
        for i in 0..crate::ai::skills::USER_SKILL_LIMIT {
            let repo = dir.join(format!("repo-{i}"));
            write_skill(&repo, "pick-{i}", &format!("pick-{i}"), "填量");
            last = Some(install_skill_from_directory_at(
                &cfg_data,
                repo.to_string_lossy().to_string(),
            ));
        }
        assert!(last.unwrap().is_ok(), "前 30 个应当装满");
        let over = dir.join("repo-over");
        write_skill(&over, "pick-over", "pick-over", "第 31 个");
        let err = install_skill_from_directory_at(&cfg_data, over.to_string_lossy().to_string())
            .err()
            .expect("超过上限应当拒绝");
        assert!(format!("{err:#}").contains("上限"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_writes_audit_log() {
        let dir = workdir("audit");
        let cfg_data = dir.join("data");
        std::fs::create_dir_all(&cfg_data).unwrap();
        let repo = dir.join("repo");
        write_skill(&repo, "audited", "audited", "审计验证");
        install_skill_from_directory_at(&cfg_data, repo.to_string_lossy().to_string()).unwrap();
        crate::ai::skills::set_enabled(&cfg_data, "audited", false).unwrap();
        let log = std::fs::read_to_string(cfg_data.join("skills/.audit.log")).unwrap();
        assert!(log.contains("\"install\""));
        assert!(log.contains("\"disable\""));
        assert!(log.contains("audited"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn git_install_from_local_path() {
        let dir = workdir("git");
        let cfg_data = dir.join("data");
        std::fs::create_dir_all(&cfg_data).unwrap();
        // 造一个本地 git 仓库
        let repo = dir.join("pick-repo");
        let skill = write_skill(&repo, "", "git-skill", "来自 git 的技能");
        run_git(&repo, &["init", "-q"]);
        run_git(&repo, &["-C", repo.to_str().unwrap(), "add", "."]);
        run_git(
            &repo,
            &["-C", repo.to_str().unwrap(), "config", "user.email", "t@t"],
        );
        run_git(
            &repo,
            &["-C", repo.to_str().unwrap(), "config", "user.name", "t"],
        );
        run_git(
            &repo,
            &["-C", repo.to_str().unwrap(), "commit", "-qm", "init"],
        );
        let _ = skill;
        let dto = install_skill_from_git_at(&cfg_data, repo.to_string_lossy().to_string()).unwrap();
        assert_eq!(dto.id, "git-skill");
        assert!(cfg_data.join("skills/git-skill/SKILL.md").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn run_git(cwd: &Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(cwd)
            .status()
            .expect("git 可用");
        assert!(status.success(), "git {args:?} 失败");
    }
}
