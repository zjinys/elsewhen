"""API tests: auth + read-only endpoints, via httpx ASGI transport."""
import os

import httpx
import pytest

from scout.server import create_app


@pytest.fixture
def seeded_db(tmp_path):
    from scout.store import Store

    db = tmp_path / "api.db"
    store = Store(db)
    store.sync_sources([{
        "id": "s1", "name": "s1", "enabled": True,
        "spec_hash": "h", "interval_minutes": 720,
    }])
    store.record_run(
        "s1", "2026-11-15T00:00:00+00:00",
        [{"id": "a", "stars": 5}],
        [{"kind": "new", "item_key": "a", "payload": {"id": "a"}, "detected_at": "2026-11-15T00:00:00+00:00"}],
        "2026-11-15T00:00:00+00:00",
    )
    store.record_failure("s1", "2026-11-15T01:00:00+00:00",
                         "2026-11-15T01:00:01+00:00", "boom")
    return db


@pytest.fixture
def client(seeded_db, tmp_path, monkeypatch):
    sources_toml = tmp_path / "sources.toml"
    sources_toml.write_text("[[source]]\nname = \"s1\"\n")
    app = create_app(str(seeded_db), str(sources_toml))
    # don't let the scheduler run anything during tests
    app.state.scheduler.start = lambda: None
    transport = httpx.ASGITransport(app=app)
    return httpx.AsyncClient(transport=transport, base_url="http://test")


async def test_health_no_auth(client):
    r = await client.get("/health")
    assert r.status_code == 200 and r.json() == {"status": "ok"}


async def test_sources_requires_token(client, monkeypatch):
    monkeypatch.setenv("SCOUT_TOKEN", "tok123")
    r = await client.get("/sources")
    assert r.status_code == 401
    r = await client.get("/sources", headers={"Authorization": "Bearer wrong"})
    assert r.status_code == 401
    r = await client.get("/sources", headers={"Authorization": "Bearer tok123"})
    assert r.status_code == 200
    assert [s["id"] for s in r.json()] == ["s1"]


async def test_findings_endpoints(client, monkeypatch):
    monkeypatch.setenv("SCOUT_TOKEN", "tok123")
    h = {"Authorization": "Bearer tok123"}
    r = await client.get("/findings", headers=h)
    assert r.status_code == 200
    rows = r.json()
    assert len(rows) == 1 and rows[0]["item_key"] == "a"

    fid = rows[0]["id"]
    r = await client.get(f"/findings/{fid}", headers=h)
    assert r.status_code == 200
    assert r.json()["history"][0]["item"]["id"] == "a"

    r = await client.get("/findings/99999", headers=h)
    assert r.status_code == 404

    r = await client.get("/findings?kind=bogus", headers=h)
    assert r.status_code == 400


async def test_snapshots_and_runs(client, monkeypatch):
    monkeypatch.setenv("SCOUT_TOKEN", "tok123")
    h = {"Authorization": "Bearer tok123"}
    r = await client.get("/snapshots/1", headers=h)
    assert r.status_code == 200
    assert r.json()["data"][0]["id"] == "a"
    r = await client.get("/snapshots/999", headers=h)
    assert r.status_code == 404

    r = await client.get("/runs", headers=h)
    assert r.status_code == 200
    statuses = {x["status"] for x in r.json()}
    assert statuses == {"succeeded", "failed"}


async def test_no_token_env_means_open(client, monkeypatch):
    monkeypatch.delenv("SCOUT_TOKEN", raising=False)
    r = await client.get("/sources")
    assert r.status_code == 200
