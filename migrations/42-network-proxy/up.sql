-- 出站代理配置（设置/网络）：与 DoH 同表（network_settings，singleton），
-- 同属「网络层决策」。语义：
--   proxy_mode = off    强制直连（屏蔽环境变量代理——用户显式关）
--   proxy_mode = auto   跟随环境变量（HTTPS_PROXY/ALL_PROXY，默认；与既有行为一致）
--   proxy_mode = custom 用 proxy_url（覆盖环境变量）——http:// 或 socks5://
-- 变更时清空 AI provider 的共享 client 池（client 长持有，不清不生效）。
ALTER TABLE network_settings ADD COLUMN proxy_mode TEXT NOT NULL DEFAULT 'auto';
ALTER TABLE network_settings ADD COLUMN proxy_url TEXT NOT NULL DEFAULT '';
