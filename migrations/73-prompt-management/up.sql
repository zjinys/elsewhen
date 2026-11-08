CREATE TABLE prompt_definitions (
    prompt_key TEXT PRIMARY KEY,
    category TEXT NOT NULL CHECK (category IN (
        'policy', 'task', 'fixed_contract', 'tool_description', 'language_directive'
    )),
    title_key TEXT NOT NULL,
    default_locale TEXT NOT NULL CHECK (length(default_locale) BETWEEN 1 AND 64),
    contract_version INTEGER NOT NULL CHECK (contract_version > 0),
    variables_version INTEGER NOT NULL CHECK (variables_version > 0),
    variables_schema_json TEXT NOT NULL CHECK (json_valid(variables_schema_json)),
    editable INTEGER NOT NULL CHECK (editable IN (0, 1)),
    max_body_bytes INTEGER NOT NULL CHECK (max_body_bytes > 0),
    catalog_version TEXT NOT NULL
);

CREATE TABLE prompt_revisions (
    id TEXT PRIMARY KEY,
    prompt_key TEXT NOT NULL REFERENCES prompt_definitions(prompt_key),
    locale TEXT NOT NULL CHECK (length(locale) BETWEEN 1 AND 64),
    origin TEXT NOT NULL CHECK (origin IN ('builtin', 'user')),
    body TEXT NOT NULL CHECK (length(trim(body)) > 0),
    body_hash TEXT NOT NULL CHECK (length(body_hash) = 64),
    contract_version INTEGER NOT NULL CHECK (contract_version > 0),
    variables_version INTEGER NOT NULL CHECK (variables_version > 0),
    builtin_pack_version TEXT,
    parent_revision_id TEXT REFERENCES prompt_revisions(id),
    base_default_revision_id TEXT REFERENCES prompt_revisions(id),
    translation_source_revision_id TEXT REFERENCES prompt_revisions(id),
    change_note TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    CHECK (
        (origin = 'builtin' AND builtin_pack_version IS NOT NULL) OR
        (origin = 'user' AND builtin_pack_version IS NULL)
    ),
    UNIQUE (prompt_key, locale, id)
);

CREATE UNIQUE INDEX prompt_builtin_pack_unique
ON prompt_revisions(prompt_key, locale, builtin_pack_version)
WHERE origin = 'builtin';

CREATE INDEX prompt_revision_history
ON prompt_revisions(prompt_key, locale, created_at DESC, id);

CREATE TABLE prompt_activations (
    prompt_key TEXT NOT NULL REFERENCES prompt_definitions(prompt_key),
    locale TEXT NOT NULL CHECK (length(locale) BETWEEN 1 AND 64),
    mode TEXT NOT NULL CHECK (mode IN ('default', 'pinned')),
    revision_id TEXT,
    generation INTEGER NOT NULL CHECK (generation > 0),
    updated_at TEXT NOT NULL,
    PRIMARY KEY (prompt_key, locale),
    FOREIGN KEY (prompt_key, locale, revision_id)
        REFERENCES prompt_revisions(prompt_key, locale, id),
    CHECK (
        (mode = 'default' AND revision_id IS NULL) OR
        (mode = 'pinned' AND revision_id IS NOT NULL)
    )
);

CREATE TABLE prompt_language_preferences (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    instruction_locale TEXT CHECK (
        instruction_locale IS NULL OR length(instruction_locale) BETWEEN 1 AND 64
    ),
    response_mode TEXT NOT NULL CHECK (response_mode IN ('auto', 'fixed')),
    response_locale TEXT,
    generation INTEGER NOT NULL CHECK (generation > 0),
    updated_at TEXT NOT NULL,
    CHECK (
        (response_mode = 'auto' AND response_locale IS NULL) OR
        (response_mode = 'fixed' AND response_locale IS NOT NULL
            AND length(response_locale) BETWEEN 1 AND 64)
    )
);

CREATE TABLE dataset_content_language (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    output_locale TEXT NOT NULL CHECK (length(output_locale) BETWEEN 1 AND 64),
    policy_version INTEGER NOT NULL CHECK (policy_version > 0),
    updated_at TEXT NOT NULL
);

CREATE TABLE prompt_bundles (
    id TEXT PRIMARY KEY,
    owner_kind TEXT NOT NULL CHECK (owner_kind IN ('chat_run', 'background_task', 'preview')),
    owner_id TEXT NOT NULL,
    ui_locale TEXT NOT NULL,
    requested_instruction_locale TEXT NOT NULL,
    response_policy_json TEXT NOT NULL CHECK (json_valid(response_policy_json)),
    knowledge_output_locale TEXT NOT NULL,
    content_language_policy_version INTEGER NOT NULL,
    catalog_version TEXT NOT NULL,
    locale_policy_version TEXT NOT NULL,
    renderer_version TEXT NOT NULL,
    bundle_hash TEXT NOT NULL CHECK (length(bundle_hash) = 64),
    created_at TEXT NOT NULL,
    UNIQUE (owner_kind, owner_id)
);

CREATE TABLE prompt_bundle_entries (
    bundle_id TEXT NOT NULL REFERENCES prompt_bundles(id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    prompt_key TEXT NOT NULL,
    requested_locale TEXT NOT NULL,
    resolved_locale TEXT NOT NULL,
    revision_id TEXT NOT NULL,
    fallback_reason TEXT,
    PRIMARY KEY (bundle_id, ordinal),
    UNIQUE (bundle_id, prompt_key),
    FOREIGN KEY (prompt_key, resolved_locale, revision_id)
        REFERENCES prompt_revisions(prompt_key, locale, id)
);

CREATE TRIGGER prompt_revision_no_update
BEFORE UPDATE ON prompt_revisions
BEGIN
    SELECT RAISE(ABORT, 'prompt_revision_immutable');
END;

CREATE TRIGGER prompt_revision_no_delete
BEFORE DELETE ON prompt_revisions
BEGIN
    SELECT RAISE(ABORT, 'prompt_revision_immutable');
END;

INSERT INTO dataset_content_language VALUES(1,'zh-Hans',1,datetime('now'));
CREATE TRIGGER prompt_bundle_no_update BEFORE UPDATE ON prompt_bundles BEGIN SELECT RAISE(ABORT,'prompt_bundle_immutable'); END;
CREATE TRIGGER prompt_bundle_no_delete BEFORE DELETE ON prompt_bundles BEGIN SELECT RAISE(ABORT,'prompt_bundle_immutable'); END;
CREATE TRIGGER prompt_entry_no_update BEFORE UPDATE ON prompt_bundle_entries BEGIN SELECT RAISE(ABORT,'prompt_bundle_immutable'); END;
CREATE TRIGGER prompt_entry_no_delete BEFORE DELETE ON prompt_bundle_entries BEGIN SELECT RAISE(ABORT,'prompt_bundle_immutable'); END;
ALTER TABLE chat_runs ADD COLUMN prompt_bundle_id TEXT REFERENCES prompt_bundles(id);
ALTER TABLE chat_run_requests ADD COLUMN prompt_bundle_id TEXT REFERENCES prompt_bundles(id);
ALTER TABLE knowledge_background_runs ADD COLUMN prompt_bundle_id TEXT REFERENCES prompt_bundles(id);
CREATE TABLE prompt_reading_tasks (
    snapshot_id TEXT NOT NULL,
    contract_version TEXT NOT NULL,
    bundle_id TEXT NOT NULL REFERENCES prompt_bundles(id),
    strategy TEXT NOT NULL,
    PRIMARY KEY(snapshot_id,contract_version)
);
CREATE TRIGGER prompt_reading_task_no_update BEFORE UPDATE ON prompt_reading_tasks BEGIN SELECT RAISE(ABORT,'prompt_task_immutable'); END;
CREATE TRIGGER prompt_reading_task_no_delete BEFORE DELETE ON prompt_reading_tasks BEGIN SELECT RAISE(ABORT,'prompt_task_immutable'); END;
