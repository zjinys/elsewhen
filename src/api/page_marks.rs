//! 翻篇（P2 手动 / P3 自动档）FFI 面。
//!
//! 手动翻篇 = 选边界消息 → AI 生成章节摘要（复用 chat-summary-v1 管道）
//! → 预览编辑（SummaryBody 五组条目）→ 固化。撤销只置 active=0，
//! 原文与 wiki 均不回滚。自动档（kind=auto）由装配预算 80% 触发线
//! 在生成链路内落标，不经本 FFI 创建，但展示/撤销共用本面。

use super::*;
use anyhow::{Context, ensure};

#[derive(Debug, Deserialize, Serialize)]
pub struct PageMarkDto {
    pub id: String,
    pub conversation_id: String,
    pub boundary_message_id: String,
    pub summary_node_id: String,
    /// 注入装配的完整摘要文本（含覆盖注记；override 优先渲染）。
    pub summary_preview: String,
    /// SummaryBody JSON（预览编辑的数据面）。
    pub body_json: String,
    pub kind: String,
    pub active: bool,
    pub created_at: String,
}

fn store_from_config() -> Result<crate::storage::Store> {
    let config = crate::config::AppConfig::load()?;
    let store = crate::storage::Store::open(&config.database_path)?;
    Ok(store)
}

pub fn list_page_marks(conversation_id: String) -> Result<Vec<PageMarkDto>> {
    let store = store_from_config()?;
    Ok(store
        .list_page_marks_with_preview(&conversation_id)?
        .into_iter()
        .map(|mark| PageMarkDto {
            id: mark.id,
            conversation_id: mark.conversation_id,
            boundary_message_id: mark.boundary_message_id,
            summary_node_id: mark.summary_node_id,
            summary_preview: mark.summary_preview,
            body_json: mark.body_json,
            kind: mark.kind,
            active: mark.active,
            created_at: mark.created_at,
        })
        .collect())
}

/// 生成翻篇摘要并落标。kind 只接受 "manual"——自动档在生成链路内触发。
/// 同步阻塞耗时（真实 AI 往返），调用方在工作线程触发。
pub fn create_page_mark(conversation_id: String, boundary_message_id: String) -> Result<String> {
    let store = store_from_config()?;
    let mut providers = store.list_ai_provider_configs_for_runtime()?;
    providers.retain(|config| config.is_active);
    let config = providers.first().context("无激活的 AI provider")?;
    ensure!(
        config.provider_type == "openai_compatible",
        "暂不支持该 provider 类型的翻篇摘要（{}）",
        config.provider_type
    );
    let provider_basis = crate::storage::knowledge::content_hash(&format!(
        "{}|{}|{}|{:?}|{:?}|{}",
        config.id,
        config.base_url,
        config.model,
        config.max_tokens,
        config.context_window,
        config.temperature
    ));
    let provider = crate::ai::provider::OpenAiCompatibleProvider::new(
        crate::ai::provider::OpenAiCompatibleConfig {
            base_url: config.base_url.clone(),
            api_key: config.api_key.clone(),
            model: config.model.clone(),
            temperature: config.temperature as f32,
            max_tokens: config.max_tokens.map(|v| v as u32),
            context_window: config.context_window.map(|v| v as u32),
        },
    )?;
    crate::ai::summary::generate_page_mark(
        &store,
        &conversation_id,
        &boundary_message_id,
        &provider,
        &config.name,
        &provider_basis,
        &config.model,
        "manual",
    )
}

/// 预览编辑（SummaryBody JSON，仍过摘要校验：条目数量/长度/非空）。
pub fn save_page_mark_override(id: String, body_json: String) -> Result<()> {
    let store = store_from_config()?;
    store.save_page_mark_override(&id, &body_json)
}

/// 撤销/恢复。撤销 = active=0：原文回到装配，已写 wiki 不回滚。
pub fn set_page_mark_active(id: String, active: bool) -> Result<()> {
    let store = store_from_config()?;
    store.set_page_mark_active(&id, active)
}
