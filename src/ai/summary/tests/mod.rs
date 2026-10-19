use super::*;
use crate::ai::{provider::ProviderEvent, tool::ToolSpec};
use std::cell::{Cell, RefCell};

// 公共测试辅助（原文件平移，pub(super) 供分组子文件复用）。
struct TempDatabase(std::path::PathBuf);
impl Drop for TempDatabase {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_file(format!("{}-wal", self.0.display()));
        let _ = std::fs::remove_file(format!("{}-shm", self.0.display()));
    }
}

pub(super) fn fixture() -> (TempDatabase, Store, String) {
    let path =
        std::env::temp_dir().join(format!("elsewhen-summary-ai-{}.db", uuid::Uuid::new_v4()));
    let store = Store::open(&path).unwrap();
    let conversation = store
        .create_conversation(Some("summary fixture"), None)
        .unwrap();
    (TempDatabase(path), store, conversation)
}

pub(super) fn add_history(store: &Store, conversation: &str) -> String {
    store
        .send_message(
            conversation,
            "user",
            &format!("不要改动原文。{}", "historical background ".repeat(2000)),
            None,
        )
        .unwrap();
    store
        .send_message(
            conversation,
            "assistant",
            "方案仍待审阅，不代表已经执行。",
            None,
        )
        .unwrap();
    store
        .send_message(conversation, "user", "保留最近完整交换", None)
        .unwrap();
    store
        .send_message(conversation, "assistant", "这是最近答复", None)
        .unwrap();
    store
        .send_message(conversation, "user", "当前问题必须原样保留", None)
        .unwrap()
}

pub(super) fn context(store: &Store, conversation: &str) -> Vec<ContextMessage> {
    let mut context = vec![ContextMessage::new("system", "required rules")];
    context.extend(
        store
            .list_messages(conversation)
            .unwrap()
            .into_iter()
            .map(ContextMessage::from_record),
    );
    context
}

pub(super) fn good_reply() -> AiReply {
    AiReply::text(
        json!({
            "goals":["整理任务"],"constraints":["不修改原文"],"facts":[],
            "decisions":[],"unfinished":["方案等待审阅，尚未执行"]
        })
        .to_string(),
    )
}

struct FakeProvider {
    requests: Cell<usize>,
    reply: AiReply,
    on_request: RefCell<Option<Box<dyn FnOnce()>>>,
}

impl FakeProvider {
    fn new(reply: AiReply) -> Self {
        Self {
            requests: Cell::new(0),
            reply,
            on_request: RefCell::new(None),
        }
    }
}

impl AiProvider for FakeProvider {
    fn generate_reply_with_tools(
        &self,
        messages: Vec<ContextMessage>,
        tools: Option<&[ToolSpec]>,
    ) -> Result<AiReply> {
        assert!(tools.is_none());
        assert_eq!(
            messages.len(),
            2,
            "summary data is one required input, not trimmable history"
        );
        assert_eq!(messages[1].role, "user");
        self.requests.set(self.requests.get() + 1);
        if let Some(action) = self.on_request.borrow_mut().take() {
            action();
        }
        Ok(self.reply.clone())
    }
    fn generate_observed(
        &self,
        messages: Vec<ContextMessage>,
        tools: Option<&[ToolSpec]>,
        observer: &mut dyn FnMut(ProviderEvent<'_>) -> Result<()>,
    ) -> Result<AiReply> {
        observer(ProviderEvent::Poll)?;
        observer(ProviderEvent::TextDelta("摘要内部正文，不应进入用户回复"))?;
        let result = self.generate_reply_with_tools(messages, tools)?;
        observer(ProviderEvent::Poll)?;
        Ok(result)
    }
}

mod page_marks {
    use super::*;

    fn first_message_id(store: &Store, conversation: &str) -> String {
        store
            .list_messages(conversation)
            .unwrap()
            .first()
            .unwrap()
            .id
            .clone()
    }

    fn boundary_reply() -> AiReply {
        AiReply::text(
            json!({
                "goals":["整理任务"],"constraints":["不修改原文"],
                "facts":["历史背景已归档为派生摘要"],
                "decisions":["先审阅再执行"],"unfinished":["方案等待审阅"]
            })
            .to_string(),
        )
    }

    #[test]
    fn manual_page_mark_skips_boundary_and_injects_summary() {
        let (_temp, store, conversation) = fixture();
        add_history(&store, &conversation);
        let boundary = first_message_id(&store, &conversation);
        let full_history: usize = store
            .list_messages(&conversation)
            .unwrap()
            .iter()
            .map(|m| m.content.chars().count())
            .sum();
        let mark_id = super::generate_page_mark(
            &store,
            &conversation,
            &boundary,
            &FakeProvider::new(boundary_reply()),
            "fixture",
            "config",
            "gpt-4o",
            "manual",
        )
        .unwrap();
        let mark = store.page_mark_by_id(&mark_id).unwrap().unwrap();
        assert!(mark.active);
        assert_eq!(mark.kind, "manual");

        let context =
            crate::ai::memory::prepare_uncompressed_context(&conversation, &store).unwrap();
        let texts: Vec<&str> = context.iter().map(|m| m.content.as_str()).collect();
        let summary_message = texts
            .iter()
            .find(|t| t.contains("已翻篇的历史摘要"))
            .expect("翻篇摘要应注入装配");
        assert!(
            summary_message.contains(boundary.as_str()),
            "摘要应标注覆盖边界"
        );
        // 边界前原文确定性跳过；边界后消息完整保留。
        assert!(texts.iter().all(|t| !t.contains("historical background")));
        assert!(
            texts.iter().any(|t| t.contains("保留最近完整交换")),
            "边界后原文必须保留"
        );
        let context_chars: usize = texts.iter().map(|t| t.chars().count()).sum();
        assert!(context_chars < full_history / 2, "翻篇后装配体积应显著下降");
    }

    #[test]
    fn boundary_at_tail_is_rejected() {
        let (_temp, store, conversation) = fixture();
        let last = add_history(&store, &conversation);
        let error = super::generate_page_mark(
            &store,
            &conversation,
            &last,
            &FakeProvider::new(boundary_reply()),
            "fixture",
            "config",
            "gpt-4o",
            "manual",
        )
        .unwrap_err();
        assert!(error.to_string().contains("不吞最新一轮"));
    }

    #[test]
    fn override_edit_updates_render_and_rejects_invalid_body() {
        let (_temp, store, conversation) = fixture();
        add_history(&store, &conversation);
        let mark_id = super::generate_page_mark(
            &store,
            &conversation,
            &first_message_id(&store, &conversation),
            &FakeProvider::new(boundary_reply()),
            "fixture",
            "config",
            "gpt-4o",
            "manual",
        )
        .unwrap();

        let invalid = r#"{"goals":[],"constraints":[],"facts":[],"decisions":[],"unfinished":[]}"#;
        assert!(
            store.save_page_mark_override(&mark_id, invalid).is_err(),
            "空摘要必须被校验挡下"
        );
        assert!(store.save_page_mark_override(&mark_id, "not json").is_err());

        let edited = json!({
            "goals":["编辑后的目标"],"constraints":["不修改原文"],"facts":[],
            "decisions":[],"unfinished":["方案等待审阅"]
        })
        .to_string();
        store.save_page_mark_override(&mark_id, &edited).unwrap();
        let mark = store.page_mark_by_id(&mark_id).unwrap().unwrap();
        let (text, body_json) = super::render_mark_text(&store, &mark).unwrap();
        assert!(text.contains("编辑后的目标"), "预览渲染应使用用户编辑版");
        assert!(text.contains("summary_id"), "覆盖注记必须随摘要注入");
        let reparsed: super::SummaryBody = serde_json::from_str(&body_json).unwrap();
        assert_eq!(reparsed.goals, vec!["编辑后的目标".to_string()]);
    }

    #[test]
    fn undo_restores_full_history_wiring() {
        let (_temp, store, conversation) = fixture();
        add_history(&store, &conversation);
        let mark_id = super::generate_page_mark(
            &store,
            &conversation,
            &first_message_id(&store, &conversation),
            &FakeProvider::new(boundary_reply()),
            "fixture",
            "config",
            "gpt-4o",
            "manual",
        )
        .unwrap();
        store.set_page_mark_active(&mark_id, false).unwrap();
        assert!(store.active_page_mark(&conversation).unwrap().is_none());
        let context =
            crate::ai::memory::prepare_uncompressed_context(&conversation, &store).unwrap();
        assert!(
            context
                .iter()
                .any(|m| m.content.contains("historical background")),
            "撤销翻篇后原文必须完整回到装配"
        );
    }

    #[test]
    fn auto_marks_keep_only_latest_per_topic() {
        let (_temp, store, conversation) = fixture();
        add_history(&store, &conversation);
        let messages = store.list_messages(&conversation).unwrap();
        let first = messages[0].id.clone();
        let second = messages[1].id.clone();
        let provider = || FakeProvider::new(boundary_reply());
        let old = super::generate_page_mark(
            &store,
            &conversation,
            &first,
            &provider(),
            "fixture",
            "config",
            "gpt-4o",
            "auto",
        )
        .unwrap();
        let new = super::generate_page_mark(
            &store,
            &conversation,
            &second,
            &provider(),
            "fixture",
            "config",
            "gpt-4o",
            "auto",
        )
        .unwrap();
        assert_ne!(old, new);
        assert!(
            !store.page_mark_by_id(&old).unwrap().unwrap().active,
            "旧 auto 必须失活（每话题只留最新一份）"
        );
        assert!(store.page_mark_by_id(&new).unwrap().unwrap().active);
        // 同一边界重复翻篇要被显式拒绝，不静默重建。
        let error = super::generate_page_mark(
            &store,
            &conversation,
            &second,
            &provider(),
            "fixture",
            "config",
            "gpt-4o",
            "auto",
        )
        .unwrap_err();
        assert!(error.to_string().contains("已存在翻篇标记"));
    }

    #[test]
    fn stale_boundary_falls_back_to_full_history() {
        let (_temp, mut store, conversation) = fixture();
        add_history(&store, &conversation);
        let mark_id = super::generate_page_mark(
            &store,
            &conversation,
            &first_message_id(&store, &conversation),
            &FakeProvider::new(boundary_reply()),
            "fixture",
            "config",
            "gpt-4o",
            "manual",
        )
        .unwrap();
        // 直接改库模拟边界消息在别的设备被删除的跨端事实。
        store
            .connection
            .execute(
                "UPDATE conversation_page_marks SET boundary_message_id='deleted-msg' WHERE id=?1",
                [&mark_id],
            )
            .unwrap();
        // 生效标记失效必须显式落事件。挂上真实的 chat run 身份后断言事件表。
        let input_id = {
            let messages = store.list_messages(&conversation).unwrap();
            messages
                .iter()
                .rev()
                .find(|m| m.role == "user")
                .unwrap()
                .id
                .clone()
        };
        let run = store
            .create_chat_run(
                &conversation,
                &input_id,
                "stale-mark-drill",
                "fixture-owner",
            )
            .unwrap();
        store.chat_run_identity = Some(crate::ai::generation::RunIdentity {
            id: run.id.clone(),
            owner: "fixture-owner".into(),
        });
        let context =
            crate::ai::memory::prepare_uncompressed_context(&conversation, &store).unwrap();
        assert!(
            context
                .iter()
                .any(|m| m.content.contains("historical background")),
            "边界失效必须退回全原文"
        );
        assert!(
            !context
                .iter()
                .any(|m| m.content.contains("已翻篇的历史摘要")),
            "失效标记的摘要不得再注入"
        );
        crate::ai::memory::prepare_uncompressed_context(&conversation, &store).unwrap();
        let stale_events: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM chat_run_events WHERE run_id=?1 AND kind='page_mark_stale'",
                [&run.id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(stale_events >= 1, "失效必须显式落事件，不静默");
    }

    #[test]
    fn apply_summary_targets_only_current_page_after_mark() {
        let (_temp, store, conversation) = fixture();
        add_history(&store, &conversation);
        let boundary = first_message_id(&store, &conversation);
        let mark_id = super::generate_page_mark(
            &store,
            &conversation,
            &boundary,
            &FakeProvider::new(boundary_reply()),
            "fixture",
            "config",
            "gpt-4o",
            "auto",
        )
        .unwrap();
        assert!(store.page_mark_by_id(&mark_id).unwrap().unwrap().active);
        // 标记生效后第二篇增长：complete_originals 守护不应挡住新篇摘要。
        store
            .send_message(
                &conversation,
                "user",
                &"new page growth ".repeat(3000),
                None,
            )
            .unwrap();
        store
            .send_message(&conversation, "assistant", "新篇答复", None)
            .unwrap();
        store
            .send_message(
                &conversation,
                "user",
                &"new page growth 2 ".repeat(3000),
                None,
            )
            .unwrap();
        store
            .send_message(&conversation, "assistant", "新篇第二轮答复", None)
            .unwrap();
        store
            .send_message(&conversation, "user", "新篇当前问题", None)
            .unwrap();
        let provider = FakeProvider::new(boundary_reply());
        let mut messages =
            crate::ai::memory::prepare_uncompressed_context(&conversation, &store).unwrap();
        super::apply_summary(
            &store,
            &conversation,
            &provider,
            "fixture",
            "config",
            "gpt-4o",
            4000,
            &mut messages,
        )
        .unwrap();
        // 4000 预算按 80% 触发线：新篇 18000+ 字符远超 3200 阈值 → 应请求摘要。
        assert!(
            provider.requests.get() >= 1,
            "翻篇后的新增长必须仍可触发摘要，不能被守护永久挡住"
        );
    }
}

mod history_requests;

mod coverage_integrity;

mod diagnostics;
