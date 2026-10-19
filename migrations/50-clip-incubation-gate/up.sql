-- 摘录孵化闭环（提案 2026-10-19-clip-incubation-and-negative-knowledge）。
-- 1) 负知识承载：knowledge_sources 加 reject_reason——丢弃（reject）时用户
--    写下的「为什么否定」成为一等公民数据，再导入同源（identity 命中）时
--    提示「你否定过这条来源」。不新建表；表随快照走（见 snapshot.rs 同步
--    登记变更），负知识跨设备可见。
-- 2) 孵化闸门：source/note 页 opinion 缺省语义从「NULL=缺省认可」收紧为
--    「NULL=孵化中」（不参与素材编译与自动召回）。存量行回填 endorse
--    保住现有行为——闸门只对「新导入、未定论」生效。knowledge_sources
--    全表即导入产物（kind 均为 source 侧），NULL 一并回填。
--    按需读取（get_wiki_page 工具、用户显式 @wiki 引用）不受闸门限制，
--    豁免面在 citation_for_page_explicit 通道与各显式入口兑现。
ALTER TABLE knowledge_sources ADD COLUMN reject_reason TEXT;

UPDATE wiki_pages SET opinion='endorse'
WHERE kind IN ('source','note') AND opinion IS NULL;

UPDATE knowledge_sources SET opinion='endorse' WHERE opinion IS NULL;
