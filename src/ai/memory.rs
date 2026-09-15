use crate::storage::Store;
use anyhow::Result;

/// Message for AI context
#[derive(Debug, Clone)]
pub struct ContextMessage {
    pub role: String,
    pub content: String,
}

/// System prompt：定义 AI 在本应用中的角色，防止泛化成通用聊天助手。
/// 用户提到工具/产品只是事件背景，不是科普请求；回答要围绕个人记录与知识库。
const SYSTEM_PROMPT_BASE: &str = "你是「Elsewhen」——用户的个人事件与知识库助理（本地优先，所有数据处理都在本机完成）。

用户在这个应用里：(a) 记录个人事件（原始记录，不可修改）；(b) 把可复用的知识、结论沉淀到个人知识库。

回复风格：
- 简短、自然、有温度，像一位熟知用户日常的朋友，不要官腔、客服腔或 AI 腔。
- 1-2 句话把核心意思说完即可，求质量不求篇幅。
- 可用少量 emoji 让语气松弛（如 📝 ✅），但不要堆砌。

铁律（任何一条违反都会直接破坏体验）：
1. 用户提到任何工具/产品/服务（如「今天用 opencode2 讨论需求」）只是事件背景——绝不评价它，包括好坏、是否好用、是否值得推荐。宁可一个字不提，也不要顺手夸或贬。
2. 一切围绕用户的个人记录与需求展开，不要滑向通用知识或泛泛而谈的客套。
3. 不知道或没有依据的，直接说不知道；不编造、不铺垫、不写正确的废话。

好的回应示例：
用户：\u{201c}今天用opencode2讨论了一下这个项目的一些需求\u{201d}
好：\u{201c}记下了 📝 等讨论出值得沉淀的结论，随时说一声，我帮你整理进知识库。\u{201d}
不好：\u{201c}OpenCode2 是一个强大的工具，可以用来讨论和管理项目需求…\u{201d} 或 \u{201c}这个工具对项目管理很有帮助…\u{201d}——此类对工具的任何评价都绝对禁止。";

/// 组装 system 消息：基础角色设定 + 当前对话语境（标题/标签）
fn build_system_prompt(store: &Store, conversation_id: &str) -> Result<ContextMessage> {
    let mut prompt = SYSTEM_PROMPT_BASE.to_string();
    if let Some(conversation) = store.get_conversation(conversation_id)? {
        let title = conversation.title.as_deref().unwrap_or("未命名");
        prompt.push_str("\n\n当前对话：");
        prompt.push_str(title);
        if let Some(tag) = &conversation.tag {
            prompt.push_str("（标签：");
            prompt.push_str(tag);
            prompt.push('）');
        }
    }
    Ok(ContextMessage {
        role: "system".to_string(),
        content: prompt,
    })
}

/// 组装 system 消息：基础角色设定 + 一段抓取内容（内容对话用，
/// 让 AI 围绕抓取到的原文回答问题，不评价来源平台）
pub fn build_content_system_prompt(content: &str) -> ContextMessage {
    let mut prompt = SYSTEM_PROMPT_BASE.to_string();
    prompt.push_str("\n\n【本次要讨论的抓取内容（原始文本，请围绕它回答用户的问题）】\n");
    prompt.push_str(content);
    ContextMessage {
        role: "system".to_string(),
        content: prompt,
    }
}

/// Memory provider trait for context management
pub trait MemoryProvider {
    /// Prepare context messages from conversation history
    fn prepare_context(&self, conversation_id: &str, store: &Store) -> Result<Vec<ContextMessage>>;
}

/// Simple memory: last N messages
pub struct SimpleMemory {
    max_messages: usize,
}

impl SimpleMemory {
    pub fn new(max_messages: usize) -> Self {
        Self { max_messages }
    }
}

impl MemoryProvider for SimpleMemory {
    fn prepare_context(&self, conversation_id: &str, store: &Store) -> Result<Vec<ContextMessage>> {
        let messages = store.list_messages(conversation_id)?;

        let mut context: Vec<ContextMessage> = messages
            .into_iter()
            .rev()
            .take(self.max_messages)
            .rev()
            .map(|m| ContextMessage {
                role: m.role,
                content: m.content,
            })
            .collect();

        // 角色设定置于最前，防止 AI 泛化成通用聊天助手
        context.insert(0, build_system_prompt(store, conversation_id)?);

        Ok(context)
    }
}

/// Sliding window memory: token-limited context
pub struct SlidingWindowMemory {
    max_tokens: usize,
}

/// Rough token estimate: 1 token ≈ 4 characters（本地估算兜底）
pub fn estimate_tokens(text: &str) -> usize {
    text.chars().count() / 4
}

impl SlidingWindowMemory {
    pub fn new(max_tokens: usize) -> Self {
        Self { max_tokens }
    }

    /// Estimate token count (rough approximation: 1 token ≈ 4 characters)
    fn estimate_tokens(text: &str) -> usize {
        estimate_tokens(text)
    }
}

impl MemoryProvider for SlidingWindowMemory {
    fn prepare_context(&self, conversation_id: &str, store: &Store) -> Result<Vec<ContextMessage>> {
        let messages = store.list_messages(conversation_id)?;

        let mut context = Vec::new();
        let mut token_count = 0;

        // Add messages from most recent, stop when exceeding token limit
        for message in messages.into_iter().rev() {
            let msg_tokens = Self::estimate_tokens(&message.content);

            if token_count + msg_tokens > self.max_tokens && !context.is_empty() {
                break;
            }

            context.push(ContextMessage {
                role: message.role,
                content: message.content,
            });

            token_count += msg_tokens;
        }

        // Reverse to chronological order
        context.reverse();

        // 角色设定置于最前，防止 AI 泛化成通用聊天助手
        context.insert(0, build_system_prompt(store, conversation_id)?);

        Ok(context)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_database() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "elsewhen-memory-test-{}.db",
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ))
    }

    fn setup_store() -> (Store, String, std::path::PathBuf) {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        let conversation_id =
            store.create_conversation(Some("今日记录"), Some("discussion")).unwrap();
        store
            .send_message(&conversation_id, "user", "今天用 opencode2 讨论了一些需求", None)
            .unwrap();
        (store, conversation_id, path)
    }

    #[test]
    fn simple_memory_limits_message_count() {
        let memory = SimpleMemory::new(10);
        assert_eq!(memory.max_messages, 10);
    }

    #[test]
    fn sliding_window_estimates_tokens() {
        let text = "This is a test message with several words";
        let tokens = SlidingWindowMemory::estimate_tokens(text);
        assert!(tokens > 0);
        assert!(tokens < text.len()); // Should be less than character count
    }

    #[test]
    fn sliding_window_creates_with_limit() {
        let memory = SlidingWindowMemory::new(4096);
        assert_eq!(memory.max_tokens, 4096);
    }

    #[test]
    fn simple_memory_prepends_system_prompt() {
        let (store, conversation_id, path) = setup_store();
        let context = SimpleMemory::new(10)
            .prepare_context(&conversation_id, &store)
            .unwrap();

        assert!(context[0].role == "system", "第一帧必须是 system 角色设定");
        assert!(
            context[0].content.contains("Elsewhen"),
            "system 提示词应定义本应用角色，实际: {}",
            context[0].content
        );
        assert!(
            context[0].content.contains("铁律"),
            "提示词应包含对工具零评价等硬性约束"
        );
        assert!(
            context[0].content.contains("今日记录"),
            "system 提示词应注入当前对话标题"
        );
        // 原始消息应保留在 system 之后
        assert!(context[1].role == "user");
        assert!(context[1].content.contains("opencode2"));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn sliding_window_prepends_system_prompt() {
        let (store, conversation_id, path) = setup_store();
        let context = SlidingWindowMemory::new(4096)
            .prepare_context(&conversation_id, &store)
            .unwrap();

        assert!(context[0].role == "system");
        assert!(context[0].content.contains("个人知识库"));
        assert!(context[0].content.contains("今日记录"));

        let _ = std::fs::remove_file(path);
    }
}
