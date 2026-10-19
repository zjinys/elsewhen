-- 知识页持续更新 P2/P3（提案 docs/notes/proposed/architecture/
-- 2026-10-19-knowledge-page-reconsolidation-and-relations.md，评审瘦身后范围）。
-- 两张表都是本机管道产物：入 SNAPSHOT_EXCLUDED_TABLES（local-knowledge-pipeline），
-- 不进快照也不产生 op；边可由内容重算，人工边将来按 origin='human' 提升。
--
-- 1) knowledge_reconsolidation_jobs：页面再汇总任务（P2）。
--    worker 复用 wiki::reconsolidate_page —— 触发阈值是**派生读**：
--    「整理点之后新到的来源事件数」直接由 events.recorded_at 对比
--    wiki_revisions 最新时间算出，本表不存证据基线列（P1 同口径）。
--    last_evidence_count 只做完成时的快照，供观测与防抖参考。
CREATE TABLE knowledge_reconsolidation_jobs (
  page_id TEXT PRIMARY KEY REFERENCES wiki_pages(id) ON DELETE CASCADE,
  trigger TEXT NOT NULL CHECK(trigger IN ('evidence_delta','scheduled','manual','related')),
  status TEXT NOT NULL DEFAULT 'pending'
    CHECK(status IN ('pending','running','retry','succeeded','skipped')),
  generation INTEGER NOT NULL DEFAULT 0,
  attempts INTEGER NOT NULL DEFAULT 0,
  available_at TEXT NOT NULL,
  last_completed_at TEXT,
  last_evidence_count INTEGER,
  detail TEXT
);

-- 2) knowledge_relations：页与页语义关联边（P3 瘦身版）。
--    v1 只写确定性派生（origin='citation'，tags 命中标题）与人工边；
--    不做 AI 写边、级联刷新与全图遍历（关系网找上下文，依赖网传播变更）。
CREATE TABLE knowledge_relations (
  id TEXT PRIMARY KEY,
  from_page_id TEXT NOT NULL REFERENCES wiki_pages(id) ON DELETE CASCADE,
  to_page_id TEXT NOT NULL REFERENCES wiki_pages(id) ON DELETE CASCADE,
  relation TEXT NOT NULL CHECK(relation IN
    ('related','part_of','mentions','supports','contradicts','same_topic')),
  note TEXT,
  confidence INTEGER NOT NULL DEFAULT 3,
  origin TEXT NOT NULL CHECK(origin IN
    ('ai_digest','ai_reconsolidation','human','citation')),
  source_event_ids TEXT NOT NULL DEFAULT '[]',
  created_at TEXT NOT NULL,
  last_confirmed_at TEXT NOT NULL,
  UNIQUE(from_page_id, to_page_id, relation)
);
CREATE INDEX idx_knowledge_relations_from ON knowledge_relations(from_page_id);
CREATE INDEX idx_knowledge_relations_to   ON knowledge_relations(to_page_id);
