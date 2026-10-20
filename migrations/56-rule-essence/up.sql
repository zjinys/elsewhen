-- 规则语义内核（essence）：AI 在规则入库时提取的一句话语义标签，
-- 用于对话装配时的轻量匹配与注入。不替代 trigger_keywords 预筛，
-- 而是提供一条不依赖人工填关键词的语义匹配通道。
ALTER TABLE rules ADD COLUMN essence TEXT;
