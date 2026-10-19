-- 定时提醒（2026-10-19 提案 notes/proposed/architecture/2026-10-19-timed-reminders.md，
-- 存储设计评审已通过）：
-- 1) todos.reminder 标记列：区分「提醒」与普通待办。默认 0 保持存量行为不变；
--    快照 QUERY/apply 必须同步携带本列（snapshot.rs），否则新列静默不同步。
-- 2) todo_reminder_fires 本机触发去重表：同一条提醒在同一设备只弹一次。
--    本表**不进快照**（登记 SNAPSHOT_EXCLUDED_TABLES，理由 local-event-reminders，
--    与 rule_hits 同纪律：「提醒在哪台设备响是设备侧行为」）——多设备各弹各的。
--    注意：snooze（更新 due_at）必须与删除本表对应行同事务，否则
--    `due_at<=now AND 本机未触发过` 永假，snooze 后永不重响。
-- 3) idx_todos_due 部分索引：到期查询 status='open' AND due_at<=now
--    不被 idx_todos_status(status, created_at) 覆盖，补一行索引成本。
ALTER TABLE todos ADD COLUMN reminder INTEGER NOT NULL DEFAULT 0;

CREATE TABLE todo_reminder_fires (
    todo_id TEXT PRIMARY KEY REFERENCES todos(id) ON DELETE CASCADE,
    fired_at TEXT NOT NULL,
    device_id TEXT NOT NULL
);

CREATE INDEX idx_todos_due ON todos(status, due_at) WHERE due_at IS NOT NULL;
