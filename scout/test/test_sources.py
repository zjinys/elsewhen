"""Spec loading + run orchestration tests (no real network)."""
import textwrap

import pytest

from scout.sources import load_specs, run_source
from scout.store import Store


@pytest.fixture
def store(tmp_path):
    return Store(tmp_path / "t.db")


def test_load_specs_slug_and_hash(tmp_path):
    p = tmp_path / "sources.toml"
    p.write_text(
        textwrap.dedent(
            """
            [[source]]
            name = "github-dart"
            url = "https://api.github.com/search/repositories"
            interval_minutes = 720
            [source.parse]
            format = "json"
            items_path = "items"
            item_id = "full_name"
            """
        )
    )
    specs = load_specs(p)
    assert len(specs) == 1
    s = specs[0]
    assert s["id"] == "github-dart"
    assert s["executor"] == "httpx"
    assert s["interval_minutes"] == 720
    assert len(s["spec_hash"]) == 64


def test_load_specs_rejects_bad_slug(tmp_path):
    p = tmp_path / "sources.toml"
    p.write_text('[[source]]\nname = "bad slug!"\n')
    with pytest.raises(ValueError):
        load_specs(p)


def test_load_specs_rejects_short_interval(tmp_path):
    p = tmp_path / "sources.toml"
    p.write_text('[[source]]\nname = "a"\ninterval_minutes = 5\n')
    with pytest.raises(ValueError):
        load_specs(p)


async def test_run_source_records_findings(store, monkeypatch):
    spec = {
        "id": "s1", "name": "s1", "enabled": True,
        "executor": "httpx", "interval_minutes": 720,
        "url": "https://example.test/api", "parse": {"format": "json"},
        "spec_hash": "h",
    }

    import scout.executors.httpx_exec as ex

    async def fake_fetch(s):
        return [{"id": "a", "stars": 5}]

    monkeypatch.setattr(ex, "fetch", fake_fetch)
    import scout.executors.registry as reg

    monkeypatch.setitem(reg._REGISTRY, "httpx", ex)

    out1 = await run_source(store, spec)
    assert out1["findings"] == 1  # first run: new
    assert out1["items"] == 1

    # second run, same data -> no findings
    out2 = await run_source(store, spec)
    assert out2["findings"] == 0


async def test_run_source_failure_recorded(store, monkeypatch):
    spec = {
        "id": "s2", "name": "s2", "enabled": True,
        "executor": "httpx", "interval_minutes": 720,
        "url": "https://invalid.invalid", "parse": {"format": "json"},
        "spec_hash": "h",
    }

    import scout.executors.httpx_exec as ex

    async def fake_fetch(s):
        raise RuntimeError("network down")

    monkeypatch.setattr(ex, "fetch", fake_fetch)
    import scout.executors.registry as reg

    monkeypatch.setitem(reg._REGISTRY, "httpx", ex)

    out = await run_source(store, spec)
    assert "network down" in out["error"]
    runs = store.list_runs("s2")
    assert runs[0]["status"] == "failed"
    assert store.last_snapshot("s2") is None
