"""Store tests: transactional runs, findings queries, item history, pruning."""
import json

import pytest

from scout.store import Store


@pytest.fixture
def store(tmp_path):
    return Store(tmp_path / "test.db")


def _spec(sid: str) -> dict:
    return {
        "id": sid,
        "name": sid,
        "enabled": True,
        "spec_hash": "h",
        "interval_minutes": 720,
    }


def _item(key: str, stars: int) -> dict:
    return {"id": key, "stars": stars}


def test_sync_sources_upsert_and_disable(store):
    store.sync_sources([_spec("a"), _spec("b")])
    assert {s["id"] for s in store.list_sources()} == {"a", "b"}
    # removing b from config disables it but keeps the row
    store.sync_sources([_spec("a")])
    rows = store.list_sources(include_disabled=True)
    b = [s for s in rows if s["id"] == "b"][0]
    assert b["enabled"] == 0
    assert {s["id"] for s in store.list_sources()} == {"a"}


def test_record_run_transactional(store):
    store.sync_sources([_spec("a")])
    snap_id, n = store.record_run(
        "a", "2026-11-15T00:00:00+00:00", [_item("x", 1)],
        [{"kind": "new", "item_key": "x", "payload": _item("x", 1),
          "detected_at": "2026-11-15T00:00:00+00:00"}],
        "2026-11-15T00:00:00+00:00",
    )
    assert n == 1
    snap = store.get_snapshot(snap_id)
    assert snap["item_count"] == 1
    assert json.loads(json.dumps(snap["data"]))[0]["id"] == "x"
    src = store.get_source("a")
    assert src["last_run_at"] == "2026-11-15T00:00:00+00:00"


def test_record_failure_isolated(store):
    store.sync_sources([_spec("a")])
    store.record_failure("a", "2026-11-15T01:00:00+00:00",
                         "2026-11-15T01:00:05+00:00", "boom")
    runs = store.list_runs("a")
    assert len(runs) == 1 and runs[0]["status"] == "failed"
    assert store.last_snapshot("a") is None


def test_findings_filters(store):
    store.sync_sources([_spec("a"), _spec("b")])
    for sid in ("a", "b"):
        store.record_run(
            sid, "2026-11-15T00:00:00+00:00",
            [_item("x", 1)],
            [{"kind": "new", "item_key": "x", "payload": {}, "detected_at": "2026-11-15T00:00:00+00:00"}],
            "2026-11-15T00:00:00+00:00",
        )
    all_f = store.list_findings()
    assert len(all_f) == 2
    assert len(store.list_findings(source_id="a")) == 1
    assert len(store.list_findings(kind="new")) == 2
    assert len(store.list_findings(kind="gone")) == 0
    assert len(store.list_findings(since="2026-11-16T00:00:00+00:00")) == 0


def test_item_history_tracks_timeline(store):
    store.sync_sources([_spec("a")])
    for stars in (5, 9, 20):
        store.record_run(
            "a", f"2026-11-15T0{stars % 3}:00:00+00:00",
            [_item("x", stars)],
            [{"kind": "new" if stars == 5 else "changed", "item_key": "x",
              "payload": _item("x", stars), "detected_at": "2026-11-15T00:00:00+00:00"}],
            "2026-11-15T00:00:00+00:00",
        )
    hist = store.item_history("a", "x")
    assert [h["item"]["stars"] for h in hist] == [5, 9, 20]


def test_prune_keeps_sample_per_day(store):
    store.sync_sources([_spec("a")])
    # three old snapshots in one day + one recent
    for i, t in enumerate(
        ["2026-07-01T01:00:00+00:00", "2026-07-01T05:00:00+00:00", "2026-07-01T09:00:00+00:00"]
    ):
        store.record_run("a", t, [_item("x", i)], [], t)
    store.record_run("a", "2026-11-15T00:00:00+00:00", [_item("x", 9)], [],
                     "2026-11-15T00:00:00+00:00")
    deleted = store.prune_snapshots(keep_days=30, sample_per_day=1)
    assert deleted == 2
    remaining = store.list_runs("a")
    # 4 runs recorded; snapshots 3 deleted from 4 -> keep 2 (one sampled + recent)
    snaps = 0
    cur = store.get_snapshot(1)
    # sampled newest of 2026-07-01 = id 3, recent = id 4
    assert store.get_snapshot(3) is not None
    assert store.get_snapshot(4) is not None
