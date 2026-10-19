use super::tool::ToolCall;
use crate::storage::Store;
use anyhow::Result;
use serde_json::json;

mod segments;
pub use segments::{
    ContextSegment, OmissionPolicy, SegmentOrigin, SegmentPurpose, SelectionOmission,
};

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
    /// Never serialized as model messages; used for selection and diagnostics.
    pub segment: ContextSegment,
    pub(crate) selection_omissions: Vec<SelectionOmission>,
    /// Original timestamp, retained for request-only temporal enrichment.
    pub(crate) message_time: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub(crate) sampled_time: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub(crate) time_reminder: bool,
}

impl ContextMessage {
    pub fn new(role: &str, content: impl Into<String>) -> Self {
        let content = content.into();
        let segment = ContextSegment::for_role(role, &content);
        Self {
            role: role.to_string(),
            content,
            segment,
            selection_omissions: Vec::new(),
            tool_calls: None,
            tool_call_id: None,
            reasoning_content: None,
            optional_background: false,
            message_time: None,
            sampled_time: None,
            time_reminder: false,
        }
    }

    pub(crate) fn with_segment(
        mut self,
        purpose: SegmentPurpose,
        origin: SegmentOrigin,
        identity: Option<&str>,
        omission: OmissionPolicy,
    ) -> Self {
        self.segment = ContextSegment::new(purpose, origin, identity, &self.content, omission);
        self.optional_background = omission == OmissionPolicy::OptionalBackground;
        self
    }

    pub(crate) fn from_record(message: crate::storage::MessageSummary) -> Self {
        let mut context = Self::new(&message.role, message.content).with_segment(
            SegmentPurpose::History,
            SegmentOrigin::StoredMessage,
            Some(&message.id),
            OmissionPolicy::WholeEarlierTurn,
        );
        context.message_time = chrono::DateTime::parse_from_rfc3339(&message.created_at).ok();
        context
    }

    /// 原样回传 assistant 的正文、工具调用和必要的推理字段。
    pub fn assistant_with_tool_calls(
        content: String,
        calls: Vec<ToolCall>,
        reasoning_content: Option<String>,
    ) -> Self {
        let mut message = Self::new("assistant", content).with_segment(
            SegmentPurpose::ToolProtocol,
            SegmentOrigin::ToolExecution,
            None,
            OmissionPolicy::Required,
        );
        message.tool_calls = Some(calls);
        message.reasoning_content = reasoning_content;
        message
    }

    /// tool 角色消息：一次工具调用的执行结果
    pub fn tool_result(call_id: String, content: String) -> Self {
        let mut message = Self::new("tool", content).with_segment(
            SegmentPurpose::ToolResult,
            SegmentOrigin::ToolExecution,
            Some(&call_id),
            OmissionPolicy::Required,
        );
        message.tool_call_id = Some(call_id);
        message
    }
}

/// System prompt：定义 AI 在本应用中的角色，防止泛化成通用聊天助手。
/// 用户提到工具/产品只是事件背景，不是科普请求；回答要围绕个人记录与知识库。
const SYSTEM_PROMPT_BASE: &str = "你是「Elsewhen」——用户的个人事件与知识库助理（本地优先，所有数据处理都在本机完成）。

用户在这个应用里：(a) 记录个人事件（原始记录，不可修改）；(b) 把可复用的知识、结论沉淀到个人知识库。

系统功能与行为概览（都是这套应用真实有的功能，用户问到时如实引导；各工具的详细用法随工具可用时注入）：
- 记录与回顾：用户随时记录工作/生活里的人和事（原始记录，不可修改，可随时回看）。
- 知识库：把可复用的结论沉淀成知识库页面；用户说「把这条存进知识库」时，是真实可做的。
- 导入：任意网址或一段文本可导入知识库（抓取预览、确认后入库）；本地目录可在项目/文件模式扫描导入。
- 个人待办：可从事件/对话分析待办，草拟后经用户确认创建。
- 联系人与关系：对重要联系人可草拟「联系人 + 关系」，确认后保存建档。
- 个人规则库：提议规则经用户确认后生效，在「设置 → 数据 → 个人规则库」管理。
- 外网搜索需用户先在「设置 → 外网搜索」配置；未配置时如实说明，引导开启。
- 范围：只服务用户的个人记录、沉淀与回顾；天气等未接入的事务，如实说帮不上，不要硬答。

工具调用（你可以调用系统内置工具查规则、搜知识库、抓网页/推文、问 token 用量、查待办等，详细用法随工具注入）：
- 需要查信息时优先调用工具，不要凭记忆瞎编；等待工具结果返回后，再组织最终回复。用户问最新/最近/新闻/热点类时效内容且已配置搜索时，用 web_search 查；没查到就说没查到，不编造。
- 对话回复结束后不会有后台任务替你继续处理本次要求。不能只说“我去做”“稍后回复”；本轮给出成果、待确认内容或具体未完成的状态。
- 用户发“？”“搞定了吗”等短追问时，结合上一件事说明实际进展；有原要求就接着处理，不要求重发，不用“模型返回空”代替回应。追问不是保存确认。
- 如果运行环境支持原生工具调用，直接以工具调用形式发起即可；若你无法发起原生调用，也可以在回复中**独占一行**输出文本格式调用，等结果返回后再回复最终文本：
[工具调用]{\"name\":\"工具名\",\"arguments\":{...}}
- 只认上面这一种文本格式，不要输出 `<tool_call>` / `<arg_key>` / `<arg_value>` / XML 等其他任何格式，也不要把调用标记混在正文里。
- 一次只调用一个工具，等结果回来再决定下一步；结果已足够回答时就不要再调。
- 每轮最多调用一两次，不要为了调用而调用。
- 其他写类工具都是「草拟确认制」：save_knowledge_draft（存知识页）、create_todo（建待办）、import_url_to_wiki（导入网址）、save_wiki_revision（修订知识页）、propose_people_relations（联系人与关系建档）、archive_conversations_by_title（归档对话）都只是登记待办草稿。必须区分三种事实：工具调用后仅有「待确认草稿，未入库」，知识库列表尚不可见（可在「今天」栏点击待入库数量，查看草稿列表及完整内容、单独保存或删除，普通短回复不能代替具体审阅）；确认执行成功且返回真实保存结果后才可说「已保存」（引用结果中的真实 slug）；查到既有页面时说「知识库已有页面」，不可把草稿说成页面。草拟后给用户内容摘要并明确提示确认；没有实际执行结果绝不声称已保存/已创建。
- 知识库按「来源/用途」分区（area），写库时要对号入座：
  - **素材库**（imported）：从外部导入的推文/网页/粘贴文本，带来源 URL。**素材原文锁定**：绝不用 save_wiki_revision 覆盖素材原文。
  - **联系人/项目**（network）：person/、topic/ 前缀的关系网实体，由联系人关系提取自动建档。
  - **知识沉淀**（insight）：AI 从对话提炼保存的知识页（save_knowledge_draft 建的页）。
  - **派生产物**（derivative）：对某页加工出的成果（总结/提炼观点/抖音文案/翻译等），挂在该页详情下，不进主列表。
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
- 尽量贴着用户给出的具体细节回复：写出具体的人名（如张三）、事件（如网站改版、王五沟通）、时间与你的判断，并给一个可执行的下一步（如：对方没做就主动发消息/邮件留痕确认），哪怕只有一句话。没有细节可贴时，宁可少说。

个人经验规则库：
- 用户常分享「踩坑 / 教训 / 心得」类经历，其中蕴含可复用、可执行的经验准则。
- 用户分享这类经历、或回复需要引用既有经验规范时，先用 list_rules 工具查库再对照：已有类似规则就顺势引用并确认，不要凭记忆改写用户的既有规则。
- 确实值得固化的新经验且库中可能没有同类规则时，在回复末尾另起一行输出提议，格式严格如下（独占一行，不加多余符号）：
[规则提议]一条具体、第一人称、可指导未来行为的话
  规则要具体可执行（谁、什么场景、怎么做），不要写「要小心」「多注意」这类空话；提议必须紧贴这段经历，不要凑数。

铁律（任何一条违反都会直接破坏体验）：
1. 用户提到任何工具/产品/服务（如「今天用 opencode2 讨论需求」）只是事件背景——绝不评价它，包括好坏、是否好用、是否值得推荐。宁可一个字不提，也不要顺手夸或贬。
2. 一切围绕用户的个人记录与需求展开，不要滑向通用知识或泛泛而谈的客套。
3. 不知道或没有依据的，直接说不知道；不编造、不铺垫、不写正确的废话。引用具体事实（事件、知识页、工具结果、草稿状态）时注明来源（如 [事件 #123]、[知识页 slug]、[工具:xxx]）；给不出来源的内容要明确说「我不确定」，不要装作确知。";
/// R2B 按需工具说明（stable_setup 瘦身）：tools 里任一工具是常驻（非
/// 延迟）时渲染为独立 system message；延迟工具（web_search、目录导入等）
/// 激活前不需要，其句式用法由 schema description 承担，不占常驻基底。
/// label 仅供测试定位，运行时不读。
const TOOL_GUIDES: &[(&str, &[&str], &str)] = &[
    (
        "web_search",
        &["web_search"],
        "若用户已在「设置 → 外网搜索」配置搜索，可用 web_search 查最新/时效信息（新闻、热点）。只在用户明确要求或查时效内容时调用；拿不准先反问要不要联网搜，不自行断定。没配置时工具会说明，引导用户去设置开启。",
    ),
    (
        "dir_import",
        &["import_directory_as_project","import_directory_files"],
        "用户明确提供本机路径后，项目模式用 import_directory_as_project：有限只读扫描、AI 分析、预览确认后保存一张 project 页，证据片段可能发送给 Provider。文件模式用 import_directory_files：先草拟确认，再扫描并逐文件保存 source 页，不调用 AI。不要未调用工具就断言没有目录权限。",
    ),
    (
        "people",
        &["propose_people_relations","batch_extract_people_relations"],
        "对话中出现的对用户重要的联系人（姓名、身份、TA 参与或负责的事情/项目），你可以识别出来，草拟「联系人 + 关系」；用户确认后保存为知识库「联系人」页与结构化关系（在对应页面可查看）。用户说「记住这个人 / 这个人是什么角色 / 他和这个项目什么关系」时，尤其要主动做。
只在联系人信息重要且具体、有实质新增时用 propose_people_relations 草拟联系人与关系；先搜索已有条目，不重复提议，不确定的 note 标注「待确认」。
用户写出的 @人名 / #事项 是明确实体标注，提取时全部纳入；同名但括号备注不同的人不可合并。批量提取用 batch_extract_people_relations，仍需确认。",
    ),
    (
        "events",
        &["list_events_by_date"],
        "用户说「XX（日期）有哪些事件 / 看看那天记录了什么」等 → 用 list_events_by_date 查当天事件（把自然日期解析成 YYYY-MM-DD，「昨天/前天」按当前日期推算），只读、可直接把结果念给用户。",
    ),
    (
        "archive",
        &["archive_conversations_by_title"],
        "用户说「把XX对话归档」→ 用 archive_conversations_by_title（title=精确标题 或 contains=标题包含；标题显示为「新对话」的空标题会话按「新对话」匹配）；仅归档主对话列表，不动知识页内聊天；草拟出匹配清单，用户确认后才归档。",
    ),
    (
        "rename",
        &["rename_wiki_page"],
        "用户说「把 X 改名为 Y」「这个项目不叫 X 实际叫 Y」→ 用 rename_wiki_page（slug=当前页标识、new_title=新名字）；改名会连可唯一标识一起换、自动迁移联系人关系引用和页内聊天会话；草拟确认制。只改正文不改名用 save_wiki_revision。",
    ),
    (
        "topic_flow",
        &["search_knowledge_base","save_wiki_revision","save_knowledge_draft"],
        "用户说「我们聊聊 X」「继续讨论 X」时，先自然回应和讨论，不要向用户暴露 topic、议题库等内部概念，也不要仅因开始讨论就创建页面。只有用户明确要求「把结论整理下来 / 保存到知识库 / 以后接着聊」或对话已经形成值得长期复用的明确结论时，先用 search_knowledge_base 查找已有内容：已有页面则用 save_wiki_revision 草拟补充，没有才用 save_knowledge_draft 草拟新页。两条路径都必须等待用户确认，绝不静默覆盖同名页面。",
    ),
    (
        "derivative",
        &["save_wiki_revision"],
        "对某一页做加工（总结、提炼要点、写抖音文案、翻译、扩写观点等「生成新内容」）→ 用 save_wiki_revision 且 **save_as=derivative + content_type**（如 总结/提炼观点/抖音文案），保存为派生产物、不改动原页；**不要覆盖素材原文**。只有当用户明确要求修改页面本身的内容（如「把这段改一下」「补充这点进去」）且该页不是素材原文时，才用 save_as=revision 直接修订正文。",
    ),
];

/// Stable instructions and live application state have separate provenance and budgets.
fn build_system_context(store: &Store, conversation_id: &str) -> Result<Vec<ContextMessage>> {
    let mut context = vec![
        ContextMessage::new("system", SYSTEM_PROMPT_BASE).with_segment(
            SegmentPurpose::StableSetup,
            SegmentOrigin::Application,
            Some("elsewhen-chat-setup-v1"),
            OmissionPolicy::Required,
        ),
    ];
    // R2B：按需工具指南——常驻（非延迟）工具的详细句式渲染为独立 system
    // message（只追加、不并入基底，保住 base 消息的前缀稳定性）；延迟工具
    // 的用法由 schema description 承担，激活前不占上下文。
    let resident = crate::ai::tool::ToolRegistry::deferred_runtime().resident_tool_names();
    let mut guide_text = String::new();
    for (_, tools, text) in TOOL_GUIDES {
        if tools.iter().any(|t| resident.contains(*t)) {
            guide_text.push_str(text);
            guide_text.push_str("\n\n");
        }
    }
    if !guide_text.is_empty() {
        context.push(
            ContextMessage::new("system", guide_text.trim_end().to_string()).with_segment(
                SegmentPurpose::StableSetup,
                SegmentOrigin::Application,
                Some("elsewhen-chat-tool-guides-v1"),
                OmissionPolicy::Required,
            ),
        );
    }
    let mut prompt = String::new();
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
    context.push(
        ContextMessage::new("system", std::mem::take(&mut prompt)).with_segment(
            SegmentPurpose::ConversationState,
            SegmentOrigin::Conversation,
            Some(conversation_id),
            OmissionPolicy::Required,
        ),
    );
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

    context.push(
        ContextMessage::new("system", std::mem::take(&mut prompt)).with_segment(
            SegmentPurpose::Goals,
            SegmentOrigin::Goals,
            Some("active-goals"),
            OmissionPolicy::Required,
        ),
    );
    // 规则库与主对话解耦（瘦身）：不再全量注入规则文本——规则库会持续增长，
    // 全量注入会让基座随规则数线性膨胀。常驻只播报规模与查询方式；全文按
    // 需经常驻 list_rules 工具获取（模型在触发场景自主查询）。
    let rule_count = store.count_active_rules()?;
    let rules_note = if rule_count == 0 {
        "\n\n个人规则库当前为空。".to_string()
    } else {
        format!(
            "\n\n个人规则库现有 {rule_count} 条生效规则，回复需要引用既有经验规范时先用 list_rules 工具查询，再对照检查。"
        )
    };
    prompt.push_str(&rules_note);
    context.push(ContextMessage::new("system", prompt).with_segment(
        SegmentPurpose::PersonalRules,
        SegmentOrigin::Rules,
        Some("active-personal-rules"),
        OmissionPolicy::Required,
    ));
    Ok(context)
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
    let mut prompt = String::from(
        "你最近和用户聊到过的事（跨对话要点，供回忆；回复时自然带入，不必逐条复述）：\n",
    );
    for msg in recent {
        prompt.push_str("- ");
        prompt.push_str(&msg.content);
        prompt.push('\n');
    }
    let message = ContextMessage::new("system", prompt).with_segment(
        SegmentPurpose::CrossConversation,
        SegmentOrigin::StoredMessage,
        Some("recent-other-conversations"),
        OmissionPolicy::OptionalBackground,
    );
    Ok(Some(message))
}

/// 今日记忆真检索（决策辅助 D6，合并今日记忆提案）：把今天记录下来的
/// 事件原文作为可选背景注入,让 AI 回答「今天记了什么」时是在真检索,
/// 不是靠模型推断编造。红线：只呈现原文快照,不评价不汇总;没有记录时
/// 不注入（AI 无素材,由基础角色的零编造约束兜底,不为此每次多花一段）。
/// knowledge_mentor / 知识库页对话模式跳过（与 recent_background 同口径）。
fn today_memory_background(store: &Store, conversation_id: &str) -> Result<Option<ContextMessage>> {
    let is_knowledge_mentor = store
        .get_conversation(conversation_id)?
        .is_some_and(|c| c.assistant_mode == "knowledge_mentor" || c.wiki_page_slug.is_some());
    if is_knowledge_mentor {
        return Ok(None);
    }
    let today = chrono::Local::now().date_naive();
    let mut entries: Vec<(String, String)> = store
        .daily_entries(today)?
        .into_iter()
        .map(|entry| (entry.recorded_at, entry.raw_text))
        .collect();
    if entries.is_empty() {
        return Ok(None);
    }
    // 记录时间正序呈现（今天一整天的时间线）。
    entries.reverse();
    let mut prompt = String::from(
        "今天（本地日期）记录下来的原始条目（真检索快照，按记录顺序；被问到今天记了什么时据实转述，只呈现不评价）：\n",
    );
    let mut count = 0;
    for (recorded_at, raw_text) in entries {
        let time = chrono::DateTime::parse_from_rfc3339(&recorded_at)
            .map(|dt| dt.with_timezone(&chrono::Local).format("%H:%M").to_string())
            .unwrap_or_else(|_| String::new());
        let text: String = raw_text.chars().take(160).collect();
        prompt.push_str(&format!("- {time} {text}\n"));
        count += 1;
        if count >= 12 {
            break;
        }
    }
    let message = ContextMessage::new("system", prompt).with_segment(
        SegmentPurpose::CrossConversation,
        SegmentOrigin::StoredMessage,
        Some("today-recorded-events"),
        OmissionPolicy::OptionalBackground,
    );
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

        // The count is a soft limit: keep the whole user exchange that crosses
        // it rather than starting the retained history inside a tool round.
        let desired = messages.len().saturating_sub(self.max_messages);
        let start = messages[..desired.min(messages.len())]
            .iter()
            .rposition(|message| message.role == "user")
            .filter(|_| {
                messages
                    .get(desired)
                    .is_none_or(|message| message.role != "user")
            })
            .unwrap_or(desired);
        let mut context: Vec<ContextMessage> = messages
            .into_iter()
            .skip(start)
            .map(ContextMessage::from_record)
            .collect();

        // 角色设定置于最前，防止 AI 泛化成通用聊天助手
        context.splice(0..0, build_system_context(store, conversation_id)?);
        let mut next_index = 1usize;
        if let Some(background) = recent_background(store, conversation_id)? {
            context.insert(next_index, background);
            next_index += 1;
        }
        if let Some(today) = today_memory_background(store, conversation_id)? {
            context.insert(next_index, today);
        }

        Ok(context)
    }
}

/// Sliding window memory: token-limited context
pub struct SlidingWindowMemory {
    max_tokens: usize,
    model: String,
}

/// Local BPE accounting for memory selection; each provider separately counts
/// the complete serialized request using its tokenizer family.
pub fn estimate_tokens(text: &str) -> usize {
    tiktoken_rs::cl100k_base_singleton()
        .encode_ordinary(text)
        .len()
}

/// Compact older conversation history using a preliminary memory estimate.
/// Required system context and the latest exchange are never truncated; the
/// provider enforces the actual budget on the complete serialized request.
pub fn compress_context(context: &mut Vec<ContextMessage>, max_tokens: usize) {
    compress_context_for_model(context, max_tokens, "gpt-4");
}

/// R5 确定性压形：装不下的较早历史轮降级为「骨架」——assistant 长回复
/// 留头 220 字、工具结果留头 160 字，用户消息与协议字段（tool_calls 的
/// id/name 配对）原样。不做模型调用（前台零延迟）；真正的语义纪要归
/// 翻篇摘要管线（apply_summary → PageMark）。压形后 content 与库内原文
/// 不再逐字一致，摘要管线的 complete_originals 护栏会自然跳过这些轮，
/// 避免把压形文本当全源再摘要。
fn degrade_turn(
    turn: &[ContextMessage],
    estimate_tokens: &dyn Fn(&str) -> usize,
) -> Vec<ContextMessage> {
    turn.iter()
        .map(|message| {
            if message.role == "user" || estimate_tokens(&message.content) < 400 {
                return message.clone();
            }
            let keep = if message.role == "tool" { 160 } else { 220 };
            let mut content: String = message.content.chars().take(keep).collect();
            content.push_str("…[后文已按预算压缩，完整原文见会话记录]");
            let mut degraded = message.clone();
            degraded.content = content;
            degraded
        })
        .collect()
}

pub(crate) fn compress_context_for_model(
    context: &mut Vec<ContextMessage>,
    max_tokens: usize,
    model: &str,
) {
    // Never make malformed native history appear valid by dropping its bad turn.
    // Preserve it intact so the provider's final gate reports the protocol error.
    if super::budget::validate_context_exchanges(context).is_err() {
        return;
    }
    let estimate_tokens = |text: &str| super::budget::text_tokens(model, text);
    if max_tokens == 0 {
        return;
    }
    let total: usize = context.iter().map(|m| estimate_tokens(&m.content)).sum();
    if total <= max_tokens {
        return;
    }
    let mut omissions: Vec<_> = context
        .iter_mut()
        .flat_map(|message| std::mem::take(&mut message.selection_omissions))
        .collect();
    // Optional background is always sacrificed before original conversation turns.
    context.retain(|message| {
        if message.optional_background {
            segments::note_omission(
                &mut omissions,
                &message.segment,
                model,
                estimate_tokens(&message.content),
                "preselection_optional_background",
            );
            false
        } else {
            true
        }
    });
    if context
        .iter()
        .map(|message| estimate_tokens(&message.content))
        .sum::<usize>()
        <= max_tokens
    {
        if let Some(first) = context.first_mut() {
            first.selection_omissions = omissions;
        }
        return;
    }

    let mut systems = context.iter().filter(|m| m.role == "system");
    let Some(primary) = systems.next().cloned() else {
        if let Some(first) = context.first_mut() {
            first.selection_omissions = omissions;
        }
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

    // Keep complete user exchanges. An assistant's native calls and their tool
    // results must never be separated by preliminary memory selection.
    //
    // 10-19 档案 P0-b（根因 A）：最近一个完整交换无条件保留（加上当前
    // 轮即 fit 侧「最近两轮不可省略」的同一口径），不参与预算装填。出事
    // 会话里 preselection 只留 7 条，模型不承认 3 分钟前刚做的确认
    // （invisible turn，run 还标 completed）。装不下就整体交给 fit 层
    // fail-fast（诚实失败），好过静默失忆后幻觉。
    let mut recent = Vec::new();
    for _ in 0..1 {
        if history.is_empty() {
            break;
        }
        let start = history.iter().rposition(|m| m.role == "user").unwrap_or(0);
        recent.push(history.split_off(start));
    }
    let mut kept = Vec::new();
    while !history.is_empty() {
        let start = history.iter().rposition(|m| m.role == "user").unwrap_or(0);
        let cost: usize = history[start..]
            .iter()
            .map(|m| {
                estimate_tokens(&m.content)
                    + m.tool_calls.as_ref().map_or(0, |calls| {
                        calls
                            .iter()
                            .map(|call| {
                                estimate_tokens(&call.name)
                                    + estimate_tokens(&call.arguments.to_string())
                                    + estimate_tokens(&call.id)
                            })
                            .sum::<usize>()
                    })
                    + m.reasoning_content
                        .as_ref()
                        .map_or(0, |text| estimate_tokens(text))
            })
            .sum();
        if cost > remaining {
            // R5：先尝试确定性压形再放弃——assistant/工具长消息大头被
            // 摘成骨架后常常能装下，模型在意的「做过什么」维度得以保留；
            // 仍装不下才整体交给 fit 层 fail-fast（诚实失败）。
            let tail = &history[start..];
            let degraded = degrade_turn(tail, &estimate_tokens);
            let degraded_cost: usize = degraded.iter().map(|m| estimate_tokens(&m.content)).sum();
            if degraded_cost >= cost || degraded_cost > remaining {
                break;
            }
            for message in tail {
                segments::note_omission(
                    &mut omissions,
                    &message.segment,
                    model,
                    estimate_tokens(&message.content),
                    "preselection_degraded_turn",
                );
            }
            kept.push(degraded);
            history.truncate(start);
            break;
        }
        remaining -= cost;
        kept.push(history.split_off(start));
    }
    if !history.is_empty() {
        for message in &history {
            segments::note_omission(
                &mut omissions,
                &message.segment,
                model,
                estimate_tokens(&message.content),
                "preselection_earlier_turn",
            );
        }
        // R8B：占位符显式给出被裁规模，模型知道「哪些内容不在」，不凭
        // 印象补全；截选不是语义摘要——语义纪要归 C06 翻篇管线。
        let omitted_marker = format!(
            "（前文省略：{} 条较早的历史消息未装入本轮上下文；只保留了你最近的活动，请勿凭记忆补全这段被省略的内容，被问到时应说明记不全）",
            history.len()
        );
        rebuilt.push(ContextMessage::new("system", omitted_marker).with_segment(
            SegmentPurpose::BudgetNotice,
            SegmentOrigin::Application,
            None,
            OmissionPolicy::Required,
        ));
    }
    for turn in kept.into_iter().rev() {
        rebuilt.extend(turn);
    }
    // recent[0] 是更早的交换、recent[1]（若有）紧贴当前轮，按时间序展开。
    for turn in recent.into_iter().rev() {
        rebuilt.extend(turn);
    }
    rebuilt.extend(current);
    if let Some(first) = rebuilt.first_mut() {
        first.selection_omissions = omissions;
    }
    *context = rebuilt;
}

impl SlidingWindowMemory {
    pub fn new(max_tokens: usize) -> Self {
        Self::for_model(max_tokens, "gpt-4")
    }

    pub(crate) fn for_model(max_tokens: usize, model: &str) -> Self {
        Self {
            max_tokens,
            model: model.to_owned(),
        }
    }

    /// Token 估算（ASCII 约 4 字符/token，CJK 约 2 字符/token）
    fn estimate_tokens(text: &str) -> usize {
        estimate_tokens(text)
    }
}

pub(crate) fn prepare_uncompressed_context(
    conversation_id: &str,
    store: &Store,
) -> Result<Vec<ContextMessage>> {
    let messages = store.list_messages(conversation_id)?;
    // 翻篇（P2 手动 / P3 自动档）：生效标记边界前的原文确定性跳过，
    // 摘要注入 system 之后、历史之前。边界消息若已不在会话中（被删/换
    // 设备截断），标记视为失效——退回全原文并落事件，不静默吞历史。
    let mut page_mark = store.active_page_mark(conversation_id)?;
    if let Some(mark) = page_mark.take() {
        let boundary_present = messages.iter().any(|m| m.id == mark.boundary_message_id);
        if boundary_present {
            page_mark = Some(mark);
        } else {
            super::generation::event(
                store,
                "page_mark_stale",
                json!({"mark_id":mark.id,"reason":"boundary_message_missing"}),
            )?;
        }
    }
    let history_start = page_mark
        .as_ref()
        .and_then(|mark| {
            messages
                .iter()
                .position(|m| m.id == mark.boundary_message_id)
                .map(|position| position + 1)
        })
        .unwrap_or(0);
    let mut context: Vec<_> = messages
        .into_iter()
        .skip(history_start)
        .map(ContextMessage::from_record)
        .collect();
    context.splice(0..0, build_system_context(store, conversation_id)?);
    if let Some(mark) = &page_mark {
        match super::summary::render_mark_text(store, mark) {
            Ok((text, _)) => {
                context.insert(
                    1.min(context.len()),
                    ContextMessage::new("system", text).with_segment(
                        SegmentPurpose::Summary,
                        SegmentOrigin::Derived,
                        Some(&mark.summary_node_id),
                        OmissionPolicy::Required,
                    ),
                );
            }
            Err(error) => {
                // 摘要失效（节点被删/覆盖不合法）不炸主链路：退回原文。
                super::generation::event(
                    store,
                    "page_mark_stale",
                    json!({"mark_id":mark.id,"reason":"summary_unusable","detail":error.to_string()}),
                )?;
            }
        }
    }
    // 动态背景段推到尾部（历史之后）：它们内容每轮都可能变，若插在
    // system 与历史之间，任何变化都会使其后的全部 KV 缓存失效——挪到
    // 尾部后，system+历史成为逐轮可复用的稳定前缀（provider 前缀缓存）。
    if let Some(background) = recent_background(store, conversation_id)? {
        context.push(background);
    }
    if let Some(today) = today_memory_background(store, conversation_id)? {
        context.push(today);
    }
    Ok(context)
}

impl MemoryProvider for SlidingWindowMemory {
    fn prepare_context(&self, conversation_id: &str, store: &Store) -> Result<Vec<ContextMessage>> {
        let messages = store.list_messages(conversation_id)?;

        let mut context: Vec<_> = messages
            .into_iter()
            .map(ContextMessage::from_record)
            .collect();
        context.splice(0..0, build_system_context(store, conversation_id)?);
        // 同 prepare_uncompressed_context：动态背景推尾部，护住前缀缓存。
        if let Some(background) = recent_background(store, conversation_id)? {
            context.push(background);
        }
        if let Some(today) = today_memory_background(store, conversation_id)? {
            context.push(today);
        }
        compress_context_for_model(&mut context, self.max_tokens, &self.model);

        Ok(context)
    }
}

#[cfg(test)]
#[path = "memory_tests/mod.rs"]
mod tests;
