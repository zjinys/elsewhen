-- 翻篇标记（P2 手动 / P3 自动档）：
-- 摘要边界持久化——标记「boundary 及更早的历史已由摘要固化」，装配时
-- 边界前原文确定性跳过、摘要注入，替代每轮重复的即时摘要。
-- 摘要本体仍存 chat_summary_nodes（复用其 span/basis 验证管道）；
-- override_json 是用户预览编辑后的 SummaryBody（优先于节点原文渲染）。
-- active 即生效开关：撤销 = 置 0（消息表原文不动，只改装配视图；
-- 已写入 wiki 的知识页不回滚）。auto 同会话只留最新一份。
CREATE TABLE conversation_page_marks (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    boundary_message_id TEXT NOT NULL,
    summary_node_id TEXT NOT NULL,
    override_json TEXT,
    kind TEXT NOT NULL CHECK (kind IN ('manual', 'auto')),
    active INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (conversation_id, boundary_message_id)
);
CREATE INDEX conversation_page_marks_conversation
    ON conversation_page_marks (conversation_id, active, created_at);
