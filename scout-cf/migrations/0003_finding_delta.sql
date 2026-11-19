-- 字段级变化（仅 kind='changed'）：{"stars":{"from":5,"to":9,"diff":4}, ...}
--
-- 为什么单独一列而不是塞进 payload_json：payload 是「变化后长什么样」，
-- delta 是「从什么样变成什么样」。消费端两件事都要（气泡里显示「+34 stars」
-- 需要 diff，显示当前值需要 payload）；混进 payload 会让 payload 不再是
-- 该 item 的干净快照，也会污染 itemHash 的语义。NULL = 该 finding 没有
-- delta（new/gone，或老数据）。
ALTER TABLE scout_findings ADD COLUMN delta_json TEXT;
