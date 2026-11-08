-- Reconcile early v74/v75 databases that predate artifact language metadata.
CREATE TABLE IF NOT EXISTS knowledge_artifact_languages (
    artifact_kind TEXT NOT NULL,
    artifact_id TEXT NOT NULL,
    metadata_json TEXT NOT NULL CHECK(json_valid(metadata_json)),
    PRIMARY KEY(artifact_kind,artifact_id)
);
