//! 自 wiki.rs 机械分段平移（行为零变化）。

use crate::ai::memory::ContextMessage;
use crate::ai::provider::{
    AiProvider, FailoverEntry, FailoverProvider, OpenAiCompatibleConfig, OpenAiCompatibleProvider,
    last_good_background_provider,
};

use crate::storage::{ContentPolicy, EventRecord, Store, WikiPage, WikiPageDraft};
use anyhow::{Context, Result};

use crate::wiki::digest::parse_proposals;
// ── 页级再汇总（提案 C 的 P1 手动入口，2026-10-20）──────────────────

const RECONSOLIDATION_SYSTEM_PROMPT: &str = r#"你是 elsewhen 个人知识库的 wiki 页面维护者。任务：把一页的全部来源事件从头重看一遍，对页面正文做【增量修补】。

铁律：
- 保留旧正文的全部既有事实，只补充/修正/细化，不丢失任何旧条目；
- 与事件冲突的旧说法，保守修正并在该要点后标注（待确认）；
- 不编造：只写事件里有依据的内容，不确定就标（待确认）或干脆不写；
- 不改 slug / kind / title，只修正文、摘要与标签；
- 输出只含一个对象的 JSON 数组，不要任何其他文字、不要代码块围栏：
  [{"op":"update","kind":"<页面 kind，原样返回>","slug":"<给定 slug>","title":"<页面标题，原样返回>","summary":"一行摘要","content":"markdown 全文，用 - 要点","tags":["..."]}]
"#;

/// 手动「重新整理此页」（提案 P1，2026-10-20）。单页再汇总：旧正文 + 全部
/// 来源事件原文 → 模型增量修补 → PreserveHumanEdits 落库（写 wiki_revisions，
/// 页面观测的「最近整理」随之推进）。人工编辑过的页不动正文、不生成建议——
/// 评审立场：人工页永远走审阅提案（propose 通道），这里只说明原因。
/// 走知识后台渠道（调用方用 digest_provider），账本 task='page-reconsolidation'。
/// P2 的证据增量/时间触发后台化后复用本函数，签名不变。
pub fn reconsolidate_page(store: &Store, provider: &dyn AiProvider, slug: &str) -> Result<String> {
    const LEDGER_TASK: &str = "page-reconsolidation";
    let started_at = chrono::Utc::now().to_rfc3339();
    let ledger = |status: &str, error: Option<&str>| -> Result<()> {
        store.connection.execute(
            "INSERT INTO knowledge_background_runs(id,task,input_key,status,started_at,finished_at,error,result_count)
             VALUES (?1,?2,?3,?4,?5,?5,?6,?7)",
            rusqlite::params![
                uuid::Uuid::new_v4().to_string(),
                LEDGER_TASK,
                slug,
                status,
                started_at,
                error,
                if status == "succeeded" { 1 } else { 0 },
            ],
        )?;
        Ok(())
    };
    let page = store
        .get_wiki_page(slug)?
        .with_context(|| format!("页面不存在：{slug}"))?;
    if page.status == "archived" {
        anyhow::bail!("页面已归档：{slug}");
    }
    if page.human_edited_at.is_some() {
        // 账本 status 枚举只有 running/succeeded/failed（复用表），跳过按
        // succeeded+result_count=0 登记，说明走返回文案（UI 原地展示）。
        ledger("succeeded", None)?;
        return Ok(
            "页面有人工编辑保护，正文不动；如需机器改写请走「提炼/建议」审阅通道".to_string(),
        );
    }
    let events = store.page_source_events(slug)?;
    let user = build_reconsolidation_user_prompt(&page, &events);
    let result = (|| -> Result<String> {
        let reply = provider
            .generate_reply(vec![
                ContextMessage::new("system", RECONSOLIDATION_SYSTEM_PROMPT.to_string()),
                ContextMessage::new("user", user),
            ])
            .context("调用模型失败")?
            .content;
        let proposals = parse_proposals(&reply)?;
        anyhow::ensure!(
            proposals.len() == 1,
            "重整理应只返回一个页面提议，实际 {} 个",
            proposals.len()
        );
        let p = &proposals[0];
        anyhow::ensure!(
            p.slug == slug,
            "重整理提议改了目标 slug：{} != {slug}",
            p.slug
        );
        anyhow::ensure!(!p.content.trim().is_empty(), "重整理提议正文为空");
        let draft = WikiPageDraft {
            slug: slug.to_string(),
            kind: page.kind.clone(),
            title: page.title.clone(),
            summary: if p.summary.trim().is_empty() {
                page.summary.clone()
            } else {
                p.summary.clone()
            },
            content_md: p.content.clone(),
            tags: if p.tags.is_empty() {
                page.tags.clone()
            } else {
                p.tags.clone()
            },
            // 证据承接现状：重整理不改证据（增补走 digest 通道）。
            source_event_ids: page.source_event_ids.clone(),
            status: "active".to_string(),
            reason: "手动重整理：全量来源重看后增量修补".to_string(),
            source_url: page.source_url.clone(),
        };
        store.upsert_wiki_page(&draft, ContentPolicy::PreserveHumanEdits)?;
        Ok(format!(
            "已重整：正文按全部 {} 条来源事件增量修补",
            events.len()
        ))
    })();
    match &result {
        Ok(_) => ledger("succeeded", None)?,
        Err(error) => ledger("failed", Some(&format!("{error:#}")))?,
    }
    result
}

fn build_reconsolidation_user_prompt(page: &WikiPage, events: &[EventRecord]) -> String {
    let mut out = format!(
        "这是要重整的页面：\nslug：{}\n标题：{}\n标签：{}\n当前正文：\n{}\n\n它的全部来源事件（[编号] 时间 | 内容，按时间升序）：\n",
        page.slug,
        page.title,
        page.tags.join("、"),
        page.content_md.chars().take(4000).collect::<String>(),
    );
    for (i, e) in events.iter().enumerate() {
        let text: String = e.raw_text.chars().take(1500).collect();
        out.push_str(&format!("[{}] {} | {}\n", i + 1, e.recorded_at, text));
    }
    out.push_str(&format!(
        "\n（共 {} 条来源事件。正文已提到的旧事实必须保留。）",
        events.len()
    ));
    out
}

/// 知识消化用的 provider：沿用当前启用配置，温度与输出上限固定为 digest 调参。
/// 没有可用配置时返回 None（事件照常保存，任务等待）。
///
/// 返回的是顺序 failover 链：后台任务与主对话分流，优先非活跃渠道；某个
/// 渠道 5xx/429/超时/鉴权失败时自动换下一个，全部失败才向上报错。上次成功
/// 的渠道会排到最前，避免每轮都先撞已知失败的渠道。
pub fn digest_provider(store: &Store) -> Result<Option<(Box<dyn AiProvider>, String)>> {
    let configs = store.list_ai_provider_configs_for_runtime()?;
    let mut usable: Vec<_> = configs
        .into_iter()
        .filter(|c| c.provider_type == "openai-compatible")
        .filter(|c| !c.api_key.trim().is_empty())
        .collect();
    if usable.is_empty() {
        return Ok(None);
    }

    // 排序：上次成功者 → 其余非活跃渠道 → 活跃渠道兜底。
    let mut ordered = Vec::with_capacity(usable.len());
    if let Some(id) = last_good_background_provider()
        && let Some(pos) = usable.iter().position(|c| c.id == id)
    {
        ordered.push(usable.remove(pos));
    }
    let (mut non_active, mut active): (Vec<_>, Vec<_>) =
        usable.into_iter().partition(|c| !c.is_active);
    ordered.append(&mut non_active);
    ordered.append(&mut active);

    let model = ordered[0].model.clone();
    let entries = ordered
        .into_iter()
        .map(|config| {
            let provider = OpenAiCompatibleProvider::new(OpenAiCompatibleConfig {
                base_url: config.base_url,
                api_key: config.api_key,
                model: config.model,
                temperature: 0.3,
                // 知识整理允许 5000 字正文；固定 3000 会在 reasoning 或
                // 长 JSON 上把输出截断。尊重用户配置，未配置时给 8192。
                max_tokens: Some(
                    config.max_tokens.filter(|value| *value > 0).unwrap_or(8192) as u32
                ),
                context_window: config.context_window.map(|v| v as u32),
            })?;
            Ok(FailoverEntry::new(
                config.id,
                config.name,
                Box::new(provider),
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Some((Box::new(FailoverProvider::new(entries)), model)))
}
