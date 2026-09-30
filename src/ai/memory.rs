use super::tool::ToolCall;
use crate::storage::Store;
use anyhow::Result;

/// Message for AI context
#[derive(Debug, Clone)]
pub struct ContextMessage {
    pub role: String,
    pub content: String,
    /// 原生 tool-calling：assistant 消息携带的工具调用（用于回传）
    pub tool_calls: Option<Vec<ToolCall>>,
    /// 原生 tool-calling：tool 角色消息对应哪个工具调用
    pub tool_call_id: Option<String>,
    pub reasoning_content: Option<String>,
    /// Optional cross-conversation background; never serialized to the provider.
    pub optional_background: bool,
}

impl ContextMessage {
    pub fn new(role: &str, content: impl Into<String>) -> Self {
        Self {
            role: role.to_string(),
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
            reasoning_content: None,
            optional_background: false,
        }
    }

    /// 原样回传 assistant 的正文、工具调用和必要的推理字段。
    pub fn assistant_with_tool_calls(
        content: String,
        calls: Vec<ToolCall>,
        reasoning_content: Option<String>,
    ) -> Self {
        Self {
            role: "assistant".to_string(),
            content,
            tool_calls: Some(calls),
            tool_call_id: None,
            reasoning_content,
            optional_background: false,
        }
    }

    /// tool 角色消息：一次工具调用的执行结果
    pub fn tool_result(call_id: String, content: String) -> Self {
        Self {
            role: "tool".to_string(),
            content,
            tool_calls: None,
            tool_call_id: Some(call_id),
            reasoning_content: None,
            optional_background: false,
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
- 本地目录导入：用户明确提供本机路径后，项目模式用 import_directory_as_project：有限只读扫描、AI 分析、预览确认后保存一张 project 页，证据片段可能发送给 Provider。文件模式用 import_directory_files：先草拟确认，再扫描并逐文件保存 source 页，不调用 AI。不要未调用工具就断言没有目录权限。
- 个人待办：你可以从事件/对话中分析出需要后续跟进的事，用 create_todo 提议待办；用户确认后创建，「待办」tab 可查看全部。
- 联系人与关系：对话中出现的对用户重要的联系人（姓名、身份、TA 参与或负责的事情/项目），你可以识别出来，草拟「联系人 + 关系」；用户确认后保存为知识库「联系人」页与结构化关系（在对应页面可查看）。用户说「记住这个人 / 这个人是什么角色 / 他和这个项目什么关系」时，尤其要主动做。
- 个人规则库：你在对话中提议、用户确认后生效的规则，可在「设置 → 数据 → 个人规则库」查看和删除。
- 设置：AI 提供商（模型/密钥/温度）、外观主题、每日 token 用量统计都在「设置」页。
- 范围：你服务的是用户的个人记录、沉淀与回顾，不负责网络、天气等外部事务——用户问到，如实说帮不上，不要硬答。

工具调用（你可以调用系统内置工具查规则、搜知识库、抓网页/推文、问 token 用量、查待办等，清单在下方）：
- 需要查信息时优先调用工具，不要凭记忆瞎编；等待工具结果返回后，再组织最终回复。
- 对话回复结束后不会有后台任务替你继续处理本次要求。不能只说“我去做”“稍后回复”；本轮给出成果、待确认内容或具体未完成的状态。
- 用户发“？”“搞定了吗”等短追问时，结合上一件事说明实际进展；有原要求就接着处理，不要求重发，不用“模型返回空”代替回应。追问不是保存确认。
- 如果运行环境支持原生工具调用，直接以工具调用形式发起即可；若你无法发起原生调用，也可以在回复中**独占一行**输出文本格式调用，等结果返回后再回复最终文本：
[工具调用]{\"name\":\"工具名\",\"arguments\":{...}}
- 只认上面这一种文本格式，不要输出 `<tool_call>` / `<arg_key>` / `<arg_value>` / XML 等其他任何格式，也不要把调用标记混在正文里。
- 一次只调用一个工具，等结果回来再决定下一步；结果已足够回答时就不要再调。
- 每轮最多调用一两次，不要为了调用而调用。
- 其他写类工具都是「草拟确认制」：save_knowledge_draft（存知识页）、create_todo（建待办）、import_url_to_wiki（导入网址）、save_wiki_revision（修订知识页）、propose_people_relations（联系人与关系建档）、archive_conversations_by_title（归档对话）都只是登记待办草稿。必须区分三种事实：工具调用后仅有「待确认草稿，未入库」，知识库列表尚不可见（可在「今天」栏点击待入库数量，查看草稿列表及完整内容、单独保存或删除，也可回复「好」确认）；确认执行成功且返回真实保存结果后才可说「已保存」（引用结果中的真实 slug）；查到既有页面时说「知识库已有页面」，不可把草稿说成页面。草拟后给用户内容摘要并明确提示确认；没有实际执行结果绝不声称已保存/已创建。
- 查看某天的事件：用户说「XX（日期）有哪些事件 / 看看那天记录了什么」等 → 用 list_events_by_date 查当天事件（把自然日期解析成 YYYY-MM-DD，「昨天/前天」按当前日期推算），只读、可直接把结果念给用户。
- 归档对话：用户说「把XX对话归档」→ 用 archive_conversations_by_title（title=精确标题 或 contains=标题包含；标题显示为「新对话」的空标题会话按「新对话」匹配）；仅归档主对话列表，不动知识页内聊天；草拟出匹配清单，用户确认后才归档。
- 知识页改名：用户说「把 X 改名为 Y」「这个项目不叫 X 实际叫 Y」→ 用 rename_wiki_page（slug=当前页标识、new_title=新名字）；改名会连可唯一标识一起换、自动迁移联系人关系引用和页内聊天会话；草拟确认制。只改正文不改名用 save_wiki_revision。
- 主题讨论与沉淀：用户说「我们聊聊 X」「继续讨论 X」时，先自然回应和讨论，不要向用户暴露 topic、议题库等内部概念，也不要仅因开始讨论就创建页面。只有用户明确要求「把结论整理下来 / 保存到知识库 / 以后接着聊」或对话已经形成值得长期复用的明确结论时，先用 search_knowledge_base 查找已有内容：已有页面则用 save_wiki_revision 草拟补充，没有才用 save_knowledge_draft 草拟新页。两条路径都必须等待用户确认，绝不静默覆盖同名页面。
- 知识库按「来源/用途」分区（area），写库时要对号入座：
  - **素材库**（imported）：从外部导入的推文/网页/粘贴文本，带来源 URL。**素材原文锁定**：绝不用 save_wiki_revision 覆盖素材原文。
  - **联系人/项目**（network）：person/、topic/ 前缀的关系网实体，由联系人关系提取自动建档。
  - **知识沉淀**（insight）：AI 从对话提炼保存的知识页（save_knowledge_draft 建的页）。
  - **派生产物**（derivative）：对某页加工出的成果（总结/提炼观点/抖音文案/翻译等），挂在该页详情下，不进主列表。
- 对某一页做加工（总结、提炼要点、写抖音文案、翻译、扩写观点等「生成新内容」）→ 用 save_wiki_revision 且 **save_as=derivative + content_type**（如 总结/提炼观点/抖音文案），保存为派生产物、不改动原页；**不要覆盖素材原文**。只有当用户明确要求修改页面本身的内容（如「把这段改一下」「补充这点进去」）且该页不是素材原文时，才用 save_as=revision 直接修订正文。
- 只在联系人信息重要且具体、有实质新增时用 propose_people_relations 草拟联系人与关系；先搜索已有条目，不重复提议，不确定的 note 标注「待确认」。
- 用户写出的 @人名 / #事项 是明确实体标注，提取时全部纳入；同名但括号备注不同的人不可合并。批量提取用 batch_extract_people_relations，仍需确认。
- 用户确认之后不要再重复提议同一条规则或同一个写操作；相关工作已生效，只需告知结果。

回复风格：
- 简短、自然、有温度，像一位熟知用户日常的朋友，不要官腔、客服腔或 AI 腔。
- 1-2 句话把核心意思说完即可，求质量不求篇幅。
- 可用少量 emoji 让语气松弛（如 📝 ✅），但不要堆砌。

情绪回应：
- 用户分享亲身经历时，先读懂他当下的情绪（委屈、气愤、得意、疲惫、焦虑、兴奋……），用一句真诚的话先接住这份情绪，再谈事情本身。不要急着给建议、不要跳过情绪直接进入分析。
- 要能托住情绪：站在用户这边共情，而不是和稀泥或中立旁观。
- 然后说明本轮已经提供的帮助或可供确认的建议；没有实际执行就不承诺稍后自动完成。
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
3. 不知道或没有依据的，直接说不知道；不编造、不铺垫、不写正确的废话。";

/// 组装 system 消息：基础角色设定 + 当前对话语境（标题/标签）+ 已生效的个人规则
fn build_system_prompt(store: &Store, conversation_id: &str) -> Result<ContextMessage> {
    let mut prompt = SYSTEM_PROMPT_BASE.to_string();
    let mut is_knowledge_mentor = false;
    if let Some(conversation) = store.get_conversation(conversation_id)? {
        is_knowledge_mentor = conversation.assistant_mode == "knowledge_mentor"
            || conversation.wiki_page_slug.is_some();
        if is_knowledge_mentor {
            prompt.push_str(
                "\n\n你当前扮演知识页专业导师，而不是主对话秘书。只围绕当前知识页工作：主动指出矛盾、漏洞、模糊表述、未经验证的假设和缺失依据；区分事实、推断、观点和待确认项；必要时直接质疑用户并说明理由。保持尊重但不要为了陪伴或迎合而泛泛附和。默认不要把页面讨论记录为个人事件，也不要主动发起无关的联系人关系或待办提议。任何页面修改只能通过确认制修订工具提出，不能直接声称已修改。",
            );
        }
        let title = conversation.title.as_deref().unwrap_or("未命名");
        prompt.push_str("\n\n当前对话：");
        prompt.push_str(title);
        if let Some(tag) = &conversation.tag {
            prompt.push_str("（标签：");
            prompt.push_str(tag);
            prompt.push('）');
        }
    }
    let input_already_recorded = store
        .latest_event_id_for_conversation(conversation_id)?
        .is_some();
    let allow_record_event = !is_knowledge_mentor && !input_already_recorded;
    if input_already_recorded {
        prompt.push_str(
            "\n\n当前这条用户输入已由系统原样保存为个人事件，并已进入后台分析队列。不要再次记录，也不要把你的摘要或解读另存为事件；直接结合上下文回复即可。",
        );
    } else if allow_record_event {
        prompt.push_str(
            "\n\n当前对话没有自动事件记录。只有在用户明确要求记录，或内容是值得长期回看的客观经历、决定、行动或进展时，才调用 record_event；寒暄、纯提问、对 AI 回复的评价和页面讨论不要记录。不要把助手自己的解读改写成事件。",
        );
    }
    // 注入活跃目标（FR-PES-005-02）。
    //
    // 与个人规则库同属「用户自述的长期上下文」，但用法相反：规则是行为约束，
    // 要 AI 逐条对照检查；目标是意图背景，只在话题相关时可用。因此这里的措辞
    // 刻意带反向约束——不要求 AI 判断用户是否偏离目标。偏差检测是后台周期任务
    // 的职责（带「不重复 / 宁缺毋滥」纪律），若对话里也开始评判，同一件事会被做
    // 两遍，且对话里那遍更频繁，会把记录变成被审计。
    let active_goals = store.list_active_goals()?;
    if active_goals.is_empty() {
        prompt.push_str("\n\n用户当前尚未设定任何目标。");
    } else {
        prompt.push_str(
            "\n\n用户当前的目标（仅作为理解其意图的背景）。除非与当前话题直接相关，不要提及这些目标；\
             不要评判用户是否偏离目标，也不要把对话引向目标。\n",
        );
        for goal in &active_goals {
            prompt.push_str(&format!("- [{}] {}\n", goal.phase.label(), goal.content));
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
    Ok(ContextMessage::new("system", prompt))
}

fn recent_background(store: &Store, conversation_id: &str) -> Result<Option<ContextMessage>> {
    let is_knowledge_mentor = store
        .get_conversation(conversation_id)?
        .is_some_and(|c| c.assistant_mode == "knowledge_mentor" || c.wiki_page_slug.is_some());
    let recent = if is_knowledge_mentor {
        Vec::new()
    } else {
        store.recent_user_messages_for_context(6, 160, Some(conversation_id))?
    };
    if recent.is_empty() {
        return Ok(None);
    }
    let mut prompt = String::from("你最近和用户聊到过的事（跨对话要点，供回忆；回复时自然带入，不必逐条复述）：\n");
    for msg in recent {
        prompt.push_str("- ");
        prompt.push_str(&msg.content);
        prompt.push('\n');
    }
    let mut message = ContextMessage::new("system", prompt);
    message.optional_background = true;
    Ok(Some(message))
}

/// 组装 system 消息：基础角色设定 + 一段抓取内容（内容对话用，
/// 让 AI 围绕抓取到的原文回答问题，不评价来源平台）
pub fn build_content_system_prompt(content: &str) -> ContextMessage {
    let mut prompt = SYSTEM_PROMPT_BASE.to_string();
    prompt.push_str(
        "\n\n你当前扮演专业内容导师，而不是主对话秘书。请围绕下面的原始材料进行审阅、解释和提炼：主动区分材料中的事实、观点、推断和缺失依据；指出矛盾、模糊处和未经证实的结论；不要为了迎合用户而泛泛附和，也不要把材料内容自动记录成用户个人事件。保持尊重但可以直接提出质疑。",
    );
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
        if let Some(background) = recent_background(store, conversation_id)? {
            context.insert(1, background);
        }

        Ok(context)
    }
}

/// Sliding window memory: token-limited context
pub struct SlidingWindowMemory {
    max_tokens: usize,
}

/// Local BPE accounting for memory selection; each provider separately counts
/// the complete serialized request using its tokenizer family.
pub fn estimate_tokens(text: &str) -> usize {
    tiktoken_rs::cl100k_base_singleton()
        .encode_ordinary(text)
        .len()
}
fn truncate_tokens(text: &str, limit: usize) -> String {
    let chars: Vec<_> = text.chars().collect();
    let (mut low, mut high) = (0, chars.len());
    while low < high {
        let mid = (low + high).div_ceil(2);
        let prefix: String = chars[..mid].iter().collect();
        if estimate_tokens(&prefix) <= limit {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    chars[..low].iter().collect()
}

/// Compact older conversation history using a preliminary memory estimate.
/// Required system context and the latest exchange are never truncated; the
/// provider enforces the actual budget on the complete serialized request.
pub fn compress_context(context: &mut Vec<ContextMessage>, max_tokens: usize) {
    if max_tokens == 0 {
        return;
    }
    let total: usize = context.iter().map(|m| estimate_tokens(&m.content)).sum();
    if total <= max_tokens {
        return;
    }

    let mut systems = context.iter().filter(|m| m.role == "system");
    let Some(primary) = systems.next().cloned() else {
        return;
    };
    let mut history: Vec<_> = context
        .iter()
        .filter(|m| m.role != "system")
        .cloned()
        .collect();
    // The latest user request is non-negotiable; never let a long page body
    // or a subsequent system injection displace it.
    let latest_user = history
        .iter()
        .rposition(|m| m.role == "user")
        .unwrap_or(history.len());
    let current = history.split_off(latest_user);
    let current_tokens: usize = current.iter().map(|m| estimate_tokens(&m.content)).sum();
    let mut remaining = max_tokens
        .saturating_sub(estimate_tokens(&primary.content))
        .saturating_sub(current_tokens);
    let mut rebuilt = vec![primary];
    let omitted_marker = "（较早对话因预算已省略）";
    let marker_budget = if history.is_empty() {
        0
    } else {
        estimate_tokens(omitted_marker)
    };
    remaining = remaining.saturating_sub(marker_budget);

    // Never truncate source evidence, rules or execution results. Optional
    // background is removed at the complete, provider-specific request boundary.
    for source in systems {
        remaining = remaining.saturating_sub(estimate_tokens(&source.content));
        rebuilt.push(source.clone());
    }

    let mut kept = Vec::new();
    while let Some(message) = history.pop() {
        let cost = estimate_tokens(&message.content);
        if cost > remaining {
            history.push(message);
            break;
        }
        remaining -= cost;
        kept.push(message);
    }
    let omitted = !history.is_empty();
    let mut summarized = false;
    if omitted && remaining >= 24 {
        let prefix = "（自动压缩的较早对话，仅供回忆）\n";
        let mut summary = prefix.to_string();
        for message in history {
            let label = if message.role == "user" {
                "用户："
            } else {
                "助手："
            };
            let room = remaining.saturating_sub(estimate_tokens(&summary) + 2);
            if room < 8 {
                break;
            }
            summary.push_str(label);
            summary.push_str(&truncate_tokens(&message.content, room.min(160)));
            summary.push('\n');
        }
        if summary.len() > prefix.len() {
            summary = truncate_tokens(&summary, remaining);
            summarized = true;
            rebuilt.push(ContextMessage::new("system", summary));
        }
    }
    if omitted && !summarized {
        rebuilt.push(ContextMessage::new("system", omitted_marker));
    }
    kept.reverse();
    rebuilt.extend(kept);
    rebuilt.extend(current);
    *context = rebuilt;
}

impl SlidingWindowMemory {
    pub fn new(max_tokens: usize) -> Self {
        Self { max_tokens }
    }

    /// Token 估算（ASCII 约 4 字符/token，CJK 约 2 字符/token）
    fn estimate_tokens(text: &str) -> usize {
        estimate_tokens(text)
    }
}

impl MemoryProvider for SlidingWindowMemory {
    fn prepare_context(&self, conversation_id: &str, store: &Store) -> Result<Vec<ContextMessage>> {
        let messages = store.list_messages(conversation_id)?;

        let mut context: Vec<_> = messages
            .into_iter()
            .map(|m| ContextMessage::new(&m.role, m.content))
            .collect();
        context.insert(0, build_system_prompt(store, conversation_id)?);
        if let Some(background) = recent_background(store, conversation_id)? {
            context.insert(1, background);
        }
        compress_context(&mut context, self.max_tokens);

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
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn setup_store() -> (Store, String, std::path::PathBuf) {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        let conversation_id = store
            .create_conversation(Some("今日记录"), Some("discussion"))
            .unwrap();
        store
            .send_message(
                &conversation_id,
                "user",
                "今天用 opencode2 讨论了一些需求",
                None,
            )
            .unwrap();
        (store, conversation_id, path)
    }

    #[test]
    fn compress_context_preserves_system_and_latest_turns() {
        let mut context = vec![
            ContextMessage::new("system", "system prompt"),
            ContextMessage::new("user", "很早以前的背景 ".repeat(80)),
            ContextMessage::new("assistant", "很早以前的回答 ".repeat(80)),
            ContextMessage::new("user", "当前问题"),
        ];

        compress_context(&mut context, 40);

        assert_eq!(
            context.first().map(|m| m.content.as_str()),
            Some("system prompt")
        );
        assert!(context.iter().any(|m| m.content == "当前问题"));
        assert!(context
            .iter()
            .any(|m| m.content.contains("自动压缩") || m.content.contains("已省略")));
        assert!(
            context
                .iter()
                .map(|m| estimate_tokens(&m.content))
                .sum::<usize>()
                <= 40
        );
    }

    #[test]
    fn compress_context_keeps_primary_prompt_and_current_question_with_large_page() {
        let primary = "核心指令和工具协议".repeat(10);
        let question = "请根据这页给出决策";
        let mut context = vec![
            ContextMessage::new("system", primary.clone()),
            ContextMessage::new("user", "旧对话".repeat(100)),
            ContextMessage::new("user", question),
            ContextMessage::new("system", format!("页面正文：{}", "内容".repeat(500))),
        ];

        let budget = estimate_tokens(&primary) + estimate_tokens(question) + 100;
        compress_context(&mut context, budget);

        assert_eq!(context[0].content, primary);
        assert_eq!(context.last().unwrap().content, question);
        assert!(context.iter().any(|m| m.content == format!("页面正文：{}", "内容".repeat(500))));
        // Required evidence may exceed the memory estimate; only the complete
        // provider boundary may reject it, never silently shorten it here.
        assert!(context.iter().map(|m| estimate_tokens(&m.content)).sum::<usize>() > budget);
    }

    #[test]
    fn compress_context_never_truncates_primary_prompt_even_if_over_budget() {
        let primary = "核心指令".repeat(100);
        let mut context = vec![
            ContextMessage::new("system", primary.clone()),
            ContextMessage::new("user", "当前问题"),
        ];

        compress_context(&mut context, 10);

        assert_eq!(context[0].content, primary);
        assert_eq!(context.last().unwrap().content, "当前问题");
    }

    #[test]
    fn compress_context_does_nothing_when_within_budget() {
        let mut context = vec![ContextMessage::new("user", "短消息")];
        let before = context.clone();
        compress_context(&mut context, 40);
        assert_eq!(context.len(), before.len());
        assert_eq!(context[0].content, before[0].content);
    }

    // ── 目标注入（FR-PES-005-02）──

    fn system_prompt_of(store: &Store, conversation_id: &str) -> String {
        build_system_prompt(store, conversation_id).unwrap().content
    }

    #[test]
    fn system_prompt_states_no_goals_when_empty() {
        let (store, conversation_id, path) = setup_store();
        let prompt = system_prompt_of(&store, &conversation_id);
        assert!(
            prompt.contains("尚未设定任何目标"),
            "空态应明说无目标，不静默省略：\n{prompt}"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn system_prompt_injects_active_goals_with_phase_labels() {
        use crate::storage::GoalPhase;
        let (store, conversation_id, path) = setup_store();
        store
            .create_goal("三个月内发 1.0", GoalPhase::Near)
            .unwrap();
        store
            .create_goal("三年内决策有据", GoalPhase::Long)
            .unwrap();

        let prompt = system_prompt_of(&store, &conversation_id);
        assert!(prompt.contains("[近期] 三个月内发 1.0"), "实际:\n{prompt}");
        assert!(prompt.contains("[长远] 三年内决策有据"), "实际:\n{prompt}");
        let _ = std::fs::remove_file(path);
    }

    /// 目标段必须带反向护栏：只作背景，不评判偏离。
    ///
    /// 这条护栏是需求里的硬约束，不是措辞偏好——没有它，对话里每轮都会拿用户
    /// 的话去比目标，偏差检测就变成审计。
    #[test]
    fn goal_block_carries_guards_against_nagging() {
        use crate::storage::GoalPhase;
        let (store, conversation_id, path) = setup_store();
        store.create_goal("上线 v1", GoalPhase::Near).unwrap();
        let prompt = system_prompt_of(&store, &conversation_id);
        assert!(
            prompt.contains("不要评判用户是否偏离目标"),
            "缺少不评判偏离的护栏：\n{prompt}"
        );
        assert!(
            prompt.contains("不要把对话引向目标"),
            "缺少不把对话引向目标的护栏：\n{prompt}"
        );
        let _ = std::fs::remove_file(path);
    }

    /// 已归档目标不得注入：那是历史，不是当前追求。
    #[test]
    fn archived_goals_are_not_injected() {
        use crate::storage::GoalPhase;
        let (store, conversation_id, path) = setup_store();
        let goal = store.create_goal("已经放弃的事", GoalPhase::Mid).unwrap();
        store.archive_goal(&goal.id).unwrap();

        let prompt = system_prompt_of(&store, &conversation_id);
        assert!(
            !prompt.contains("已经放弃的事"),
            "已归档目标不应进上下文：\n{prompt}"
        );
        let _ = std::fs::remove_file(path);
    }

    /// 目标与规则语义必须分离：规则是「对照检查」，目标是「仅作背景」。
    #[test]
    fn goals_and_rules_use_distinct_instructions() {
        use crate::storage::GoalPhase;
        let (store, conversation_id, path) = setup_store();
        store.create_goal("上线 v1", GoalPhase::Near).unwrap();
        // 规则为空时注入的是「当前为空」那句，找不到带「对照检查」的措辞，
        // 所以先建一条生效规则让两段都真实存在。
        store
            .add_rule("回复别用 emoji", crate::storage::RuleStatus::Active, None)
            .unwrap();

        let prompt = system_prompt_of(&store, &conversation_id);
        let goal_at = prompt.find("[近期]").expect("目标段应存在");
        let rule_at = prompt
            .find("个人规则库当前内容（回复时对照检查")
            .expect("规则段应存在");

        // 目标段在规则段之前，且各自措辞不同
        assert!(goal_at < rule_at, "目标段应与规则段分开");
        // 从目标段起点回看一整段，注意按字符边界切（提示词以中文为主，字节切分会切坏 UTF-8）
        // 用锚点取目标段，不按字节偏移回切：提示词以中文为主，按字节切会切坏
        // UTF-8 边界（`floor_char_boundary` 需 nightly，本仓库跑 stable）。
        let goal_block = prompt
            .split("用户当前的目标（仅作为理解其意图的背景）")
            .nth(1)
            .expect("目标段锚点应存在")
            .split('[')
            .next()
            .unwrap();
        assert!(
            !goal_block.contains("对照检查"),
            "目标不得继承规则的行为约束措辞：\n{goal_block}"
        );
        let _ = std::fs::remove_file(path);
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
    fn estimate_tokens_weights_cjk_more_than_ascii() {
        // 中文在主流 tokenizer 中约 1 字/token：同字符数的中文估算必须明显高于
        // ASCII（此前 chars()/4 对 CJK 低估 4 倍，请求体易硬超 provider 上限）。
        let cjk = "事件记录与任务跟进".repeat(10); // 90 个 CJK 字符
        let ascii = "a".repeat(90);
        assert!(cjk.chars().count() == ascii.chars().count());
        assert!(
            estimate_tokens(&cjk) > estimate_tokens(&ascii),
            "CJK 估算 {} 应高于 ASCII 估算 {}",
            estimate_tokens(&cjk),
            estimate_tokens(&ascii)
        );
        assert_eq!(
            estimate_tokens(&cjk),
            tiktoken_rs::cl100k_base_singleton()
                .encode_ordinary(&cjk)
                .len()
        );
        assert_eq!(
            estimate_tokens(&ascii),
            tiktoken_rs::cl100k_base_singleton()
                .encode_ordinary(&ascii)
                .len()
        );
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
    fn auto_recorded_main_input_hides_record_event_tool() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        let conversation_id = store
            .create_conversation(Some("主对话流"), Some("diary"))
            .unwrap();
        store
            .submit_conversation_input(&conversation_id, "今天完成了整理", Some("main-1"))
            .unwrap();

        let context = SimpleMemory::new(10)
            .prepare_context(&conversation_id, &store)
            .unwrap();
        let system = &context[0].content;
        assert!(system.contains("已由系统原样保存"));
        assert!(!system.contains("record_event"));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn legacy_conversation_without_auto_record_keeps_record_event_tool() {
        let (store, conversation_id, path) = setup_store();
        let context = SimpleMemory::new(10)
            .prepare_context(&conversation_id, &store)
            .unwrap();
        let system = &context[0].content;
        assert!(system.contains("调用 record_event"));
        assert!(!system.contains("可用工具（name：用途）"));
        assert!(system.contains("当前对话没有自动事件记录"));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn knowledge_mentor_hides_record_event_tool() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        let conversation_id = store
            .create_wiki_chat_conversation("topic/test", "测试主题")
            .unwrap();
        store
            .send_message(&conversation_id, "user", "分析一下这页", None)
            .unwrap();

        let context = SimpleMemory::new(10)
            .prepare_context(&conversation_id, &store)
            .unwrap();
        let system = &context[0].content;
        assert!(system.contains("知识页专业导师"));
        assert!(!system.contains("record_event"));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn recent_background_excludes_current_stream_and_wiki_before_limiting() {
        let (store, conversation_id, path) = setup_store();
        assert!(recent_background(&store, &conversation_id).unwrap().is_none());
        let other = store.create_conversation(Some("历史普通对话"), None).unwrap();
        store.send_message(&other, "user", "历史普通会话背景", None).unwrap();
        let wiki = store.create_wiki_chat_conversation("topic/test", "知识页").unwrap();
        for _ in 0..8 {
            store.send_message(&wiki, "user", "知识页无关讨论", None).unwrap();
            store.send_message(&conversation_id, "user", "当前流已有消息", None).unwrap();
        }
        let background = recent_background(&store, &conversation_id).unwrap().unwrap();
        assert!(background.optional_background);
        assert!(background.content.contains("历史普通会话背景"));
        assert!(!background.content.contains("知识页无关讨论"));
        assert!(!background.content.contains("当前流已有消息"));
        assert!(recent_background(&store, &wiki).unwrap().is_none());
        drop(store);
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
            .send_message(
                &other_id,
                "user",
                "昨天给海油服的张玮沟通了双链路付款的事情，必须留痕",
                None,
            )
            .unwrap();

        let context = SimpleMemory::new(10)
            .prepare_context(&conversation_id, &store)
            .unwrap();

        assert!(!context[0].content.contains("昨天给海油服"));
        let background = context.iter().find(|m| m.optional_background).unwrap();
        let system = &background.content;
        assert!(
            system.contains("张玮"),
            "跨对话近期消息应被注入 system 提示，实际: {}",
            system
        );
        assert!(system.contains("留痕"), "近期消息内容应整体可见");

        let _ = std::fs::remove_file(path);
    }
}
