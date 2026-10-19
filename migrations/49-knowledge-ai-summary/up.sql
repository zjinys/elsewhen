-- 知识建议补一列挂载摘要（2026-10-18 事故「预算前移」配套）。
-- wiki_pages.summary 此前只是正文前 120 字符的机械预览，对话挂载时没有
-- 更短的独立版本可用，客户端只能整包携带 content_md 顶爆请求。编译管线
-- 改为让 AI 一并产出 1-2 句摘要；建议表随行保存，应用时落到 wiki_pages。
-- 建议存量行（summary=''）与 wiki_pages 存量页保持预览语义，由下一次
-- 编译/审阅随 AI 重生成覆盖；sync 不涉及本表（知识管线表均为本地产物）。
ALTER TABLE knowledge_proposals ADD COLUMN summary TEXT NOT NULL DEFAULT '';
