-- 「必表态」契约（R5-2）。
--
-- 事故里 9996 封信堆到 relay 的 MAX_MAILS：接收端每一圈都扫到它们、每一圈都
-- 拒绝、每一圈都不 ack（因为 ack 的语义是「已消费」，而这些信永远不会被消费），
-- 于是只剩 7 天 TTL 能清掉它们。
--
-- 这里记的是「同一封邮件被扫描拒绝了几次」。达到阈值后客户端必须表态：
-- 要么 drop（relay 立即删，本地留审计），要么升级为 blocked 上 UI。
-- `dropped_at` 非空 = 已经表态放弃，这一行从此只是审计（哪个 subject、为什么、
-- 几次之后放弃），不再参与判定。
--
-- 为什么要落库而不是放内存：扫描游标本身是持久的，进程重启后同一封坏信会被
-- 重新扫到；计数放内存就等于每次重启重新开始数，阈值永远到不了。
CREATE TABLE sync_wire_rejections (
    -- relay 的邮件 id（fetch 返回的游标语义 id），删信后这一行仍留着做审计。
    mail_id TEXT PRIMARY KEY,
    mailbox_id TEXT NOT NULL,
    sender_mailbox_id TEXT NOT NULL,
    -- 信封头里的业务 message_id：人看不懂 mail_id，排查时要靠它。
    message_id TEXT NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0 CHECK(attempts >= 0),
    -- 最后一次的失败码（如 sync_protocol_unsupported），修好部署后据此判断是否同类。
    last_error TEXT NOT NULL DEFAULT '',
    first_seen_at INTEGER NOT NULL,
    last_seen_at INTEGER NOT NULL,
    -- 非空 = 已表态放弃（drop 成功或 relay 已无此信）。
    dropped_at INTEGER
);
CREATE INDEX sync_wire_rejections_pending
    ON sync_wire_rejections (mailbox_id, dropped_at)
    WHERE dropped_at IS NULL;
