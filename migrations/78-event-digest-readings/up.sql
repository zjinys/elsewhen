-- Local derived work only: never exported as business events or prompt bundles.
CREATE TABLE event_reading_authority (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    generation INTEGER NOT NULL DEFAULT 0 CHECK(generation>=0)
);
INSERT INTO event_reading_authority(singleton) VALUES(1);
CREATE TABLE event_reading_tasks (
    id TEXT PRIMARY KEY,
    event_id TEXT NOT NULL REFERENCES events(id),
    digest_version TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    total_chars INTEGER NOT NULL CHECK(total_chars BETWEEN 1 AND 2000000),
    bundle_id TEXT NOT NULL REFERENCES prompt_bundles(id),
    strategy TEXT NOT NULL,
    dependency_hash TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','ready','succeeded','cancelled')),
    generation INTEGER NOT NULL DEFAULT 0 CHECK(generation>=0),
    lease_token TEXT,
    lease_until INTEGER,
    authority_generation INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE(event_id,digest_version,content_hash,bundle_id,strategy),
    CHECK((lease_token IS NULL)=(lease_until IS NULL))
);
CREATE TABLE event_reading_nodes (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES event_reading_tasks(id),
    kind TEXT NOT NULL CHECK(kind IN ('leaf','merge')),
    level INTEGER NOT NULL CHECK(level BETWEEN 0 AND 16),
    start_char INTEGER NOT NULL CHECK(start_char>=0),
    end_char INTEGER NOT NULL CHECK(end_char>start_char),
    input_hash TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','running','succeeded')),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK(attempts>=0),
    available_at INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    output_json TEXT CHECK(output_json IS NULL OR (json_valid(output_json) AND length(CAST(output_json AS BLOB))<=16384)),
    output_hash TEXT,
    receipt_token TEXT,
    receipt_generation INTEGER,
    UNIQUE(task_id,kind,level,start_char,end_char),
    UNIQUE(task_id,id),
    CHECK((kind='leaf' AND level=0) OR (kind='merge' AND level>0)),
    CHECK((status='succeeded')=(output_json IS NOT NULL))
);
CREATE TABLE event_reading_dependencies (
    task_id TEXT NOT NULL,
    parent_id TEXT NOT NULL,
    position INTEGER NOT NULL CHECK(position BETWEEN 0 AND 7),
    child_id TEXT NOT NULL,
    output_hash TEXT NOT NULL,
    PRIMARY KEY(parent_id,position),
    UNIQUE(task_id,child_id),
    FOREIGN KEY(task_id,parent_id) REFERENCES event_reading_nodes(task_id,id),
    FOREIGN KEY(task_id,child_id) REFERENCES event_reading_nodes(task_id,id),
    CHECK(parent_id<>child_id)
);
CREATE INDEX event_reading_tasks_queue ON event_reading_tasks(status,updated_at);
CREATE INDEX event_reading_nodes_queue ON event_reading_nodes(task_id,status,available_at);
CREATE TRIGGER event_reading_task_identity BEFORE UPDATE OF event_id,digest_version,content_hash,total_chars,bundle_id,strategy,dependency_hash ON event_reading_tasks
BEGIN SELECT RAISE(ABORT,'event_reading_identity_immutable'); END;
CREATE TRIGGER event_reading_node_identity BEFORE UPDATE OF task_id,kind,level,start_char,end_char,input_hash ON event_reading_nodes
BEGIN SELECT RAISE(ABORT,'event_reading_node_immutable'); END;
CREATE TRIGGER event_reading_result_immutable BEFORE UPDATE ON event_reading_nodes WHEN OLD.status='succeeded'
BEGIN SELECT RAISE(ABORT,'event_reading_result_immutable'); END;
CREATE TRIGGER event_reading_dependency_immutable BEFORE UPDATE ON event_reading_dependencies
BEGIN SELECT RAISE(ABORT,'event_reading_dependency_immutable'); END;
-- Any reclassification invalidates in-flight work, including false -> true
-- before the old network response arrives. Completed cache remains local.
CREATE TRIGGER event_reading_reclassification AFTER INSERT ON event_recordability_decisions
BEGIN
 UPDATE event_reading_tasks SET status='cancelled',generation=generation+1,lease_token=NULL,lease_until=NULL
 WHERE event_id=NEW.event_id AND status IN ('active','ready');
END;
CREATE TRIGGER event_reading_authority_sync_replica_insert AFTER INSERT ON sync_replica
BEGIN UPDATE event_reading_authority SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER event_reading_authority_sync_replica_update AFTER UPDATE OF dataset_id,device_id,stream_id,processor_device_id ON sync_replica
WHEN OLD.dataset_id IS NOT NEW.dataset_id OR OLD.device_id IS NOT NEW.device_id OR OLD.stream_id IS NOT NEW.stream_id OR OLD.processor_device_id IS NOT NEW.processor_device_id
BEGIN UPDATE event_reading_authority SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER event_reading_authority_sync_replica_delete AFTER DELETE ON sync_replica
BEGIN UPDATE event_reading_authority SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER event_reading_authority_sync_processor_state_insert AFTER INSERT ON sync_processor_state
BEGIN UPDATE event_reading_authority SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER event_reading_authority_sync_processor_state_update AFTER UPDATE OF state,from_device_id,to_device_id,final_sequence ON sync_processor_state
WHEN OLD.state IS NOT NEW.state OR OLD.from_device_id IS NOT NEW.from_device_id OR OLD.to_device_id IS NOT NEW.to_device_id OR OLD.final_sequence IS NOT NEW.final_sequence
BEGIN UPDATE event_reading_authority SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER event_reading_authority_sync_processor_state_delete AFTER DELETE ON sync_processor_state
BEGIN UPDATE event_reading_authority SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER event_reading_authority_sync_members_insert AFTER INSERT ON sync_members
BEGIN UPDATE event_reading_authority SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER event_reading_authority_sync_members_update AFTER UPDATE OF device_id,stream_id,active ON sync_members
WHEN OLD.device_id IS NOT NEW.device_id OR OLD.stream_id IS NOT NEW.stream_id OR OLD.active IS NOT NEW.active
BEGIN UPDATE event_reading_authority SET generation=generation+1 WHERE singleton=1; END;
CREATE TRIGGER event_reading_authority_sync_members_delete AFTER DELETE ON sync_members
BEGIN UPDATE event_reading_authority SET generation=generation+1 WHERE singleton=1; END;

CREATE TRIGGER event_reading_claim_guard BEFORE UPDATE OF lease_token ON event_reading_tasks
WHEN NEW.lease_token IS NOT NULL AND NOT (SELECT allowed FROM sync_background_permission)
BEGIN SELECT RAISE(ABORT,'sync_background_processor_required'); END;
CREATE TRIGGER event_reading_node_guard BEFORE UPDATE OF status ON event_reading_nodes
WHEN NEW.status IN ('running','succeeded') AND NOT (SELECT allowed FROM sync_background_permission)
BEGIN SELECT RAISE(ABORT,'sync_background_processor_required'); END;
