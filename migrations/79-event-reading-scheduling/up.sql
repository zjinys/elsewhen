ALTER TABLE event_reading_tasks ADD COLUMN available_at INTEGER NOT NULL DEFAULT 0;
ALTER TABLE event_reading_tasks ADD COLUMN last_error TEXT;
ALTER TABLE event_reading_tasks ADD COLUMN last_tick INTEGER NOT NULL DEFAULT 0;
CREATE TABLE event_reading_schedule (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    prefer_reading INTEGER NOT NULL CHECK(prefer_reading IN (0,1)),
    ticks INTEGER NOT NULL DEFAULT 0
);
INSERT INTO event_reading_schedule VALUES(1,0,0);
CREATE INDEX event_reading_tasks_available ON event_reading_tasks(status,available_at,updated_at);
