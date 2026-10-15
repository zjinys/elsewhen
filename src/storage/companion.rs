//! 低打扰情感性回访（决策辅助 D6）。
//!
//! 红线：触发面仅限「打开应用时」的一次读面检查；不注册系统级推送、
//! 不写通知通道、不做后台唤醒。关闭开关后 check 直接返回、零查询——
//! 「关闭后零侵入」的结构保证。素材全部来自既有事件与分析（被记住的事），
//! 呈现为一条温和的回引，用户可以完全无视。

use anyhow::Result;
use rusqlite::{OptionalExtension, params};

use super::Store;

/// 回访偏好（singleton）。「频率上限可配」= interval_days 下限。
#[derive(Debug, Clone)]
pub struct CompanionSettings {
    pub revisit_enabled: bool,
    pub revisit_interval_days: i64,
    pub last_revisit_at: Option<String>,
}

/// 一条回访素材：被记住的事件 +（若有）当时的情绪感受。
#[derive(Debug, Clone)]
pub struct CompanionRevisitItem {
    pub event_id: String,
    pub recorded_at: String,
    /// 原文摘录（已截短），呈现为「共同记忆回引」。
    pub excerpt: String,
    pub feeling: Option<String>,
}

const EXCERPT_CHARS: usize = 220;

fn excerpt_of(raw: &str) -> String {
    let raw_len = raw.chars().count();
    let cut: String = raw.chars().take(EXCERPT_CHARS).collect();
    if raw_len > EXCERPT_CHARS {
        format!("{cut}…")
    } else {
        cut
    }
}

fn item_from_parts(
    event_id: String,
    recorded_at: String,
    raw_text: String,
    feeling: Option<String>,
) -> CompanionRevisitItem {
    let excerpt = excerpt_of(&raw_text);
    CompanionRevisitItem {
        event_id,
        recorded_at,
        excerpt,
        feeling: feeling.filter(|f| !f.trim().is_empty()),
    }
}

impl Store {
    pub fn companion_settings(&self) -> Result<CompanionSettings> {
        let row = self
            .connection
            .query_row(
                "SELECT revisit_enabled, revisit_interval_days, last_revisit_at
                 FROM companion_settings WHERE singleton=1",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .optional()?;
        let (enabled, interval, last) = row.unwrap_or((1, 7, None));
        Ok(CompanionSettings {
            revisit_enabled: enabled == 1,
            revisit_interval_days: interval,
            last_revisit_at: last,
        })
    }

    /// 更新偏好（间隔即频率下限；展示后 interval 天内不再打扰）。
    pub fn set_companion_settings(&self, enabled: bool, interval_days: i64) -> Result<()> {
        anyhow::ensure!(interval_days >= 1, "回访间隔至少 1 天");
        self.connection.execute(
            "UPDATE companion_settings SET revisit_enabled=?1, revisit_interval_days=?2,
                updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE singleton=1",
            params![enabled as i64, interval_days],
        )?;
        Ok(())
    }

    /// 回访检查（打开应用时调用一次）：
    /// enabled=0 → Ok(None)，不查任何业务表（零侵入的结构保证）；
    /// 距上次回访不足 interval_days → Ok(None)；
    /// 找到值得回引的记忆 → Ok(Some(item)) 并记账本次回访时间；
    /// 素材不足 → Ok(None) 且不记账（等下次）。
    ///
    /// 素材选择（确定性，不做权重推断）：间隔期之外的记录里，优先带
    /// 情绪标注的最新一条（情感性回访），否则取最新的一般事件。
    pub fn check_companion_revisit(&self) -> Result<Option<CompanionRevisitItem>> {
        let settings = self.companion_settings()?;
        if !settings.revisit_enabled {
            return Ok(None);
        }
        if !self.revisit_due(&settings)? {
            return Ok(None);
        }
        let Some(item) = self.revisit_candidate()? else {
            return Ok(None);
        };
        self.touch_last_revisit()?;
        Ok(Some(item))
    }

    /// 偏好开关外的间隔判定：解析失败视作到期（宁可多用一次回访，不可漏）。
    fn revisit_due(&self, settings: &CompanionSettings) -> Result<bool> {
        let Some(last) = settings.last_revisit_at.as_deref() else {
            return Ok(true);
        };
        let Ok(last) = chrono::DateTime::parse_from_rfc3339(last) else {
            return Ok(true);
        };
        let now = self.utc_now()?;
        let Some(last) = last
            .with_timezone(&chrono::Utc)
            .checked_sub_signed(chrono::Duration::zero())
        else {
            return Ok(true);
        };
        Ok(now.signed_duration_since(last)
            >= chrono::Duration::days(settings.revisit_interval_days))
    }

    fn utc_now(&self) -> Result<chrono::DateTime<chrono::Utc>> {
        let now: String = self.connection.query_row(
            "SELECT strftime('%Y-%m-%dT%H:%M:%fZ','now')",
            [],
            |row| row.get(0),
        )?;
        Ok(chrono::DateTime::parse_from_rfc3339(&now)?.with_timezone(&chrono::Utc))
    }

    /// 候选记忆：interval_days 之前（间隔期外）的记录。
    /// 首选带情绪标注的最新一条；json_each 遇坏 JSON 行降级 Rust 逐行解析。
    /// 没有情绪素材 → 取间隔期外最新的一般事件。
    fn revisit_candidate(&self) -> Result<Option<CompanionRevisitItem>> {
        let settings = self.companion_settings()?;
        let cutoff = self.utc_now()? - chrono::Duration::days(settings.revisit_interval_days);
        let cutoff_text = cutoff.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

        let statement = self.connection.prepare(
            "SELECT e.id, e.recorded_at, e.raw_text,
                    (SELECT je.value FROM json_each(a.result_json, '$.emotion') je LIMIT 1)
             FROM events e
             JOIN event_analyses a ON a.event_id = e.id AND a.result_json IS NOT NULL
             WHERE e.recorded_at < ?1
             ORDER BY e.recorded_at DESC, a.created_at DESC
             LIMIT 100",
        );
        if let Ok(mut statement) = statement {
            let mapped = statement.query_map(params![cutoff_text], |row| {
                // emotion 子查询返回完整 JSON 对象文本；取其 feeling 字段。
                let emotion_raw: Option<String> = row.get(3)?;
                let feeling = emotion_raw
                    .as_deref()
                    .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
                    .and_then(|value| {
                        value
                            .pointer("/feeling")
                            .and_then(|f| f.as_str().map(str::to_owned))
                    });
                Ok(item_from_parts(
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    feeling,
                ))
            });
            match mapped {
                Ok(rows) => {
                    let mut items: Vec<CompanionRevisitItem> = rows
                        .flatten()
                        .filter(|item| item.feeling.is_some())
                        .collect();
                    if let Some(item) = items.pop() {
                        return Ok(Some(item));
                    }
                }
                // 坏 JSON 行会让 json_each 报错——落进一般候选。
                Err(_) => {}
            }
        }

        // 一般候选：间隔期外最新一条事件（无分析/无情绪也可被记得）。
        let general = self
            .connection
            .query_row(
                "SELECT e.id, e.recorded_at, e.raw_text
                 FROM events e
                 WHERE e.recorded_at < ?1
                 ORDER BY e.recorded_at DESC LIMIT 1",
                params![cutoff_text],
                |row| Ok(item_from_parts(row.get(0)?, row.get(1)?, row.get(2)?, None)),
            )
            .optional()?;
        Ok(general)
    }

    fn touch_last_revisit(&self) -> Result<()> {
        self.connection.execute(
            "UPDATE companion_settings SET last_revisit_at=strftime('%Y-%m-%dT%H:%M:%fZ','now'),
                updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE singleton=1",
            [],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::NewEvent;
    use crate::storage::Store;

    fn temporary_database() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "elsewhen-companion-{}-{}.db",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let _ = std::fs::remove_file(&path);
        path
    }

    fn event_days_ago(store: &Store, text: &str, days: i64) -> String {
        let recorded = chrono::Utc::now() - chrono::Duration::days(days);
        store
            .insert_event(NewEvent {
                raw_text: text,
                occurred_at: recorded,
                recorded_at: recorded,
                source: "capture",
            })
            .unwrap()
    }

    fn emotion_analysis(store: &Store, event_id: &str) {
        let mut result = serde_json::json!({
            "schema_version": "event-analysis-v3",
            "clarifications": [], "people": [], "projects": [],
            "activities": [], "follow_ups": [],
            "emotion": [
                {"target": "对事件本身", "feeling": "有点累但踏实"}
            ]
        });
        // 解析层要求非空字段结构齐全：直接写库，绕过 parse（历史快照同构）。
        result["schema_version"] = serde_json::json!("event-analysis-v3");
        store
            .connection
            .execute(
                "INSERT INTO event_analyses (id, event_id, prompt_version, result_json, created_at)
                 VALUES (?1, ?2, 'event-analysis-v3', ?3, strftime('%Y-%m-%dT%H:%M:%fZ','now'))",
                rusqlite::params![format!("analysis-{event_id}"), event_id, result.to_string()],
            )
            .unwrap();
    }

    /// 回访素材不足 → None 且不记账；到期后 emotional 优先。
    #[test]
    fn companion_revisit_gates_and_prefers_emotion() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();

        // 冷库：没有任何素材 → None，且 last_revisit 保持空（不记账）。
        assert!(store.check_companion_revisit().unwrap().is_none());
        assert!(
            store
                .companion_settings()
                .unwrap()
                .last_revisit_at
                .is_none()
        );

        event_days_ago(&store, "一般记录", 30);
        // 只有一般事件（30 天前）也可被记得（一般候选）。
        let first = store.check_companion_revisit().unwrap().unwrap();
        assert_eq!(first.feeling, None);
        // 已记账：间隔 7 天内不再打扰。
        assert!(
            store
                .companion_settings()
                .unwrap()
                .last_revisit_at
                .is_some()
        );
        assert!(store.check_companion_revisit().unwrap().is_none());

        // 现在有带情绪的旧记忆：把 last_revisit 清掉模拟到期。
        // （events.recorded_at 有防篡改触发器，测试里不挪已有记录。）
        store
            .connection
            .execute("UPDATE companion_settings SET last_revisit_at=NULL", [])
            .unwrap();
        let emotional_id = event_days_ago(&store, "跑完半马", 14);
        emotion_analysis(&store, &emotional_id);
        // 一条 2 天前的情绪记录不应进入候选（间隔期外红线：只回看过去）。
        let recent_id = event_days_ago(&store, "刚做的小记录", 2);
        emotion_analysis(&store, &recent_id);
        let second = store.check_companion_revisit().unwrap().unwrap();
        assert_eq!(second.event_id, emotional_id, "情绪素材优先");
        assert_eq!(second.feeling.as_deref(), Some("有点累但踏实"));
        assert!(second.excerpt.contains("跑完半马"));
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    /// 关闭回访：即便素材与间隔都满足也零返回——「零侵入」开关语义。
    #[test]
    fn companion_disabled_returns_none_and_is_untouched() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        store.set_companion_settings(false, 7).unwrap();
        let id = event_days_ago(&store, "被记住的事", 10);
        let _ = id;
        assert!(store.check_companion_revisit().unwrap().is_none());
        assert!(
            store
                .companion_settings()
                .unwrap()
                .last_revisit_at
                .is_none()
        );
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    /// 间隔即频率下限可配：3 天内显示过则第 3 天仍拒绝。
    #[test]
    fn companion_interval_is_enforceable() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        store.set_companion_settings(true, 3).unwrap();
        event_days_ago(&store, "旧记录一条", 30);
        assert!(store.check_companion_revisit().unwrap().is_some());
        // 把 last_revisit 拨回 2 天前 → 间隔 3 天尚未满足。
        let last = (chrono::Utc::now() - chrono::Duration::days(2)).to_rfc3339();
        store
            .connection
            .execute(
                "UPDATE companion_settings SET last_revisit_at=?1",
                rusqlite::params![last],
            )
            .unwrap();
        assert!(store.check_companion_revisit().unwrap().is_none());
        drop(store);
        let _ = std::fs::remove_file(path);
    }
}
