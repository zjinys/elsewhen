-- 26: DoH 启用策略三态（off / auto / force），把「启用」与「地址」解耦。
--
-- 之前 v25 用「doh_uri 留空 = 关」把启用与地址耦合：用户没法「填了地址但暂时关掉」，
-- 也没法表达「失败后要不要回退直连」。改为显式三态：
--   'off'   强制系统直连，忽略已填的 DoH 地址；
--   'auto'  填了地址就优先 DoH+ECH，连不上自动回退直连（日常默认，可达性与可用性兼顾）；
--   'force' 只用 DoH+ECH，失败即报错、不悄悄降级（受限网络下必须走隧道时用）。
-- 默认值取 'auto'：已有 v25 用户填了地址就维持「开了就走 DoH」的旧行为，语义最接近。

ALTER TABLE network_settings
    ADD COLUMN doh_mode TEXT NOT NULL DEFAULT 'auto'
    CHECK(doh_mode IN ('off','auto','force'));
