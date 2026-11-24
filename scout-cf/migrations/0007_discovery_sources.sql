-- Retire narrow presets while keeping snapshots/findings/observations available.
-- New discovery scopes get their own baseline on first collection.
UPDATE scout_sources SET enabled = 0
WHERE id IN ('github-dart', 'github-rust', 'cws-watchlist');
