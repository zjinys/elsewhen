//! 页面指引/情绪标签/伴侣偏好相关 API。

use super::*;

/// 知识库页聚合视图（D4）：哪些事件的分析 guidance 引用了此页。
#[derive(Debug, serde::Serialize)]
pub struct PageGuidanceDto {
    pub analysis_id: String,
    pub event_id: String,
    pub recorded_at: String,
    pub note: Option<String>,
    pub lessons: Vec<String>,
    pub follow_ups_refined: Vec<String>,
    pub feedback: Option<String>,
}

pub fn list_page_guidance(slug: String) -> Result<Vec<PageGuidanceDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    Ok(store
        .page_guidance_items(slug.trim())?
        .into_iter()
        .map(|item| PageGuidanceDto {
            analysis_id: item.analysis_id,
            event_id: item.event_id,
            recorded_at: item.recorded_at,
            note: item.note,
            lessons: item.lessons,
            follow_ups_refined: item.follow_ups_refined,
            feedback: item.feedback,
        })
        .collect())
}

/// 设置单条 guidance 条目的采纳反馈（遗留收口切片 B，只记录不推断）；
/// feedback None = 清除（toggle off）。未知值 fail-closed 报错。
pub fn set_guidance_feedback(
    analysis_id: String,
    slug: String,
    note: Option<String>,
    feedback: Option<String>,
) -> Result<bool> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    Ok(store.set_guidance_feedback(
        analysis_id.trim(),
        slug.trim(),
        note.as_deref().map(str::trim).filter(|n| !n.is_empty()),
        feedback.as_deref().map(str::trim).filter(|f| !f.is_empty()),
    )?)
}

/// 情绪标注列表（D5 用户模型面板）：最近分析的 emotion 派生。
pub fn list_event_emotions() -> Result<Vec<EventEmotionDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    Ok(store
        .list_event_emotions(100)?
        .into_iter()
        .map(|item| EventEmotionDto {
            id: item.analysis_id,
            event_id: item.event_id,
            recorded_at: item.recorded_at,
            target: item.target,
            feeling: item.feeling,
        })
        .collect())
}

/// 删除一条情绪标注：只编辑分析产物的 emotion 字段，事件真源不动。
pub fn delete_event_emotion(id: String, target: Option<String>, feeling: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let removed = store.delete_event_emotion(id.trim(), target.as_deref(), feeling.trim())?;
    anyhow::ensure!(removed, "情绪标注不存在或已被删除");
    Ok(())
}

/// 情感回访偏好（D6：开关键 + 频率上限；面板可见可改）。
#[derive(Clone, Debug)]
pub struct CompanionPrefsDto {
    pub revisit_enabled: bool,
    pub revisit_interval_days: i64,
}

/// 一次回访素材（D6）：被记住的事件 +（若有）当时情绪。
#[derive(Clone, Debug)]
pub struct CompanionRevisitDto {
    pub event_id: String,
    pub recorded_at: String,
    pub excerpt: String,
    pub feeling: Option<String>,
}

/// 读取回访偏好。
pub fn get_companion_prefs() -> Result<CompanionPrefsDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let settings = store.companion_settings()?;
    Ok(CompanionPrefsDto {
        revisit_enabled: settings.revisit_enabled,
        revisit_interval_days: settings.revisit_interval_days,
    })
}

/// 更新回访偏好（间隔即频率下限；关 = 之后 check 零查询零成本）。
pub fn set_companion_prefs(enabled: bool, interval_days: i64) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.set_companion_settings(enabled, interval_days)?;
    Ok(())
}

/// 打开应用时的一次回访检查：返回可展示的温和回引（有则记账本次展示）；
/// 关闭/未到期/无素材都返回 NULL。
pub fn check_companion_revisit() -> Result<Option<CompanionRevisitDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    Ok(store
        .check_companion_revisit()?
        .map(|item| CompanionRevisitDto {
            event_id: item.event_id,
            recorded_at: item.recorded_at,
            excerpt: item.excerpt,
            feeling: item.feeling,
        }))
}
