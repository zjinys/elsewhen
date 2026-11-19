"""Diff engine: compare consecutive item lists into findings.

Deterministic, no AI. Items are dicts with a required stable `id` field;
"changed" means the content hash of the item changed. Change-magnitude
filtering (e.g. only report big star jumps) belongs to source normalization,
not here.
"""
from __future__ import annotations

import hashlib
import json
from typing import Any


def _item_hash(item: dict[str, Any]) -> str:
    canonical = json.dumps(item, sort_keys=True, ensure_ascii=False)
    return hashlib.sha256(canonical.encode("utf-8")).hexdigest()


def diff_items(
    current: list[dict[str, Any]], previous: list[dict[str, Any]] | None
) -> list[dict[str, Any]]:
    """Return findings list: [{"kind": new|changed|gone, "item_key", "payload"}].

    detected_at is attached by the caller (sources.run_source) so this stays
    pure. When previous is None (first run) every item is "new" — callers that
    dislike the initial flood can pass previous=[] semantics themselves; we
    keep first-run=new because the service starts tracking from day one.
    """
    prev_map: dict[str, str] = {}
    if previous:
        for item in previous:
            key = item.get("id")
            if key is not None:
                prev_map[str(key)] = _item_hash(item)

    cur_map: dict[str, tuple[str, dict[str, Any]]] = {}
    for item in current:
        key = item.get("id")
        if key is None:
            raise ValueError("normalized item missing stable 'id' field")
        cur_map[str(key)] = (_item_hash(item), item)

    findings: list[dict[str, Any]] = []
    for key, (_, item) in cur_map.items():
        if key not in prev_map:
            findings.append({"kind": "new", "item_key": key, "payload": item})
        elif prev_map[key] != _item_hash(item):
            findings.append({"kind": "changed", "item_key": key, "payload": item})
    for key in prev_map:
        if key not in cur_map:
            findings.append({"kind": "gone", "item_key": key, "payload": {"id": key}})
    return findings
