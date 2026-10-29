//! AI provider 配置 FRB 门面。

use super::*;

/// Get active AI provider info
pub fn get_ai_provider() -> Result<Option<String>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    if let Some(provider) = store.active_ai_provider_metadata()? {
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
    /// 明文密钥只在保存时上行；读取时不回传（用 has_api_key 判断是否已配置）
    pub api_key: String,
    /// 明文或可解密封存任一存在即 true；锁定态也能正确判定（只查存在性不解密）
    pub has_api_key: bool,
    pub is_active: bool,
    pub temperature: f64,
    pub max_tokens: Option<i64>,
    pub context_window: Option<i64>,
    /// Local edit basis captured with the displayed metadata; never refreshed on save.
    pub revision_token: Option<String>,
}

fn dto_from_active(
    store: &Store,
    p: crate::storage::AiProviderConfig,
) -> Result<AiProviderConfigDto> {
    let revision_token = Some(store.provider_edit_token(&p.id)?);
    Ok(AiProviderConfigDto {
        id: p.id,
        name: p.name,
        provider_type: p.provider_type,
        base_url: p.base_url,
        model: p.model,
        api_key_source: p.api_key_source,
        api_key: String::new(),
        // active_ai_provider_metadata 的 WHERE 已过滤「必须有 key」，恒 true。
        has_api_key: true,
        is_active: p.is_active,
        temperature: p.temperature,
        max_tokens: p.max_tokens,
        context_window: p.context_window,
        revision_token,
    })
}

/// Get the active AI provider full config (for settings page prefill)
pub fn get_ai_provider_config() -> Result<Option<AiProviderConfigDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let tx = store.connection.unchecked_transaction()?;
    let result = store
        .active_ai_provider_metadata()?
        .map(|p| dto_from_active(&store, p))
        .transpose()?;
    tx.commit()?;
    Ok(result)
}

/// 列出全部 AI provider 配置（多配置，仅一个 is_active=true）
pub fn list_ai_provider_configs() -> Result<Vec<AiProviderConfigDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let tx = store.connection.unchecked_transaction()?;
    let result = store
        .list_ai_provider_configs()?
        .into_iter()
        .map(|r| {
            let revision_token = Some(store.provider_edit_token(&r.id)?);
            Ok(AiProviderConfigDto {
                id: r.id,
                name: r.name,
                provider_type: r.provider_type,
                base_url: r.base_url,
                model: r.model,
                api_key_source: r.api_key_source,
                api_key: String::new(),
                has_api_key: r.has_api_key,
                is_active: r.is_active,
                temperature: r.temperature,
                max_tokens: r.max_tokens,
                context_window: r.context_window,
                revision_token,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    tx.commit()?;
    Ok(result)
}

/// 复制（duplicate）一个 AI provider 配置：字段连同 API Key 一起克隆到新
/// 配置，返回新 id。Key 明文只在 Rust 内解密并重新密封到新 id，不进 Dart/日志。
/// 新配置永不激活；名称接「X 副本」，撞名自动递增序号。
pub fn duplicate_ai_provider_config(id: String) -> Result<String> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.duplicate_ai_provider_config(&id)
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
    if let Some(id) = id {
        store.validate_provider_edit_token(id, provider.revision_token.as_deref())?;
    }
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
pub fn set_active_ai_provider_config(id: String, revision_token: Option<String>) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let tx = store.begin_write_transaction()?;
    store.validate_provider_edit_token(&id, revision_token.as_deref())?;
    store.set_active_ai_provider_config(&id)?;
    if let Some(tx) = tx {
        tx.commit()?;
    }
    Ok(())
}

/// 删除一个 AI provider 配置；若删除的是激活项，剩余第一条自动激活
pub fn delete_ai_provider_config(id: String, revision_token: Option<String>) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let tx = store.begin_write_transaction()?;
    store.validate_provider_edit_token(&id, revision_token.as_deref())?;
    store.delete_ai_provider_config(&id)?;
    if let Some(tx) = tx {
        tx.commit()?;
    }
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
