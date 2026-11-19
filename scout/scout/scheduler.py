"""APScheduler wiring: run all due sources every minute tick."""
from __future__ import annotations

import asyncio
import logging
from pathlib import Path
from typing import Any

from apscheduler.schedulers.asyncio import AsyncIOScheduler

from .sources import load_specs, run_all_due
from .store import Store

log = logging.getLogger("scout.scheduler")

_TICK_SECONDS = 60


class Scheduler:
    def __init__(self, store: Store, sources_path: Path) -> None:
        self.store = store
        self.sources_path = sources_path
        self._sched = AsyncIOScheduler()
        self._specs: list[dict[str, Any]] = []
        self._spec_mtime: float | None = None

    def reload_specs(self) -> None:
        self._specs = load_specs(self.sources_path)
        self.store.sync_sources(self._specs)
        self._spec_mtime = self.sources_path.stat().st_mtime
        log.info("specs loaded: %d sources", len(self._specs))

    def maybe_reload(self) -> None:
        try:
            mtime = self.sources_path.stat().st_mtime
            if self._spec_mtime is None or mtime != self._spec_mtime:
                self.reload_specs()
        except FileNotFoundError:
            log.warning("sources.toml missing at %s", self.sources_path)

    async def tick(self) -> None:
        self.maybe_reload()
        results = await run_all_due(self.store, self._specs)
        for r in results:
            if r.get("error"):
                log.warning("run %s failed: %s", r["source"], r["error"])
            elif r.get("skipped"):
                log.debug("run %s skipped (disabled)", r["source"])
            else:
                log.info(
                    "run %s: %d items, %d findings",
                    r["source"],
                    r["items"],
                    r["findings"],
                )

    def start(self) -> None:
        self.reload_specs()
        self._sched.add_job(
            self._tick_job, "interval", seconds=_TICK_SECONDS, id="tick",
            max_instances=1, coalesce=True,
        )
        self._sched.start()

    async def _tick_job(self) -> None:
        try:
            await self.tick()
        except Exception:  # noqa: BLE001 — scheduler must survive any tick
            log.exception("tick failed")

    def shutdown(self) -> None:
        if self._sched.running:
            self._sched.shutdown(wait=False)


async def run_once(store: Store, sources_path: Path) -> list[dict[str, Any]]:
    """One immediate pass over due sources (CLI/startup smoke)."""
    specs = load_specs(sources_path)
    store.sync_sources(specs)
    return await run_all_due(store, specs)
