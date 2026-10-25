-- review 2026-10-25（F5）三处结构性收口：
--   m5    拒收计数的主键缺信箱维度；
--   m4    relay 上的 blob 回收失败没有重试路径；
--   m10-a `sync_snapshots.transport` 没有取值约束（`exports` 有对称约束）；
--   m10-b `sync_wire_rejections_pending` 索引没有消费者。

-- ---------- m5：拒收计数必须按信箱分别算 ----------
--
-- relay 的每个信箱是**一个独立的 SQLite 文件**，`mail_id` 只是那个文件里的
-- rowid —— 换信箱（重新配对、vault 重建）之后，新信箱的第 1 封与旧信箱的第 1 封
-- 是同一个字符串。主键只有 `mail_id` 时，新信箱里一封普通信件会继承旧信箱那封
-- 坏信的拒绝次数，数到阈值（`MAX_WIRE_REJECTIONS`）就被「表态放弃」——
-- 一次重新配对足以让好信被当成毒信在 relay 上 drop 掉。
--
-- 重建表而不是 ADD COLUMN：主键没法 ALTER 进去。`dropped_at` 和审计列原样保留。
CREATE TABLE sync_wire_rejections_v2 (
    -- relay 的邮件 id（fetch 返回的游标语义 id），只在**同一个信箱**里唯一。
    mail_id TEXT NOT NULL,
    mailbox_id TEXT NOT NULL,
    sender_mailbox_id TEXT NOT NULL,
    message_id TEXT NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0 CHECK(attempts >= 0),
    last_error TEXT NOT NULL DEFAULT '',
    first_seen_at INTEGER NOT NULL,
    last_seen_at INTEGER NOT NULL,
    dropped_at INTEGER,
    PRIMARY KEY (mail_id, mailbox_id)
);
INSERT INTO sync_wire_rejections_v2(mail_id, mailbox_id, sender_mailbox_id, message_id,
                                    attempts, last_error, first_seen_at, last_seen_at, dropped_at)
    SELECT mail_id, mailbox_id, sender_mailbox_id, message_id,
           attempts, last_error, first_seen_at, last_seen_at, dropped_at
    FROM sync_wire_rejections ORDER BY rowid;
DROP TABLE sync_wire_rejections;
ALTER TABLE sync_wire_rejections_v2 RENAME TO sync_wire_rejections;

-- m10-b：`(mailbox_id, dropped_at) WHERE dropped_at IS NULL` 没有读者 ——
-- 阈值判定按主键走，审计清理（`prune_wire_rejections`）按 `last_seen_at` 走，
-- 这张表每次写入却要维护一个没人查的部分索引。删掉。
DROP INDEX IF EXISTS sync_wire_rejections_pending;

-- ---------- m4 + m10-a：快照接收侧 ----------
--
-- m4：`blob_close` 失败原来直接 `?` 冒出去，而发布 + 回执已经在同一个事务里
-- 提交完了 —— 行不再满足「在途导入」的查询条件，于是**再也没有下一拍去关它**，
-- relay 上那块 blob 只能等 30 分钟 TTL 兜底。加 `blob_closed_at`：非空才是
-- 「已回收」，NULL 的行每拍重试一次 close（close 在 relay 侧本来就是幂等的）。
--
-- m10-a：`transport` 只有 'mail' / 'blob' 两个值，写错一个字母的快照行会让
-- 「分片在哪个 blob 上」变成不可判定 —— 导出侧早就用 CHECK 挡住了，接收侧没有。
--
-- 这里用触发器而不是重建表加 CHECK，是踩过之后的结论：`sync_snapshots` 被
-- `sync_snapshot_chunks` 用外键引用，还被 `sync_background_permission` 视图与
-- 八个 `sync_*_claim_*` 触发器从 SQL 里引用。重建 = CREATE + COPY + DROP +
-- RENAME，而 `DROP TABLE` 在开启外键时会对子表行累加**延迟**违规计数，重新插入
-- 相同 id 并不会把这个计数减回去 —— 提交时报 `FOREIGN KEY constraint failed`
-- （见本迁移对应的 v63 升级测试）。官方 12 步要求先关 `foreign_keys`，那是
-- 连接级开关，不能在迁移事务内改。触发器给出同样的保证：非法值写不进去。
CREATE TRIGGER sync_snapshots_transport_insert BEFORE INSERT ON sync_snapshots
WHEN NEW.transport NOT IN ('mail','blob')
BEGIN SELECT RAISE(ABORT,'sync_snapshot_transport_invalid'); END;
CREATE TRIGGER sync_snapshots_transport_update BEFORE UPDATE OF transport ON sync_snapshots
WHEN NEW.transport NOT IN ('mail','blob')
BEGIN SELECT RAISE(ABORT,'sync_snapshot_transport_invalid'); END;

-- m4：relay 上的 blob 回收状态。NULL = 「还没关掉」，非空 = 已回收。
ALTER TABLE sync_snapshots ADD COLUMN blob_closed_at INTEGER;
