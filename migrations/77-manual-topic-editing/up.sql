-- The Create Topic form previously called save_text_page, creating note pages.
-- Restore only the explicit local text-creation provenance. Keep IDs, slugs,
-- text, tags, immutable source snapshots and all revision references intact.
UPDATE wiki_pages
SET kind='topic', area='insight', human_edited_at=COALESCE(human_edited_at,updated_at)
WHERE kind='note' AND slug GLOB 'note-*'
  AND source_url IS NULL AND based_on IS NULL
  AND EXISTS (
    SELECT 1 FROM wiki_revisions r
    WHERE r.page_id=wiki_pages.id AND r.reason='用户粘贴文本导入'
  )
  AND NOT EXISTS (
    SELECT 1 FROM knowledge_source_pages m
    JOIN knowledge_sources s ON s.id=m.source_id
    WHERE m.page_id=wiki_pages.id AND s.locator IS NOT NULL
  );
