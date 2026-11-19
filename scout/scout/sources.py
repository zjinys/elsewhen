"""Load sources.toml and orchestrate per-source collection runs.

A run is: fetch -> normalize -> diff vs last snapshot -> record
(snapshot + findings + run row) in one transaction. Failures are recorded
as failed runs and never block other sources.
"""
from __future__ import annotations

import datetime as dt
import hashlib
import json
import tomllib
from pathlib import Path
from typing import Any

from .diff import diff_items
from .executors import registry
from .store import Store

VALID_EXECUTORS = ("httpx",)


def now_iso() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat()


def load_specs(path: str | Path) -> list[dict[str, Any]]:
    """Parse sources.toml into normalized spec dicts (with id + spec_hash)."""
    data = tomllib.loads(Path(path).read_text())
    specs: list[dict[str, Any]] = []
    for raw in data.get("source", []):
        name = raw.get("name", "").strip()
        if not name:
            raise ValueError("sources.toml: a [[source]] is missing 'name'")
        slug = raw.get("id") or name
        import re

        if not re.fullmatch(r"[A-Za-z0-9_-]+", slug):
            raise ValueError(
                f"sources.toml: source id/name {slug!r} must match [A-Za-z0-9_-]+"
            )
        executor = raw.get("executor", "httpx")
        interval = int(raw.get("interval_minutes", 720))
        if interval < 15:
            raise ValueError(f"source {slug}: interval_minutes must be >= 15")
        spec = dict(raw)
        spec["id"] = slug
        spec["name"] = name
        spec["executor"] = executor
        spec["interval_minutes"] = interval
        spec["enabled"] = bool(raw.get("enabled", True))
        spec["spec_hash"] = hashlib.sha256(
            json.dumps(spec, sort_keys=True, ensure_ascii=False).encode()
        ).hexdigest()
        specs.append(spec)
    if not specs:
        raise ValueError("sources.toml: no [[source]] blocks found")
    return specs


async def run_source(store: Store, spec: dict[str, Any]) -> dict[str, Any]:
    """One collection run for one source. Never raises; failure is recorded."""
    source_id = spec["id"]
    started_at = now_iso()
    if not spec.get("enabled", True):
        return {"source": source_id, "skipped": True}

    try:
        executor = registry.get_executor(spec.get("executor", "httpx"))
        # ensure the source row exists & spec_hash stays current (FK + freshness)
        store.sync_sources([spec])
        items = await executor.fetch(spec)
        if not isinstance(items, list):
            raise ValueError("executor returned non-list")
        for item in items:
            if "id" not in item:
                raise ValueError("normalized item missing 'id'")

        prev_snap = store.last_snapshot(source_id)
        previous = prev_snap["data"] if prev_snap else None
        taken_at = now_iso()
        findings = diff_items(items, previous)
        for f in findings:
            f["detected_at"] = taken_at
        snapshot_id, findings_count = store.record_run(
            source_id, taken_at, items, findings, started_at
        )
        return {
            "source": source_id,
            "skipped": False,
            "snapshot_id": snapshot_id,
            "items": len(items),
            "findings": findings_count,
        }
    except Exception as e:  # noqa: BLE001 — failures are data, not crashes
        finished_at = now_iso()
        store.record_failure(source_id, started_at, finished_at, str(e))
        return {"source": source_id, "skipped": False, "error": str(e)}


async def run_all_due(store: Store, specs: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """Run every enabled source whose interval has elapsed since last_run."""
    out = []
    for spec in specs:
        if not spec.get("enabled", True):
            continue
        src = store.get_source(spec["id"])
        due = True
        if src and src.get("last_run_at"):
            try:
                last = dt.datetime.fromisoformat(src["last_run_at"])
                next_due = last + dt.timedelta(minutes=spec["interval_minutes"])
                due = dt.datetime.now(dt.timezone.utc) >= next_due
            except ValueError:
                due = True
        if due:
            out.append(await run_source(store, spec))
    return out
