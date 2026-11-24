# Agent Note: Make chat evidence understandable

Status: implemented

## Problem

Chat tool progress exposed event UUIDs and knowledge snapshot UUIDs beside evidence. Those identifiers are useful for storage and audit, but they do not help a person understand what the cited material is.

## Decision

The progress projection keeps the returned evidence excerpt, source title and source version. It omits `event_id` and `snapshot_id` from the ordinary chat display. The IDs remain in stored tool results for audit and are not removed from the data contract.

## Alternatives considered

- Show the full UUID with a tooltip: rejected because the identifier still dominates the useful evidence and users cannot act on it.
- Resolve every ID with a new live database lookup in the widget: rejected because tool results must remain a stable historical projection and late lookups can attach the wrong current identity.
- Remove the entire evidence line: rejected because the excerpt and source title are useful for checking the answer.

## Consequences

Chat evidence reads as content rather than storage internals. Audit payloads preserve the original IDs. The display does not offer direct navigation to a raw event yet; that can be added later through an explicit citation action with a stable message contract.
