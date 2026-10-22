-- 习惯主动问候（2026-10-22 提案 notes/proposed/product/2026-10-22-proactive-habit-checkins.md）：
-- 1) habit_nudges：用户确认过的问候计划（问法/时间/按钮/记录句），随多设备同步。
-- 2) habit_nudge_fires：本机触发与回答状态，不进快照（多设备各问各的）。
-- 3) messages.kind/ref：把问候和回答标记成独立消息类型，旧消息默认 chat。
CREATE TABLE habit_nudges (
    id           TEXT PRIMARY KEY,
    page_slug    TEXT NOT NULL,
    question     TEXT NOT NULL,
    done_label   TEXT NOT NULL,
    done_text    TEXT NOT NULL,
    missed_label TEXT NOT NULL,
    missed_text  TEXT NOT NULL,
    weekdays     INTEGER NOT NULL,
    local_time   TEXT NOT NULL,
    enabled      INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0,1)),
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);

-- v1 一个习惯页只保留一条计划（UI 只暴露一条；后续多条时去掉唯一索引即可）。
CREATE UNIQUE INDEX idx_habit_nudges_page ON habit_nudges(page_slug);
CREATE INDEX idx_habit_nudges_enabled ON habit_nudges(enabled);

CREATE TABLE habit_nudge_fires (
    nudge_id     TEXT NOT NULL,
    local_date   TEXT NOT NULL,
    fired_at     TEXT NOT NULL,
    ask_count    INTEGER NOT NULL DEFAULT 1,
    snooze_until TEXT,
    response     TEXT,
    device_id    TEXT NOT NULL,
    message_id   TEXT,
    PRIMARY KEY (nudge_id, local_date)
);

ALTER TABLE messages ADD COLUMN kind TEXT NOT NULL DEFAULT 'chat';
ALTER TABLE messages ADD COLUMN ref TEXT;

CREATE INDEX idx_messages_ref ON messages(ref) WHERE ref IS NOT NULL;
CREATE INDEX idx_messages_kind ON messages(kind) WHERE kind != 'chat';
