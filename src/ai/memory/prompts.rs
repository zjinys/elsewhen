//! 常驻 system 提示与工具引导文案常量。

/// System prompt：定义 AI 在本应用中的角色，防止泛化成通用聊天助手。
/// 用户提到工具/产品只是事件背景，不是科普请求；回答要围绕个人记录与知识库。
pub(crate) const SYSTEM_PROMPT_BASE: &str = concat!(
    include_str!("../../../resources/prompts/chat-policy.zh-Hans.txt"),
    "\n",
    include_str!("../../../resources/prompts/chat-contract.zh-Hans.txt")
);
/// R2B 按需工具说明（stable_setup 瘦身）：tools 里任一工具是常驻（非
/// 延迟）时渲染为独立 system message；延迟工具（web_search、目录导入等）
/// 激活前不需要，其句式用法由 schema description 承担，不占常驻基底。
/// label 仅供测试定位，运行时不读。
pub(crate) const TOOL_GUIDES: &[(&str, &[&str], &str)] = &[
    (
        "web_search",
        &["web_search"],
        "时效信息用 web_search 查证；未配置则引导开启。",
    ),
    (
        "dir_import",
        &["import_directory_as_project", "import_directory_files"],
        "本机目录由用户给路径；项目导入先分析预览，文件导入经确认后保存素材。",
    ),
    (
        "people",
        &["propose_people_relations", "batch_extract_people_relations"],
        "重要联系人与关系先查重，再草拟确认；同名不同身份不合并。",
    ),
    (
        "events",
        &["list_events_by_date"],
        "查某天记录用 list_events_by_date，日期按当前时钟转 YYYY-MM-DD。",
    ),
    (
        "archive",
        &["archive_conversations_by_title"],
        "归档对话用 archive_conversations_by_title，先匹配预览再确认。",
    ),
    (
        "rename",
        &["rename_wiki_page"],
        "改页名用 rename_wiki_page；改正文用 save_wiki_revision。",
    ),
    (
        "topic_flow",
        &[
            "search_knowledge_base",
            "save_wiki_revision",
            "save_knowledge_draft",
        ],
        "讨论不自动建页；需保存结论时先检索，再草拟补充或新页并等待确认。",
    ),
    (
        "derivative",
        &["save_wiki_revision"],
        "加工页内容用 save_as=derivative；明确修订非锁定页才用 revision。",
    ),
];
