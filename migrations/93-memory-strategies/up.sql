ALTER TABLE personal_memory_claims ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0 CHECK(pinned IN (0,1));
ALTER TABLE personal_memory_claims ADD COLUMN excluded INTEGER NOT NULL DEFAULT 0 CHECK(excluded IN (0,1));
ALTER TABLE personal_memory_claims ADD COLUMN salience TEXT NOT NULL DEFAULT 'normal' CHECK(salience IN ('low','normal','high'));
CREATE TABLE personal_memory_goal_links (
    claim_id TEXT NOT NULL REFERENCES personal_memory_claims(id) ON DELETE CASCADE,
    goal_id TEXT NOT NULL REFERENCES goals(id) ON DELETE CASCADE,
    reason TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY(claim_id,goal_id)
);
CREATE INDEX personal_memory_goal_lookup ON personal_memory_goal_links(goal_id,claim_id);
CREATE TABLE personal_memory_inspections (
    claim_id TEXT NOT NULL REFERENCES personal_memory_claims(id) ON DELETE CASCADE,
    input_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    PRIMARY KEY(claim_id,input_id)
);
CREATE INDEX personal_memory_inspection_input ON personal_memory_inspections(input_id,claim_id);
