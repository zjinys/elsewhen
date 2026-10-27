-- 快照只走 blob 数据面（2026-10-26 C5-A）。
--
-- `mail` 分片传输已从代码里删除：几百封信推进 `sync_wire_outbox`、靠「整份塞进
-- outbox + 重发兜底」收敛，实测 0.38 MB/s、上线 2.67×，而 blob 数据面是 4.1 MB/s、
-- 1.33×。删代码之后，留在库里的 `transport='mail'` 行变成**永不可达**的数据：
-- 发送端不会再为它发一片，接收端也不会再为它拉一片。它们不只是垃圾 ——
--   1. 导出侧：`state='ready'` 的 mail 导出会永久占着 `snapshot_allowed` 的语义位，
--      面板永远停在「正在发送初始快照」；
--   2. 接收侧：`state='staging'` 的 mail 快照让 `ensure_empty_snapshot_target()`
--      一直为假，下一次快照连 staging 都建不起来。
-- 所以这里直接清掉在途的（`ready`/`building`/`aborted` 导出、`staging`/`aborted`
-- 导入）以及它们的信箱副本。已 `applied`/`acknowledged` 的历史行不动：它们描述
-- 的是已经落地的事实，删掉没有任何收益，而且 `sync_snapshots` 被视图与触发器从
-- SQL 里引用，能少动就少动。

-- ---------- 1. 信箱里已无主的分片信封 ----------
--
-- 只认 `snapshot:{id}:{n}` 这个形状：它是 mail 分片专用的消息 id，新代码不会再
-- 生成，也不会再有人 ack 它（`snapshot-end:` 是 manifest，仍由代码使用，留给
-- 第 2 步按导出状态精确处理）。
--
-- 顺序不能反：`sync_snapshot_export_wires` 与 `sync_operation_fragment_outbox`
-- 都有指向 `sync_wire_outbox` 的外键，先删父行会被外键拒掉。分片不在
-- `sync_operation_fragment_outbox` 里（那是业务操作分片），所以只需先清导出登记。
DELETE FROM sync_snapshot_export_wires WHERE message_id LIKE 'snapshot:%';
DELETE FROM sync_wire_outbox WHERE message_id LIKE 'snapshot:%';

-- ---------- 2. 在途的 mail 导出 ----------
--
-- 先把要退休的信封 id 抄进临时表：`sync_snapshot_export_wires` 有指向
-- `sync_wire_outbox` 的外键，只能先删子行再删父行，而子行删完就查不到「这些
-- 信封属于谁」了。
CREATE TEMP TABLE mail_export_wires AS
    SELECT x.recipient_device_id AS recipient_device_id, x.message_id AS message_id
    FROM sync_snapshot_export_wires x
    JOIN sync_snapshot_exports s ON s.id = x.snapshot_id
    WHERE s.transport = 'mail' AND s.state IN ('building','ready','aborted');
DELETE FROM sync_snapshot_export_wires
WHERE snapshot_id IN (SELECT id FROM sync_snapshot_exports
                      WHERE transport = 'mail' AND state IN ('building','ready','aborted'));
DELETE FROM sync_wire_outbox
WHERE EXISTS (SELECT 1 FROM mail_export_wires t
              WHERE t.recipient_device_id = sync_wire_outbox.recipient_device_id
                AND t.message_id = sync_wire_outbox.message_id);
DROP TABLE mail_export_wires;
DELETE FROM sync_snapshot_exports
WHERE transport = 'mail' AND state IN ('building','ready','aborted');

-- ---------- 3. 接收侧的 mail staging ----------
--
-- 先放手：`sync_snapshot_target` 指向它、`sync_snapshot_chunks` 引用它。目标行是
-- 单例（下一次 staging 会用 `ON CONFLICT(singleton) DO UPDATE` 重建），直接删行
-- 比留一个 `''` 哨兵更干净 —— 视图都是 `JOIN ... ON t.snapshot_id=s.id`，空串与
-- 不存在的行等价，但没人愿意猜 `''` 是什么意思。
DELETE FROM sync_snapshot_target
WHERE snapshot_id IN (SELECT id FROM sync_snapshots WHERE transport = 'mail');
DELETE FROM sync_snapshot_chunks WHERE snapshot_id IN (
    SELECT id FROM sync_snapshots WHERE transport = 'mail'
);
DELETE FROM sync_snapshots WHERE transport = 'mail' AND state IN ('staging','aborted');

-- ---------- 4. 不允许再写 mail ----------
--
-- 两列的 DEFAULT 都是 `'mail'`（60-snapshot-transport 加的）。SQLite 改不了列
-- 默认值（要重建表，而这两张表被外键与八个 `sync_*_claim_*` 触发器引用），所以
-- 用触发器把「忘了写 transport」变成**写入即失败**：新代码处处显式写 `'blob'`，
-- 漏写就是 bug，不该被默认值悄悄兜成一条已删除的传输方式。
CREATE TRIGGER sync_snapshots_no_mail_insert BEFORE INSERT ON sync_snapshots
WHEN NEW.transport IS NOT 'blob'
BEGIN SELECT RAISE(ABORT,'sync_snapshot_transport_removed'); END;
CREATE TRIGGER sync_snapshots_no_mail_update BEFORE UPDATE OF transport ON sync_snapshots
WHEN NEW.transport IS NOT 'blob'
BEGIN SELECT RAISE(ABORT,'sync_snapshot_transport_removed'); END;
CREATE TRIGGER sync_snapshot_exports_no_mail_insert BEFORE INSERT ON sync_snapshot_exports
WHEN NEW.transport IS NOT 'blob'
BEGIN SELECT RAISE(ABORT,'sync_snapshot_transport_removed'); END;
CREATE TRIGGER sync_snapshot_exports_no_mail_update BEFORE UPDATE OF transport ON sync_snapshot_exports
WHEN NEW.transport IS NOT 'blob'
BEGIN SELECT RAISE(ABORT,'sync_snapshot_transport_removed'); END;
