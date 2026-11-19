//! Scout 情报的精选卡片（提案 2026-11-15 §4.2 / D10）。
//!
//! 每日最多一张、每张最多三个有证据的产品信号；无可信新信号保持安静。
//! 只使用服务端完整窗口/价格证据，不从未读数量推断重要性。
//! 22:00–07:00 静默；卡片为本机消息，旧证据跨日去重。

use super::Store;
use anyhow::Result;
use chrono::{DateTime, Local, Timelike};
use rusqlite::OptionalExtension;

/// 夜间静默：本地时间 22:00–07:00 不出新卡（与 digest_insights 相同窗口）。
const QUIET_START_HOUR: u32 = 22;
const QUIET_END_HOUR: u32 = 7;

fn local_date(now: DateTime<Local>) -> String {
    now.format("%Y-%m-%d").to_string()
}

impl Store {
    /// 主对话流会话（与 digest_insights 同一求值：无 wiki 绑定的非助理会话，
    /// 「主对话流」优先）。
    fn scout_main_conversation(&self) -> Result<Option<String>> {
        self.connection
            .query_row(
                "SELECT id FROM conversations
                 WHERE archived=0 AND wiki_page_slug IS NULL
                   AND assistant_mode NOT IN ('knowledge_mentor','goal_mentor')
                 ORDER BY CASE WHEN title='主对话流' THEN 0 ELSE 1 END, created_at, id
                 LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    /// At most three new, substantive product signals from a fresh service response.
    /// Never infer demand from finding volume or first monitoring appearances.
    fn scout_signal_brief(&self, now: DateTime<Local>) -> Result<Option<String>> {
        use serde_json::{Value, json};
        let cache: Value = serde_json::from_str(&self.cached_scout_products()?)?;
        let page = &cache["/v1/products?view=signals"];
        let Some(as_of) = page["as_of"]
            .as_str()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        else {
            return Ok(None);
        };
        let age = now.signed_duration_since(as_of);
        if page["stale"] == true || age.num_seconds() < -300 || age.num_hours() >= 24 {
            return Ok(None);
        }
        let instance = page["instance_id"].as_str().unwrap_or("");
        if instance.is_empty() {
            return Ok(None);
        }
        let mut seen = std::collections::HashSet::new();
        let mut query = self.connection.prepare("SELECT content FROM messages WHERE kind='scout_brief' AND content LIKE 'scout-brief-v2%' ORDER BY created_at DESC LIMIT 60")?;
        for body in query.query_map([], |r| r.get::<_, String>(0))? {
            if let Ok(value) =
                serde_json::from_str::<Value>(body?.trim_start_matches("scout-brief-v2\n"))
            {
                for item in value["items"].as_array().into_iter().flatten() {
                    if let Some(key) = item["key"].as_str() {
                        seen.insert(key.to_owned());
                    }
                }
            }
        }
        let studies = self.scout_research_list()?;
        let mut selected = Vec::new();
        let mut products = std::collections::HashSet::new();
        for product in page["items"].as_array().into_iter().flatten().take(40) {
            let id = product["product_id"].as_str().unwrap_or("");
            if id.is_empty()
                || product["title"]
                    .as_str()
                    .is_none_or(|s| s.trim().is_empty())
                || products.contains(id)
            {
                continue;
            }
            if studies.as_array().into_iter().flatten().any(|study| {
                study["instance_id"] == instance
                    && (study["product_id"] == id
                        || product["aliases"]
                            .as_array()
                            .is_some_and(|aliases| aliases.contains(&study["product_id"])))
                    && study["state"] == "rejected"
                    && [
                        "stargazers_count",
                        "forks_count",
                        "rating_count",
                        "ratings_count",
                        "rating",
                        "ratings_average",
                        "rank",
                        "price_cents",
                        "currency_code",
                    ]
                    .iter()
                    .all(|key| study["evidence"]["payload"][key] == product["payload"][key])
            }) {
                continue;
            }
            for reason in product["reasons"].as_array().into_iter().flatten() {
                if !matches!(
                    reason["kind"].as_str(),
                    Some("price_change" | "sustained_growth" | "rank_rise")
                ) || reason["complete"] != true
                {
                    continue;
                }
                let Some(until) = reason["valid_until"]
                    .as_str()
                    .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                else {
                    continue;
                };
                if until < now {
                    continue;
                }
                if reason["snapshot_ids"]
                    .as_array()
                    .is_none_or(|ids| ids.is_empty())
                    || reason["from"].as_f64().is_none()
                    || reason["to"].as_f64().is_none()
                {
                    continue;
                }
                if reason["kind"] == "price_change"
                    && product["payload"]["currency_code"]
                        .as_str()
                        .is_none_or(|s| s.is_empty())
                {
                    continue;
                }
                let key = json!([
                    instance,
                    id,
                    reason["kind"],
                    reason["source_id"],
                    reason["metric"],
                    reason["from"],
                    reason["to"],
                    product["payload"]["currency_code"]
                ])
                .to_string();
                if seen.contains(&key) {
                    continue;
                }
                selected.push(json!({"key":key,"product_id":id,"title":product["title"],"platform":product["platform"],"currency":product["payload"]["currency_code"],"reason":reason}));
                products.insert(id);
                break;
            }
            if selected.len() == 3 {
                break;
            }
        }
        if selected.is_empty() {
            return Ok(None);
        }
        Ok(Some(format!(
            "scout-brief-v2\n{}",
            json!({"version":2,"as_of":page["as_of"],"items":selected})
        )))
    }

    /// 今天栏卡片 tick。返回新建卡片的消息 id（无新卡返回空）。
    ///
    /// 频控：每日至多一张（ref 里有日期，天然去重）；夜间静默不出。
    /// 同日引用已存在时不再新增消息。
    pub fn tick_scout_brief_card(&self) -> Result<Vec<String>> {
        self.tick_scout_brief_card_at(Local::now())
    }

    pub(crate) fn tick_scout_brief_card_at(&self, now: DateTime<Local>) -> Result<Vec<String>> {
        // 夜间静默。
        let hour = now.hour();
        if hour < QUIET_END_HOUR || hour >= QUIET_START_HOUR {
            return Ok(Vec::new());
        }
        let Some(brief) = self.scout_signal_brief(now)? else {
            return Ok(Vec::new());
        };
        let Some(conversation_id) = self.scout_main_conversation()? else {
            return Ok(Vec::new());
        };
        let transaction = self.begin_write_transaction()?;
        let date = local_date(now);
        let reference = format!("scout_brief:{date}");
        // 今日已有卡片（无论内容）就不再出——每日至多一张。
        let existing: Option<(String,String)> = self
            .connection
            .query_row(
                "SELECT id,content FROM messages WHERE kind='scout_brief' AND ref=?1 AND content LIKE 'scout-brief-v2%' LIMIT 1",
                [&reference],
                |row| Ok((row.get(0)?,row.get(1)?)),
            )
            .optional()?;
        if let Some((id, content)) = existing {
            if content.starts_with("scout-brief-v2\n") {
                return Ok(vec![id]);
            }
            // A legacy volume-only card must not block the new evidence card.
        }
        let message_id = self.insert_message_with_kind(
            &conversation_id,
            "assistant",
            &brief,
            None,
            None,
            "scout_brief",
            Some(&reference),
        )?;
        if let Some(transaction) = transaction {
            transaction.commit()?;
        }
        Ok(vec![message_id])
    }
}
