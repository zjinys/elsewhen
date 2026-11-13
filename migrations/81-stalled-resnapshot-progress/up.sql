-- 重快照导出的最后进展时刻（毫秒）。mark_snapshot_blob_progress /
-- set_snapshot_blob_id / begin_snapshot_export 每次触达都刷新。
-- claim_sync_resnapshot 靠它识别「building 但没有任何人在推进」的卡死导出：
-- 超过 STALL 阈值就 abort，让续传/换代机制接管，而不是永久空转。
-- 2026-11-12 事故：GC 把配对中的设备标记重快照，building 导出无人续传，
-- 配对流程被 request id 校验每拍炸一次，双端互锁 17 小时。
ALTER TABLE sync_snapshot_exports ADD COLUMN progress_at INTEGER NOT NULL DEFAULT 0;
UPDATE sync_snapshot_exports SET progress_at = (SELECT strftime('%s','now')) WHERE state IN ('building','ready');
