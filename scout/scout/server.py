"""FastAPI app: read-only endpoints with bearer-token auth.

Env:
  SCOUT_DB      — sqlite path (default ./scout.db)
  SCOUT_SOURCES — sources.toml path (default ./sources.toml)
  SCOUT_TOKEN   — shared bearer token; unset => auth disabled (dev only)
"""
from __future__ import annotations

import logging
import os
from contextlib import asynccontextmanager
from pathlib import Path
from typing import Any

from fastapi import Depends, FastAPI, Header, HTTPException, Query

from .scheduler import Scheduler
from .store import Store

log = logging.getLogger("scout.server")


def _check_token(authorization: str | None) -> None:
    token = os.environ.get("SCOUT_TOKEN", "").strip()
    if not token:
        return  # dev mode: no token configured
    if not authorization or not authorization.startswith("Bearer "):
        raise HTTPException(status_code=401, detail="missing bearer token")
    supplied = authorization.removeprefix("Bearer ").strip()
    if supplied != token:
        raise HTTPException(status_code=401, detail="invalid token")


def _auth(authorization: str | None = Header(default=None)) -> None:
    _check_token(authorization)


@asynccontextmanager
async def lifespan(app: FastAPI):
    store = app.state.store
    sched = app.state.scheduler
    sched.start()
    log.info("scout started (db=%s)", store.path)
    yield
    sched.shutdown()


def create_app(
    db_path: str | None = None, sources_path: str | None = None
) -> FastAPI:
    db = Path(db_path or os.environ.get("SCOUT_DB", "scout.db"))
    src = Path(sources_path or os.environ.get("SCOUT_SOURCES", "sources.toml"))

    app = FastAPI(title="scout", version="0.1.0", lifespan=lifespan)
    app.state.store = Store(db)
    app.state.scheduler = Scheduler(app.state.store, src)
    app.state.sources_path = src

    @app.get("/health")
    async def health() -> dict[str, str]:
        return {"status": "ok"}

    @app.get("/sources", dependencies=[Depends(_auth)])
    async def sources() -> list[dict[str, Any]]:
        return app.state.store.list_sources()

    @app.get("/findings", dependencies=[Depends(_auth)])
    async def findings(
        source: str | None = Query(default=None),
        kind: str | None = Query(default=None),
        since: str | None = Query(default=None),
        limit: int = Query(default=200, ge=1, le=1000),
    ) -> list[dict[str, Any]]:
        if kind and kind not in ("new", "changed", "gone"):
            raise HTTPException(status_code=400, detail="kind must be new|changed|gone")
        return app.state.store.list_findings(source, kind, since, limit)

    @app.get("/findings/{finding_id}", dependencies=[Depends(_auth)])
    async def finding_detail(finding_id: int) -> dict[str, Any]:
        f = app.state.store.get_finding(finding_id)
        if not f:
            raise HTTPException(status_code=404, detail="finding not found")
        f["history"] = app.state.store.item_history(f["source_id"], f["item_key"])
        return f

    @app.get("/snapshots/{snapshot_id}", dependencies=[Depends(_auth)])
    async def snapshot(snapshot_id: int) -> dict[str, Any]:
        s = app.state.store.get_snapshot(snapshot_id)
        if not s:
            raise HTTPException(status_code=404, detail="snapshot not found")
        return s

    @app.get("/runs", dependencies=[Depends(_auth)])
    async def runs(
        source: str | None = Query(default=None),
        limit: int = Query(default=50, ge=1, le=500),
    ) -> list[dict[str, Any]]:
        return app.state.store.list_runs(source, limit)

    return app


def main() -> None:
    import uvicorn

    logging.basicConfig(level=logging.INFO)
    uvicorn.run(
        "scout.server:create_app",
        factory=True,
        host=os.environ.get("SCOUT_HOST", "0.0.0.0"),
        port=int(os.environ.get("SCOUT_PORT", "8787")),
    )


if __name__ == "__main__":
    main()
