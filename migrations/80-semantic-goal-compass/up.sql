-- Local derived assessments and interaction state. Never part of a sync snapshot.
CREATE TABLE goal_compass_authority (
 singleton INTEGER PRIMARY KEY CHECK(singleton=1), generation INTEGER NOT NULL DEFAULT 0
);
INSERT INTO goal_compass_authority VALUES(1,0);
CREATE TABLE goal_compass_assessments (
 id TEXT PRIMARY KEY, goal_id TEXT NOT NULL, plan_id TEXT,
 input_fingerprint TEXT NOT NULL, input_json TEXT NOT NULL CHECK(json_valid(input_json)),
 prompt_bundle_id TEXT NOT NULL REFERENCES prompt_bundles(id), authority_generation INTEGER NOT NULL,
 status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','running','succeeded','cancelled','failed')),
 generation INTEGER NOT NULL DEFAULT 0, attempts INTEGER NOT NULL DEFAULT 0,
 lease_token TEXT, lease_until INTEGER, available_at INTEGER NOT NULL DEFAULT 0,
 output_json TEXT CHECK(output_json IS NULL OR json_valid(output_json)),
 usage_json TEXT CHECK(usage_json IS NULL OR json_valid(usage_json)), model TEXT, last_error TEXT,
 created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
 UNIQUE(goal_id,input_fingerprint)
);
CREATE INDEX goal_compass_assessment_queue ON goal_compass_assessments(goal_id,status,available_at,updated_at);
CREATE TABLE goal_compass_feedback (
 id TEXT PRIMARY KEY, assessment_id TEXT NOT NULL REFERENCES goal_compass_assessments(id),
 nudge_id TEXT NOT NULL, kind TEXT NOT NULL CHECK(kind IN ('still_progressing','changed_direction','snooze','discuss')),
 note TEXT NOT NULL DEFAULT '', created_at INTEGER NOT NULL
);
CREATE INDEX goal_compass_feedback_assessment ON goal_compass_feedback(assessment_id,created_at DESC);
ALTER TABLE goal_compass_nudges ADD COLUMN assessment_id TEXT REFERENCES goal_compass_assessments(id);
CREATE UNIQUE INDEX goal_compass_nudge_assessment ON goal_compass_nudges(assessment_id) WHERE assessment_id IS NOT NULL;
-- Pre-semantic pending cards have no verified evidence. Preserve their history
-- and existing cooldown, but stop presenting them as a semantic conclusion.
UPDATE goal_compass_nudges SET status='realigned' WHERE status='pending';
CREATE TRIGGER goal_compass_input_immutable BEFORE UPDATE OF goal_id,plan_id,input_fingerprint,input_json,prompt_bundle_id ON goal_compass_assessments
BEGIN SELECT RAISE(ABORT,'compass_input_immutable'); END;
CREATE TRIGGER goal_compass_result_immutable BEFORE UPDATE ON goal_compass_assessments WHEN OLD.status='succeeded'
BEGIN SELECT RAISE(ABORT,'compass_result_immutable'); END;
CREATE TRIGGER goal_compass_feedback_immutable BEFORE UPDATE ON goal_compass_feedback
BEGIN SELECT RAISE(ABORT,'compass_feedback_immutable'); END;
CREATE TRIGGER goal_compass_claim_guard BEFORE UPDATE OF status ON goal_compass_assessments
WHEN NEW.status IN ('running','succeeded') AND NOT (SELECT allowed FROM sync_background_permission)
BEGIN SELECT RAISE(ABORT,'sync_background_processor_required'); END;
CREATE TRIGGER goal_compass_goal_change AFTER UPDATE ON goals
BEGIN
 UPDATE goal_compass_authority SET generation=generation+1 WHERE singleton=1;
 UPDATE goal_compass_nudges SET status='realigned' WHERE goal_id=NEW.id AND status='pending' AND assessment_id IS NOT NULL;
END;
CREATE TRIGGER goal_compass_plan_change AFTER UPDATE ON goal_plans
BEGIN
 UPDATE goal_compass_authority SET generation=generation+1 WHERE singleton=1;
 UPDATE goal_compass_nudges SET status='realigned' WHERE plan_id=NEW.id AND status='pending' AND assessment_id IS NOT NULL;
END;
CREATE TRIGGER goal_compass_reclassification AFTER INSERT ON event_recordability_decisions
BEGIN
 UPDATE goal_compass_authority SET generation=generation+1 WHERE singleton=1;
 UPDATE goal_compass_nudges SET status='realigned' WHERE status='pending' AND assessment_id IS NOT NULL
 AND EXISTS(SELECT 1 FROM goal_compass_assessments a,json_each(a.input_json,'$.evidence') j
 WHERE a.id=assessment_id AND json_extract(j.value,'$.source_kind')='event' AND json_extract(j.value,'$.source_id')=NEW.event_id);
END;
CREATE TRIGGER goal_compass_feedback_fence AFTER INSERT ON goal_compass_feedback
BEGIN UPDATE goal_compass_authority SET generation=generation+1 WHERE singleton=1; END;

CREATE TRIGGER goal_compass_authority_sync_replica_insert AFTER INSERT ON sync_replica
BEGIN UPDATE goal_compass_authority SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER goal_compass_authority_sync_replica_update AFTER UPDATE OF dataset_id,device_id,stream_id,processor_device_id ON sync_replica
WHEN OLD.dataset_id IS NOT NEW.dataset_id OR OLD.device_id IS NOT NEW.device_id OR OLD.stream_id IS NOT NEW.stream_id OR OLD.processor_device_id IS NOT NEW.processor_device_id
BEGIN UPDATE goal_compass_authority SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER goal_compass_authority_sync_replica_delete AFTER DELETE ON sync_replica
BEGIN UPDATE goal_compass_authority SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER goal_compass_authority_sync_processor_state_insert AFTER INSERT ON sync_processor_state
BEGIN UPDATE goal_compass_authority SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER goal_compass_authority_sync_processor_state_update AFTER UPDATE OF state,from_device_id,to_device_id,final_sequence ON sync_processor_state
WHEN OLD.state IS NOT NEW.state OR OLD.from_device_id IS NOT NEW.from_device_id OR OLD.to_device_id IS NOT NEW.to_device_id OR OLD.final_sequence IS NOT NEW.final_sequence
BEGIN UPDATE goal_compass_authority SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER goal_compass_authority_sync_processor_state_delete AFTER DELETE ON sync_processor_state
BEGIN UPDATE goal_compass_authority SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER goal_compass_authority_sync_members_insert AFTER INSERT ON sync_members
BEGIN UPDATE goal_compass_authority SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER goal_compass_authority_sync_members_update AFTER UPDATE OF device_id,stream_id,active ON sync_members
WHEN OLD.device_id IS NOT NEW.device_id OR OLD.stream_id IS NOT NEW.stream_id OR OLD.active IS NOT NEW.active
BEGIN UPDATE goal_compass_authority SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER goal_compass_authority_sync_members_delete AFTER DELETE ON sync_members
BEGIN UPDATE goal_compass_authority SET generation=generation+1 WHERE singleton=1; END;

CREATE TRIGGER goal_compass_events_insert AFTER INSERT ON events
BEGIN
 UPDATE goal_compass_authority SET generation=generation+1 WHERE singleton=1;
 UPDATE goal_compass_nudges SET status='realigned' WHERE status='pending' AND assessment_id IS NOT NULL;
END;
CREATE TRIGGER goal_compass_events_delete AFTER DELETE ON events
BEGIN
 UPDATE goal_compass_authority SET generation=generation+1 WHERE singleton=1;
 UPDATE goal_compass_nudges SET status='realigned' WHERE status='pending' AND assessment_id IS NOT NULL;
END;
CREATE TRIGGER goal_compass_messages_insert AFTER INSERT ON messages
WHEN NEW.role='user'
BEGIN
 UPDATE goal_compass_authority SET generation=generation+1 WHERE singleton=1;
 UPDATE goal_compass_nudges SET status='realigned' WHERE status='pending' AND assessment_id IS NOT NULL;
END;
CREATE TRIGGER goal_compass_messages_update AFTER UPDATE ON messages
WHEN NEW.role='user'
BEGIN
 UPDATE goal_compass_authority SET generation=generation+1 WHERE singleton=1;
 UPDATE goal_compass_nudges SET status='realigned' WHERE status='pending' AND assessment_id IS NOT NULL;
END;
CREATE TRIGGER goal_compass_messages_delete AFTER DELETE ON messages
WHEN OLD.role='user'
BEGIN
 UPDATE goal_compass_authority SET generation=generation+1 WHERE singleton=1;
 UPDATE goal_compass_nudges SET status='realigned' WHERE status='pending' AND assessment_id IS NOT NULL;
END;
CREATE TRIGGER goal_compass_event_analyses_insert AFTER INSERT ON event_analyses
BEGIN
 UPDATE goal_compass_authority SET generation=generation+1 WHERE singleton=1;
 UPDATE goal_compass_nudges SET status='realigned' WHERE status='pending' AND assessment_id IS NOT NULL;
END;
CREATE TRIGGER goal_compass_event_analyses_delete AFTER DELETE ON event_analyses
BEGIN
 UPDATE goal_compass_authority SET generation=generation+1 WHERE singleton=1;
 UPDATE goal_compass_nudges SET status='realigned' WHERE status='pending' AND assessment_id IS NOT NULL;
END;
CREATE TRIGGER goal_compass_input_records_insert AFTER INSERT ON input_records
BEGIN
 UPDATE goal_compass_authority SET generation=generation+1 WHERE singleton=1;
 UPDATE goal_compass_nudges SET status='realigned' WHERE status='pending' AND assessment_id IS NOT NULL;
END;
CREATE TRIGGER goal_compass_input_records_update AFTER UPDATE ON input_records
BEGIN
 UPDATE goal_compass_authority SET generation=generation+1 WHERE singleton=1;
 UPDATE goal_compass_nudges SET status='realigned' WHERE status='pending' AND assessment_id IS NOT NULL;
END;
CREATE TRIGGER goal_compass_input_records_delete AFTER DELETE ON input_records
BEGIN
 UPDATE goal_compass_authority SET generation=generation+1 WHERE singleton=1;
 UPDATE goal_compass_nudges SET status='realigned' WHERE status='pending' AND assessment_id IS NOT NULL;
END;
CREATE TRIGGER goal_compass_goals_delete AFTER DELETE ON goals
BEGIN UPDATE goal_compass_authority SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER goal_compass_goal_plans_delete AFTER DELETE ON goal_plans
BEGIN UPDATE goal_compass_authority SET generation=generation+1 WHERE singleton=1; END;
