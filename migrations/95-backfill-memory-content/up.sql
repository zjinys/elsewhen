UPDATE personal_memory_claims
SET content = (
    SELECT w.content_md
    FROM wiki_pages w
    WHERE w.slug = personal_memory_claims.page_slug
)
WHERE content IS NULL
  AND EXISTS (
    SELECT 1
    FROM wiki_pages w
    WHERE w.slug = personal_memory_claims.page_slug
  );
