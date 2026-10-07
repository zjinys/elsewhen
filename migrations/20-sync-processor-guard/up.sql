CREATE VIEW sync_background_permission AS
SELECT NOT EXISTS(SELECT 1 FROM sync_replica)
    OR EXISTS(
        SELECT 1 FROM sync_replica r
        JOIN sync_members m ON m.device_id=r.device_id AND m.stream_id=r.stream_id
        WHERE r.processor_device_id=r.device_id AND m.active=1
          AND NOT EXISTS(
              SELECT 1 FROM sync_snapshot_target t JOIN sync_snapshots s ON s.id=t.snapshot_id
              WHERE s.state='staging'
          )
    ) AS allowed;

CREATE TRIGGER sync_analysis_claim_insert BEFORE INSERT ON analysis_jobs
WHEN NEW.status='running' AND NOT (SELECT allowed FROM sync_background_permission)
BEGIN SELECT RAISE(ABORT,'sync_background_processor_required'); END;
CREATE TRIGGER sync_analysis_claim_update BEFORE UPDATE OF status ON analysis_jobs
WHEN NEW.status='running' AND NOT (SELECT allowed FROM sync_background_permission)
BEGIN SELECT RAISE(ABORT,'sync_background_processor_required'); END;
CREATE TRIGGER sync_digest_claim_insert BEFORE INSERT ON knowledge_digest_jobs
WHEN NEW.status='running' AND NOT (SELECT allowed FROM sync_background_permission)
BEGIN SELECT RAISE(ABORT,'sync_background_processor_required'); END;
CREATE TRIGGER sync_digest_claim_update BEFORE UPDATE OF status ON knowledge_digest_jobs
WHEN NEW.status='running' AND NOT (SELECT allowed FROM sync_background_permission)
BEGIN SELECT RAISE(ABORT,'sync_background_processor_required'); END;
CREATE TRIGGER sync_digest_run_insert BEFORE INSERT ON knowledge_digest_runs
WHEN NEW.status='running' AND NOT (SELECT allowed FROM sync_background_permission)
BEGIN SELECT RAISE(ABORT,'sync_background_processor_required'); END;
CREATE TRIGGER sync_digest_run_update BEFORE UPDATE OF status ON knowledge_digest_runs
WHEN NEW.status='running' AND NOT (SELECT allowed FROM sync_background_permission)
BEGIN SELECT RAISE(ABORT,'sync_background_processor_required'); END;
CREATE TRIGGER sync_knowledge_run_insert BEFORE INSERT ON knowledge_background_runs
WHEN NEW.status='running' AND NOT (SELECT allowed FROM sync_background_permission)
BEGIN SELECT RAISE(ABORT,'sync_background_processor_required'); END;
CREATE TRIGGER sync_knowledge_run_update BEFORE UPDATE OF status ON knowledge_background_runs
WHEN NEW.status='running' AND NOT (SELECT allowed FROM sync_background_permission)
BEGIN SELECT RAISE(ABORT,'sync_background_processor_required'); END;
CREATE TRIGGER sync_refresh_claim_insert BEFORE INSERT ON knowledge_refresh_jobs
WHEN NEW.status='running' AND NOT (SELECT allowed FROM sync_background_permission)
BEGIN SELECT RAISE(ABORT,'sync_background_processor_required'); END;
CREATE TRIGGER sync_refresh_claim_update BEFORE UPDATE OF status ON knowledge_refresh_jobs
WHEN NEW.status='running' AND NOT (SELECT allowed FROM sync_background_permission)
BEGIN SELECT RAISE(ABORT,'sync_background_processor_required'); END;
