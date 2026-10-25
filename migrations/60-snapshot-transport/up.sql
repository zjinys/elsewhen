-- 快照传输方式落表（R4-6）。
--
-- 为什么必须落表而不是每次读环境变量：一份导出在途时可能跨进程重启、跨多个
-- tick、回执也可能在几天后才到。发送端用 mail、接收端却按 blob 拉，或者反过来
-- 中途切换传输方式，都会让「这份快照到底在哪」变成不可判定 —— 快照是唯一会
-- 触发数据库整体替换的路径，不允许出现这种不确定性。
--
-- 因此：`transport` 在 begin_export 时写入，此后该导出的续传、回执、关闭全部
-- 只认行里的值；回滚 = 下一次快照（或重新配对），不做运行时嗅探。
ALTER TABLE sync_snapshot_exports ADD COLUMN transport TEXT NOT NULL DEFAULT 'mail'
    CHECK(transport IN ('mail','blob'));
-- blob 上传进度（已传完的片数）。blob 传输是流式分片上传，一拍传不完一份 122 MB
-- 的快照，必须能跨拍续传。续传时 exporter 直接跳到这一片继续，不重封不重传。
ALTER TABLE sync_snapshot_exports ADD COLUMN blob_index INTEGER NOT NULL DEFAULT 0
    CHECK(blob_index >= 0);
-- 在途 blob 的身份。sender 侧是它 open 出来的 id，接收端从 manifest 里读同一个值；
-- 存在导出行上让「重试」复用同一个 blob，而不是每次都开一个新的（旧的会靠 TTL 收）。
ALTER TABLE sync_snapshot_exports ADD COLUMN blob_id TEXT;
-- 建立导出时的逻辑时钟。blob 传输跨拍续传，重开导出器前要确认源数据一格没动：
-- 漂了就换新请求 id，绝不把半新半旧的分片拼成一份快照（manifest 摘要对不上，
-- 接收端会整份作废）。
ALTER TABLE sync_snapshot_exports ADD COLUMN blob_clock INTEGER NOT NULL DEFAULT -1;

-- 接收端也要知道「这份快照的分片在哪个 blob 上」，否则它只能等 manifest 到齐
-- 才敢动，而 manifest 要等最后一片才生成 —— 窗口流控会死锁（发送方等 ack，
-- 接收方等 manifest）。这两列由 `snapshot-start` 信令写入，manifest 到达时
-- 再校验一次，所以它们只是「早到的线索」，不是权威。
ALTER TABLE sync_snapshots ADD COLUMN transport TEXT NOT NULL DEFAULT 'mail';
ALTER TABLE sync_snapshots ADD COLUMN blob_id TEXT;
