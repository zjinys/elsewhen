-- 目标页内 AI 讨论会话（goal_mentor）。
--
-- 与 wiki 页内 AI 处理会话（wiki_page_slug + knowledge_mentor）同构：
-- 每条活跃目标一个专属会话，AI 围绕该目标的认知切片讨论，变更一律走
-- create_goal / update_goal_facets / pause_goal 等确认制工具。
--
-- * conversations.goal_id：目标专属会话绑定；普通会话为 NULL。
-- * assistant_mode 增加 'goal_mentor'：角色注入（system_context）与
--   工具面守卫（不记事件、不注入跨对话背景）按此分支。
-- * 历史会话不受影响：SQLite ALTER TABLE ADD COLUMN 带非常量默认值
--   在新库/老库上都按行求值，这里用常量 NULL 默认，旧行保持 NULL。
-- * 同步协议：ConversationRecord 序列化为 JSON 对象携带 goal_id 字段
--   （旧端收到带未知字段会拒绝——两端必须一起升级，与快照守卫同批）。

ALTER TABLE conversations ADD COLUMN goal_id TEXT REFERENCES goals(id);

CREATE INDEX idx_conversations_goal
    ON conversations(goal_id) WHERE goal_id IS NOT NULL;
