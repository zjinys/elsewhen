use crate::storage::Store;
use anyhow::Result;
use super::tool::ToolCall;

/// Message for AI context
#[derive(Debug, Clone)]
pub struct ContextMessage {
    pub role: String,
    pub content: String,
    /// 原生 tool-calling：assistant 消息携带的工具调用（用于回传）
    pub tool_calls: Option<Vec<ToolCall>>,
    /// 原生 tool-calling：tool 角色消息对应哪个工具调用
    pub tool_call_id: Option<String>,
}

impl ContextMessage {
    pub fn new(role: &str, content: impl Into<String>) -> Self {
        Self {
            role: role.to_string(),
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// assistant 消息：携带工具调用（内容通常为空，等待工具结果）
    pub fn assistant_with_tool_calls(calls: Vec<ToolCall>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: String::new(),
            tool_calls: Some(calls),
            tool_call_id: None,
        }
    }

    /// tool 角色消息：一次工具调用的执行结果
    pub fn tool_result(call_id: String, content: String) -> Self {
        Self {
            role: "tool".to_string(),
            content,
            tool_calls: None,
            tool_call_id: Some(call_id),
        }
    }
}

/// System prompt：定义 AI 在本应用中的角色，防止泛化成通用聊天助手。
/// 用户提到工具/产品只是事件背景，不是科普请求；回答要围绕个人记录与知识库。
const SYSTEM_PROMPT_BASE: &str = "你是「Elsewhen」——用户的个人事件与知识库助理（本地优先，所有数据处理都在本机完成）。

用户在这个应用里：(a) 记录个人事件（原始记录，不可修改）；(b) 把可复用的知识、结论沉淀到个人知识库。

系统能力（你是这套应用内置的助手，这些是它真实有的功能，用户问到时如实引导）：
- 记录与回顾：用户会随时记录工作/生活里的人和事（原始记录，不可修改，可随时回看）。
- 知识库：把可复用的结论沉淀成知识库页面（左侧「知识库」tab 可看）；用户说「把这条存进知识库」时，是真实可做的。
- 导入：用户把任意网址（x.com/twitter.com 推文或其他网页）粘贴到「导入」tab，或直接在对话里说「导入这个链接到知识库」→ 抓取预览 → 点「保存到知识库」确认后入库；也可以直接贴一段文本保存。用户问怎么导入时，告知这个入口。
- 个人待办：你可以从事件/对话中分析出需要后续跟进的事，用 create_todo 提议待办；用户确认后创建，「待办」tab 可查看全部。
- 人物与关系：对话中出现的对用户重要的人物（姓名、身份、TA 参与或负责的事情/项目），你可以识别出来，草拟「人物 + 关系」；用户确认后保存为知识库「人物」页与结构化关系（在对应页面可查看）。用户说「记住这个人 / 这个人是什么角色 / 他和这个项目什么关系」时，尤其要主动做。
- 个人规则库：你在对话中提议、用户确认后生效的规则，可在「设置 → 数据 → 个人规则库」查看和删除。
- 设置：AI 提供商（模型/密钥/温度）、外观主题、每日 token 用量统计都在「设置」页。
- 范围：你服务的是用户的个人记录、沉淀与回顾，不负责网络、天气等外部事务——用户问到，如实说帮不上，不要硬答。

工具调用（你可以调用系统内置工具查规则、搜知识库、抓网页/推文、问 token 用量、查待办等，清单在下方）：
- 需要查信息时优先调用工具，不要凭记忆瞎编；等待工具结果返回后，再组织最终回复。
- 如果运行环境支持原生工具调用，直接以工具调用形式发起即可；若你无法发起原生调用，也可以在回复中**独占一行**输出文本格式调用，等结果返回后再回复最终文本：
[工具调用]{\"name\":\"工具名\",\"arguments\":{...}}
- 一次只调用一个工具，等结果回来再决定下一步；结果已足够回答时就不要再调。
- 每轮最多调用一两次，不要为了调用而调用。
- 写类工具：记录事件（record_event）会**立即保存**到事件记录，调用后可直接告诉用户「已记下」。**记录事件直接用 record_event，绝不用 save_knowledge_draft 草拟事件**。
- 其他写类工具都是「草拟确认制」：save_knowledge_draft（存知识页）、create_todo（建待办）、import_url_to_wiki（导入网址）、save_wiki_revision（修订知识页）、propose_people_relations（人物与关系建档）、archive_conversations_by_title（归档对话）都只是登记待办草稿——调用后必须先把你草拟的内容原样告诉用户（摘要即可），并明确请用户确认（回复「好」）。**确认前绝不声称已保存/已创建**，系统会在用户确认后替你真正写入。
- 查看某天的事件：用户说「XX（日期）有哪些事件 / 看看那天记录了什么」等 → 用 list_events_by_date 查当天事件（把自然日期解析成 YYYY-MM-DD，「昨天/前天」按当前日期推算），只读、可直接把结果念给用户。
- 归档对话：用户说「把XX对话归档」→ 用 archive_conversations_by_title（title=精确标题 或 contains=标题包含；标题显示为「新对话」的空标题会话按「新对话」匹配）；仅归档主对话列表，不动知识页内聊天；草拟出匹配清单，用户确认后才归档。
- 人物关系（propose_people_relations）：当对话里出现**新的、或信息有实质更新**的重要人物及其参与/负责的事情/项目时草拟。要求：只针对对用户重要、且信息具体的人物（有称呼/身份/参与的具体事情），不要为随口一提、没有可用信息的名字草拟；同一人同一件事若已建档存过（先用 list_wiki_pages / search_knowledge_base 查一下），不要重复提议；不确定的地方在 note 里标注「待确认」。
- 规范标注 @ / #：用户可以（在事件或对话里）用 @人名 明确标注「这是人」、用 #事情/项目 明确标注「这是事情」，如「@张伟 负责 #双链路付款」。这些标注是用户写死的权威实体——草拟人物/关系时必须全部纳入；同名但在名字里带了括号备注（如「张伟（市场部）」「张伟（设计）」）的是不同的人，不能合并。未标注时再按上下文识别。批量提取用 batch_extract_people_relations（扫描全部事件，同一待确认机制）。
- 用户确认之后不要再重复提议同一条规则或同一个写操作；相关工作已生效，只需告知结果。

回复风格：
- 简短、自然、有温度，像一位熟知用户日常的朋友，不要官腔、客服腔或 AI 腔。
- 1-2 句话把核心意思说完即可，求质量不求篇幅。
- 可用少量 emoji 让语气松弛（如 📝 ✅），但不要堆砌。

情绪回应：
- 用户分享亲身经历时，先读懂他当下的情绪（委屈、气愤、得意、疲惫、焦虑、兴奋……），用一句真诚的话先接住这份情绪，再谈事情本身。不要急着给建议、不要跳过情绪直接进入分析。
- 要能托住情绪：站在用户这边共情，而不是和稀泥或中立旁观。
- 然后可以点一句你准备怎么做（比如：把这条教训记下来、下次遇到类似情况用规则提醒你），让用户感到被认真对待。
- 仍然要简短：情绪共情 + 一句要点，两段以内。
- **严禁**输出「保持自己的节奏」「多留个心眼」「给自己留条后路」「相信自己」「加油」「放轻松」这类万金油套话、空泛建议或鼓励——空洞的话比不说话更糟。
- 尽量贴着用户给出的具体细节回复：写出具体的人名（如张玮）、事件（如双链路付款、和太极沟通）、时间与你的判断，并给一个可执行的下一步（如：对方没做就主动发消息/邮件留痕确认），哪怕只有一句话。没有细节可贴时，宁可少说。

个人经验规则库：
- 用户常分享「踩坑 / 教训 / 心得」类经历，其中往往蕴含一条可复用、可执行的经验准则（例如：「和大型企业的人沟通重要事项必须留痕」）。
- 生成回复前先对照下方规则库：若用户经历对应的准则**已有类似规则**，顺势引用它并确认即可；若**没有**类似规则，且确实值得固化为规则，请在回复末尾**另起一行**输出一条提议，格式严格如下（独占一行，不要加多余符号）：
[规则提议]一条具体、第一人称、可指导未来行为的话
  规则要具体可执行（谁、什么场景、怎么做），不要写「要小心」「多注意」这类空话。
- 规则的提议必须紧贴用户这段经历，不要为了凑数而提议。

铁律（任何一条违反都会直接破坏体验）：
1. 用户提到任何工具/产品/服务（如「今天用 opencode2 讨论需求」）只是事件背景——绝不评价它，包括好坏、是否好用、是否值得推荐。宁可一个字不提，也不要顺手夸或贬。
2. 一切围绕用户的个人记录与需求展开，不要滑向通用知识或泛泛而谈的客套。
3. 不知道或没有依据的，直接说不知道；不编造、不铺垫、不写正确的废话。

好的回应示例：
用户：\u{201c}今天用opencode2讨论了一下这个项目的一些需求\u{201d}
好：\u{201c}记下了 📝 等讨论出值得沉淀的结论，随时说一声，我帮你整理进知识库。\u{201d}
不好：\u{201c}OpenCode2 是一个强大的工具，可以用来讨论和管理项目需求…\u{201d} 或 \u{201c}这个工具对项目管理很有帮助…\u{201d}——此类对工具的任何评价都绝对禁止。";

/// 组装 system 消息：基础角色设定 + 当前对话语境（标题/标签）+ 已生效的个人规则
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
    // 注入已生效的个人规则，让 AI 回复时对照检查
    let active_rules = store.list_active_rules()?;
    if active_rules.is_empty() {
        prompt.push_str("\n\n个人规则库当前为空。");
    } else {
        prompt.push_str("\n\n个人规则库当前内容（回复时对照检查，已有类似规则就不要再次提议）：\n");
        for rule in active_rules {
            prompt.push_str("- ");
            prompt.push_str(&rule.content);
            prompt.push('\n');
        }
    }
    // 注入跨对话的近期用户消息，弥补单对话上下文断裂：让 AI 回忆起最近聊过的人与事
    let recent = store.recent_user_messages(6, 160)?;
    if !recent.is_empty() {
        prompt.push_str("\n\n你最近和用户聊到过的事（跨对话要点，供回忆；回复时自然带入，不必逐条复述）：\n");
        for msg in recent {
            prompt.push_str("- ");
            prompt.push_str(&msg.content);
            prompt.push('\n');
        }
    }
    // 注入可用工具清单（供工具调用；与原生 tools 字段/文本协议保持一致）
    let registry = super::tool::ToolRegistry::default();
    prompt.push_str("\n\n");
    prompt.push_str(&registry.prompt_block());
    Ok(ContextMessage::new("system", prompt))
}

/// 组装 system 消息：基础角色设定 + 一段抓取内容（内容对话用，
/// 让 AI 围绕抓取到的原文回答问题，不评价来源平台）
pub fn build_content_system_prompt(content: &str) -> ContextMessage {
    let mut prompt = SYSTEM_PROMPT_BASE.to_string();
    prompt.push_str("\n\n【本次要讨论的抓取内容（原始文本，请围绕它回答用户的问题）】\n");
    prompt.push_str(content);
    ContextMessage::new("system", prompt)
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
            .map(|m| ContextMessage::new(&m.role, m.content))
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

            context.push(ContextMessage::new(&message.role, message.content));

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

    #[test]
    fn system_prompt_describes_own_capabilities() {
        let (store, conversation_id, path) = setup_store();
        let context = SimpleMemory::new(10)
            .prepare_context(&conversation_id, &store)
            .unwrap();

        let system = &context[0].content;
        // AI 应知道这套系统真实具备的能力与入口，而不是瞎猜
        assert!(system.contains("导入"), "应描述导入能力（推文/网页/文本）");
        assert!(system.contains("知识库"), "应描述知识库能力");
        assert!(system.contains("个人规则库"), "应描述规则库能力");
        assert!(system.contains("保存到知识库"), "应描述推文保存入库入口");
        assert!(system.contains("设置"), "应描述设置页能力");

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn system_prompt_injects_cross_conversation_recent_messages() {
        let (store, conversation_id, path) = setup_store();
        // 在另一个对话里发一条「近期事件」，验证它会被注入到本对话的 system 提示
        let other_id = store
            .create_conversation(Some("另一个对话"), Some("general"))
            .unwrap();
        store
            .send_message(&other_id, "user", "昨天给海油服的张玮沟通了双链路付款的事情，必须留痕", None)
            .unwrap();

        let context = SimpleMemory::new(10)
            .prepare_context(&conversation_id, &store)
            .unwrap();

        let system = &context[0].content;
        assert!(
            system.contains("张玮"),
            "跨对话近期消息应被注入 system 提示，实际: {}",
            system
        );
        assert!(
            system.contains("留痕"),
            "近期消息内容应整体可见"
        );

        let _ = std::fs::remove_file(path);
    }
}
