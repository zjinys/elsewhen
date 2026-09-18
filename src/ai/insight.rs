//! 认知推微引擎（v2，LLM wiki 版）：
//! 导航"关于用户的个人知识库"（wiki 页面）+ 最近事件 → 四透镜生成反常识认知，
//! 并把洞察**归档回 wiki**（好答案写回知识库，探索也复利）。
//!
//! 方法论参考（视频《认知推微》）："副业不是加法，是乘法"——机会藏在
//! "反正都要做"的动作和"已经付出"的成本里，而不是另找一件新的事。

use super::provider::{AiProvider, OpenAiCompatibleConfig, OpenAiCompatibleProvider};
use crate::event::EventSummary;
use crate::storage::{Store, WikiPageDraft};
use crate::wiki::{self, INSIGHT_KINDS};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Prompt 版本号，insights / wiki 页会记录，便于以后升级后重新生成。
pub const INSIGHT_PROMPT_VERSION: &str = "insight-v2-wiki";

/// 生成选项
#[derive(Debug, Clone)]
pub struct InsightOptions {
    /// 回看天数窗口
    pub days: i64,
    /// 最多喂给模型的事件数
    pub max_events: usize,
}

impl Default for InsightOptions {
    fn default() -> Self {
        Self {
            days: 14,
            max_events: 60,
        }
    }
}

/// 一条结构化洞察（derived data）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Insight {
    pub lens: String,
    pub title: String,
    pub observation: String,
    /// 引用的 wiki 页面 slug（知识溯源）
    #[serde(default)]
    pub source_slugs: Vec<String>,
    #[serde(default)]
    pub related_events: Vec<String>,
    #[serde(default)]
    pub action: Option<String>,
}

const SYSTEM_PROMPT: &str = r#"你是"认知推微"引擎。输入有两部分：用户最近的事件记录（日记窗口）和关于用户的个人知识库（wiki 页面，已积累多日）。你的任务：从中挖掘反常识认知。

方法论（四个透镜，逐个扫描）：
1. 反复固定成本：找出用户"反正都要做、都要付出成本"的反复动作或固定支出（通勤、固定费用、例行事务）。已付出的成本是副业的杠杆点。
2. 闲置产能：找出用户已拥有但未被充分利用的"空位"——时间、设备、技能、空间、关系。
3. 加收钱动作：评估"在已有动作上叠加一个收钱动作"的可行路径。核心认知：副业不是加法是乘法——不是另找一件事做，而是在已经在做的事情上加一个收钱的动作。
4. 案例类比：有没有现实中被验证过的类似商业模式（如 BlaBlaCar 出售高速上本来就空着的座位）作为参照。

硬性要求：
- 只基于提供的 wiki 页面与事件说话，不许编造事实与数字；每个论断必须能用 source_slugs / related_events 溯源。
- 每条洞察必须具体到用户自己的事实，避免鸡汤、避免泛泛而谈。
- 结合"已输出过的洞察"递进：不重复已给过的认知。
- 宁缺毋滥：最多输出 3 条；没有真正值得说的，返回空数组 []。

输出：严格 JSON 数组，不要输出任何其他文字或代码块标记。
[{"lens":"1|2|3|4","title":"一句话标题，反常识、让人眼前一亮","observation":"详细认知拆解，引用具体事实","source_slugs":["<引用的wiki页slug>"],"related_events":["事件原文片段"],"action":"具体、可执行的第一步行动建议，可为空字符串"}]"#;

fn build_user_prompt(
    events: &[EventSummary],
    pages: &[crate::storage::WikiPage],
    past: &[crate::storage::InsightSummary],
) -> String {
    let mut out = String::from("这是用户最近的事件记录（时间 | 内容）：\n");
    for e in events {
        out.push_str(&format!("- {} | {}\n", e.recorded_at, e.raw_text));
    }

    out.push_str("\n个人知识库（wiki 页面，按证据强度选取，主要依据）：\n");
    if pages.is_empty() {
        out.push_str("（知识库暂无相关页面）\n");
    }
    for p in pages {
        let cost = p.content_md.len().min(3000);
        out.push_str(&format!(
            "\n--- {} ---\n{}",
            p.slug,
            p.content_md.chars().take(cost).collect::<String>()
        ));
    }

    out.push_str("\n已输出过的洞察（避免重复，或在其认知上递进）：\n");
    if past.is_empty() {
        out.push_str("（暂无）\n");
    }
    for ins in past.iter().take(20) {
        out.push_str(&format!("- 透镜{} | {}\n", ins.lens, ins.title));
    }
    out
}

/// 从 provider 回复解析洞察 JSON（容忍代码块与前后杂文本）。
fn parse_insights(reply: &str) -> Result<Vec<Insight>> {
    let trimmed = reply.trim();
    let inner = if trimmed.starts_with("```") {
        trimmed
            .lines()
            .filter(|l| !l.starts_with("```"))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string()
    } else {
        trimmed.to_string()
    };
    if let Ok(list) = serde_json::from_str::<Vec<Insight>>(&inner) {
        return Ok(list);
    }
    if let Some(start) = inner.find('[') {
        if let Some(end) = inner.rfind(']') {
            let slice = &inner[start..=end];
            if let Ok(list) = serde_json::from_str::<Vec<Insight>>(slice) {
                return Ok(list);
            }
        }
    }
    anyhow::bail!(
        "无法解析 AI 返回的洞察 JSON。返回内容前 200 字：{}",
        &reply.chars().take(200).collect::<String>()
    )
}

/// 把洞察归档回 wiki（kind=insight），并写操作日志。
fn file_back_insights(store: &Store, insights: &[Insight]) -> Result<()> {
    for ins in insights {
        let slug = format!("insight/{}", wiki::slugify(&ins.title));
        let mut content = format!("# {}\n\n{}", ins.title, ins.observation);
        if let Some(a) = &ins.action {
            if !a.is_empty() {
                content.push_str(&format!("\n\n## 行动建议\n\n{}", a));
            }
        }
        if !ins.source_slugs.is_empty() {
            let links: Vec<String> = ins
                .source_slugs
                .iter()
                .map(|s| format!("- [[{}]]", s))
                .collect();
            content.push_str(&format!("\n\n## 来源\n\n{}", links.join("\n")));
        }
        let draft = WikiPageDraft {
            slug,
            kind: "insight".to_string(),
            title: ins.title.clone(),
            summary: ins.title.clone(),
            content_md: content,
            tags: vec![format!("lens-{}", ins.lens)],
            source_event_ids: Vec::new(),
            status: "active".to_string(),
            reason: "认知推微归档：好答案写回知识库".to_string(),
            source_url: None,
        };
        store.upsert_wiki_page(&draft)?;
    }
    if !insights.is_empty() {
        let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let titles: Vec<String> = insights.iter().map(|i| i.title.clone()).collect();
        store.append_wiki_log(&format!(
            "## [{}] insight | archived: {}",
            date,
            titles.join("; ")
        ))?;
    }
    Ok(())
}

/// 生成认知推微洞察：导航 wiki + 最近事件 → provider → 入库 + 归档写回。
pub fn generate_insights(store: &Store, options: &InsightOptions) -> Result<Vec<Insight>> {
    let events = store.recent_events(options.days, options.max_events)?;
    if events.is_empty() {
        anyhow::bail!(
            "最近 {} 天内没有任何事件记录。先用 `elsewhen record \"...\"` 记录一些事件，或加宽 --days。",
            options.days
        );
    }

    // 导航知识库
    let all_pages = store.list_wiki_pages(None, None)?;
    let pages = wiki::select_context_pages(all_pages, INSIGHT_KINDS, 6000);
    let past = store.list_insights()?;

    let user = build_user_prompt(&events, &pages, &past);

    let ai_config = store
        .active_ai_provider_config()?
        .context("没有可用的 AI Provider 配置，请先 `elsewhen settings` 配置")?;
    let provider_config = OpenAiCompatibleConfig {
        base_url: ai_config.base_url,
        api_key: ai_config.api_key,
        model: ai_config.model,
        temperature: 0.8,
        max_tokens: Some(1500),
    };
    let provider = OpenAiCompatibleProvider::new(provider_config)?;
    let messages = vec![
        super::memory::ContextMessage::new("system", SYSTEM_PROMPT.to_string()),
        super::memory::ContextMessage::new("user", user),
    ];
    let reply = provider
        .generate_reply(messages)
        .context("调用 AI provider 失败")?;
    let reply_text = reply.content;
    if std::env::var("ELSEWHEN_DEBUG").is_ok() {
        eprintln!("[debug] insight raw reply:\n{}", reply_text);
    }
    let insights = parse_insights(&reply_text)?;

    for insight in &insights {
        store.insert_insight(
            options.days,
            INSIGHT_PROMPT_VERSION,
            &insight.lens,
            &insight.title,
            &insight.observation,
            &insight.related_events,
            insight.action.as_deref(),
        )?;
    }
    file_back_insights(store, &insights)?;
    Ok(insights)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<EventSummary> {
        vec![
            EventSummary {
                recorded_at: "2026-09-04T09:00:00Z".into(),
                raw_text: "每周从东莞往返惠州，来回过路费 60 元".into(),
            },
            EventSummary {
                recorded_at: "2026-09-05T09:00:00Z".into(),
                raw_text: "想顺便开顺风车平掉往返成本".into(),
            },
        ]
    }

    #[test]
    fn user_prompt_contains_events_and_wiki_hint() {
        let prompt = build_user_prompt(&sample(), &[], &[]);
        assert!(prompt.contains("惠州"));
        assert!(prompt.contains("顺风车"));
        assert!(prompt.contains("知识库暂无相关页面"));
    }

    #[test]
    fn parse_plain_json_array() {
        let reply = r#"[{"lens":"3","title":"顺路收钱","observation":"基于通勤事件的分析","related_events":["惠州"],"action":"注册顺风车平台"}]"#;
        let list = parse_insights(reply).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].lens, "3");
        assert_eq!(list[0].source_slugs, Vec::<String>::new());
        assert_eq!(list[0].action.as_deref(), Some("注册顺风车平台"));
    }

    #[test]
    fn parse_with_source_slugs_and_fence() {
        let reply = r#"```json
[{"lens":"1","title":"x","observation":"y","source_slugs":["recurring-cost/dg-huizhou"],"related_events":[],"action":""}]
```"#;
        let list = parse_insights(reply).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].source_slugs, vec!["recurring-cost/dg-huizhou"]);
    }

    #[test]
    fn parse_embedded_array_in_text() {
        let reply = "分析如下：\n[{\"lens\":\"2\",\"title\":\"空位\",\"observation\":\"z\",\"related_events\":[],\"action\":null}]\n完毕";
        let list = parse_insights(reply).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].title, "空位");
    }

    #[test]
    fn parse_garbage_errors() {
        assert!(parse_insights("完全不是 JSON").is_err());
    }
}
