from __future__ import annotations

import json
import sqlite3
import threading
from pathlib import Path
from typing import Any

_SCHEMA_PATH = Path(__file__).parent / "schema.sql"


def _connect(path: str | Path) -> sqlite3.Connection:
    conn = sqlite3.connect(path, check_same_thread=False)
    conn.row_factory = sqlite3.Row
    conn.execute("PRAGMA journal_mode=WAL")
    conn.execute("PRAGMA foreign_keys=ON")
    return conn


class Store:
    """Thread-safe wrapper around a single SQLite connection.

    Writes are serialized with a lock; concurrent reads are fine via WAL.
    Each collection run commits snapshot + findings + run row in one
    transaction (see record_run).
    """

    def __init__(self, path: str | Path) -> None:
        self.path = Path(path)
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self._lock = threading.Lock()
        self._conn = _connect(self.path)
        with self._lock:
            self._conn.executescript(_SCHEMA_PATH.read_text())
            self._conn.commit()

    # ---- sources -------------------------------------------------------

    def sync_sources(self, specs: list[dict[str, Any]]) -> None:
        """Upsert source rows from parsed TOML specs; disable rows whose
        slug disappeared from the config (keeps history, hides from API).
        """
        with self._lock:
            for spec in specs:
                self._conn.execute(
                    """
                    INSERT INTO scout_sources
                        (id, name, enabled, spec_hash, interval_minutes, last_run_at)
                    VALUES (?, ?, ?, ?, ?,
                            (SELECT last_run_at FROM scout_sources WHERE id = ?))
                    ON CONFLICT(id) DO UPDATE SET
                        name = excluded.name,
                        enabled = excluded.enabled,
                        spec_hash = excluded.spec_hash,
                        interval_minutes = excluded.interval_minutes
                    """,
                    (
                        spec["id"],
                        spec["name"],
                        1 if spec.get("enabled", True) else 0,
                        spec["spec_hash"],
                        spec["interval_minutes"],
                        spec["id"],
                    ),
                )
            known = {s["id"] for s in specs}
            for row in self._conn.execute("SELECT id FROM scout_sources").fetchall():
                if row["id"] not in known:
                    self._conn.execute(
                        "UPDATE scout_sources SET enabled = 0 WHERE id = ?", (row["id"],)
                    )
            self._conn.commit()

    def list_sources(self, include_disabled: bool = False) -> list[dict[str, Any]]:
        sql = "SELECT * FROM scout_sources"
        if not include_disabled:
            sql += " WHERE enabled = 1"
        sql += " ORDER BY id"
        with self._lock:
            rows = self._conn.execute(sql).fetchall()
        return [dict(r) for r in rows]

    def get_source(self, source_id: str) -> dict[str, Any] | None:
        with self._lock:
            row = self._conn.execute(
                "SELECT * FROM scout_sources WHERE id = ?", (source_id,)
            ).fetchone()
        return dict(row) if row else None

    def set_last_run(self, source_id: str, taken_at: str) -> None:
        with self._lock:
            self._conn.execute(
                "UPDATE scout_sources SET last_run_at = ? WHERE id = ?",
                (taken_at, source_id),
            )
            self._conn.commit()

    def is_spec_current(self, source_id: str, spec_hash: str) -> bool:
        row = self.get_source(source_id)
        return bool(row) and row["spec_hash"] == spec_hash  # type: ignore[index]

    # ---- snapshots / findings / runs (one transaction per run) ----------

    def record_run(
        self,
        source_id: str,
        taken_at: str,
        items: list[dict[str, Any]],
        findings: list[dict[str, Any]],
        started_at: str,
        error: str | None = None,
    ) -> tuple[int, int]:
        """Commit snapshot + findings + run row in a single transaction.

        Returns (snapshot_id, findings_written). On failure raises; nothing
        is persisted (whole-run rollback).
        """
        with self._lock:
            try:
                cur = self._conn.execute(
                    "INSERT INTO scout_snapshots (source_id, taken_at, data_json, item_count)"
                    " VALUES (?, ?, ?, ?)",
                    (source_id, taken_at, json.dumps(items, ensure_ascii=False), len(items)),
                )
                snapshot_id = cur.lastrowid
                assert snapshot_id is not None
                for f in findings:
                    self._conn.execute(
                        "INSERT INTO scout_findings"
                        " (source_id, snapshot_id, kind, item_key, payload_json, detected_at)"
                        " VALUES (?, ?, ?, ?, ?, ?)",
                        (
                            source_id,
                            snapshot_id,
                            f["kind"],
                            f["item_key"],
                            json.dumps(f.get("payload"), ensure_ascii=False),
                            f["detected_at"],
                        ),
                    )
                self._conn.execute(
                    "INSERT INTO scout_runs"
                    " (source_id, status, started_at, finished_at, items_count,"
                    "  findings_count, error)"
                    " VALUES (?, ?, ?, ?, ?, ?, ?)",
                    (
                        source_id,
                        "failed" if error else "succeeded",
                        started_at,
                        taken_at,
                        len(items),
                        len(findings),
                        error,
                    ),
                )
                self._conn.execute(
                    "UPDATE scout_sources SET last_run_at = ? WHERE id = ?",
                    (taken_at, source_id),
                )
                self._conn.commit()
                return snapshot_id, len(findings)
            except Exception:
                self._conn.rollback()
                raise

    def record_failure(self, source_id: str, started_at: str, finished_at: str, error: str) -> None:
        """Record a failed run (no snapshot) in its own transaction."""
        with self._lock:
            self._conn.execute(
                "INSERT INTO scout_runs"
                " (source_id, status, started_at, finished_at, items_count,"
                "  findings_count, error)"
                " VALUES (?, 'failed', ?, ?, 0, 0, ?)",
                (source_id, started_at, finished_at, error[:2000]),
            )
            self._conn.commit()

    def last_snapshot(self, source_id: str) -> dict[str, Any] | None:
        with self._lock:
            row = self._conn.execute(
                "SELECT * FROM scout_snapshots WHERE source_id = ?"
                " ORDER BY id DESC LIMIT 1",
                (source_id,),
            ).fetchone()
        if not row:
            return None
        d = dict(row)
        d["data"] = json.loads(d.pop("data_json"))
        return d

    def list_findings(
        self,
        source_id: str | None = None,
        kind: str | None = None,
        since: str | None = None,
        limit: int = 200,
    ) -> list[dict[str, Any]]:
        sql = "SELECT * FROM scout_findings WHERE 1=1"
        params: list[Any] = []
        if source_id:
            sql += " AND source_id = ?"
            params.append(source_id)
        if kind:
            sql += " AND kind = ?"
            params.append(kind)
        if since:
            sql += " AND detected_at >= ?"
            params.append(since)
        sql += " ORDER BY id DESC LIMIT ?"
        params.append(min(limit, 1000))
        with self._lock:
            rows = self._conn.execute(sql, params).fetchall()
        out = []
        for r in rows:
            d = dict(r)
            d["payload"] = json.loads(d.pop("payload_json"))
            out.append(d)
        return out

    def get_finding(self, finding_id: int) -> dict[str, Any] | None:
        with self._lock:
            row = self._conn.execute(
                "SELECT * FROM scout_findings WHERE id = ?", (finding_id,)
            ).fetchone()
        if not row:
            return None
        d = dict(row)
        d["payload"] = json.loads(d.pop("payload_json"))
        return d

    def item_history(self, source_id: str, item_key: str) -> list[dict[str, Any]]:
        """All snapshot values seen for one item key (research timeline)."""
        with self._lock:
            rows = self._conn.execute(
                "SELECT s.id AS snapshot_id, s.taken_at, s.data_json"
                " FROM scout_snapshots s WHERE s.source_id = ? ORDER BY s.id",
                (source_id,),
            ).fetchall()
        out = []
        for r in rows:
            for item in json.loads(r["data_json"]):
                if item.get("id") == item_key:
                    out.append({"snapshot_id": r["snapshot_id"], "taken_at": r["taken_at"], "item": item})
                    break
        return out

    def get_snapshot(self, snapshot_id: int) -> dict[str, Any] | None:
        with self._lock:
            row = self._conn.execute(
                "SELECT * FROM scout_snapshots WHERE id = ?", (snapshot_id,)
            ).fetchone()
        if not row:
            return None
        d = dict(row)
        d["data"] = json.loads(d.pop("data_json"))
        return d

    def list_runs(self, source_id: str | None = None, limit: int = 50) -> list[dict[str, Any]]:
        sql = "SELECT * FROM scout_runs"
        params: list[Any] = []
        if source_id:
            sql += " WHERE source_id = ?"
            params.append(source_id)
        sql += " ORDER BY id DESC LIMIT ?"
        params.append(min(limit, 500))
        with self._lock:
            rows = self._conn.execute(sql, params).fetchall()
        return [dict(r) for r in rows]

    def prune_snapshots(self, keep_days: int = 90, sample_per_day: int = 1) -> int:
        """Delete snapshots older than keep_days except sample_per_day newest
        per UTC day per source. Findings keep their snapshot FK; pruning only
        removes bulk raw data, sampled snapshots retain research timeline.
        """
        cutoff = _utc_now_minus(days=keep_days)
        with self._lock:
            rows = self._conn.execute(
                "SELECT id, source_id, taken_at FROM scout_snapshots"
                " WHERE taken_at < ? ORDER BY id DESC",
                (cutoff,),
            ).fetchall()
            keep_ids: set[int] = set()
            seen_per_day: dict[tuple[str, str], int] = {}
            for r in rows:
                day = r["taken_at"][:10]
                k = (r["source_id"], day)
                n = seen_per_day.get(k, 0)
                if n < sample_per_day:
                    seen_per_day[k] = n + 1
                    keep_ids.add(r["id"])
            deleted = 0
            for r in rows:
                if r["id"] not in keep_ids:
                    self._conn.execute("DELETE FROM scout_snapshots WHERE id = ?", (r["id"],))
                    deleted += 1
            self._conn.commit()
        return deleted


def _utc_now_minus(days: int) -> str:
    import datetime as dt

    return (dt.datetime.now(dt.timezone.utc) - dt.timedelta(days=days)).isoformat()
