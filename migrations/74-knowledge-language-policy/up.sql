ALTER TABLE dataset_content_language ADD COLUMN generation INTEGER NOT NULL DEFAULT 0 CHECK(generation>=0);
ALTER TABLE dataset_content_language ADD COLUMN authority_revision INTEGER NOT NULL DEFAULT 0 CHECK(authority_revision>=0);
ALTER TABLE dataset_content_language ADD COLUMN changed_by TEXT;
ALTER TABLE dataset_content_language ADD COLUMN dataset_id TEXT;
ALTER TABLE dataset_content_language ADD COLUMN owner_signature TEXT;

-- Self-declared application capabilities are authenticated by their operation
-- origin, not by relay transport support. Stream identity prevents reuse by a
-- reinstalled device. Full prompt revisions and bundles remain local.
CREATE TABLE sync_language_capabilities (
    device_id TEXT NOT NULL,
    stream_id TEXT NOT NULL,
    generation INTEGER NOT NULL CHECK(generation>0),
    roster_revision INTEGER NOT NULL CHECK(roster_revision>=0),
    engine_version INTEGER NOT NULL CHECK(engine_version>0),
    max_policy_version INTEGER NOT NULL CHECK(max_policy_version>0),
    locales_json TEXT NOT NULL CHECK(json_valid(locales_json)),
    updated_at TEXT NOT NULL,
    PRIMARY KEY(device_id,stream_id)
);

ALTER TABLE prompt_reading_tasks RENAME TO prompt_reading_tasks_legacy;
CREATE TABLE prompt_reading_tasks (
    snapshot_id TEXT NOT NULL,
    contract_version TEXT NOT NULL,
    bundle_id TEXT NOT NULL REFERENCES prompt_bundles(id),
    strategy TEXT NOT NULL,
    policy_generation INTEGER NOT NULL DEFAULT 0 CHECK(policy_generation>=0),
    PRIMARY KEY(snapshot_id,contract_version,policy_generation)
);
INSERT INTO prompt_reading_tasks SELECT snapshot_id,contract_version,bundle_id,strategy,0 FROM prompt_reading_tasks_legacy;
DROP TABLE prompt_reading_tasks_legacy;
CREATE TRIGGER prompt_reading_task_no_update BEFORE UPDATE ON prompt_reading_tasks BEGIN SELECT RAISE(ABORT,'prompt_task_immutable'); END;
CREATE TRIGGER prompt_reading_task_no_delete BEFORE DELETE ON prompt_reading_tasks BEGIN SELECT RAISE(ABORT,'prompt_task_immutable'); END;

-- A separate append-only relation preserves the schema and hashes of existing
-- bundles. Missing rows mean generation zero for pre-migration runs.
CREATE TABLE prompt_bundle_language_generations (
    bundle_id TEXT PRIMARY KEY REFERENCES prompt_bundles(id),
    policy_generation INTEGER NOT NULL CHECK(policy_generation>=0)
);
CREATE TRIGGER prompt_bundle_language_no_update BEFORE UPDATE ON prompt_bundle_language_generations BEGIN SELECT RAISE(ABORT,'prompt_bundle_immutable'); END;
CREATE TRIGGER prompt_bundle_language_no_delete BEFORE DELETE ON prompt_bundle_language_generations BEGIN SELECT RAISE(ABORT,'prompt_bundle_immutable'); END;

ALTER TABLE analysis_jobs ADD COLUMN prompt_bundle_id TEXT REFERENCES prompt_bundles(id);
ALTER TABLE analysis_jobs ADD COLUMN prompt_generation INTEGER NOT NULL DEFAULT 0 CHECK(prompt_generation>=0);
ALTER TABLE knowledge_digest_jobs ADD COLUMN prompt_bundle_id TEXT REFERENCES prompt_bundles(id);
ALTER TABLE knowledge_digest_runs ADD COLUMN prompt_bundle_id TEXT REFERENCES prompt_bundles(id);

CREATE TABLE knowledge_language_artifacts (
    artifact_kind TEXT NOT NULL,
    artifact_id TEXT NOT NULL,
    output_locale TEXT NOT NULL,
    policy_version INTEGER NOT NULL,
    policy_generation INTEGER NOT NULL,
    dependency TEXT NOT NULL,
    prompt_bundle_id TEXT NOT NULL REFERENCES prompt_bundles(id),
    created_at TEXT NOT NULL,
    PRIMARY KEY(artifact_kind,artifact_id)
);

-- Small provenance travels with the authenticated artifact itself; full
-- templates and local bundle identifiers never leave the installation.
CREATE TABLE knowledge_artifact_languages (
    artifact_kind TEXT NOT NULL,
    artifact_id TEXT NOT NULL,
    metadata_json TEXT NOT NULL CHECK(json_valid(metadata_json)),
    PRIMARY KEY(artifact_kind,artifact_id)
);

-- Retain the processor/lease/membership contract and add a connection-owned
-- capability check. An older executable has no such SQLite function and fails
-- closed even if it opens an already-upgraded database after a downgrade.
DROP VIEW sync_background_permission;
CREATE VIEW sync_background_permission AS
SELECT (
    NOT EXISTS(SELECT 1 FROM sync_replica)
    OR EXISTS(
        SELECT 1 FROM sync_replica r
        JOIN sync_members m ON m.device_id=r.device_id AND m.stream_id=r.stream_id
        WHERE r.processor_device_id=r.device_id AND m.active=1
          AND NOT EXISTS(
              SELECT 1 FROM sync_snapshot_target t JOIN sync_snapshots s ON s.id=t.snapshot_id
              WHERE s.state='staging'
          )
          AND NOT EXISTS(
              SELECT 1 FROM sync_processor_state ps
              WHERE ps.singleton=1 AND ps.state IN ('relinquished', 'pending_takeover')
          )
    )
) AND EXISTS(
    SELECT 1 FROM dataset_content_language p
    WHERE p.singleton=1 AND elsewhen_language_policy_supported(p.output_locale,p.policy_version)=1
) AS allowed;
