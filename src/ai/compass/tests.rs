use super::*;
#[test]
fn strict_output_rejects_unknown_fields_and_truncation() {
    assert!(parse(r#"{"classification":"insufficient","confidence":0.1,"reason":"没有足够记录","question":"","citations":[],"recommendations":[],"execute":true}"#).is_err());
    assert!(parse("{\"classification\":").is_err());
    assert!(parse("```json\n{\"classification\":\"insufficient\",\"confidence\":0.1,\"reason\":\"不足\",\"question\":\"\",\"citations\":[],\"recommendations\":[]}\n```").is_ok());
}

struct Fixture {
    store: Store,
    directory: std::path::PathBuf,
}
impl Fixture {
    fn new(records: &[&str]) -> Self {
        use crate::{event::NewEvent, storage::GoalPhase};
        let directory =
            std::env::temp_dir().join(format!("compass-model-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let store = Store::open(&directory.join("synthetic.db")).unwrap();
        let goal = store
            .create_goal_with_facets(
                "验证手工记账服务是否有人愿意付费",
                GoalPhase::Near,
                Some("先找到真实需求"),
                Some("本周与三位潜在客户访谈并记录他们的痛点"),
                None,
                Some("三份访谈记录"),
            )
            .unwrap();
        store
            .connection
            .execute(
                "UPDATE goals SET updated_at=?2 WHERE id=?1",
                rusqlite::params![
                    goal.id,
                    (chrono::Utc::now() - chrono::Duration::days(6)).to_rfc3339()
                ],
            )
            .unwrap();
        for (i, text) in records.iter().enumerate() {
            let at = chrono::Utc::now() - chrono::Duration::days((records.len() - i) as i64);
            store
                .insert_event(NewEvent {
                    raw_text: text,
                    occurred_at: at,
                    recorded_at: at,
                    source: "capture",
                })
                .unwrap();
        }
        Self { store, directory }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

struct Scripted {
    calls: std::cell::Cell<usize>,
    budget: bool,
    malformed: bool,
}
impl AiProvider for Scripted {
    fn fits_request(&self, _: &[ContextMessage]) -> Result<bool> {
        Ok(self.budget)
    }
    fn visit_request_budgets(
        &self,
        v: &mut dyn FnMut(&super::super::provider::RequestFit<'_>) -> Result<()>,
    ) -> Result<()> {
        v(&|m| self.fits_request(m))
    }
    fn generate_reply_with_tools(
        &self,
        _: Vec<ContextMessage>,
        _: Option<&[crate::ai::tool::ToolSpec]>,
    ) -> Result<AiReply> {
        self.calls.set(self.calls.get() + 1);
        let mut reply = AiReply::text(if self.malformed {
            "{broken"
        } else {
            r#"{"classification":"insufficient","confidence":0.1,"reason":"证据不足","question":"","citations":[],"recommendations":[]}"#
        });
        reply.usage = Some(super::super::provider::TokenUsage {
            prompt_tokens: 101,
            completion_tokens: 22,
            total_tokens: 123,
            prompt_cached_tokens: None,
        });
        Ok(reply)
    }
}
#[test]
fn paid_boundary_checks_budget_dedupes_success_and_backs_off_bad_json() {
    let f = Fixture::new(&["今天处理报销", "今天整理文档"]);
    let model = Scripted {
        calls: std::cell::Cell::new(0),
        budget: false,
        malformed: false,
    };
    assert!(tick_with_provider(&f.store, &model).is_err());
    assert_eq!(model.calls.get(), 0);
    assert!(tick_with_provider(&f.store, &model).unwrap().is_none());
    assert_eq!(model.calls.get(), 0);
    let f = Fixture::new(&["今天处理报销", "今天整理文档"]);
    let model = Scripted {
        calls: std::cell::Cell::new(0),
        budget: true,
        malformed: false,
    };
    assert!(tick_with_provider(&f.store, &model).unwrap().is_none());
    assert!(tick_with_provider(&f.store, &model).unwrap().is_none());
    assert_eq!(model.calls.get(), 1);
    let f = Fixture::new(&["今天处理报销", "今天整理文档"]);
    let model = Scripted {
        calls: std::cell::Cell::new(0),
        budget: true,
        malformed: true,
    };
    assert!(tick_with_provider(&f.store, &model).is_err());
    assert!(tick_with_provider(&f.store, &model).unwrap().is_none());
    assert_eq!(model.calls.get(), 1);
    let (status, usage): (String, String) = f
        .store
        .connection
        .query_row(
            "SELECT status,usage_json FROM goal_compass_assessments",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(status, "failed");
    assert!(usage.contains("123"));
    assert_eq!(
        f.store
            .connection
            .query_row("SELECT SUM(total_tokens) FROM token_usage", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        123
    );
}

#[path = "http_tests.rs"]
mod http_tests;
#[path = "live_tests.rs"]
mod live_tests;

struct ConcurrentCapture {
    database: std::path::PathBuf,
}
impl AiProvider for ConcurrentCapture {
    fn fits_request(&self, _: &[ContextMessage]) -> Result<bool> {
        Ok(true)
    }
    fn visit_request_budgets(
        &self,
        v: &mut dyn FnMut(&super::super::provider::RequestFit<'_>) -> Result<()>,
    ) -> Result<()> {
        v(&|m| self.fits_request(m))
    }
    fn generate_reply_with_tools(
        &self,
        _: Vec<ContextMessage>,
        _: Option<&[crate::ai::tool::ToolSpec]>,
    ) -> Result<AiReply> {
        // A second connection must be able to capture while the provider runs.
        let store = Store::open(&self.database)?;
        store.insert_event(crate::event::NewEvent {
            raw_text: "访谈已经完成，新增反证",
            occurred_at: chrono::Utc::now(),
            recorded_at: chrono::Utc::now(),
            source: "capture",
        })?;
        Ok(AiReply::text(
            r#"{"classification":"insufficient","confidence":0.1,"reason":"证据不足","question":"","citations":[],"recommendations":[]}"#,
        ))
    }
}
#[test]
fn network_has_no_write_lock_and_new_counterevidence_fences_response() {
    let f = Fixture::new(&["今天调整配色", "今天做了图标"]);
    assert!(
        tick_with_provider(
            &f.store,
            &ConcurrentCapture {
                database: f.directory.join("synthetic.db")
            }
        )
        .is_err()
    );
    assert!(f.store.get_active_compass_nudge().unwrap().is_none());
    assert_eq!(
        f.store
            .connection
            .query_row("SELECT count(*) FROM events", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        3
    );
}
