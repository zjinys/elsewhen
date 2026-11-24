# Agent Note: Normalize wiki tags for reuse

Status: implemented

## Problem

Topic tags are stored only as a JSON array on each `wiki_pages` row. The editor can only infer suggestions by loading a bounded page list, so older tags disappear from autocomplete and tag identity is not reusable across pages.

## Decision

The database owns a canonical `wiki_tags` vocabulary and a `wiki_page_tags` membership table. The existing `wiki_pages.tags` JSON column remains as a compatibility and snapshot field. Migration triggers backfill and maintain the normalized index for every page insert or tag update, including sync and background writes. The tag autocomplete reads the vocabulary endpoint and no longer depends on the 500-page metadata window.

2026-11-20 autocomplete interaction: the input and its suggestions share one per-instance `TextFieldTapRegion` group. Pressing or scrolling a suggestion no longer triggers the text field's outside-click blur or clears the token before release. Outside clicks close the list immediately; no timeout races remain. Every edit refreshes suggestions even if the token start offset stays unchanged.

Validation: a mouse press held for 300 ms on `烟丝` reproduces the old failure (`烟` remains) and passes after the fix. The actual create-topic sheet completes `烟` to `烟丝` and passes `['烟丝']` to `saveTopicPage`; existing token replacement, duplicate exclusion, prefix refresh and outside-click tests pass. Vocabulary is mocked at its provider boundary because the bridge vocabulary helper is an extension method.

## Alternatives considered

- Keep aggregating `wiki_pages.tags`: rejected because the result is bounded by page pagination and has no reusable tag identity.
- Remove the JSON column immediately: rejected because snapshots, search, older readers and existing sync payloads still use it.
- Add only a tag table without page membership: rejected because usage counts and reliable cleanup would still require scanning every page JSON value.

## Consequences

New and edited pages populate one shared vocabulary, so autocomplete includes older tags and can order them by usage. The compatibility JSON column remains duplicated data, but SQLite triggers keep both representations synchronized. Future migration can move search and sync to the normalized relation after all readers are updated.
