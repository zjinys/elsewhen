-- 对话影子校验（规则生命周期与主对话解耦·第三件）：聊天轮次不落事件卷宗，
-- 但主回复完成后按 trigger_keywords 预筛用户消息，命中的规则异步对照判定，
-- 产出 conversation 侧命中记录。与迁移 35 的事件链同构（预筛 pending →
-- 后台 LLM 判定 → 状态回填），但不复用 rule_hits：rule_hits 锚定 events
-- （FK），聊天消息不进 events，别把两套语义混在一张表上。
--
-- 红线：影子校验绝不改写主回复、不向对话注入内容；产出只落本表，展示走
-- 规则管理页审查面。预筛失败静默（预警不阻塞对话，同 screen_event_on 惯例）。
CREATE TABLE chat_rule_hits (
  id TEXT PRIMARY KEY,
  rule_id TEXT NOT NULL REFERENCES rules(id) ON DELETE CASCADE,
  conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
  input_message_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
  status TEXT NOT NULL DEFAULT 'pending'
    CHECK(status IN ('pending','applicable','excluded')),
  confidence REAL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE(rule_id, input_message_id)
);
CREATE INDEX idx_chat_rule_hits_status ON chat_rule_hits(status, created_at);
CREATE INDEX idx_chat_rule_hits_rule ON chat_rule_hits(rule_id, created_at);
