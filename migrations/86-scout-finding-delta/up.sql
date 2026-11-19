-- 字段级变化（仅 kind='changed'）：{"stargazers_count":{"from":5,"to":9,"diff":4}}
-- 服务端由 diff.ts 计算并随 /findings 返回；本地镜像原样保存。
--
-- 为什么不塞进 payload_json：payload 是「变化后长什么样」，delta 是
-- 「从什么样变成什么样」。UI 两者都要（气泡显示「+34 stars」用 diff，
-- 展示当前值用 payload）；混进 payload 会让 payload 不再干净，也会
-- 破坏「payload 就是该 item 快照」这个语义。NULL = 无 delta
-- （new/gone，或改造前的存量数据）。
ALTER TABLE scout_findings_mirror ADD COLUMN delta_json TEXT;
