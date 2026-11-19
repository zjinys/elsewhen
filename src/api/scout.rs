//! Scout 情报消费端 FRB 门面（提案 2026-11-15 §4）。
//!
//! 设置（base url + token 命名密钥 + 间隔）、手动/后台拉取、镜像查询、
//! 已读管理、源状态只读列表。网络纪律在 [`crate::scout::ScoutClient`]；
//! 这里只做编排：打开库 → 读设置 → 解析密钥 → 调客户端 → 落镜像。

use super::*;

/// scout 连接设置 DTO（token 不回传明文，用 has_token 判断）。
#[derive(Clone, Debug)]
pub struct ScoutSettingsDto {
    pub base_url: String,
    pub poll_interval_minutes: i64,
    pub enabled: bool,
    pub has_token: bool,
    pub last_polled_at: Option<String>,
}

/// 一条镜像 finding（payload 保持 JSON 字符串，Dart 侧按源解析）。
#[derive(Clone, Debug)]
pub struct ScoutFindingDto {
    pub id: i64,
    pub source_id: String,
    pub snapshot_id: i64,
    pub kind: String,
    pub item_key: String,
    pub payload_json: String,
    /// 字段级变化（仅 kind=changed），JSON 对象字符串；None = 无明细。
    pub delta_json: Option<String>,
    pub detected_at: String,
    pub read_at: Option<String>,
}

/// 手动/后台拉取的结果。
#[derive(Clone, Debug)]
pub struct ScoutPollResultDto {
    /// 本批从服务端拉到的条数。
    pub fetched: i64,
    /// 新插入镜像的条数（去重后）。
    pub new_count: i64,
    /// 拉取后的未读总数。
    pub unread_total: i64,
}

/// 数据源页的源状态（服务端 /sources 实时数据，不落库）。
#[derive(Clone, Debug)]
pub struct ScoutSourceDto {
    pub id: String,
    pub name: String,
    pub interval_minutes: i64,
    pub enabled: bool,
    pub last_status: Option<String>,
    pub last_run_at: Option<String>,
}

/// token 在命名密钥里的固定代号（设置页把明文存进来，这里只存引用）。
const SCOUT_TOKEN_SECRET: &str = "scout_token";

fn open_store() -> Result<crate::storage::Store> {
    let config = crate::config::AppConfig::load()?;
    crate::storage::Store::open(&config.database_path)
}

/// 读取 scout 设置（未配置返回 None 形状的默认值标记 configured=false）。
pub fn get_scout_settings() -> Result<Option<ScoutSettingsDto>> {
    let store = open_store()?;
    let Some(s) = store.scout_settings()? else {
        return Ok(None);
    };
    Ok(Some(ScoutSettingsDto {
        base_url: s.base_url,
        poll_interval_minutes: s.poll_interval_minutes,
        enabled: s.enabled,
        has_token: s.token_secret.is_some(),
        last_polled_at: s.last_polled_at,
    }))
}

/// 保存 scout 设置。`token` 传空串 = 保留已存 token；非空则写入命名密钥
/// `scout_token` 并在设置里记引用（明文不落 scout_settings）。
pub fn save_scout_settings(
    base_url: String,
    token: String,
    poll_interval_minutes: i64,
    enabled: bool,
) -> Result<()> {
    let store = open_store()?;
    let existing = store.scout_settings()?;
    let token_secret = if token.trim().is_empty() {
        // 不改动已存 token；但从未配置过且传空 = 无 token（免鉴权部署）。
        existing.as_ref().and_then(|s| s.token_secret.clone())
    } else {
        store.save_named_secret(SCOUT_TOKEN_SECRET, "Scout API token", token.trim())?;
        Some(SCOUT_TOKEN_SECRET.to_string())
    };
    if existing
        .as_ref()
        .is_some_and(|s| s.base_url.trim_end_matches('/') != base_url.trim().trim_end_matches('/'))
    {
        store.clear_scout_mirror()?;
    }
    store.save_scout_settings(&crate::scout::ScoutSettings {
        base_url,
        token_secret,
        poll_interval_minutes,
        enabled,
        last_polled_at: existing.and_then(|s| s.last_polled_at),
    })
}

fn resolve_token(
    store: &crate::storage::Store,
    settings: &crate::scout::ScoutSettings,
) -> Result<String> {
    match &settings.token_secret {
        Some(name) => store.read_named_secret(name),
        None => Ok(String::new()),
    }
}

/// 连接测试（设置页「测试连接」按钮）：health + 源列表可达性。
pub fn test_scout_connection() -> Result<String> {
    let store = open_store()?;
    let Some(settings) = store.scout_settings()? else {
        anyhow::bail!("尚未配置 scout 服务地址");
    };
    let token = resolve_token(&store, &settings)?;
    let client = crate::scout::ScoutClient::new(&settings.base_url)?;
    if !client.health()? {
        anyhow::bail!("scout 服务 /health 不可达");
    }
    let sources = client.fetch_sources(&token)?;
    Ok(format!("ok: {} 个源", sources.len()))
}

/// 拉取一轮 findings 进镜像（后台 worker tick 与手动「立即同步」共用）。
///
/// 以服务端升序 ID 和固定批次上界续传，每页落库与游标在同一事务提交。
/// `last_polled_at` 仅控制轮询频率。网络失败保留已提交进度和离线镜像。
pub fn poll_scout_now() -> Result<ScoutPollResultDto> {
    let store = open_store()?;
    let Some(settings) = store.scout_settings()? else {
        anyhow::bail!("尚未配置 scout 服务地址");
    };
    anyhow::ensure!(settings.enabled, "scout 拉取已停用");
    let token = resolve_token(&store, &settings)?;
    let client = crate::scout::ScoutClient::new(&settings.base_url)?;
    let started = std::time::Instant::now();
    let mut fetched = 0_i64;
    let mut new_count = 0_i64;
    // Each committed page advances a persistent server cursor, never the local clock.
    // Bound one tick; if interrupted or budget-exhausted the next tick resumes its upper bound.
    loop {
        let cursor = store.scout_cursor()?;
        let after = cursor.as_ref().map_or(0, |c| c.1);
        let upper = cursor.as_ref().and_then(|c| c.2);
        let page = match client.finding_page(&token, after, upper) {
            Ok(page) => page,
            Err(e)
                if e.downcast_ref::<crate::scout::client::LegacyScout>()
                    .is_some() =>
            {
                // Legacy Python can supply a bounded activity view only. Never
                // advance an unsafe time cursor or present it as complete history.
                let items = client.fetch_findings(&token, None, 1000)?;
                fetched = items.len() as i64;
                new_count = store.upsert_findings(&items)? as i64;
                break;
            }
            Err(e) => return Err(e),
        };
        fetched += page.findings()?.len() as i64;
        new_count += store.apply_scout_page(&page, after)? as i64;
        if !page.has_more || fetched >= 5000 || started.elapsed().as_secs() >= 20 {
            break;
        }
    }
    let watermark = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    store.touch_scout_polled_at(&watermark)?;
    let unread_total = store.scout_unread_count()?;
    // Refresh a bounded, already-ranked signal page independently of the open UI.
    // Signal failures must never turn a successful findings sync into a failed sync.
    match client.research_get(&token, "/v1/products?view=signals") {
        Ok(page) => {
            if let Some(instance) = page["instance_id"].as_str() {
                if let Err(error) = store.cache_scout_products(
                    "/v1/products?view=signals",
                    instance,
                    &page.to_string(),
                ) {
                    crate::logging::log_always(&format!("[scout] signal cache: {error}"));
                } else if let Err(error) = store.tick_scout_brief_card() {
                    crate::logging::log_always(&format!("[scout] brief: {error}"));
                }
            }
        }
        Err(error) => crate::logging::log_always(&format!("[scout] signals unavailable: {error}")),
    }
    Ok(ScoutPollResultDto {
        fetched,
        new_count,
        unread_total,
    })
}

/// 镜像 findings 分页查询（发现页）。
pub fn list_scout_findings(
    kind: Option<String>,
    source_id: Option<String>,
    unread_only: bool,
    limit: i64,
    offset: i64,
) -> Result<Vec<ScoutFindingDto>> {
    let store = open_store()?;
    let rows = store.list_scout_findings(
        kind.as_deref(),
        source_id.as_deref(),
        unread_only,
        limit,
        offset,
    )?;
    Ok(rows
        .into_iter()
        .map(|r| ScoutFindingDto {
            id: r.id,
            source_id: r.source_id,
            snapshot_id: r.snapshot_id,
            kind: r.kind,
            item_key: r.item_key,
            payload_json: r.payload_json,
            delta_json: r.delta_json,
            detected_at: r.detected_at,
            read_at: r.read_at,
        })
        .collect())
}

/// 未读计数（侧栏徽标 + 今天栏卡片）。
pub fn scout_unread_count() -> Result<i64> {
    let store = open_store()?;
    store.scout_unread_count()
}

/// 各源未读数（侧栏徽标）。
pub fn scout_unread_by_source() -> Result<Vec<ScoutSourceUnreadDto>> {
    let store = open_store()?;
    Ok(store
        .scout_unread_by_source()?
        .into_iter()
        .map(|(source_id, count)| ScoutSourceUnreadDto { source_id, count })
        .collect())
}

#[derive(Clone, Debug)]
pub struct ScoutSourceUnreadDto {
    pub source_id: String,
    pub count: i64,
}

/// 按源聚合摘要（总览卡 + 发现页分组卡）。一次 GROUP BY 拿全：
/// 未读、各 kind 计数、总数、最近发现时间。
pub fn scout_source_summaries() -> Result<Vec<ScoutSourceSummaryDto>> {
    let store = open_store()?;
    Ok(store
        .scout_source_summaries()?
        .into_iter()
        .map(|s| ScoutSourceSummaryDto {
            source_id: s.source_id,
            unread: s.unread,
            new_count: s.new_count,
            changed_count: s.changed_count,
            gone_count: s.gone_count,
            total: s.total,
            latest_detected_at: s.latest_detected_at,
        })
        .collect())
}

/// 一个源的镜像聚合（FRB 边界 DTO）。
#[derive(Clone, Debug)]
pub struct ScoutSourceSummaryDto {
    pub source_id: String,
    pub unread: i64,
    pub new_count: i64,
    pub changed_count: i64,
    pub gone_count: i64,
    pub total: i64,
    pub latest_detected_at: Option<String>,
}

/// 标记已读（ids 空 = 全部已读）。
pub fn mark_scout_findings_read(ids: Vec<i64>) -> Result<i64> {
    let store = open_store()?;
    store.mark_findings_read(&ids).map(|n| n as i64)
}

/// 数据源页：服务端源状态实时拉取（只读，D7——编辑在服务端/管理页）。
pub fn list_scout_sources() -> Result<Vec<ScoutSourceDto>> {
    let store = open_store()?;
    let Some(settings) = store.scout_settings()? else {
        anyhow::bail!("尚未配置 scout 服务地址");
    };
    let token = resolve_token(&store, &settings)?;
    let client = crate::scout::ScoutClient::new(&settings.base_url)?;
    Ok(client
        .fetch_sources(&token)?
        .into_iter()
        .map(|s| ScoutSourceDto {
            id: s.id,
            name: s.name,
            interval_minutes: s.interval_minutes,
            enabled: s.enabled != 0,
            last_status: s.last_status,
            last_run_at: s.last_run_at,
        })
        .collect())
}

/// finding 详情（含服务端算好的历史轨迹）：原样回传 JSON 字符串，
/// Dart 侧解析——服务端详情形状随源 payload 变化，镜像层不建模。
pub fn scout_finding_detail(id: i64) -> Result<String> {
    let store = open_store()?;
    let Some(settings) = store.scout_settings()? else {
        anyhow::bail!("尚未配置 scout 服务地址");
    };
    let token = resolve_token(&store, &settings)?;
    let client = crate::scout::ScoutClient::new(&settings.base_url)?;
    let value = client.fetch_finding_detail(&token, id)?;
    Ok(serde_json::to_string(&value)?)
}

/// 清空镜像（设置页「重新同步」）：保留设置，下次拉取全量重建。
pub fn clear_scout_mirror() -> Result<()> {
    let store = open_store()?;
    store.clear_scout_mirror()
}

/// Product discovery cache. Network failure returns the last successful local view with stale=true.
pub fn scout_products(
    query: String,
    source: Option<String>,
    cursor: Option<String>,
    as_of: Option<String>,
    view: Option<String>,
) -> Result<String> {
    let store = open_store()?;
    let settings = store
        .scout_settings()?
        .ok_or_else(|| anyhow::anyhow!("scout_not_configured"))?;
    let token = resolve_token(&store, &settings)?;
    let client = crate::scout::ScoutClient::new(&settings.base_url)?;
    let path = format!(
        "/v1/products?q={}&source={}&before={}&as_of={}&view={}",
        crate::web_search::urlencoding_simple(&query),
        crate::web_search::urlencoding_simple(source.as_deref().unwrap_or("")),
        crate::web_search::urlencoding_simple(cursor.as_deref().unwrap_or("")),
        crate::web_search::urlencoding_simple(as_of.as_deref().unwrap_or("")),
        crate::web_search::urlencoding_simple(view.as_deref().unwrap_or("signals"))
    );
    scout_cached_get(&store, &client, &token, &path)
}
fn scout_cached_get(
    store: &crate::storage::Store,
    client: &crate::scout::ScoutClient,
    token: &str,
    path: &str,
) -> Result<String> {
    match client.research_get(token, path) {
        Ok(value) => {
            if let Some(instance) = value["instance_id"].as_str() {
                store.cache_scout_products(path, instance, &value.to_string())?;
            } else if let Some((instance, _, _)) = store.scout_cursor()? {
                store.cache_scout_products(path, &instance, &value.to_string())?;
            }
            Ok(value.to_string())
        }
        Err(error) => {
            let cache: serde_json::Value = serde_json::from_str(&store.cached_scout_products()?)?;
            if let Some(cached) = cache.get(path) {
                let mut value = cached.clone();
                value["stale"] = serde_json::json!(true);
                Ok(value.to_string())
            } else {
                Err(error)
            }
        }
    }
}
/// Product observations are bounded and cursor-paged; successful pages remain readable offline.
pub fn scout_product_detail(product_id: String, before: Option<i64>) -> Result<String> {
    let store = open_store()?;
    let settings = store
        .scout_settings()?
        .ok_or_else(|| anyhow::anyhow!("scout_not_configured"))?;
    let token = resolve_token(&store, &settings)?;
    let client = crate::scout::ScoutClient::new(&settings.base_url)?;
    scout_cached_get(
        &store,
        &client,
        &token,
        &format!(
            "/v1/products/detail?id={}&before={}",
            crate::web_search::urlencoding_simple(&product_id),
            before.unwrap_or(i64::MAX)
        ),
    )
}

/// Synchronized human research documents, independent of the disposable Scout mirror.
pub fn list_scout_research() -> Result<String> {
    Ok(open_store()?.scout_research_list()?.to_string())
}
/// Explicit edit with a content token; stale edits cannot overwrite a newer decision.
pub fn save_scout_research(
    document_json: String,
    expected_token: Option<String>,
) -> Result<String> {
    Ok(open_store()?
        .save_scout_research(
            serde_json::from_str(&document_json)?,
            expected_token.as_deref(),
        )?
        .to_string())
}

/// Processes at most one explicitly enabled Scout study; no knowledge digest trigger.
pub fn tick_scout_research() -> Result<bool> {
    crate::scout::analysis::tick(&open_store()?)
}
/// Candidate analysis stays separate from human decisions and is marked stale on evidence change.
pub fn scout_research_analysis(study_slug: String) -> Result<String> {
    Ok(crate::scout::analysis::result(&open_store()?, &study_slug)?.to_string())
}

/// Explicit personal criteria, held in synchronized human-authored knowledge.
pub fn scout_preferences() -> Result<String> {
    Ok(open_store()?.scout_preferences()?.to_string())
}
/// Empty criteria withdraw preferences; previous research retains its recorded basis.
pub fn save_scout_preferences(
    document_json: String,
    expected_token: Option<String>,
) -> Result<String> {
    Ok(open_store()?
        .save_scout_preferences(
            serde_json::from_str(&document_json)?,
            expected_token.as_deref(),
        )?
        .to_string())
}

/// Local incomplete edits, excluded from synchronization and background AI.
pub fn scout_research_drafts() -> Result<String> {
    Ok(open_store()?.scout_research_drafts()?.to_string())
}
pub fn scout_research_draft(instance_id: String, product_id: String) -> Result<String> {
    Ok(open_store()?
        .scout_research_draft(&instance_id, &product_id)?
        .to_string())
}
pub fn save_scout_research_draft(document_json: String) -> Result<()> {
    open_store()?.save_scout_research_draft(serde_json::from_str(&document_json)?)
}
pub fn delete_scout_research_draft(instance_id: String, product_id: String) -> Result<()> {
    open_store()?.delete_scout_research_draft(&instance_id, &product_id)
}
