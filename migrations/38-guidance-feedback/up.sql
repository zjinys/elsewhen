-- 闭环度量（遗留收口切片 B）：guidance 采纳反馈。
-- * guidance 不是独立对象（D6 定案）：它是 event_analyses.result_json 的
--   $.guidance.related[] 条目，条目无稳定 id。定位键用
--   (analysis_id, slug, note) 三元组——analysis id 唯一确定一次分析，
--   slug/note 是该条目在知识页维度上的用户可见文本；同文本重复条目
--   共享一行反馈（同文本同反馈，无语义损失）。
-- * 与 rule_hits feedback 同纪律：只记录不推断——没有任何自动行为
--   消费它（不外推、不调阈值、不改分析）；表本身排除于快照
--   （反馈是本机度量，analysis 随快照走但反馈不跨设备复制）。
-- * 可清：删除行即清除（toggle 语义，与 rule_hit 再点一次清除一致）。

CREATE TABLE guidance_feedback (
    analysis_id TEXT NOT NULL REFERENCES event_analyses(id) ON DELETE CASCADE,
    slug TEXT NOT NULL,
    note TEXT NOT NULL DEFAULT '',
    feedback TEXT NOT NULL CHECK(feedback IN ('helpful','not_relevant')),
    created_at TEXT NOT NULL,
    PRIMARY KEY (analysis_id, slug, note)
);

CREATE INDEX idx_guidance_feedback_slug ON guidance_feedback(slug);
