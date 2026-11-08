use crate::storage::{Store, goal_grounding::PlanningProfile};
use anyhow::Result;
fn store() -> Result<Store> {
    Store::open(&crate::config::AppConfig::load()?.database_path)
}
pub struct PlanningProfileDto {
    pub skills: String,
    pub resources: String,
    pub limits: String,
    pub preferences: String,
    pub updated_at: Option<String>,
}
pub fn get_planning_profile() -> Result<PlanningProfileDto> {
    let (p, updated_at) = store()?.planning_profile()?;
    Ok(PlanningProfileDto {
        skills: p.skills,
        resources: p.resources,
        limits: p.limits,
        preferences: p.preferences,
        updated_at,
    })
}
pub fn save_planning_profile(profile: PlanningProfileDto) -> Result<()> {
    store()?.save_planning_profile(
        &PlanningProfile {
            skills: profile.skills,
            resources: profile.resources,
            limits: profile.limits,
            preferences: profile.preferences,
        },
        profile.updated_at.as_deref(),
    )
}
pub struct GoalReferenceDto {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub summary: String,
}
pub struct GoalReferencesDto {
    pub pages: Vec<GoalReferenceDto>,
    pub updated_at: Option<String>,
}
fn dto(p: crate::storage::goal_grounding::GoalReference) -> GoalReferenceDto {
    GoalReferenceDto {
        id: p.id,
        slug: p.slug,
        title: p.title,
        summary: p.summary,
    }
}
pub fn get_goal_references(goal_id: String) -> Result<GoalReferencesDto> {
    let (pages, updated_at) = store()?.goal_references(&goal_id)?;
    Ok(GoalReferencesDto {
        pages: pages.into_iter().map(dto).collect(),
        updated_at,
    })
}
pub fn find_goal_references(query: String) -> Result<Vec<GoalReferenceDto>> {
    Ok(store()?
        .find_goal_references(&query)?
        .into_iter()
        .map(dto)
        .collect())
}
pub fn save_goal_references(
    goal_id: String,
    page_ids: Vec<String>,
    expected_updated_at: Option<String>,
) -> Result<()> {
    store()?.save_goal_references(&goal_id, &page_ids, expected_updated_at.as_deref())
}
