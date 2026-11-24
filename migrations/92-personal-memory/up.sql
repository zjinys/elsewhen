CREATE TABLE personal_memory_claims (
    id TEXT PRIMARY KEY,
    page_slug TEXT NOT NULL,
    body_hash TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('semantic','procedural')),
    subject TEXT NOT NULL,
    predicate TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','active','superseded','conflicted','dismissed','deleted')),
    scope TEXT NOT NULL DEFAULT 'reference' CHECK(scope IN ('reference','conversation','personal')),
    conversation_id TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1 CHECK(version>0),
    confirmed_hash TEXT,
    support TEXT NOT NULL DEFAULT 'unassessed' CHECK(support IN ('unassessed','supported','contradicted','insufficient')),
    valid_from TEXT,
    valid_until TEXT,
    superseded_by TEXT,
    basis_json TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    review_after TEXT NOT NULL,
    UNIQUE(kind,subject,predicate,body_hash)
);
CREATE INDEX personal_memory_search ON personal_memory_claims(state,kind,subject,predicate);
CREATE TABLE personal_memory_decisions (
    id TEXT PRIMARY KEY,
    claim_id TEXT,
    conversation_id TEXT NOT NULL,
    operation TEXT NOT NULL,
    expected_version INTEGER,
    args_json TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE TABLE personal_memory_grants (
    source_id TEXT NOT NULL,
    target_id TEXT NOT NULL,
    message_bound INTEGER NOT NULL,
    source_hash TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    enabled INTEGER NOT NULL CHECK(enabled IN (0,1)),
    expires_at TEXT,
    PRIMARY KEY(source_id,target_id)
);
CREATE TABLE personal_memory_policy (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE personal_memory_receipts (
    attempt_id TEXT NOT NULL,
    request_id TEXT NOT NULL,
    claim_id TEXT NOT NULL,
    claim_version INTEGER NOT NULL,
    access_kind TEXT NOT NULL CHECK(access_kind IN ('opened','retrieved','cited','injected')),
    outcome TEXT NOT NULL CHECK(outcome IN ('prepared','sent','failed','unknown')),
    created_at TEXT NOT NULL,
    PRIMARY KEY(attempt_id,claim_id,claim_version,access_kind)
);
CREATE INDEX personal_memory_usage ON personal_memory_receipts(request_id,claim_id,claim_version,access_kind);
