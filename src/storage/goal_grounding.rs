//! Human-owned goal context carried by existing revisioned/synced wiki pages.
use super::{ContentPolicy, Store, WikiPageDraft};
use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
const PROFILE: &str = "profile/me";
#[derive(Default, Serialize, Deserialize)]
pub(crate) struct PlanningProfile {
    pub skills: String,
    pub resources: String,
    pub limits: String,
    pub preferences: String,
}
#[derive(Clone)]
pub(crate) struct GoalReference {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub summary: String,
}
impl Store {
    pub(crate) fn planning_profile(&self) -> Result<(PlanningProfile, Option<String>)> {
        match self.get_wiki_page(PROFILE)? {
            Some(p) => Ok((
                parse_profile(&p.content_md).context("个人档案格式已改变，请在知识页检查")?,
                Some(p.updated_at),
            )),
            None => {
                if let Some(old) = self.get_wiki_page("capability/personal-planning-profile")? {
                    return Ok((parse_profile(&old.content_md)?, None));
                }
                Ok((PlanningProfile::default(), None))
            }
        }
    }
    fn save_planning_page(
        &self,
        slug: &str,
        title: &str,
        body: &str,
        expected: Option<&str>,
    ) -> Result<String> {
        let tx = self.begin_write_transaction()?;
        let page = if self.get_wiki_page(slug)?.is_some() {
            ensure!(expected.is_some(), "内容已存在，请刷新后再保存");
            self.save_wiki_page_content(slug, body, "更新目标规划资料", expected)?
        } else {
            ensure!(expected.is_none(), "资料已删除，请刷新");
            self.upsert_wiki_page(
                &WikiPageDraft {
                    slug: slug.into(),
                    kind: if slug == PROFILE {
                        "profile".into()
                    } else {
                        "capability".into()
                    },
                    title: title.into(),
                    summary: String::new(),
                    content_md: body.into(),
                    tags: vec!["human-planning-context".into()],
                    source_event_ids: vec![],
                    status: "active".into(),
                    reason: "人工维护目标规划资料".into(),
                    source_url: None,
                },
                ContentPolicy::Always,
            )?
            .page
        };
        // Explicit human ownership prevents background digest overwrites.
        if page.human_edited_at.is_none() {
            self.save_wiki_page_content(
                slug,
                body,
                "人工确认目标规划资料",
                Some(&page.updated_at),
            )?;
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(slug.into())
    }
    pub(crate) fn save_planning_profile(
        &self,
        p: &PlanningProfile,
        expected: Option<&str>,
    ) -> Result<()> {
        for value in [&p.skills, &p.resources, &p.limits, &p.preferences] {
            ensure!(value.chars().count() <= 1500, "每项最多1500字");
        }
        self.save_planning_page(PROFILE, "关于我", &render_profile(p), expected)?;
        Ok(())
    }
    fn reference_slug(&self, goal: &str) -> Result<String> {
        let g = self.get_goal(goal)?.context("目标不存在")?;
        Ok(format!(
            "capability/goal-references-{}",
            g.plan_id.unwrap_or(g.id)
        ))
    }
    pub(crate) fn goal_references(
        &self,
        goal: &str,
    ) -> Result<(Vec<GoalReference>, Option<String>)> {
        let page = self.get_wiki_page(&self.reference_slug(goal)?)?;
        let Some(page) = page else {
            return Ok((vec![], None));
        };
        let ids: Vec<String> = serde_json::from_str(&page.content_md)?;
        let mut refs = Vec::new();
        for id in ids {
            if let Some(r)=self.connection.query_row("SELECT id,slug,title,summary FROM wiki_pages WHERE id=?1 AND status<>'archived' AND kind<>'memo' AND (kind NOT IN ('source','note') OR opinion='endorse') AND COALESCE(opinion,'')<>'reject'",[id],|r|Ok(GoalReference{id:r.get(0)?,slug:r.get(1)?,title:r.get(2)?,summary:r.get(3)?})).optional()?{refs.push(r);}
        }
        Ok((refs, Some(page.updated_at)))
    }
    pub(crate) fn save_goal_references(
        &self,
        goal: &str,
        ids: &[String],
        expected: Option<&str>,
    ) -> Result<()> {
        ensure!(ids.len() <= 3, "最多关联3篇参考资料");
        let mut unique = std::collections::HashSet::new();
        for id in ids {
            ensure!(unique.insert(id), "参考资料重复");
            let valid:bool=self.connection.query_row("SELECT EXISTS(SELECT 1 FROM wiki_pages WHERE id=?1 AND status<>'archived' AND kind<>'memo' AND (kind NOT IN ('source','note') OR opinion='endorse') AND COALESCE(opinion,'')<>'reject' AND NOT EXISTS(SELECT 1 FROM json_each(tags) WHERE value='human-planning-context'))",[id],|r|r.get(0))?;
            ensure!(valid, "参考页不存在、未认可或不能用于目标资料");
        }
        self.save_planning_page(
            &self.reference_slug(goal)?,
            "目标参考资料",
            &serde_json::to_string(ids)?,
            expected,
        )?;
        Ok(())
    }
    pub(crate) fn find_goal_references(&self, query: &str) -> Result<Vec<GoalReference>> {
        ensure!(query.chars().count() <= 200, "搜索词过长");
        let pattern = format!("%{}%", query.trim().replace('%', "\\%").replace('_', "\\_"));
        let mut q=self.connection.prepare("SELECT id,slug,title,summary FROM wiki_pages WHERE status<>'archived' AND kind<>'memo' AND (kind NOT IN ('source','note') OR opinion='endorse') AND COALESCE(opinion,'')<>'reject' AND NOT EXISTS(SELECT 1 FROM json_each(tags) WHERE value='human-planning-context') AND (title LIKE ?1 ESCAPE '\\' OR summary LIKE ?1 ESCAPE '\\') ORDER BY updated_at DESC LIMIT 20")?;
        Ok(q.query_map(params![pattern], |r| {
            Ok(GoalReference {
                id: r.get(0)?,
                slug: r.get(1)?,
                title: r.get(2)?,
                summary: r.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::memory::MemoryProvider;

    #[test]
    fn goal_studio_checks_missing_profile_before_planning() {
        let s = Store::open(
            &std::env::temp_dir().join(format!("profile-studio-{}.db", uuid::Uuid::new_v4())),
        )
        .unwrap();
        let studio = s.ensure_goal_studio_conversation().unwrap();
        let context = crate::ai::memory::SimpleMemory::new(10)
            .prepare_context(&studio, &s)
            .unwrap();
        assert!(
            context
                .iter()
                .any(|m| m.content.contains("尚无可用个人背景"))
        );
        s.save_planning_profile(
            &PlanningProfile {
                skills: "开发十年".into(),
                ..Default::default()
            },
            None,
        )
        .unwrap();
        let context = crate::ai::memory::SimpleMemory::new(10)
            .prepare_context(&studio, &s)
            .unwrap();
        assert!(
            context
                .iter()
                .any(|m| m.content.contains("尚未明确：已有资源和当前情况"))
        );
        assert!(
            !context
                .iter()
                .any(|m| m.content.contains("尚无可用个人背景"))
        );
    }
    #[test]
    fn profile_and_references_preserve_human_ownership_and_context() {
        let s = Store::open(
            &std::env::temp_dir().join(format!("grounding-{}.db", uuid::Uuid::new_v4())),
        )
        .unwrap();
        let p = PlanningProfile {
            skills: "Rust开发".into(),
            resources: "已有产品".into(),
            limits: "每周10小时".into(),
            preferences: "不做外包".into(),
        };
        s.save_planning_profile(&p, None).unwrap();
        assert!(
            s.get_wiki_page(PROFILE)
                .unwrap()
                .unwrap()
                .human_edited_at
                .is_some()
        );
        assert!(s.save_planning_profile(&p, None).is_err());
        assert_eq!(s.planning_profile().unwrap().0.skills, "Rust开发");
        let studio = s.ensure_goal_studio_conversation().unwrap();
        let context = crate::ai::memory::SimpleMemory::new(10)
            .prepare_context(&studio, &s)
            .unwrap();
        assert!(
            context
                .iter()
                .any(|m| m.content.contains("每周10小时") && m.content.contains("不做外包"))
        );

        let g = s
            .create_goal("验证付费意愿", super::super::GoalPhase::Near)
            .unwrap();
        let page = s
            .upsert_wiki_page(
                &WikiPageDraft {
                    slug: "topic/validation".into(),
                    kind: "topic".into(),
                    title: "先验证需求".into(),
                    summary: "先问客户".into(),
                    content_md: "访谈与付费测试".into(),
                    tags: vec![],
                    source_event_ids: vec![],
                    status: "active".into(),
                    reason: "test".into(),
                    source_url: None,
                },
                ContentPolicy::Always,
            )
            .unwrap()
            .page;
        s.save_goal_references(&g.id, &[page.id.clone()], None)
            .unwrap();
        assert_eq!(s.goal_references(&g.id).unwrap().0[0].slug, page.slug);
        assert!(
            s.save_goal_references(&g.id, &vec![page.id.clone(); 4], None)
                .is_err()
        );
        assert!(
            s.find_goal_references("验证")
                .unwrap()
                .iter()
                .any(|r| r.id == page.id)
        );
        assert!(
            !s.find_goal_references("")
                .unwrap()
                .iter()
                .any(|r| r.slug == PROFILE)
        );
        let profile = s.get_wiki_page(PROFILE).unwrap().unwrap();
        assert!(s.save_goal_references(&g.id, &[profile.id], None).is_err());
        let old = s.goal_references(&g.id).unwrap().1;
        s.save_goal_references(&g.id, &[], old.as_deref()).unwrap();
        assert!(s.goal_references(&g.id).unwrap().0.is_empty());
    }
}

fn render_profile(p: &PlanningProfile) -> String {
    format!(
        "# 关于我\n\n## 能力与经验\n{}\n\n## 已有资源\n{}\n\n## 时间与现实限制\n{}\n\n## 偏好与底线\n{}\n",
        p.skills, p.resources, p.limits, p.preferences
    )
}
fn parse_profile(raw: &str) -> Result<PlanningProfile> {
    if raw.trim_start().starts_with('{') {
        return Ok(serde_json::from_str(raw)?);
    }
    let mut p = PlanningProfile::default();
    let mut section = "";
    for line in raw.lines() {
        if let Some(title) = line.strip_prefix("## ") {
            section = title;
            continue;
        }
        let target = match section {
            "能力与经验" => &mut p.skills,
            "已有资源" => &mut p.resources,
            "时间与现实限制" => &mut p.limits,
            "偏好与底线" => &mut p.preferences,
            _ => continue,
        };
        if !target.is_empty() {
            target.push('\n');
        }
        target.push_str(line);
    }
    p.skills = p.skills.trim().into();
    p.resources = p.resources.trim().into();
    p.limits = p.limits.trim().into();
    p.preferences = p.preferences.trim().into();
    Ok(p)
}
