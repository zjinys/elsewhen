CREATE TABLE knowledge_dependencies (
    page_id TEXT NOT NULL REFERENCES wiki_pages(id) ON DELETE CASCADE,
    upstream_id TEXT NOT NULL REFERENCES wiki_pages(id),
    basis TEXT,
    PRIMARY KEY(page_id,upstream_id),
    CHECK(page_id<>upstream_id)
);
-- Historical derivatives did not record which revision they used. Require review
-- instead of falsely certifying the current upstream revision as their basis.
INSERT OR IGNORE INTO knowledge_dependencies(page_id,upstream_id)
SELECT d.id,p.id FROM wiki_pages d JOIN wiki_pages p ON p.slug=d.based_on
WHERE d.id<>p.id AND NOT EXISTS(SELECT 1 FROM knowledge_source_pages s WHERE s.page_id=p.id);

ALTER TABLE knowledge_proposals ADD COLUMN dependency_bases TEXT;
ALTER TABLE knowledge_review_decisions ADD COLUMN before_content TEXT;
ALTER TABLE knowledge_review_decisions ADD COLUMN result_content TEXT;
ALTER TABLE knowledge_review_decisions ADD COLUMN result_applicable TEXT;
ALTER TABLE knowledge_maintenance_reviews ADD COLUMN issue_description TEXT;
ALTER TABLE knowledge_maintenance_reviews ADD COLUMN resolution_note TEXT;
ALTER TABLE knowledge_maintenance_reviews ADD COLUMN target_page_id TEXT REFERENCES wiki_pages(id);
