-- 投递幂等键：relay 见同一 (sender_mb, seq_hint) 会把旧行替换掉，而不是再插一行。
-- 没有这个键，客户端每次重发（丢响应、租约恢复、每拍补发）都在 relay 上留一份
-- 新副本，实测把同一份负载放大到 4.4×，是配额被打爆的直接原因。
--
-- 为什么是「单调计数器」而不是内容哈希或 rowid：
--   1) 内容哈希：碰撞就等于丢信（relay 会把新信当旧信替换掉），不可接受；
--   2) rowid：outbox 行被清理后 rowid 会被复用，复用的 seq 撞上仍在 relay 上的
--      旧邮件时同样会丢信；
--   3) 单调计数器由本表独占发号，永不重复，且发号动作与插入同一事务。
-- 老行按 rowid 回填：既稳定又唯一，让在途的 legacy 行也能享受去重。
ALTER TABLE sync_wire_outbox ADD COLUMN seq_hint INTEGER NOT NULL DEFAULT 0;
UPDATE sync_wire_outbox SET seq_hint = rowid;
-- 配额阻塞原因：relay 回 507 时记 'relay_quota'，让配对面板能说清"不是卡在
-- 传输，是在等空间"，也让恢复后能清掉。普通失败（超时/网络）不写这里。
ALTER TABLE sync_wire_outbox ADD COLUMN blocked_reason TEXT;
CREATE TABLE sync_wire_seq (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    next      INTEGER NOT NULL
);
INSERT INTO sync_wire_seq(singleton,next)
VALUES (1, (SELECT COALESCE(MAX(rowid),0)+1 FROM sync_wire_outbox));
