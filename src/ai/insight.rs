//! Cognitive insight schema and parser. The only runtime generation path is
//! `knowledge_background`, which validates exact sources and commits atomically.
use anyhow::Result;
use serde::{Deserialize, Serialize};

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

/// 从 provider 回复解析洞察 JSON（容忍代码块与前后杂文本）。
pub(crate) fn parse_insights(reply: &str) -> Result<Vec<Insight>> {
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

#[cfg(test)]
mod tests {
    use super::*;

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
