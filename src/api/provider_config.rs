//! AI provider 配置 FRB 门面。

use super::*;

/// Get active AI provider info
pub fn get_ai_provider() -> Result<Option<String>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    if let Some(provider) = store.active_ai_provider_config()? {
        Ok(Some(format!("{} - {}", provider.model, provider.base_url)))
    } else {
        Ok(None)
    }
}

/// AI provider config DTO for Flutter settings page
#[derive(Clone, Debug)]
pub struct AiProviderConfigDto {
    pub id: String,
    pub name: String,
    pub provider_type: String,
    pub base_url: String,
    pub model: String,
    pub api_key_source: String,
    /// 明文密钥只在保存时上行；读取时不回传（用 api_key_source 判断是否已配置）
    pub api_key: String,
    pub is_active: bool,
    pub temperature: f64,
    pub max_tokens: Option<i64>,
    pub context_window: Option<i64>,
}

fn dto_from_active(p: crate::storage::AiProviderConfig) -> AiProviderConfigDto {
    AiProviderConfigDto {
        id: p.id,
        name: p.name,
        provider_type: p.provider_type,
        base_url: p.base_url,
        model: p.model,
        api_key_source: p.api_key_source,
        api_key: String::new(),
        is_active: p.is_active,
        temperature: p.temperature,
        max_tokens: p.max_tokens,
        context_window: p.context_window,
    }
}

/// Get the active AI provider full config (for settings page prefill)
pub fn get_ai_provider_config() -> Result<Option<AiProviderConfigDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    Ok(store.active_ai_provider_config()?.map(dto_from_active))
}

/// 列出全部 AI provider 配置（多配置，仅一个 is_active=true）
pub fn list_ai_provider_configs() -> Result<Vec<AiProviderConfigDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    Ok(store
        .list_ai_provider_configs()?
        .into_iter()
        .map(|r| AiProviderConfigDto {
            id: r.id,
            name: r.name,
            provider_type: r.provider_type,
            base_url: r.base_url,
            model: r.model,
            api_key_source: r.api_key_source,
            api_key: String::new(),
            is_active: r.is_active,
            temperature: r.temperature,
            max_tokens: r.max_tokens,
            context_window: r.context_window,
        })
        .collect())
}

/// 新增或编辑 AI provider 配置；返回配置 id。
/// provider.id 为空表示新建；api_key 传空串表示保留原 key 不变（新建则必填）。
pub fn save_ai_provider_config(provider: AiProviderConfigDto) -> Result<String> {
    if provider.name.trim().is_empty() {
        anyhow::bail!("配置名称不能为空");
    }
    if provider.base_url.trim().is_empty() || provider.model.trim().is_empty() {
        anyhow::bail!("base_url 和 model 均不能为空");
    }
    if provider.id.trim().is_empty() && provider.api_key.trim().is_empty() {
        anyhow::bail!("新建配置时必须填写 API Key");
    }
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let id = if provider.id.trim().is_empty() {
        None
    } else {
        Some(provider.id.trim())
    };
    if let Some(output) = provider.max_tokens {
        anyhow::ensure!(
            output > 0 && output <= 1_000_000,
            "输出上限应为正数且不超过 1000000 token"
        );
    }
    if let Some(window) = provider.context_window {
        anyhow::ensure!(
            (4096..=2_000_000).contains(&window),
            "上下文窗口应为 4096 至 2000000 token"
        );
        anyhow::ensure!(
            provider.max_tokens.unwrap_or(4096) + (window / 20).clamp(256, 4096) < window,
            "上下文窗口必须大于输出预留与安全余量"
        );
    }
    let tx = rusqlite::Transaction::new_unchecked(
        &store.connection,
        rusqlite::TransactionBehavior::Immediate,
    )?;
    let saved = store.save_ai_provider_config(
        id,
        provider.name.trim(),
        &provider.provider_type,
        provider.base_url.trim(),
        provider.model.trim(),
        provider.api_key.trim(),
        provider.temperature,
        provider.max_tokens,
    )?;
    store.connection.execute(
        "UPDATE ai_provider_configs SET context_window=?2 WHERE id=?1",
        rusqlite::params![saved, provider.context_window],
    )?;
    tx.commit()?;
    Ok(saved)
}

/// 将指定配置设为激活（唯一激活项）
pub fn set_active_ai_provider_config(id: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.set_active_ai_provider_config(&id)?;
    Ok(())
}

/// 删除一个 AI provider 配置；若删除的是激活项，剩余第一条自动激活
pub fn delete_ai_provider_config(id: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.delete_ai_provider_config(&id)?;
    Ok(())
}

/// Upsert the active AI provider config (settings page save)
pub fn update_ai_provider_config(base_url: String, model: String, api_key: String) -> Result<()> {
    if base_url.trim().is_empty() || model.trim().is_empty() || api_key.trim().is_empty() {
        anyhow::bail!("base_url、model 和 api_key 均不能为空");
    }
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.upsert_ai_provider_config(&base_url, &model, &api_key)?;
    Ok(())
}
