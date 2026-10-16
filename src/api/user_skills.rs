//! 用户技能管理：列表、启停、打开目录。
//!
//! 技能文件只由用户放置/修改（AI 无写技能目录的工具，见
//! docs/roadmap/2026-10-16-user-skills-roadmap.md 的信任边界）；这里的启停
//! 状态写在 `skills_state.json`，不影响磁盘上的技能文件。

use super::*;

pub struct UserSkillDto {
    /// 目录名，同时是 enable_tools 的技能 id。
    pub id: String,
    /// frontmatter name，缺省用 id。
    pub title: String,
    /// description 单行截断到 200 chars。
    pub brief: String,
    pub enabled: bool,
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
